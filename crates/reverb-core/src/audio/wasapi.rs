//! WASAPI exclusive-mode output (Windows only).
//!
//! cpal always opens WASAPI in *shared* mode, where the OS mixer owns the
//! device: it resamples to the mix format and mixes other apps in. That is the
//! opposite of bit-perfect, so exclusive mode needs its own backend rather
//! than a flag — unlike macOS, where hog mode is a device property that leaves
//! the cpal stream intact.
//!
//! The render thread here plays the same role as cpal's data callback and is
//! bound by the same rule: no allocation, no locks, no syscalls beyond the
//! WASAPI buffer handoff.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread::JoinHandle;

use windows::core::PCWSTR;
use windows::Win32::Devices::FunctionDiscovery::PKEY_Device_FriendlyName;
use windows::Win32::Foundation::{CloseHandle, HANDLE, WAIT_OBJECT_0};
use windows::Win32::Media::Audio::{
    eConsole, eRender, IAudioClient, IAudioRenderClient, IMMDevice, IMMDeviceEnumerator,
    MMDeviceEnumerator, AUDCLNT_E_BUFFER_SIZE_NOT_ALIGNED, AUDCLNT_E_DEVICE_IN_USE,
    AUDCLNT_SHAREMODE_EXCLUSIVE, AUDCLNT_STREAMFLAGS_EVENTCALLBACK, DEVICE_STATE_ACTIVE,
    WAVEFORMATEX, WAVEFORMATEXTENSIBLE, WAVEFORMATEXTENSIBLE_0,
};
// The two KS subtype GUIDs sit in different modules in windows-rs.
use windows::Win32::Media::KernelStreaming::{KSDATAFORMAT_SUBTYPE_PCM, WAVE_FORMAT_EXTENSIBLE};
use windows::Win32::Media::Multimedia::KSDATAFORMAT_SUBTYPE_IEEE_FLOAT;
use windows::Win32::System::Com::{
    CoCreateInstance, CoInitializeEx, CoTaskMemFree, CoUninitialize, CLSCTX_ALL,
    COINIT_MULTITHREADED, STGM_READ,
};
use windows::Win32::System::Threading::{
    AvRevertMmThreadCharacteristics, AvSetMmThreadCharacteristicsW, CreateEventW,
    WaitForSingleObject,
};

use super::eq::EqCoeffSet;
use super::render::Renderer;
use super::ring::SharedAudioState;

/// 100-nanosecond units, the unit WASAPI states durations in.
const HNS_PER_SEC: i64 = 10_000_000;
/// A device that goes this long without asking for audio has stalled.
const WAIT_TIMEOUT_MS: u32 = 2000;

/// Sample formats to offer the hardware, best first. Exclusive mode does no
/// conversion, so one of these has to match the DAC exactly.
const FORMAT_CANDIDATES: [(u16, bool); 4] = [
    (32, true),  // 32-bit float
    (32, false), // 32-bit PCM
    (24, false), // 24-bit PCM in a 32-bit container
    (16, false), // 16-bit PCM
];

pub struct ExclusiveStream {
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
    pub sample_rate: u32,
    pub channels: usize,
}

impl Drop for ExclusiveStream {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        if let Some(thread) = self.thread.take() {
            // The render thread wakes at least every WAIT_TIMEOUT_MS, so this
            // joins promptly even if the device stopped signalling.
            let _ = thread.join();
        }
    }
}

/// COM apartment for a thread, uninitialized on drop.
struct ComScope;

impl ComScope {
    fn enter() -> Self {
        // Already-initialized (S_FALSE / RPC_E_CHANGED_MODE) is fine: some
        // other component in the process got there first.
        unsafe { let _ = CoInitializeEx(None, COINIT_MULTITHREADED); }
        Self
    }
}

impl Drop for ComScope {
    fn drop(&mut self) {
        unsafe { CoUninitialize() };
    }
}

/// Owns a WAVEFORMATEX allocated by WASAPI (GetMixFormat).
struct MixFormat(*mut WAVEFORMATEX);

impl Drop for MixFormat {
    fn drop(&mut self) {
        if !self.0.is_null() {
            unsafe { CoTaskMemFree(Some(self.0.cast())) };
        }
    }
}

fn enumerator() -> windows::core::Result<IMMDeviceEnumerator> {
    unsafe { CoCreateInstance::<_, IMMDeviceEnumerator>(&MMDeviceEnumerator, None, CLSCTX_ALL) }
}

/// Friendly name of a device, matching what the cpal device list shows so a
/// saved device preference resolves the same way in both backends.
unsafe fn device_name(device: &IMMDevice) -> Option<String> {
    unsafe {
        let store = device.OpenPropertyStore(STGM_READ).ok()?;
        let value = store.GetValue(&PKEY_Device_FriendlyName).ok()?;
        Some(value.to_string())
    }
}

/// Resolve a device by friendly name, falling back to the default endpoint —
/// the same fallback the cpal path uses when a configured device vanishes.
unsafe fn resolve_device(wanted: Option<&str>) -> windows::core::Result<IMMDevice> {
    let enumerator = enumerator()?;
    unsafe {
        if let Some(wanted) = wanted {
            let collection = enumerator.EnumAudioEndpoints(eRender, DEVICE_STATE_ACTIVE)?;
            for i in 0..collection.GetCount()? {
                let device = collection.Item(i)?;
                if device_name(&device).as_deref() == Some(wanted) {
                    return Ok(device);
                }
            }
        }
        enumerator.GetDefaultAudioEndpoint(eRender, eConsole)
    }
}

/// The shared mixer's sample rate — the rate the hardware actually runs at
/// when we are *not* in exclusive mode, whatever rate our stream claims.
pub fn shared_mix_rate(device_name: Option<&str>) -> Option<u32> {
    let _com = ComScope::enter();
    unsafe {
        let device = resolve_device(device_name).ok()?;
        let client: IAudioClient = device.Activate(CLSCTX_ALL, None).ok()?;
        let mix = MixFormat(client.GetMixFormat().ok()?);
        Some((*mix.0).nSamplesPerSec)
    }
}

/// Build a WAVEFORMATEXTENSIBLE for one candidate format.
fn waveformat(rate: u32, channels: u16, bits: u16, float: bool) -> WAVEFORMATEXTENSIBLE {
    // 24-bit audio travels in a 32-bit container; everything else is packed.
    let container_bits: u16 = if bits == 24 { 32 } else { bits };
    let block_align = channels * container_bits / 8;
    WAVEFORMATEXTENSIBLE {
        Format: WAVEFORMATEX {
            wFormatTag: WAVE_FORMAT_EXTENSIBLE as u16,
            nChannels: channels,
            nSamplesPerSec: rate,
            nAvgBytesPerSec: rate * u32::from(block_align),
            nBlockAlign: block_align,
            wBitsPerSample: container_bits,
            cbSize: (size_of::<WAVEFORMATEXTENSIBLE>() - size_of::<WAVEFORMATEX>()) as u16,
        },
        Samples: WAVEFORMATEXTENSIBLE_0 { wValidBitsPerSample: bits },
        // Stereo front left/right; anything wider gets a plain speaker mask.
        dwChannelMask: if channels == 2 { 0x3 } else { (1u32 << channels) - 1 },
        SubFormat: if float {
            KSDATAFORMAT_SUBTYPE_IEEE_FLOAT
        } else {
            KSDATAFORMAT_SUBTYPE_PCM
        },
    }
}

/// The sample format the renderer should write, decided by the negotiated
/// format. 24-bit shares the 32-bit integer path (valid bits are the high
/// ones, which is what a full-range i32 conversion produces).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SampleKind {
    F32,
    I32,
    I16,
}

struct Negotiated {
    format: WAVEFORMATEXTENSIBLE,
    kind: SampleKind,
    rate: u32,
    channels: usize,
}

/// Find a format the device accepts in exclusive mode. The requested rate is
/// tried first (that is the whole point of bit-perfect playback), then the
/// mix-format rate, then the usual suspects.
unsafe fn negotiate(
    client: &IAudioClient,
    preferred_rate: Option<u32>,
    mix_rate: u32,
    channels: u16,
) -> Option<Negotiated> {
    let mut rates: Vec<u32> = Vec::with_capacity(8);
    rates.extend(preferred_rate);
    rates.push(mix_rate);
    for rate in [48_000, 44_100, 96_000, 88_200, 192_000, 176_400] {
        rates.push(rate);
    }
    rates.dedup();

    for rate in rates {
        for (bits, float) in FORMAT_CANDIDATES {
            let format = waveformat(rate, channels, bits, float);
            let hr = unsafe {
                client.IsFormatSupported(
                    AUDCLNT_SHAREMODE_EXCLUSIVE,
                    std::ptr::addr_of!(format.Format),
                    None,
                )
            };
            if hr.is_ok() {
                let kind = match (bits, float) {
                    (32, true) => SampleKind::F32,
                    (16, false) => SampleKind::I16,
                    _ => SampleKind::I32,
                };
                log::info!(
                    "WASAPI exclusive: {rate} Hz, {bits}-bit {}",
                    if float { "float" } else { "PCM" }
                );
                return Some(Negotiated { format, kind, rate, channels: channels as usize });
            }
        }
    }
    None
}

/// Initialize the client, handling the buffer-alignment retry WASAPI requires
/// in exclusive mode: on AUDCLNT_E_BUFFER_SIZE_NOT_ALIGNED the client is spent
/// and has to be replaced with one sized to the frame count it just reported.
unsafe fn initialize(
    device: &IMMDevice,
    client: IAudioClient,
    format: &WAVEFORMATEXTENSIBLE,
    period_hns: i64,
    rate: u32,
) -> windows::core::Result<(IAudioClient, u32)> {
    unsafe {
        let init = |client: &IAudioClient, period: i64| {
            client.Initialize(
                AUDCLNT_SHAREMODE_EXCLUSIVE,
                AUDCLNT_STREAMFLAGS_EVENTCALLBACK,
                period,
                // Exclusive mode requires periodicity == buffer duration.
                period,
                std::ptr::addr_of!(format.Format),
                None,
            )
        };

        match init(&client, period_hns) {
            Ok(()) => {
                let frames = client.GetBufferSize()?;
                Ok((client, frames))
            }
            Err(e) if e.code() == AUDCLNT_E_BUFFER_SIZE_NOT_ALIGNED => {
                let aligned_frames = client.GetBufferSize()?;
                // Round the period to the aligned frame count, per MSDN.
                let period = (HNS_PER_SEC * i64::from(aligned_frames) + i64::from(rate) / 2)
                    / i64::from(rate);
                let client: IAudioClient = device.Activate(CLSCTX_ALL, None)?;
                init(&client, period)?;
                let frames = client.GetBufferSize()?;
                log::info!("WASAPI exclusive: retried with an aligned {frames}-frame buffer");
                Ok((client, frames))
            }
            Err(e) => Err(e),
        }
    }
}

/// An exclusive-mode client that is initialized but not yet rendering.
///
/// Everything that can fail happens here, before the ring buffer and EQ
/// handles are consumed — so a caller can fall back to shared mode with those
/// still in hand.
pub struct Prepared {
    client: IAudioClient,
    render_client: IAudioRenderClient,
    event: HANDLE,
    kind: SampleKind,
    buffer_frames: u32,
    pub sample_rate: u32,
    pub channels: usize,
}

/// Claim the device in exclusive mode. Errors are for the caller to fall back
/// on — a device already held exclusively by another app is the common one.
pub fn prepare(device_name: Option<&str>, preferred_rate: Option<u32>) -> Result<Prepared, String> {
    let _com = ComScope::enter();

    unsafe {
        let device = resolve_device(device_name).map_err(|e| format!("no output device: {e}"))?;
        let client: IAudioClient =
            device.Activate(CLSCTX_ALL, None).map_err(|e| format!("device busy: {e}"))?;

        let mix = MixFormat(client.GetMixFormat().map_err(|e| format!("no mix format: {e}"))?);
        let (mix_rate, mix_channels) = {
            let mix = &*mix.0;
            (mix.nSamplesPerSec, mix.nChannels.max(2))
        };

        let negotiated = negotiate(&client, preferred_rate, mix_rate, mix_channels)
            .ok_or_else(|| "device accepts no exclusive-mode format".to_owned())?;

        // Default device period keeps a safety margin over the minimum; the
        // ring is fed by a decoder thread, not a real-time one.
        let mut default_period: i64 = 0;
        let mut min_period: i64 = 0;
        client
            .GetDevicePeriod(Some(&mut default_period), Some(&mut min_period))
            .map_err(|e| format!("could not query device period: {e}"))?;
        let period = default_period.max(min_period);

        let (client, buffer_frames) =
            initialize(&device, client, &negotiated.format, period, negotiated.rate).map_err(
                |e| {
                    if e.code() == AUDCLNT_E_DEVICE_IN_USE {
                        "another application holds the device exclusively".to_owned()
                    } else {
                        format!("exclusive mode refused: {e}")
                    }
                },
            )?;

        let event = CreateEventW(None, false, false, PCWSTR::null())
            .map_err(|e| format!("could not create render event: {e}"))?;
        client.SetEventHandle(event).map_err(|e| format!("could not arm render event: {e}"))?;
        let render_client: IAudioRenderClient =
            client.GetService().map_err(|e| format!("no render client: {e}"))?;

        Ok(Prepared {
            client,
            render_client,
            event,
            kind: negotiated.kind,
            buffer_frames,
            sample_rate: negotiated.rate,
            channels: negotiated.channels,
        })
    }
}

impl Prepared {
    /// Hand the stream its audio source and start rendering.
    pub fn start(
        self,
        shared: Arc<SharedAudioState>,
        consumer: rtrb::Consumer<f32>,
        eq_output: triple_buffer::Output<EqCoeffSet>,
        error_tx: crossbeam_channel::Sender<String>,
    ) -> ExclusiveStream {
        let renderer =
            Renderer::new(shared, consumer, eq_output, self.sample_rate, self.channels);
        let stop = Arc::new(AtomicBool::new(false));
        let thread = spawn_render_thread(
            self.client,
            self.render_client,
            self.event,
            renderer,
            self.kind,
            self.channels,
            self.buffer_frames,
            Arc::clone(&stop),
            error_tx,
        );

        ExclusiveStream {
            stop,
            thread: Some(thread),
            sample_rate: self.sample_rate,
            channels: self.channels,
        }
    }
}

/// COM interfaces are not `Send`, but these are only ever touched from the
/// render thread once moved there, and the stream owns them for its lifetime.
struct SendPtr<T>(T);
unsafe impl<T> Send for SendPtr<T> {}

impl<T> SendPtr<T> {
    /// Taken by method call on purpose: destructuring `moved.0` inside the
    /// closure would capture the field and bypass the `Send` impl above.
    fn into_inner(self) -> T {
        self.0
    }
}

#[allow(clippy::too_many_arguments)]
fn spawn_render_thread(
    client: IAudioClient,
    render_client: IAudioRenderClient,
    event: HANDLE,
    mut renderer: Renderer,
    kind: SampleKind,
    channels: usize,
    buffer_frames: u32,
    stop: Arc<AtomicBool>,
    error_tx: crossbeam_channel::Sender<String>,
) -> JoinHandle<()> {
    let moved = SendPtr((client, render_client, event));

    std::thread::Builder::new()
        .name("reverb-wasapi".into())
        .spawn(move || {
            let (client, render_client, event) = moved.into_inner();
            let _com = ComScope::enter();

            // "Pro Audio" is the MMCSS task class WASAPI expects for
            // event-driven exclusive rendering; without it the thread can be
            // preempted long enough to glitch.
            let mut task_index = 0u32;
            let mmcss = unsafe {
                AvSetMmThreadCharacteristicsW(windows::core::w!("Pro Audio"), &mut task_index)
            };

            let fill = |frames: u32, renderer: &mut Renderer| -> windows::core::Result<()> {
                unsafe {
                    let ptr = render_client.GetBuffer(frames)?;
                    let samples = frames as usize * channels;
                    match kind {
                        SampleKind::F32 => {
                            renderer.render(std::slice::from_raw_parts_mut(ptr.cast::<f32>(), samples))
                        }
                        SampleKind::I32 => {
                            renderer.render(std::slice::from_raw_parts_mut(ptr.cast::<i32>(), samples))
                        }
                        SampleKind::I16 => {
                            renderer.render(std::slice::from_raw_parts_mut(ptr.cast::<i16>(), samples))
                        }
                    }
                    render_client.ReleaseBuffer(frames, 0)
                }
            };

            // WASAPI wants the first buffer filled before the clock starts.
            let start = unsafe {
                fill(buffer_frames, &mut renderer).and_then(|()| client.Start())
            };
            if let Err(e) = start {
                let _ = error_tx.send(format!("exclusive stream failed to start: {e}"));
                return;
            }

            while !stop.load(Ordering::Acquire) {
                let wait = unsafe { WaitForSingleObject(event, WAIT_TIMEOUT_MS) };
                if stop.load(Ordering::Acquire) {
                    break;
                }
                if wait != WAIT_OBJECT_0 {
                    let _ = error_tx.send("exclusive stream stalled".to_owned());
                    break;
                }
                // In exclusive mode the whole buffer is ours every period.
                if let Err(e) = fill(buffer_frames, &mut renderer) {
                    let _ = error_tx.send(format!("exclusive stream lost the device: {e}"));
                    break;
                }
            }

            unsafe {
                let _ = client.Stop();
                if let Ok(handle) = mmcss {
                    let _ = AvRevertMmThreadCharacteristics(handle);
                }
                let _ = CloseHandle(event);
            }
        })
        .expect("spawning the WASAPI render thread")
}
