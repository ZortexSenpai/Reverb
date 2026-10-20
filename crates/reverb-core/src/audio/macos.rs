//! CoreAudio device-level control (macOS only). cpal configures the
//! *stream*; these calls configure the *device*: switching its nominal
//! sample rate so playback is actually bit-perfect (the DAC re-clocks), and
//! hog mode for exclusive access.

use coreaudio::audio_unit::macos_helpers::{
    get_default_device_id, get_device_id_from_name, get_hogging_pid, set_device_sample_rate,
    toggle_hog_mode,
};
use objc2_core_audio::AudioDeviceID;

fn resolve_device(name: Option<&str>) -> Option<AudioDeviceID> {
    name.and_then(|n| get_device_id_from_name(n, false))
        .or_else(|| get_default_device_id(false))
}

fn our_pid() -> i32 {
    std::process::id() as i32
}

/// Switch the device's nominal sample rate (blocks until the hardware
/// reports the new rate, ~up to 2 s). Returns whether the device now runs
/// at `rate`; on `false` the caller keeps resampling.
pub fn switch_device_rate(device_name: Option<&str>, rate: u32) -> bool {
    let Some(device_id) = resolve_device(device_name) else {
        return false;
    };
    match set_device_sample_rate(device_id, f64::from(rate)) {
        Ok(()) => {
            log::info!("output device switched to {rate} Hz");
            true
        }
        Err(e) => {
            log::info!("device won't run at {rate} Hz natively ({e}); resampling instead");
            false
        }
    }
}

/// Owns hog-mode (exclusive) access to one device; released on drop.
pub struct HogGuard {
    device_id: AudioDeviceID,
}

impl HogGuard {
    /// Best effort: returns None (with a log) when another process holds
    /// the device or CoreAudio refuses.
    pub fn acquire(device_name: Option<&str>) -> Option<Self> {
        let device_id = resolve_device(device_name)?;
        match get_hogging_pid(device_id) {
            Ok(pid) if pid == our_pid() => Some(Self { device_id }),
            Ok(-1) => match toggle_hog_mode(device_id) {
                Ok(pid) if pid == our_pid() => {
                    log::info!("took exclusive control of the output device");
                    Some(Self { device_id })
                }
                Ok(pid) => {
                    log::warn!("exclusive request ended up owned by pid {pid}");
                    None
                }
                Err(e) => {
                    log::warn!("could not take exclusive control of the output device: {e}");
                    None
                }
            },
            Ok(pid) => {
                log::warn!("output device is exclusively held by pid {pid}");
                None
            }
            Err(e) => {
                log::warn!("hog mode query failed: {e}");
                None
            }
        }
    }
}

impl Drop for HogGuard {
    fn drop(&mut self) {
        // Release only while we still own it — toggling otherwise would
        // steal the device from whoever holds it now.
        if get_hogging_pid(self.device_id).ok() == Some(our_pid()) {
            let _ = toggle_hog_mode(self.device_id);
            log::info!("released exclusive control of the output device");
        }
    }
}
