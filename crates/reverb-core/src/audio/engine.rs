//! The engine thread: owns the decoder, resampler, cpal stream and the
//! producer side of the PCM ring. Controlled via `EngineCommand`s, reports
//! back via `EngineEvent`s.
//!
//! Queue philosophy: the frontend is the brain. The engine only ever knows
//! the *current* track and an optional *next* track (for gapless flow).

use std::collections::VecDeque;
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::{Duration, Instant};

use crossbeam_channel::{unbounded, Receiver, Sender};

use super::decoder::TrackDecoder;
use super::eq::{EqCoeffSet, EqState};
use super::events::{EngineEvent, PlaybackState, SignalPathInfo, TrackChangeReason};
use super::output::{is_direct_device, open_output, AudioOutput, ExclusiveAccess};
use super::resampler::StereoResampler;
use super::ring::{volume_taper, SharedAudioState, RING_CAPACITY_SECS};
use super::source::{open_stream_source, stream_http_client};
use crate::subsonic::models::ReplayGain;

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ReplayGainMode {
    Off,
    Track,
    Album,
}

#[derive(Debug, Clone, Copy, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReplayGainSettings {
    pub mode: ReplayGainMode,
    /// Extra gain applied on top of the tag value, in dB.
    pub preamp_db: f32,
    /// Cap gain so `peak * gain <= 1.0` when peak data is available.
    pub clip_protection: bool,
    /// Gain for tracks WITHOUT usable ReplayGain tags, in dB. Untagged
    /// tracks otherwise play at full mastered loudness next to normalized
    /// ones.
    #[serde(default)]
    pub fallback_db: f32,
}

impl Default for ReplayGainSettings {
    fn default() -> Self {
        Self {
            mode: ReplayGainMode::Off,
            preamp_db: 0.0,
            clip_protection: true,
            fallback_db: 0.0,
        }
    }
}

impl ReplayGainSettings {
    /// Linear gain for a track (1.0 when off; the fallback when the track
    /// has no usable tags).
    fn gain_for(&self, rg: Option<&ReplayGain>) -> f32 {
        if self.mode == ReplayGainMode::Off {
            return 1.0;
        }
        let fallback = 10f32.powf(self.fallback_db / 20.0);
        let Some(rg) = rg else { return fallback };
        let (gain_db, peak) = match self.mode {
            ReplayGainMode::Track => (rg.track_gain.or(rg.album_gain), rg.track_peak),
            ReplayGainMode::Album => (rg.album_gain.or(rg.track_gain), rg.album_peak),
            ReplayGainMode::Off => unreachable!("handled above"),
        };
        let Some(gain_db) = gain_db else { return fallback };
        let mut gain = 10f32.powf((gain_db + self.preamp_db) / 20.0);
        if self.clip_protection {
            if let Some(peak) = peak.filter(|p| *p > 0.0) {
                gain = gain.min(1.0 / peak);
            }
        }
        gain
    }
}

/// Everything the engine needs to play one track. Built by the app layer
/// (URL comes from the Subsonic client). Display metadata rides along so the
/// OS media session can be updated without a server round-trip.
#[derive(Debug, Clone)]
pub struct TrackSource {
    pub track_id: String,
    pub url: String,
    /// File extension hint for the format probe ("flac", "mp3", ...).
    pub ext_hint: Option<String>,
    /// Duration from server metadata (fallback when the container lies).
    pub duration_ms: Option<u64>,
    pub title: Option<String>,
    /// The full credit as the server formats it ("A feat. B") — what the OS
    /// media session and the server's own scrobble see.
    pub artist: Option<String>,
    /// Just the first credited artist, when the server enumerated them
    /// separately. `None` on servers without the OpenSubsonic artists
    /// extension, where the credit is one opaque string.
    pub first_artist: Option<String>,
    pub album: Option<String>,
    /// kbps from server metadata (signal path display).
    pub bit_rate: Option<u32>,
    /// Loudness tags for ReplayGain, when the server provides them.
    pub replay_gain: Option<ReplayGain>,
}

#[derive(Debug)]
pub enum EngineCommand {
    PlayTrack(TrackSource),
    SetNext(Option<TrackSource>),
    Pause,
    Resume,
    Toggle,
    Stop,
    Seek { position_ms: u64 },
    SetVolume { volume: f32 },
    SetEq(EqState),
    SetReplayGain(ReplayGainSettings),
    /// Percentage of a song that must play before it scrobbles (10–100).
    SetScrobblePercent(u32),
    /// Crossfade length between tracks in ms (0 = gapless transitions).
    SetCrossfade { ms: u64 },
    /// Bit-perfect output options: switch the device to each track's native
    /// sample rate, and/or hold exclusive (hog) access to it.
    SetOutputMode { auto_rate: bool, exclusive: bool },
    /// Switch output device (`None` = system default). Reopens the stream.
    SetDevice(Option<String>),
    Shutdown,
}

#[derive(Clone)]
pub struct EngineHandle {
    cmd_tx: Sender<EngineCommand>,
    pub shared: Arc<SharedAudioState>,
}

impl EngineHandle {
    pub fn send(&self, cmd: EngineCommand) {
        let _ = self.cmd_tx.send(cmd);
    }
}

pub fn spawn_engine(event_tx: Sender<EngineEvent>, initial_device: Option<String>) -> EngineHandle {
    let (cmd_tx, cmd_rx) = unbounded();
    let shared = Arc::new(SharedAudioState::new());
    let shared_for_thread = shared.clone();
    let event_tx_for_err = event_tx.clone();

    std::thread::Builder::new()
        .name("reverb-audio-engine".into())
        .spawn(move || match Engine::init(cmd_rx, event_tx, shared_for_thread, initial_device) {
            Ok(mut engine) => engine.run(),
            Err(e) => {
                log::error!("audio engine failed to start: {e}");
                let _ = event_tx_for_err.send(EngineEvent::Error {
                    code: "audio-init".into(),
                    message: e,
                    track_id: None,
                    recoverable: false,
                });
            }
        })
        .expect("spawning the audio engine thread cannot fail");

    EngineHandle { cmd_tx, shared }
}

/// Playback bookkeeping for the track currently being *decoded*.
struct CurrentTrack {
    src: TrackSource,
    decoder: TrackDecoder,
    resampler: StereoResampler,
    duration_ms: u64,
    /// Linear ReplayGain factor baked into the samples as they're pushed.
    replay_gain: f32,
    /// Decoder has hit EOF; whatever remains is in `pending`/ring.
    ended: bool,
    /// Output-domain frames produced so far, in track time (resynced on
    /// seek). Drives the crossfade trigger.
    out_frames: u64,
}

/// An in-flight crossfade: the incoming track decodes in parallel with the
/// outgoing one and the two are mixed with equal-power gains before they
/// enter the ring.
struct CrossfadeState {
    incoming: CurrentTrack,
    /// Fade length in output frames (>= 1).
    total_frames: u64,
    mixed_frames: u64,
    /// Resampled output-domain samples waiting to be mixed.
    out_queue: VecDeque<f32>,
    in_queue: VecDeque<f32>,
}

/// Bookkeeping for the track currently being *heard* (lags `CurrentTrack`
/// by up to the ring length during gapless transitions).
struct DisplayTrack {
    src: TrackSource,
    /// `frames_played` value at which this track started.
    start_frame: u64,
    /// Playback position at `start_frame`, in ms.
    base_ms: f64,
    duration_ms: u64,
    scrobbled: bool,
}

enum BoundaryKind {
    /// The next track becomes audible at this frame.
    TrackStart(Box<DisplayTrack>),
    /// Playback runs out at this frame.
    End { last_track_id: String },
}

struct Boundary {
    at_frame: u64,
    kind: BoundaryKind,
}

/// Whether opening `device` under `exclusive` will take the hardware for
/// itself, which is what forces the release-then-open ordering in
/// `rebuild_output`.
///
/// On Linux, exclusive access *is* a property of the device: it means
/// addressing a card directly (`hw:` rather than `plughw:`), so a sound-server
/// route has no card to take and the flag on its own must not force a teardown.
/// macOS hog mode and WASAPI exclusive mode apply to the default device too.
fn takes_device(device: Option<&str>, exclusive: bool) -> bool {
    let direct = device.is_some_and(is_direct_device);
    #[cfg(target_os = "linux")]
    {
        let _ = exclusive;
        direct
    }
    #[cfg(not(target_os = "linux"))]
    {
        direct || exclusive
    }
}

struct Engine {
    cmd_rx: Receiver<EngineCommand>,
    event_tx: Sender<EngineEvent>,
    shared: Arc<SharedAudioState>,
    output: AudioOutput,
    output_error_rx: Receiver<String>,
    output_error_tx: Sender<String>,
    producer: rtrb::Producer<f32>,
    eq_input: triple_buffer::Input<EqCoeffSet>,
    http: reqwest::blocking::Client,

    current: Option<CurrentTrack>,
    next: Option<TrackSource>,
    display: Option<DisplayTrack>,
    /// Resampled stereo samples waiting for ring space.
    pending: VecDeque<f32>,
    /// Reusable decode buffer.
    scratch: Vec<f32>,
    boundaries: VecDeque<Boundary>,
    /// In-flight track-to-track crossfade (None = gapless transitions).
    fade: Option<CrossfadeState>,
    crossfade_ms: u64,
    /// Switch the output device to each track's native rate (bit-perfect).
    auto_rate: bool,
    exclusive_output: bool,
    /// Holds hog-mode access when `exclusive_output` is on (macOS).
    exclusive_access: ExclusiveAccess,
    /// Rate-limits the "we have no output at all" recovery attempts.
    last_output_recovery: Instant,
    /// A natural transition parked while the ring drains, so the device can
    /// change rate before this track starts.
    rate_switch: Option<CurrentTrack>,
    /// Producer-side frame counter, kept in the `frames_played` domain
    /// (resynced to `frames_played` after every flush).
    frames_pushed: u64,
    state: PlaybackState,
    eq_state: EqState,
    replay_gain: ReplayGainSettings,
    scrobble_percent: u32,
    preferred_device: Option<String>,
    /// Keeps the sound server off the card we are playing to, for as long as we
    /// hold it (Linux). Declared after `output` so that dropping the engine
    /// closes the stream before the card is handed back.
    #[cfg(target_os = "linux")]
    card_claim: Option<super::reserve::CardClaim>,
    buffering_reported: bool,
    last_tick: Instant,
    /// Navidrome expires now-playing entries; re-announce periodically.
    last_now_playing: Instant,
}

impl Engine {
    fn init(
        cmd_rx: Receiver<EngineCommand>,
        event_tx: Sender<EngineEvent>,
        shared: Arc<SharedAudioState>,
        initial_device: Option<String>,
    ) -> Result<Self, String> {
        let (output_error_tx, output_error_rx) = unbounded();
        // Probe the device rate first so the ring can be sized for it.
        // (Ring is created before the stream because the consumer moves in.)
        let probe_rate = 48_000u32;
        let ring_capacity =
            ((f64::from(probe_rate) * RING_CAPACITY_SECS) as usize).next_power_of_two() * 2;
        let (mut producer, consumer) = rtrb::RingBuffer::new(ring_capacity);

        let (mut eq_input, eq_output) = triple_buffer::triple_buffer(&EqCoeffSet::IDENTITY);

        // A saved direct device needs the card freed before it can be opened,
        // exactly as on a later switch — otherwise the app starts with no
        // audio at all whenever the sound server happens to hold it. Resolved
        // inside the `cfg` because no other platform has a card to claim, and a
        // binding used only by a `cfg`-ed statement is an unused one elsewhere.
        #[cfg(target_os = "linux")]
        let mut card_claim = initial_device
            .as_deref()
            .filter(|device| is_direct_device(device))
            .and_then(super::reserve::CardClaim::acquire);
        let exclusive_access = ExclusiveAccess::inactive();

        let output = match open_output(
            shared.clone(),
            consumer,
            eq_output,
            output_error_tx.clone(),
            initial_device.as_deref(),
            None,
            false,
        ) {
            Ok(output) => output,
            // Starting on the wrong device beats starting mute: the frontend
            // pushes the real preferences moments later anyway.
            Err(e) if initial_device.is_some() => {
                log::warn!("{e}; starting on the default device instead");
                // The shared default device is not the card we took.
                #[cfg(target_os = "linux")]
                {
                    card_claim = None;
                }
                let (fallback_producer, fallback_consumer) = rtrb::RingBuffer::new(ring_capacity);
                let (fallback_eq_input, fallback_eq_output) =
                    triple_buffer::triple_buffer(&EqCoeffSet::IDENTITY);
                producer = fallback_producer;
                eq_input = fallback_eq_input;
                open_output(
                    shared.clone(),
                    fallback_consumer,
                    fallback_eq_output,
                    output_error_tx.clone(),
                    None,
                    None,
                    false,
                )
                .map_err(|e| e.to_string())?
            }
            Err(e) => return Err(e.to_string()),
        };

        // A saved device that is not plugged in at all: `open_output` opened the
        // system default, so the preference follows it rather than sitting there
        // inert — nothing here re-attaches a device that comes back, and a stale
        // preference would keep the settings picker naming absent hardware.
        let initial_device = if output.device_missing {
            if let Some(lost) = initial_device {
                log::warn!("saved output device {lost} is gone; starting on the system default");
                // The shared default device is not the card we took.
                #[cfg(target_os = "linux")]
                {
                    card_claim = None;
                }
                let _ = event_tx.send(EngineEvent::OutputDeviceLost { device: lost });
            }
            None
        } else {
            initial_device
        };

        let _ = event_tx.send(EngineEvent::Format { sample_rate: output.sample_rate });

        Ok(Self {
            cmd_rx,
            event_tx,
            shared,
            output,
            output_error_rx,
            output_error_tx,
            producer,
            eq_input,
            http: stream_http_client(),
            current: None,
            next: None,
            display: None,
            pending: VecDeque::new(),
            scratch: Vec::new(),
            boundaries: VecDeque::new(),
            fade: None,
            crossfade_ms: 0,
            auto_rate: false,
            exclusive_output: false,
            exclusive_access,
            last_output_recovery: Instant::now(),
            rate_switch: None,
            frames_pushed: 0,
            state: PlaybackState::Stopped,
            eq_state: EqState::default(),
            replay_gain: ReplayGainSettings::default(),
            scrobble_percent: 80,
            preferred_device: initial_device,
            #[cfg(target_os = "linux")]
            card_claim,
            buffering_reported: false,
            last_tick: Instant::now(),
            last_now_playing: Instant::now(),
        })
    }

    fn run(&mut self) {
        loop {
            while let Ok(cmd) = self.cmd_rx.try_recv() {
                if matches!(cmd, EngineCommand::Shutdown) {
                    return;
                }
                self.handle(cmd);
            }
            while let Ok(err) = self.output_error_rx.try_recv() {
                log::warn!("audio stream error: {err}; reopening output device");
                if let Err(rebuild_err) = self.rebuild_output(self.auto_rate_target()) {
                    self.emit(EngineEvent::Error {
                        code: "audio-stream".into(),
                        message: format!("{err}; reopen failed: {rebuild_err}"),
                        track_id: None,
                        recoverable: false,
                    });
                }
            }

            // A rebuild that failed after releasing the device leaves us with
            // no output — and, worse, would leave the user silent for good.
            // Keep trying, slowly, until something opens.
            if self.output.is_closed()
                && self.last_output_recovery.elapsed() >= Duration::from_secs(1)
            {
                self.last_output_recovery = Instant::now();
                if self.rebuild_output(self.auto_rate_target()).is_ok() {
                    log::info!("audio output recovered");
                }
            }

            let did_work = self.pump();
            self.tick();

            if !did_work {
                // Idle or ring full: sleep on the command channel so control
                // stays responsive.
                match self.cmd_rx.recv_timeout(Duration::from_millis(10)) {
                    Ok(EngineCommand::Shutdown) => return,
                    Ok(cmd) => self.handle(cmd),
                    Err(_) => {}
                }
            }
        }
    }

    fn emit(&self, event: EngineEvent) {
        let _ = self.event_tx.send(event);
    }

    fn set_state(&mut self, state: PlaybackState) {
        if self.state != state {
            self.state = state;
            self.emit(EngineEvent::State { state });
        }
    }

    // ---- command handling -------------------------------------------------

    fn handle(&mut self, cmd: EngineCommand) {
        match cmd {
            EngineCommand::PlayTrack(src) => self.play_track(src),
            EngineCommand::SetNext(next) => self.next = next,
            EngineCommand::Pause => {
                if self.state == PlaybackState::Playing || self.state == PlaybackState::Buffering {
                    self.shared.paused.store(true, Ordering::Relaxed);
                    self.set_state(PlaybackState::Paused);
                }
            }
            EngineCommand::Resume => {
                if self.state == PlaybackState::Paused {
                    self.shared.paused.store(false, Ordering::Relaxed);
                    self.set_state(PlaybackState::Playing);
                    // Navidrome's now-playing entry likely expired while paused.
                    if let Some(display) = self.display.as_ref() {
                        self.emit(EngineEvent::Scrobble {
                            track_id: display.src.track_id.clone(),
                            submission: false,
                            title: display.src.title.clone(),
                            artist: display.src.artist.clone(),
                            first_artist: display.src.first_artist.clone(),
                            album: display.src.album.clone(),
                            duration_ms: display.duration_ms,
                        });
                        self.last_now_playing = Instant::now();
                    }
                }
            }
            EngineCommand::Toggle => {
                let cmd = if self.state == PlaybackState::Paused {
                    EngineCommand::Resume
                } else {
                    EngineCommand::Pause
                };
                self.handle(cmd);
            }
            EngineCommand::Stop => self.stop(),
            EngineCommand::Seek { position_ms } => self.seek(position_ms),
            EngineCommand::SetVolume { volume } => {
                self.shared.set_volume_amp(volume_taper(volume));
            }
            EngineCommand::SetEq(state) => {
                self.eq_state = state;
                let set = EqCoeffSet::from_state(&self.eq_state, f64::from(self.output.sample_rate));
                self.eq_input.write(set);
                self.emit_signal_path();
            }
            EngineCommand::SetScrobblePercent(percent) => {
                self.scrobble_percent = percent.clamp(10, 100);
            }
            EngineCommand::SetCrossfade { ms } => {
                // Applies from the next transition; an in-flight fade plays out.
                self.crossfade_ms = ms.min(15_000);
            }
            EngineCommand::SetOutputMode { auto_rate, exclusive } => {
                let changed = self.auto_rate != auto_rate || self.exclusive_output != exclusive;
                self.auto_rate = auto_rate;
                self.exclusive_output = exclusive;
                if changed {
                    // Re-open at the current track's rate (or the device
                    // default when auto-rate just turned off).
                    if let Err(e) = self.rebuild_output(self.auto_rate_target()) {
                        self.emit(EngineEvent::Error {
                            code: "audio-device".into(),
                            message: format!("could not apply output mode: {e}"),
                            track_id: None,
                            recoverable: true,
                        });
                    }
                }
            }
            EngineCommand::SetReplayGain(settings) => {
                self.replay_gain = settings;
                // Applies from the next track (gain is baked in at push time).
                if let Some(current) = self.current.as_mut() {
                    current.replay_gain = settings.gain_for(current.src.replay_gain.as_ref());
                }
                self.emit_signal_path();
            }
            EngineCommand::SetDevice(device) => {
                self.preferred_device = device;
                if let Err(e) = self.rebuild_output(self.auto_rate_target()) {
                    self.emit(EngineEvent::Error {
                        code: "audio-device".into(),
                        message: format!("could not switch output device: {e}"),
                        track_id: None,
                        recoverable: true,
                    });
                }
            }
            EngineCommand::Shutdown => unreachable!("handled in run()"),
        }
    }

    fn play_track(&mut self, src: TrackSource) {
        self.set_state(PlaybackState::Buffering);
        self.flush_ring();
        self.pending.clear();
        self.boundaries.clear();
        self.fade = None;
        self.rate_switch = None;
        self.current = None;
        self.shared.paused.store(false, Ordering::Relaxed);

        match self.open_track(&src) {
            Ok(mut current) => {
                // Bit-perfect: run the device at the track's native rate.
                let source_rate = current.decoder.sample_rate();
                if self.auto_rate && source_rate != self.output.sample_rate {
                    if let Err(e) = self.rebuild_output(Some(source_rate)) {
                        log::warn!("rate switch failed; keeping current output: {e}");
                    }
                    match StereoResampler::new(source_rate, self.output.sample_rate) {
                        Ok(resampler) => current.resampler = resampler,
                        Err(e) => {
                            self.display = None;
                            self.set_state(PlaybackState::Stopped);
                            self.emit(EngineEvent::Error {
                                code: "track-open".into(),
                                message: e.to_string(),
                                track_id: Some(src.track_id),
                                recoverable: true,
                            });
                            return;
                        }
                    }
                }
                let duration_ms = current.duration_ms;
                self.display = Some(DisplayTrack {
                    src: src.clone(),
                    start_frame: self.shared.frames_played.load(Ordering::Relaxed),
                    base_ms: 0.0,
                    duration_ms,
                    scrobbled: false,
                });
                self.current = Some(current);
                self.emit(EngineEvent::TrackChanged {
                    track_id: src.track_id.clone(),
                    reason: TrackChangeReason::Command,
                    title: src.title.clone(),
                    artist: src.artist.clone(),
                    album: src.album.clone(),
                    duration_ms,
                });
                self.emit(EngineEvent::Scrobble {
                    track_id: src.track_id,
                    submission: false,
                    title: src.title,
                    artist: src.artist,
                    first_artist: src.first_artist,
                    album: src.album,
                    duration_ms,
                });
                self.last_now_playing = Instant::now();
                self.set_state(PlaybackState::Playing);
                self.emit_position();
                self.emit_signal_path();
            }
            Err(message) => {
                self.current = None;
                self.display = None;
                self.set_state(PlaybackState::Stopped);
                self.emit(EngineEvent::Error {
                    code: "track-open".into(),
                    message,
                    track_id: Some(src.track_id),
                    recoverable: true,
                });
            }
        }
    }

    fn open_track(&self, src: &TrackSource) -> Result<CurrentTrack, String> {
        let url = reqwest::Url::parse(&src.url).map_err(|e| format!("bad stream URL: {e}"))?;
        let source = open_stream_source(self.http.clone(), url)
            .map_err(|e| format!("stream open: {e}"))?;
        let decoder =
            TrackDecoder::open(source, src.ext_hint.as_deref()).map_err(|e| e.to_string())?;
        let resampler = StereoResampler::new(decoder.sample_rate(), self.output.sample_rate)
            .map_err(|e| e.to_string())?;
        let duration_ms = src.duration_ms.or(decoder.duration_ms()).unwrap_or(0);
        let replay_gain = self.replay_gain.gain_for(src.replay_gain.as_ref());
        Ok(CurrentTrack {
            src: src.clone(),
            decoder,
            resampler,
            duration_ms,
            replay_gain,
            ended: false,
            out_frames: 0,
        })
    }

    fn stop(&mut self) {
        self.flush_ring();
        self.pending.clear();
        self.boundaries.clear();
        self.fade = None;
        self.rate_switch = None;
        self.current = None;
        self.display = None;
        self.shared.paused.store(false, Ordering::Relaxed);
        self.set_state(PlaybackState::Stopped);
    }

    fn seek(&mut self, position_ms: u64) {
        let Some(display_src) = self.display.as_ref().map(|d| d.src.clone()) else { return };

        // A parked rate switch means the queued track hasn't started yet;
        // put it back so the paths below treat it as the plain "next".
        if let Some(parked) = self.rate_switch.take() {
            if self.next.is_none() {
                self.next = Some(parked.src);
            }
        }

        // Seeking during a crossfade: abandon the fade and restart cleanly
        // from the track the user is actually hearing.
        if let Some(fade) = self.fade.take() {
            let incoming_src = fade.incoming.src;
            if display_src.track_id != incoming_src.track_id && self.next.is_none() {
                // Still inside the outgoing track: the incoming one goes
                // back to being the queued next.
                self.next = Some(incoming_src);
            }
            self.play_track(display_src);
            self.seek_current(position_ms);
            return;
        }

        let current_id = self.current.as_ref().map(|c| c.src.track_id.clone());

        // Gapless tail edge case: the audible track's decoder is already done
        // and `current` is decoding the *next* track. Re-open the audible
        // track, keeping the queued next intact.
        if current_id.as_deref() != Some(display_src.track_id.as_str()) {
            let queued_next = self.current.take().map(|c| c.src);
            if self.next.is_none() {
                self.next = queued_next;
            }
            self.play_track(display_src);
        }
        self.seek_current(position_ms);
    }

    fn seek_current(&mut self, position_ms: u64) {
        let Some(track_id) = self.current.as_ref().map(|c| c.src.track_id.clone()) else {
            return;
        };
        let seekable = self.current.as_ref().is_some_and(|c| c.decoder.is_seekable());
        if !seekable {
            self.emit(EngineEvent::Error {
                code: "seek-unsupported".into(),
                message: "this stream cannot be seeked (transcoded)".into(),
                track_id: Some(track_id),
                recoverable: true,
            });
            return;
        }

        self.flush_ring();
        self.pending.clear();
        self.boundaries.clear();

        // Surface buffering BEFORE the seek: locating the target can involve
        // network I/O that blocks this thread, and the UI clock must pause
        // for that whole window, not just the ring refill afterwards.
        if self.state == PlaybackState::Playing {
            self.buffering_reported = true;
            self.set_state(PlaybackState::Buffering);
        }

        let secs = position_ms as f64 / 1000.0;
        let result = {
            let current = self.current.as_mut().expect("checked above");
            let result = current.decoder.seek(secs);
            if result.is_ok() {
                current.resampler.reset();
                current.ended = false;
            }
            result
        };
        match result {
            Ok(actual_secs) => {
                // Keep the decoded-frames counter in track time so the
                // crossfade trigger still knows how far from the end we are.
                if let Some(c) = self.current.as_mut() {
                    c.out_frames = (actual_secs * f64::from(self.output.sample_rate)) as u64;
                }
                let frames_played = self.shared.frames_played.load(Ordering::Relaxed);
                if let Some(d) = self.display.as_mut() {
                    d.start_frame = frames_played;
                    d.base_ms = actual_secs * 1000.0;
                }
                self.emit_position();
            }
            Err(e) => {
                self.emit(EngineEvent::Error {
                    code: "seek-failed".into(),
                    message: e.to_string(),
                    track_id: Some(track_id),
                    recoverable: true,
                });
            }
        }
    }

    /// The rate the output should run at right now under auto-rate (None =
    /// device default, or auto-rate off).
    fn auto_rate_target(&self) -> Option<u32> {
        if !self.auto_rate {
            return None;
        }
        self.current
            .as_ref()
            .map(|c| c.decoder.sample_rate())
            .or_else(|| self.rate_switch.as_ref().map(|t| t.decoder.sample_rate()))
    }

    /// Hold `device`'s card against the sound server, handing back whatever we
    /// were holding before. `None` gives the card back and takes nothing.
    ///
    /// Re-claiming a card we already have would mean letting go of it and taking
    /// it straight back — a window the server would use to grab the device — so
    /// an unchanged card is left alone. Callers must have closed the output
    /// stream first when the card is changing.
    #[cfg(target_os = "linux")]
    fn claim_card(&mut self, device: Option<&str>) {
        let wanted = device.and_then(super::reserve::card_number);
        if self.card_claim.as_ref().map(|claim| claim.card()) == wanted {
            return;
        }
        self.card_claim = None; // hands the previous card back
        if wanted.is_some() {
            self.card_claim = device.and_then(super::reserve::CardClaim::acquire);
        }
    }

    /// Reopen the output device — after a stream error (e.g. a USB DAC was
    /// unplugged), a device change, or to run at a track's native rate.
    /// Playback resumes from the current position.
    fn rebuild_output(&mut self, target_rate: Option<u32>) -> Result<(), String> {
        // Size the ring for the highest rate it might carry.
        let ring_rate = f64::from(target_rate.unwrap_or(48_000).max(48_000));
        let ring_capacity =
            ((ring_rate * RING_CAPACITY_SECS) as usize).next_power_of_two() * 2;
        // Ring contents are lost in the swap; pin the position clock to the
        // audible moment so the UI doesn't drift.
        let live_position_ms = self.position_ms().map(|(pos, _)| pos);

        // Order matters here. A device we hold exclusively cannot be opened
        // twice, and the sound server will not release a card while we are
        // still streaming *through* it — so our own stream goes first, then
        // the server is asked to let go, and only then do we open. Shared
        // rebuilds keep the old stream until the new one is up, which keeps
        // audio alive if the open fails.
        // A direct device bypasses the sound server whether or not exclusive
        // access is on, so either way the card has to be freed first; see
        // `takes_device` for what counts as taking the hardware.
        let target_direct = takes_device(self.preferred_device.as_deref(), self.exclusive_output);
        let was_exclusive = self.output.exclusive;
        if was_exclusive || target_direct {
            self.output.close();
        }

        // macOS hog mode is a device property and outlives the stream.
        self.exclusive_access = if self.exclusive_output {
            ExclusiveAccess::acquire(self.preferred_device.as_deref())
        } else {
            ExclusiveAccess::inactive()
        };

        // On Linux the card has to be prised out of the sound server, and kept
        // out of it for as long as we hold the card. Letting the server have its
        // node back while we still have the device does not just cost it an
        // open — it retries, endlessly, and every attempt republishes the sink's
        // volume, which the desktop answers with its volume OSD.
        #[cfg(target_os = "linux")]
        {
            let card = target_direct.then(|| self.preferred_device.clone()).flatten();
            self.claim_card(card.as_deref());
        }

        // Each attempt consumes its ring and EQ handles, so every try gets a
        // fresh pair.
        let attempt = |device: Option<&str>, exclusive: bool| {
            let (producer, consumer) = rtrb::RingBuffer::new(ring_capacity);
            let (eq_input, eq_output) = triple_buffer::triple_buffer(&EqCoeffSet::IDENTITY);
            open_output(
                self.shared.clone(),
                consumer,
                eq_output,
                self.output_error_tx.clone(),
                device,
                target_rate,
                exclusive,
            )
            .map(|output| (output, producer, eq_input))
        };

        let device = self.preferred_device.clone();
        let mut result = attempt(device.as_deref(), self.exclusive_output);
        // cpal closes an ALSA device from its own thread, so a device we just
        // released can still report EBUSY for a moment — and the process
        // holding it would be us. Back off a few times before believing it.
        if result.is_err() && (was_exclusive || target_direct) {
            for delay_ms in [100, 250, 500] {
                std::thread::sleep(Duration::from_millis(delay_ms));
                result = attempt(device.as_deref(), self.exclusive_output);
                if result.is_ok() {
                    break;
                }
            }
        }
        // Nothing is playing once the device was let go, so rather than leave
        // the user in silence, come back on the shared default and say why.
        let mut fallback_error = None;
        let result = match result {
            Err(e) if was_exclusive || target_direct => {
                log::warn!("exclusive output unavailable ({e}); using the default device");
                fallback_error = Some(e.to_string());
                attempt(None, false)
                    .map_err(|open_err| format!("{e} (default device also failed: {open_err})"))
            }
            other => other.map_err(|e| e.to_string()),
        };
        let (output, producer, mut eq_input) = result?;
        // A fallback to the shared default device is not using the card we took.
        #[cfg(target_os = "linux")]
        if !output.exclusive {
            self.claim_card(None);
        }
        // The device asked for is gone, not merely busy — `open_output` already
        // fell back to the system default. Drop the preference so the picker and
        // every rebuild after this one stop chasing hardware that has left;
        // nothing here re-attaches a device that comes back.
        if output.device_missing {
            if let Some(lost) = self.preferred_device.take() {
                log::warn!("output device {lost} is gone; staying on the system default");
                self.emit(EngineEvent::OutputDeviceLost { device: lost });
            }
        }
        if let Some(message) = fallback_error {
            self.emit(EngineEvent::Error {
                code: "audio-device".into(),
                message,
                track_id: None,
                recoverable: true,
            });
        }
        eq_input.write(EqCoeffSet::from_state(&self.eq_state, f64::from(output.sample_rate)));

        let old_rate = self.output.sample_rate;
        let rate_changed = output.sample_rate != old_rate;
        self.output = output; // drops (and closes) the old stream
        self.producer = producer;
        self.eq_input = eq_input;
        self.pending.clear();
        self.frames_pushed = self.shared.frames_played.load(Ordering::Relaxed);

        if let Some(position_ms) = live_position_ms {
            let frames_played = self.shared.frames_played.load(Ordering::Relaxed);
            if let Some(d) = self.display.as_mut() {
                d.start_frame = frames_played;
                d.base_ms = position_ms as f64;
            }
        }

        if rate_changed {
            let new_rate = self.output.sample_rate;
            if let Some(current) = self.current.as_mut() {
                current.resampler =
                    StereoResampler::new(current.decoder.sample_rate(), new_rate)
                        .map_err(|e| e.to_string())?;
                current.out_frames =
                    current.out_frames * u64::from(new_rate) / u64::from(old_rate);
            }
            if let Some(fade) = self.fade.as_mut() {
                fade.incoming.resampler =
                    StereoResampler::new(fade.incoming.decoder.sample_rate(), new_rate)
                        .map_err(|e| e.to_string())?;
                fade.incoming.out_frames =
                    fade.incoming.out_frames * u64::from(new_rate) / u64::from(old_rate);
            }
            self.emit(EngineEvent::Format { sample_rate: self.output.sample_rate });
        }
        log::info!("audio output reopened at {} Hz", self.output.sample_rate);
        self.emit_signal_path();
        Ok(())
    }

    /// Request the callback to drain the ring, wait for the ack, and resync
    /// the producer-side frame counter.
    fn flush_ring(&mut self) {
        let generation = self.shared.request_flush();
        let deadline = Instant::now() + Duration::from_millis(250);
        while !self.shared.flush_complete(generation) {
            if Instant::now() > deadline {
                log::warn!("ring flush timed out (audio stream stalled?)");
                break;
            }
            std::thread::sleep(Duration::from_millis(1));
        }
        self.frames_pushed = self.shared.frames_played.load(Ordering::Relaxed);
        // Any starvation the callback flagged before the flush is stale now.
        self.shared.underrun.store(false, Ordering::Relaxed);
    }

    // ---- audio pumping ----------------------------------------------------

    /// Move data along: pending → ring, decoder → pending. Returns whether
    /// any forward progress happened (used to decide sleeping).
    fn pump(&mut self) -> bool {
        if self.state == PlaybackState::Stopped {
            return false;
        }
        if self.rate_switch.is_some() {
            return self.try_complete_rate_switch();
        }

        let mut progressed = self.push_pending();
        let mut failure: Option<String> = None;

        enum Step {
            Decoded,
            TrackDone,
            Idle,
        }

        // Keep decoding while there's ring space to fill (pending empties
        // fully into the ring only when the ring has room).
        while self.pending.is_empty() && failure.is_none() {
            // A crossfade takes over ordinary decoding until the outgoing
            // track drains and the incoming one is promoted.
            if self.fade.is_some() {
                match self.pump_crossfade() {
                    Ok(true) => {
                        progressed = true;
                        self.push_pending();
                    }
                    Ok(false) => break,
                    Err(message) => failure = Some(message),
                }
                continue;
            }
            if self.should_start_crossfade() {
                self.start_crossfade();
                continue;
            }

            let step = {
                let Some(current) = self.current.as_mut() else { break };
                if current.ended {
                    Step::TrackDone
                } else {
                    let mut buf = std::mem::take(&mut self.scratch);
                    let step = match decode_chunk(current, &mut buf, &mut self.pending) {
                        Ok(()) => Step::Decoded,
                        Err(e) => {
                            failure = Some(e);
                            Step::Idle
                        }
                    };
                    self.scratch = buf;
                    step
                }
            };

            match step {
                Step::Decoded => {
                    progressed = true;
                    self.push_pending();
                    if !self.pending.is_empty() {
                        break; // ring is full; stop decoding
                    }
                }
                Step::TrackDone => {
                    // Fully decoded and fully pushed: line up what's next.
                    self.advance_after_current();
                    if self.current.is_none() {
                        break;
                    }
                    progressed = true;
                }
                Step::Idle => break,
            }
        }

        if let Some(message) = failure {
            self.fail_current_track(message);
        }

        if progressed && self.buffering_reported && self.state == PlaybackState::Buffering {
            self.buffering_reported = false;
            self.set_state(PlaybackState::Playing);
        }

        progressed
    }

    /// The current track is fully in the ring. Open the next track (gapless)
    /// or schedule the end of playback.
    fn advance_after_current(&mut self) {
        let finished = self.current.take().expect("advance called with a current track");

        if let Some(next_src) = self.next.take() {
            match self.open_track(&next_src) {
                Ok(current) => {
                    // Different native rate under auto-rate: park the track
                    // until the ring drains so the device can re-clock first
                    // (a brief silence instead of resampling).
                    if self.auto_rate && current.decoder.sample_rate() != self.output.sample_rate
                    {
                        self.rate_switch = Some(current);
                        return;
                    }
                    let duration_ms = current.duration_ms;
                    self.boundaries.push_back(Boundary {
                        at_frame: self.frames_pushed,
                        kind: BoundaryKind::TrackStart(Box::new(DisplayTrack {
                            src: next_src,
                            start_frame: self.frames_pushed,
                            base_ms: 0.0,
                            duration_ms,
                            scrobbled: false,
                        })),
                    });
                    self.current = Some(current);
                    return;
                }
                Err(message) => {
                    self.emit(EngineEvent::Error {
                        code: "track-open".into(),
                        message,
                        track_id: Some(next_src.track_id),
                        recoverable: true,
                    });
                    // Fall through to end-of-playback.
                }
            }
        }

        self.boundaries.push_back(Boundary {
            at_frame: self.frames_pushed,
            kind: BoundaryKind::End { last_track_id: finished.src.track_id },
        });
    }

    /// A parked rate switch completes once the outgoing track has fully
    /// left the ring; then the device re-clocks and the new track starts.
    fn try_complete_rate_switch(&mut self) -> bool {
        let frames_played = self.shared.frames_played.load(Ordering::Relaxed);
        if frames_played < self.frames_pushed {
            return false; // outgoing track still audible
        }
        let Some(mut track) = self.rate_switch.take() else { return false };

        let source_rate = track.decoder.sample_rate();
        if let Err(e) = self.rebuild_output(Some(source_rate)) {
            log::warn!("rate switch failed; continuing at {} Hz: {e}", self.output.sample_rate);
        }
        // Target whatever rate the device actually came up at (passthrough
        // when the switch worked).
        match StereoResampler::new(source_rate, self.output.sample_rate) {
            Ok(resampler) => track.resampler = resampler,
            Err(e) => {
                self.emit(EngineEvent::Error {
                    code: "track-open".into(),
                    message: e.to_string(),
                    track_id: Some(track.src.track_id),
                    recoverable: true,
                });
                if let Some(next) = self.next.take() {
                    self.play_track(next);
                } else {
                    self.stop();
                }
                return true;
            }
        }

        let duration_ms = track.duration_ms;
        self.boundaries.push_back(Boundary {
            at_frame: self.frames_pushed,
            kind: BoundaryKind::TrackStart(Box::new(DisplayTrack {
                src: track.src.clone(),
                start_frame: self.frames_pushed,
                base_ms: 0.0,
                duration_ms,
                scrobbled: false,
            })),
        });
        self.current = Some(track);
        true
    }

    /// True when the outgoing track is close enough to its end that the
    /// queued next track should start mixing in.
    fn should_start_crossfade(&self) -> bool {
        if self.crossfade_ms == 0 || self.next.is_none() {
            return false;
        }
        let Some(current) = self.current.as_ref() else { return false };
        // Short tracks (interludes, gapless album cuts) keep the normal
        // gapless transition — a fade covering half the track hurts.
        if current.ended || current.duration_ms < self.crossfade_ms * 2 {
            return false;
        }
        let decoded_ms = current.out_frames * 1000 / u64::from(self.output.sample_rate);
        current.duration_ms.saturating_sub(decoded_ms) <= self.crossfade_ms
    }

    /// Open the queued next track and begin mixing it under the outgoing
    /// one. The UI switches to the incoming track the moment the fade
    /// becomes audible, mirroring the gapless boundary flow.
    fn start_crossfade(&mut self) {
        let Some(next_src) = self.next.take() else { return };
        let incoming = match self.open_track(&next_src) {
            Ok(incoming) => incoming,
            Err(message) => {
                self.emit(EngineEvent::Error {
                    code: "track-open".into(),
                    message,
                    track_id: Some(next_src.track_id),
                    recoverable: true,
                });
                return; // outgoing plays out; playback ends via the normal path
            }
        };
        let rate = u64::from(self.output.sample_rate);
        let current = self.current.as_ref().expect("crossfade requires a current track");
        let decoded_ms = current.out_frames * 1000 / rate;
        let fade_ms = current.duration_ms.saturating_sub(decoded_ms).min(self.crossfade_ms);
        let duration_ms = incoming.duration_ms;
        self.boundaries.push_back(Boundary {
            at_frame: self.frames_pushed,
            kind: BoundaryKind::TrackStart(Box::new(DisplayTrack {
                src: next_src,
                start_frame: self.frames_pushed,
                base_ms: 0.0,
                duration_ms,
                scrobbled: false,
            })),
        });
        self.fade = Some(CrossfadeState {
            incoming,
            total_frames: (fade_ms * rate / 1000).max(1),
            mixed_frames: 0,
            out_queue: VecDeque::new(),
            in_queue: VecDeque::new(),
        });
    }

    /// One unit of crossfade work: top up both decode queues, mix what's
    /// available into `pending`, and promote the incoming track once the
    /// outgoing one is fully drained. Returns whether progress happened.
    fn pump_crossfade(&mut self) -> Result<bool, String> {
        /// Keep roughly this many staged samples per side (~85 ms at 48 kHz).
        const QUEUE_TARGET: usize = 8192;

        let mut progressed = false;
        {
            let Some(fade) = self.fade.as_mut() else { return Ok(false) };
            let current =
                self.current.as_mut().expect("crossfade requires a current track");

            // Decode errors mid-fade degrade to "that side just ended" so the
            // music keeps playing instead of the whole transition failing.
            if !current.ended && fade.out_queue.len() < QUEUE_TARGET {
                let mut buf = std::mem::take(&mut self.scratch);
                if let Err(e) = decode_chunk(current, &mut buf, &mut fade.out_queue) {
                    log::warn!("outgoing track decode failed during crossfade: {e}");
                    current.ended = true;
                }
                self.scratch = buf;
                progressed = true;
            }
            if !fade.incoming.ended && fade.in_queue.len() < QUEUE_TARGET {
                let mut buf = std::mem::take(&mut self.scratch);
                if let Err(e) = decode_chunk(&mut fade.incoming, &mut buf, &mut fade.in_queue) {
                    log::warn!("incoming track decode failed during crossfade: {e}");
                    fade.incoming.ended = true;
                }
                self.scratch = buf;
                progressed = true;
            }

            // Mix frame by frame; a side that has ended contributes silence.
            loop {
                let out_has = fade.out_queue.len() >= 2;
                let in_has = fade.in_queue.len() >= 2;
                // Each side must have samples or be genuinely finished.
                if (!out_has && !current.ended) || (!in_has && !fade.incoming.ended) {
                    break; // need more decode first
                }
                if !out_has && !in_has {
                    break; // both drained
                }
                let (out_l, out_r) = pop_frame(&mut fade.out_queue);
                let (in_l, in_r) = pop_frame(&mut fade.in_queue);
                let (gain_out, gain_in) = fade_gains(fade.mixed_frames, fade.total_frames);
                self.pending.push_back((out_l * gain_out + in_l * gain_in).clamp(-1.0, 1.0));
                self.pending.push_back((out_r * gain_out + in_r * gain_in).clamp(-1.0, 1.0));
                fade.mixed_frames += 1;
                progressed = true;
            }
        }

        // Outgoing fully decoded and mixed → the incoming track carries on
        // alone through the normal path.
        let outgoing_drained = self.current.as_ref().is_some_and(|c| c.ended)
            && self.fade.as_ref().is_some_and(|f| f.out_queue.is_empty());
        if outgoing_drained {
            let fade = self.fade.take().expect("fade checked above");
            // Anything the incoming decoder produced past the fade window
            // continues at full level (the mix loop has already drained it
            // in the usual case).
            self.pending.extend(fade.in_queue);
            self.current = Some(fade.incoming);
            progressed = true;
        }
        Ok(progressed)
    }

    fn fail_current_track(&mut self, message: String) {
        let track_id = self.current.as_ref().map(|c| c.src.track_id.clone());
        self.emit(EngineEvent::Error {
            code: "decode".into(),
            message,
            track_id: track_id.clone(),
            recoverable: true,
        });
        // Try to keep music going: jump to the next track if we have one.
        if let Some(next) = self.next.take() {
            self.play_track(next);
        } else {
            self.stop();
        }
    }

    /// Push staged samples into the ring (whole frames only).
    fn push_pending(&mut self) -> bool {
        let mut pushed_any = false;
        loop {
            let slots = self.producer.slots();
            let available = self.pending.len().min(slots);
            let whole = available - (available % 2);
            if whole == 0 {
                break;
            }
            for _ in 0..whole {
                let sample = self.pending.pop_front().expect("len checked above");
                if self.producer.push(sample).is_err() {
                    // Should not happen (slots checked), but never spin.
                    return pushed_any;
                }
            }
            self.frames_pushed += (whole / 2) as u64;
            pushed_any = true;
        }
        pushed_any
    }

    // ---- periodic reporting -----------------------------------------------

    fn tick(&mut self) {
        let frames_played = self.shared.frames_played.load(Ordering::Relaxed);

        // Cross track boundaries at the audible moment.
        while let Some(boundary) = self.boundaries.front() {
            if boundary.at_frame > frames_played {
                break;
            }
            let boundary = self.boundaries.pop_front().expect("front checked above");
            match boundary.kind {
                BoundaryKind::TrackStart(display) => {
                    self.emit(EngineEvent::TrackChanged {
                        track_id: display.src.track_id.clone(),
                        reason: TrackChangeReason::Natural,
                        title: display.src.title.clone(),
                        artist: display.src.artist.clone(),
                        album: display.src.album.clone(),
                        duration_ms: display.duration_ms,
                    });
                    self.emit(EngineEvent::Scrobble {
                        track_id: display.src.track_id.clone(),
                        submission: false,
                        title: display.src.title.clone(),
                        artist: display.src.artist.clone(),
                        first_artist: display.src.first_artist.clone(),
                        album: display.src.album.clone(),
                        duration_ms: display.duration_ms,
                    });
                    self.last_now_playing = Instant::now();
                    self.display = Some(*display);
                    self.emit_position();
                    self.emit_signal_path();
                }
                BoundaryKind::End { last_track_id } => {
                    self.display = None;
                    self.current = None;
                    self.set_state(PlaybackState::Stopped);
                    self.emit(EngineEvent::TrackEnded { track_id: last_track_id });
                }
            }
        }

        // Underrun → buffering. Only meaningful while playing, and only once
        // this track has actually made sound (the ring is legitimately empty
        // during the initial fill).
        let started_audibly = self
            .display
            .as_ref()
            .is_some_and(|d| frames_played > d.start_frame);
        if self.shared.underrun.swap(false, Ordering::Relaxed)
            && self.state == PlaybackState::Playing
            && self.current.is_some()
            && started_audibly
        {
            self.buffering_reported = true;
            self.set_state(PlaybackState::Buffering);
        }

        if self.last_tick.elapsed() >= Duration::from_millis(500) {
            self.last_tick = Instant::now();
            if self.state == PlaybackState::Playing || self.state == PlaybackState::Buffering {
                self.emit_position();
                self.maybe_scrobble();
                // Keep the server's now-playing entry alive.
                if self.last_now_playing.elapsed() >= Duration::from_secs(30) {
                    if let Some(display) = self.display.as_ref() {
                        self.emit(EngineEvent::Scrobble {
                            track_id: display.src.track_id.clone(),
                            submission: false,
                            title: display.src.title.clone(),
                            artist: display.src.artist.clone(),
                            first_artist: display.src.first_artist.clone(),
                            album: display.src.album.clone(),
                            duration_ms: display.duration_ms,
                        });
                        self.last_now_playing = Instant::now();
                    }
                }
            }
        }
    }

    fn position_ms(&self) -> Option<(u64, u64)> {
        let display = self.display.as_ref()?;
        let frames_played = self.shared.frames_played.load(Ordering::Relaxed);
        let frames_into_track = frames_played.saturating_sub(display.start_frame);
        let ms = display.base_ms
            + frames_into_track as f64 * 1000.0 / f64::from(self.output.sample_rate);
        Some((ms as u64, display.duration_ms))
    }

    fn emit_position(&self) {
        if let Some((position_ms, duration_ms)) = self.position_ms() {
            self.emit(EngineEvent::Position { position_ms, duration_ms });
        }
    }

    /// Publish the current audio chain (source → DSP → output).
    fn emit_signal_path(&self) {
        let Some(current) = self.current.as_ref() else { return };
        let ext = current.src.ext_hint.as_deref().unwrap_or("").to_lowercase();
        let lossless_source = matches!(ext.as_str(), "flac" | "wav" | "alac" | "aiff" | "aif" | "ape");
        let source_rate = current.decoder.sample_rate();
        let resampled = source_rate != self.output.sample_rate;
        let replay_gain_db = ((current.replay_gain - 1.0).abs() > 1e-6)
            .then(|| 20.0 * current.replay_gain.log10());
        let eq_bands = if self.eq_state.enabled {
            self.eq_state.bands.iter().filter(|b| b.enabled).count() as u32
        } else {
            0
        };
        // What the device is really doing, which the stream rate alone does
        // not establish: ALSA's `default` and WASAPI shared mode both accept
        // our rate and resample underneath.
        let device_rate = super::output::hardware_rate(
            self.preferred_device.as_deref(),
            self.output.exclusive,
            self.output.sample_rate,
        );
        let os_resampling = device_rate.is_some_and(|rate| rate != self.output.sample_rate);

        let dsp_active = resampled || replay_gain_db.is_some() || eq_bands > 0 || os_resampling;
        let quality = if dsp_active {
            "enhanced"
        } else if lossless_source {
            "lossless"
        } else {
            "high"
        };
        self.emit(EngineEvent::SignalPath(SignalPathInfo {
            codec: (!ext.is_empty()).then(|| ext.to_uppercase()),
            source_sample_rate: source_rate,
            bit_rate: current.src.bit_rate,
            lossless_source,
            replay_gain_db,
            resample_to: resampled.then_some(self.output.sample_rate),
            eq_bands,
            output_sample_rate: self.output.sample_rate,
            output_device: self.output.device.clone(),
            device_rate,
            os_resampling,
            quality: quality.into(),
        }));
    }

    /// Submit the play once the configured percentage of the song has been
    /// heard (4 minutes when the duration is unknown).
    fn maybe_scrobble(&mut self) {
        let Some((position_ms, duration_ms)) = self.position_ms() else { return };
        let percent = u64::from(self.scrobble_percent);
        let Some(display) = self.display.as_mut() else { return };
        if display.scrobbled {
            return;
        }
        let threshold =
            if duration_ms == 0 { 240_000 } else { duration_ms * percent / 100 };
        if position_ms >= threshold {
            display.scrobbled = true;
            let track_id = display.src.track_id.clone();
            let title = display.src.title.clone();
            let artist = display.src.artist.clone();
            let first_artist = display.src.first_artist.clone();
            let album = display.src.album.clone();
            self.emit(EngineEvent::Scrobble {
                track_id,
                submission: true,
                title,
                artist,
                first_artist,
                album,
                duration_ms,
            });
        }
    }
}

/// Decode one chunk from `track`, downmix, apply ReplayGain, resample, and
/// append to `sink` (output-domain interleaved stereo). Sets `ended` at EOF.
fn decode_chunk(
    track: &mut CurrentTrack,
    scratch: &mut Vec<f32>,
    sink: &mut VecDeque<f32>,
) -> Result<(), String> {
    let before = sink.len();
    match track.decoder.next_chunk_into(scratch) {
        Ok(Some(info)) => {
            let mut stereo = downmix_to_stereo(scratch, info.channels);
            if (track.replay_gain - 1.0).abs() > 1e-6 {
                let gain = track.replay_gain;
                for sample in stereo.iter_mut() {
                    *sample = (*sample * gain).clamp(-1.0, 1.0);
                }
            }
            track
                .resampler
                .process(&stereo, |out| sink.extend(out.iter().copied()))
                .map_err(|e| format!("resampler: {e}"))?;
        }
        Ok(None) => {
            let _ = track.resampler.flush(|out| sink.extend(out.iter().copied()));
            track.ended = true;
        }
        Err(e) => return Err(e.to_string()),
    }
    track.out_frames += ((sink.len() - before) / 2) as u64;
    Ok(())
}

/// Pop one stereo frame; missing samples read as silence.
fn pop_frame(queue: &mut VecDeque<f32>) -> (f32, f32) {
    let l = queue.pop_front().unwrap_or(0.0);
    let r = queue.pop_front().unwrap_or(0.0);
    (l, r)
}

/// Equal-power crossfade gains (outgoing, incoming) at `mixed` of `total`
/// frames through the fade. Constant perceived loudness across the overlap.
fn fade_gains(mixed: u64, total: u64) -> (f32, f32) {
    let x = (mixed as f32 / total.max(1) as f32).min(1.0);
    let angle = x * std::f32::consts::FRAC_PI_2;
    (angle.cos(), angle.sin())
}

/// Downmix interleaved samples with any channel count to stereo.
fn downmix_to_stereo(samples: &[f32], channels: usize) -> Vec<f32> {
    match channels {
        0 => Vec::new(),
        1 => {
            let mut out = Vec::with_capacity(samples.len() * 2);
            for &s in samples {
                out.push(s);
                out.push(s);
            }
            out
        }
        2 => samples.to_vec(),
        n => {
            // L/R + center folded in at -3 dB; other channels dropped (v1).
            let mut out = Vec::with_capacity(samples.len() / n * 2);
            for frame in samples.chunks_exact(n) {
                let center = if n > 2 { frame[2] * std::f32::consts::FRAC_1_SQRT_2 } else { 0.0 };
                out.push((frame[0] + center).clamp(-1.0, 1.0));
                out.push((frame[1] + center).clamp(-1.0, 1.0));
            }
            out
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_a_real_device_takeover_forces_the_release_then_open_ordering() {
        // The shared default takes nothing, so a rebuild can keep the old
        // stream alive until the new one is up.
        assert!(!takes_device(None, false));

        #[cfg(target_os = "linux")]
        {
            // Exclusive access with no device to take is inert here — the flag
            // picks `hw:` over `plughw:`, and there is neither to pick. Forcing
            // the takeover ordering would tear the stream down for nothing.
            assert!(!takes_device(None, true));
            // A direct device takes the card either way, so the sound server
            // has to be asked to let go first.
            assert!(takes_device(Some("alsa:hw:CARD=1,DEV=0"), false));
            assert!(takes_device(Some("alsa:plughw:CARD=1,DEV=0"), true));
            // A sound-server route is not a takeover, whatever the flag says.
            assert!(!takes_device(Some("alsa:pulse"), true));
        }
        // Hog mode and WASAPI exclusive mode claim the default device too.
        #[cfg(not(target_os = "linux"))]
        assert!(takes_device(None, true));
    }

    fn rg(track_gain: Option<f32>, album_gain: Option<f32>, track_peak: Option<f32>) -> ReplayGain {
        ReplayGain { track_gain, album_gain, track_peak, album_peak: None }
    }

    #[test]
    fn replay_gain_off_is_unity() {
        let s = ReplayGainSettings::default();
        assert_eq!(s.gain_for(Some(&rg(Some(-6.0), None, None))), 1.0);
    }

    #[test]
    fn replay_gain_track_mode_applies_db() {
        let s = ReplayGainSettings {
            mode: ReplayGainMode::Track,
            preamp_db: 0.0,
            clip_protection: false,
            fallback_db: 0.0,
        };
        let gain = s.gain_for(Some(&rg(Some(-6.0), None, None)));
        assert!((gain - 0.501).abs() < 0.01, "got {gain}");
        // Falls back to album gain when track gain is missing.
        let gain = s.gain_for(Some(&rg(None, Some(6.0), None)));
        assert!((gain - 1.995).abs() < 0.01, "got {gain}");
        // No tags, no fallback configured → unity.
        assert_eq!(s.gain_for(None), 1.0);
    }

    #[test]
    fn replay_gain_clip_protection_caps_at_peak() {
        let s = ReplayGainSettings {
            mode: ReplayGainMode::Track,
            preamp_db: 0.0,
            clip_protection: true,
            fallback_db: 0.0,
        };
        // +6 dB boost but peak is already 0.9 → capped to 1/0.9.
        let gain = s.gain_for(Some(&rg(Some(6.0), None, Some(0.9))));
        assert!((gain - 1.0 / 0.9).abs() < 1e-3, "got {gain}");
    }

    #[test]
    fn replay_gain_fallback_applies_to_untagged_tracks() {
        let s = ReplayGainSettings {
            mode: ReplayGainMode::Track,
            preamp_db: 0.0,
            clip_protection: true,
            fallback_db: -6.0,
        };
        // No tags at all → fallback.
        let gain = s.gain_for(None);
        assert!((gain - 0.501).abs() < 0.01, "got {gain}");
        // Tags present but no gain values → fallback.
        let gain = s.gain_for(Some(&rg(None, None, Some(0.95))));
        assert!((gain - 0.501).abs() < 0.01, "got {gain}");
        // Tagged tracks are unaffected by the fallback.
        let gain = s.gain_for(Some(&rg(Some(0.0), None, None)));
        assert!((gain - 1.0).abs() < 1e-3, "got {gain}");
        // Off mode ignores the fallback.
        let off = ReplayGainSettings { mode: ReplayGainMode::Off, ..s };
        assert_eq!(off.gain_for(None), 1.0);
    }

    #[test]
    fn fade_gains_start_and_end() {
        let (out, inc) = fade_gains(0, 1000);
        assert!((out - 1.0).abs() < 1e-6 && inc.abs() < 1e-6);
        let (out, inc) = fade_gains(1000, 1000);
        assert!(out.abs() < 1e-6 && (inc - 1.0).abs() < 1e-6);
        // Past the window the gains stay pinned.
        let (out, inc) = fade_gains(5000, 1000);
        assert!(out.abs() < 1e-6 && (inc - 1.0).abs() < 1e-6);
    }

    #[test]
    fn fade_gains_are_equal_power() {
        for mixed in [0u64, 250, 500, 750, 1000] {
            let (out, inc) = fade_gains(mixed, 1000);
            let power = out * out + inc * inc;
            assert!((power - 1.0).abs() < 1e-5, "power {power} at {mixed}");
        }
    }

    #[test]
    fn pop_frame_pads_with_silence() {
        let mut q: VecDeque<f32> = VecDeque::from(vec![0.3]);
        assert_eq!(pop_frame(&mut q), (0.3, 0.0));
        assert_eq!(pop_frame(&mut q), (0.0, 0.0));
    }

    #[test]
    fn downmix_mono_duplicates() {
        assert_eq!(downmix_to_stereo(&[0.5, -0.5], 1), vec![0.5, 0.5, -0.5, -0.5]);
    }

    #[test]
    fn downmix_stereo_passthrough() {
        assert_eq!(downmix_to_stereo(&[0.1, 0.2], 2), vec![0.1, 0.2]);
    }

    #[test]
    fn downmix_51_folds_center() {
        // One 5.1 frame: FL FR C LFE SL SR
        let out = downmix_to_stereo(&[0.2, 0.4, 0.2, 0.9, 0.1, 0.1], 6);
        assert_eq!(out.len(), 2);
        assert!((out[0] - (0.2 + 0.2 * std::f32::consts::FRAC_1_SQRT_2)).abs() < 1e-6);
    }
}
