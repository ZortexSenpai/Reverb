//! Output stream setup. The rendering itself lives in `render.rs`; this
//! module picks a device, negotiates a config, and opens a backend:
//! cpal everywhere, or WASAPI exclusive mode on Windows when the user asks
//! for it (cpal can only open shared-mode WASAPI streams).

use std::sync::Arc;

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{FromSample, SizedSample};

use super::eq::EqCoeffSet;
use super::render::Renderer;
use super::ring::SharedAudioState;

#[derive(Debug, thiserror::Error)]
pub enum OutputError {
    #[error("no audio output device available")]
    NoDevice,
    #[error("could not query device config: {0}")]
    Config(String),
    #[error("could not open audio stream: {0}")]
    Stream(String),
}

/// The open stream, whichever backend produced it. Only ever held, never
/// read: dropping it is what stops playback.
#[allow(dead_code)]
enum Backend {
    Cpal(cpal::Stream),
    #[cfg(target_os = "windows")]
    Exclusive(super::wasapi::ExclusiveStream),
    /// Released ahead of a rebuild (see `AudioOutput::close`).
    Closed,
}

pub struct AudioOutput {
    _backend: Backend,
    pub sample_rate: u32,
    pub channels: usize,
    /// Display name of the device the stream actually opened, when a specific
    /// one was asked for; `None` means the system default. Device *ids* are
    /// what preferences hold, and they are not meant to be read by anyone —
    /// "alsa:hw:CARD=1,DEV=0" is the same DAC as "FIIO BTR17". This also tells
    /// the truth after a fallback, where the device opened is not the one asked
    /// for.
    pub device: Option<String>,
    /// True when the stream owns the device outright (WASAPI exclusive, or a
    /// direct ALSA device on Linux). macOS hog mode is tracked separately, as
    /// it is a device property rather than a stream one.
    pub exclusive: bool,
    /// The device asked for was not there at all, so the system default was
    /// opened instead. Distinct from a device that exists but would not open
    /// (busy, no format in common): that one is worth retrying, this one means
    /// the saved preference points at hardware that has left.
    pub device_missing: bool,
}

impl AudioOutput {
    /// Stop and release the device now, rather than when this value is
    /// dropped. A device held exclusively cannot be opened twice, so a
    /// rebuild has to let go before it can take it again.
    pub fn close(&mut self) {
        self._backend = Backend::Closed;
    }

    /// No stream is open — the engine must reopen one or stay silent.
    pub fn is_closed(&self) -> bool {
        matches!(self._backend, Backend::Closed)
    }
}

/// Device-level exclusive access, held independently of the stream so the
/// stream can be rebuilt (e.g. for a rate switch) without losing the claim.
pub struct ExclusiveAccess {
    #[cfg(target_os = "macos")]
    _guard: Option<super::macos::HogGuard>,
}

impl ExclusiveAccess {
    pub fn inactive() -> Self {
        Self {
            #[cfg(target_os = "macos")]
            _guard: None,
        }
    }

    /// Best effort; a no-op on platforms without device hogging.
    pub fn acquire(device_name: Option<&str>) -> Self {
        #[cfg(target_os = "macos")]
        {
            Self { _guard: super::macos::HogGuard::acquire(device_name) }
        }
        #[cfg(not(target_os = "macos"))]
        {
            let _ = device_name;
            Self {}
        }
    }
}

/// A supported stream config at exactly `rate`, keeping the default
/// config's channel count and sample format.
fn config_at_rate(
    device: &cpal::Device,
    rate: u32,
    default: &cpal::SupportedStreamConfig,
) -> Option<cpal::SupportedStreamConfig> {
    device
        .supported_output_configs()
        .ok()?
        .filter(|c| {
            c.channels() == default.channels() && c.sample_format() == default.sample_format()
        })
        .find(|c| c.min_sample_rate() <= rate && rate <= c.max_sample_rate())
        .map(|c| c.with_sample_rate(rate))
}

/// Ask the OS to run the device at `rate`. On macOS this re-clocks the DAC
/// (nominal rate); elsewhere the stream config request is all there is, so
/// the attempt always proceeds.
fn try_native_rate(device_name: Option<&str>, rate: u32) -> bool {
    #[cfg(target_os = "macos")]
    {
        super::macos::switch_device_rate(device_name, rate)
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = (device_name, rate);
        true
    }
}

/// The rate the hardware is *actually* running at, when that is knowable.
///
/// The stream rate alone proves nothing: ALSA's `default` device and WASAPI
/// shared mode both accept any rate and resample below us. `None` means "could
/// not tell" and must never be read as confirmation.
pub fn hardware_rate(device_name: Option<&str>, exclusive: bool, stream_rate: u32) -> Option<u32> {
    #[cfg(target_os = "macos")]
    {
        // cpal opens the stream at the device's nominal rate, and
        // `switch_device_rate` already re-clocked the DAC to it.
        let _ = (device_name, exclusive);
        Some(stream_rate)
    }
    #[cfg(target_os = "windows")]
    {
        // Exclusive mode negotiated the format with the hardware directly.
        // Shared mode goes through the mixer, whose format is the real rate.
        if exclusive {
            Some(stream_rate)
        } else {
            super::wasapi::shared_mix_rate(device_name)
        }
    }
    #[cfg(target_os = "linux")]
    {
        let _ = stream_rate;
        // A direct device is one we named, so its card can be read on its own.
        // Otherwise we are on a sound-server route with no way to tell which
        // card is carrying it, and every open card is a candidate.
        match exclusive.then_some(device_name).flatten().and_then(super::reserve::card_number) {
            Some(card) => card_pcm_rate(&std::path::Path::new("/proc/asound").join(format!("card{card}"))),
            None => linux_pcm_rate(),
        }
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
    {
        let _ = (device_name, exclusive, stream_rate);
        None
    }
}

/// Rate of the open ALSA playback PCM on one card, read from `/proc/asound`.
#[cfg(target_os = "linux")]
fn card_pcm_rate(card: &std::path::Path) -> Option<u32> {
    for pcm in std::fs::read_dir(card).ok().into_iter().flatten().flatten() {
        // Playback PCMs only: "pcm0p", "pcm1p", ...
        let name = pcm.file_name();
        let name = name.to_string_lossy();
        if !(name.starts_with("pcm") && name.ends_with('p')) {
            continue;
        }
        for sub in std::fs::read_dir(pcm.path()).ok().into_iter().flatten().flatten() {
            let text = std::fs::read_to_string(sub.path().join("hw_params")).ok();
            if let Some(rate) = text.as_deref().and_then(parse_hw_params) {
                return Some(rate);
            }
        }
    }
    None
}

/// Rate of the open ALSA playback PCM across every card — the best that can be
/// done for a device we did not name. Ambiguous when several cards are playing
/// (another app on another device), which is reported as unknown rather than
/// guessed at.
#[cfg(target_os = "linux")]
fn linux_pcm_rate() -> Option<u32> {
    let mut found: Option<u32> = None;
    for card in std::fs::read_dir("/proc/asound").ok()?.flatten() {
        match (found, card_pcm_rate(&card.path())) {
            (_, None) => {}
            (Some(seen), Some(rate)) if seen != rate => return None,
            (_, Some(rate)) => found = Some(rate),
        }
    }
    found
}

/// Pull the rate out of one ALSA `hw_params` file. Reads `closed` (and so
/// yields nothing) whenever the PCM is idle.
#[cfg(target_os = "linux")]
fn parse_hw_params(text: &str) -> Option<u32> {
    text.lines()
        .find_map(|line| line.trim().strip_prefix("rate:"))
        .and_then(|rest| rest.split_whitespace().next())
        .and_then(|n| n.parse().ok())
}

/// One selectable output, identified by cpal's `DeviceId` (which is designed
/// to be persisted) rather than by its display name — ALSA gives eleven
/// aliases of one DAC the *same* name, so names cannot identify a device.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OutputDevice {
    pub id: String,
    pub label: String,
    /// Addresses the hardware directly, bypassing any sound server. On Linux
    /// this is the only way to get bit-perfect output — and it takes the card
    /// away from every other application while the stream is open.
    pub direct: bool,
}

/// Available output devices for the settings picker, curated per platform.
pub fn list_output_devices() -> Vec<OutputDevice> {
    let host = cpal::default_host();
    let Ok(devices) = host.output_devices() else { return Vec::new() };

    let mut out: Vec<OutputDevice> = Vec::new();
    for device in devices {
        let (Ok(desc), Ok(id)) = (device.description(), device.id()) else { continue };
        let id = id.to_string();
        let label = desc.name().to_owned();

        #[cfg(target_os = "linux")]
        {
            // ALSA advertises one card under many plugin aliases — all with
            // the *same* display name, which is what made the picker repeat
            // the FiiO eleven times. Keep one row per card, holding the alias
            // that addresses the hardware most directly; the sound-server
            // routes stay reachable as "System default".
            let Some(rank) = alsa_rank(&id) else { continue };
            match out.iter().position(|d| d.label == label) {
                Some(i) => {
                    if rank < alsa_rank(&out[i].id).unwrap_or(u8::MAX) {
                        out[i].id = id;
                    }
                }
                None => out.push(OutputDevice { id, label, direct: true }),
            }
        }
        #[cfg(not(target_os = "linux"))]
        {
            out.push(OutputDevice { id, label, direct: false });
        }
    }
    out
}

/// cpal renders ids as `<host>:<device>`; on Linux the second half is the
/// ALSA PCM name.
#[cfg(target_os = "linux")]
fn alsa_pcm(id: &str) -> Option<&str> {
    id.strip_prefix("alsa:")
}

/// How directly an ALSA alias addresses the card — lower is better. `None`
/// for the aliases we never want to offer: sound-server routes, the null
/// sink, and the surround/S-PDIF variants of a card we already list.
#[cfg(target_os = "linux")]
fn alsa_rank(id: &str) -> Option<u8> {
    let pcm = alsa_pcm(id)?;
    let plugin = pcm.split(':').next()?;
    match plugin {
        // Raw hardware: bit-perfect, no conversion, no sharing.
        "hw" => Some(0),
        // Same card, but ALSA converts rate/format when the DAC cannot.
        "plughw" => Some(1),
        // The card's own default PCM.
        "sysdefault" => Some(2),
        _ => None,
    }
}

/// On Linux, a device is addressed strictly (`hw:`) for bit-perfect output or
/// tolerantly (`plughw:`, ALSA converts rate/format as needed). Everything
/// else passes through untouched.
#[cfg(target_os = "linux")]
fn resolve_alsa_id(id: &str, exclusive: bool) -> Option<String> {
    if exclusive {
        return None;
    }
    let rest = alsa_pcm(id)?.strip_prefix("hw:")?;
    Some(format!("alsa:plughw:{rest}"))
}

/// ALSA's EBUSY on a direct device means the sound server (or another app)
/// holds the card — worth saying plainly, since the fix is a user action.
fn explain_busy(e: OutputError) -> OutputError {
    let text = e.to_string();
    if text.contains("Device or resource busy") || text.contains("temporarily busy") {
        OutputError::Stream(
            "device is in use — the sound server or another app holds it. Stop other \
             playback, or pick System default to share the device."
                .to_owned(),
        )
    } else {
        e
    }
}

/// Whether a device id addresses hardware directly, bypassing any sound
/// server — which means the server has to be asked to let go of the card
/// first, whether or not exclusive access is switched on.
pub fn is_direct_device(id: &str) -> bool {
    #[cfg(target_os = "linux")]
    {
        // `sysdefault:CARD=…` counts: it is the card's own PCM, not a
        // sound-server route, so it takes the hardware like the others do.
        alsa_pcm(id).is_some_and(|pcm| {
            ["hw:", "plughw:", "sysdefault:"].iter().any(|direct| pcm.starts_with(direct))
        })
    }
    #[cfg(not(target_os = "linux"))]
    {
        let _ = id;
        false
    }
}

/// Resolve a saved device preference. Preferences are cpal `DeviceId`s, but
/// settings written before that switch hold a display name — those still
/// resolve, ambiguously but no worse than they did then.
fn find_device(host: &cpal::Host, wanted: &str, exclusive: bool) -> Option<cpal::Device> {
    // Ids first, in preference order. On Linux a non-exclusive stream prefers
    // the `plughw` alias of the same card, so ALSA converts rather than
    // refusing a rate the DAC cannot do; exclusive keeps the raw `hw` alias.
    let mut ids: Vec<String> = Vec::new();
    #[cfg(target_os = "linux")]
    ids.extend(resolve_alsa_id(wanted, exclusive));
    #[cfg(not(target_os = "linux"))]
    let _ = exclusive;
    ids.push(wanted.to_owned());

    let mut devices: Vec<cpal::Device> = host.output_devices().ok()?.collect();
    for id in ids {
        if let Some(index) =
            devices.iter().position(|d| d.id().is_ok_and(|got| got.to_string() == id))
        {
            return Some(devices.swap_remove(index));
        }
    }
    // Settings written before ids were stored hold a display name.
    let index = devices
        .iter()
        .position(|d| d.description().is_ok_and(|desc| desc.name() == wanted))?;
    Some(devices.swap_remove(index))
}

pub fn open_output(
    shared: Arc<SharedAudioState>,
    consumer: rtrb::Consumer<f32>,
    eq_output: triple_buffer::Output<EqCoeffSet>,
    error_tx: crossbeam_channel::Sender<String>,
    device_name: Option<&str>,
    preferred_rate: Option<u32>,
    exclusive: bool,
) -> Result<AudioOutput, OutputError> {
    // Exclusive mode is a different backend on Windows, not a flag on this
    // one. Any failure (device held by another app, no format in common)
    // falls through to the shared-mode path rather than leaving no audio.
    #[cfg(target_os = "windows")]
    let (consumer, eq_output) = if exclusive {
        match super::wasapi::prepare(device_name, preferred_rate) {
            Ok(prepared) => {
                let stream = prepared.start(shared.clone(), consumer, eq_output, error_tx);
                return Ok(AudioOutput {
                    sample_rate: stream.sample_rate,
                    channels: stream.channels,
                    device: device_name.and_then(device_label),
                    exclusive: true,
                    // WASAPI resolved the device itself and would have errored
                    // out (into the shared path below) if it were gone.
                    device_missing: false,
                    _backend: Backend::Exclusive(stream),
                });
            }
            // Nothing was consumed, so shared mode can still have them.
            Err(e) => {
                log::warn!("exclusive output unavailable ({e}); using shared mode");
                (consumer, eq_output)
            }
        }
    } else {
        (consumer, eq_output)
    };
    #[cfg(not(target_os = "windows"))]
    let _ = exclusive;

    let host = cpal::default_host();
    // A configured device that disappeared falls back to the default — and says
    // so, so the caller can stop pointing at hardware that has left.
    let requested = device_name.and_then(|wanted| find_device(&host, wanted, exclusive));
    let device_missing = device_name.is_some() && requested.is_none();
    let device =
        requested.or_else(|| host.default_output_device()).ok_or(OutputError::NoDevice)?;
    let supported = device
        .default_output_config()
        .map_err(|e| explain_busy(OutputError::Config(e.to_string())))?;

    // Bit-perfect path: re-clock the device to the source rate and open the
    // stream at that rate. Any failure falls back to the default config
    // (the engine resamples, as before).
    let resolved_name = device.description().ok().map(|d| d.name().to_owned());
    let chosen = preferred_rate
        .filter(|rate| *rate != supported.sample_rate())
        .filter(|rate| try_native_rate(resolved_name.as_deref(), *rate))
        .and_then(|rate| config_at_rate(&device, rate, &supported))
        .unwrap_or(supported);

    let sample_format = chosen.sample_format();
    let config: cpal::StreamConfig = chosen.config();
    let sample_rate = config.sample_rate;
    let channels = config.channels as usize;

    let err_cb = move |e: cpal::Error| {
        let _ = error_tx.send(e.to_string());
    };

    let stream = match sample_format {
        cpal::SampleFormat::F32 => build_stream::<f32>(&device, config, shared, consumer, eq_output, err_cb),
        cpal::SampleFormat::I16 => build_stream::<i16>(&device, config, shared, consumer, eq_output, err_cb),
        cpal::SampleFormat::U16 => build_stream::<u16>(&device, config, shared, consumer, eq_output, err_cb),
        cpal::SampleFormat::I32 => build_stream::<i32>(&device, config, shared, consumer, eq_output, err_cb),
        other => return Err(OutputError::Stream(format!("unsupported sample format {other:?}"))),
    }
    .map_err(explain_busy)?;

    stream.play().map_err(|e| OutputError::Stream(e.to_string()))?;

    // A direct ALSA device bypasses the sound server, which is what exclusive
    // access amounts to on Linux.
    #[cfg(target_os = "linux")]
    let exclusive = device.id().is_ok_and(|id| {
        // Ids are rendered "<host>:<pcm>", so the ALSA plugin name is the
        // second segment: "alsa:hw:CARD=1,DEV=0".
        alsa_pcm(&id.to_string())
            .is_some_and(|pcm| pcm.starts_with("hw:") || pcm.starts_with("plughw:"))
    });
    #[cfg(not(target_os = "linux"))]
    let exclusive = false;

    Ok(AudioOutput {
        _backend: Backend::Cpal(stream),
        sample_rate,
        channels,
        device: device_name.and(resolved_name),
        exclusive,
        device_missing,
    })
}

/// The display name behind a device id. Only the WASAPI exclusive path needs
/// this: it resolves the device itself, so unlike the cpal path it never holds
/// a `cpal::Device` to ask.
#[cfg(target_os = "windows")]
fn device_label(id: &str) -> Option<String> {
    let device = cpal::default_host()
        .output_devices()
        .ok()?
        .find(|d| d.id().is_ok_and(|got| got.to_string() == id))?;
    device.description().ok().map(|d| d.name().to_owned())
}

fn build_stream<T>(
    device: &cpal::Device,
    config: cpal::StreamConfig,
    shared: Arc<SharedAudioState>,
    consumer: rtrb::Consumer<f32>,
    eq_output: triple_buffer::Output<EqCoeffSet>,
    err_cb: impl FnMut(cpal::Error) + Send + 'static,
) -> Result<cpal::Stream, OutputError>
where
    T: SizedSample + FromSample<f32>,
{
    let channels = config.channels as usize;
    let mut renderer = Renderer::new(shared, consumer, eq_output, config.sample_rate, channels);

    device
        .build_output_stream(
            config,
            move |data: &mut [T], _info: &cpal::OutputCallbackInfo| renderer.render(data),
            err_cb,
            None,
        )
        .map_err(|e| OutputError::Stream(e.to_string()))
}

#[cfg(all(test, target_os = "linux"))]
mod tests {
    use super::{card_pcm_rate, parse_hw_params};

    /// A stand-in for one `/proc/asound/card<N>` directory.
    fn fake_card(pcms: &[(&str, &str)]) -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        for (pcm, hw_params) in pcms {
            let sub = dir.path().join(pcm).join("sub0");
            std::fs::create_dir_all(&sub).unwrap();
            std::fs::write(sub.join("hw_params"), hw_params).unwrap();
        }
        dir
    }

    #[test]
    fn reads_the_rate_of_a_cards_open_playback_pcm() {
        let card = fake_card(&[("pcm0p", "format: S32_LE\nrate: 96000 (96000/1)\n")]);
        assert_eq!(card_pcm_rate(card.path()), Some(96_000));
    }

    #[test]
    fn ignores_capture_pcms_and_idle_ones() {
        // A microphone's rate is not the output's.
        let capture = fake_card(&[("pcm0c", "rate: 16000 (16000/1)\n")]);
        assert_eq!(card_pcm_rate(capture.path()), None);
        // Nothing playing: no rate to report, which must not read as confirmed.
        let idle = fake_card(&[("pcm0p", "closed\n")]);
        assert_eq!(card_pcm_rate(idle.path()), None);
        // A card with no PCMs at all, and a path that does not exist.
        assert_eq!(card_pcm_rate(fake_card(&[]).path()), None);
        assert_eq!(card_pcm_rate(std::path::Path::new("/proc/asound/card99")), None);
    }

    #[test]
    fn reads_the_rate_of_an_open_pcm() {
        let open = "access: MMAP_INTERLEAVED\n\
                    format: S32_LE\n\
                    subformat: STD\n\
                    channels: 2\n\
                    rate: 48000 (48000/1)\n\
                    period_size: 1024\n\
                    buffer_size: 8192\n";
        assert_eq!(parse_hw_params(open), Some(48_000));
    }

    #[test]
    fn an_idle_pcm_reports_nothing() {
        // An unused PCM holds the literal string "closed" — no rate to read,
        // which must not be mistaken for a confirmed one.
        assert_eq!(parse_hw_params("closed\n"), None);
        assert_eq!(parse_hw_params(""), None);
    }
}
