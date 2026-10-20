//! Lock-free state shared between the engine thread and the real-time audio
//! callback. The PCM itself travels through an `rtrb` SPSC ring; everything
//! else (position, flush handshake, pause, volume) lives here as atomics.

use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};

/// How much device-rate PCM the ring holds (per channel).
pub const RING_CAPACITY_SECS: f64 = 0.5;

pub struct SharedAudioState {
    /// Stereo frames actually rendered to the device (not counting silence).
    pub frames_played: AtomicU64,
    /// Bumped by the engine to request the callback drain the ring.
    pub flush_target_gen: AtomicU32,
    /// Set by the callback once the drain for a given generation is done.
    pub flush_ack_gen: AtomicU32,
    /// When set, the callback ramps to silence and stops consuming the ring.
    pub paused: AtomicBool,
    /// Set by the callback when it starves while playing; cleared by the engine.
    pub underrun: AtomicBool,
    /// Target amplitude (post-taper, linear) as f32 bits.
    volume_bits: AtomicU32,
}

impl SharedAudioState {
    pub fn new() -> Self {
        Self {
            frames_played: AtomicU64::new(0),
            flush_target_gen: AtomicU32::new(0),
            flush_ack_gen: AtomicU32::new(0),
            paused: AtomicBool::new(false),
            underrun: AtomicBool::new(false),
            volume_bits: AtomicU32::new(1.0f32.to_bits()),
        }
    }

    pub fn set_volume_amp(&self, amp: f32) {
        self.volume_bits.store(amp.to_bits(), Ordering::Relaxed);
    }

    pub fn volume_amp(&self) -> f32 {
        f32::from_bits(self.volume_bits.load(Ordering::Relaxed))
    }

    /// Request a flush; returns the generation to wait for.
    pub fn request_flush(&self) -> u32 {
        self.flush_target_gen.fetch_add(1, Ordering::AcqRel) + 1
    }

    pub fn flush_complete(&self, generation: u32) -> bool {
        self.flush_ack_gen.load(Ordering::Acquire) == generation
    }
}

impl Default for SharedAudioState {
    fn default() -> Self {
        Self::new()
    }
}

/// Map a 0..=1 UI volume to a linear amplitude with a dB taper
/// (linear-in-dB over a 60 dB range, hard mute below 0.1%).
pub fn volume_taper(ui: f32) -> f32 {
    let v = ui.clamp(0.0, 1.0);
    if v < 0.001 {
        0.0
    } else {
        10f32.powf(((1.0 - v) * -60.0) / 20.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn taper_endpoints() {
        assert_eq!(volume_taper(0.0), 0.0);
        assert!((volume_taper(1.0) - 1.0).abs() < 1e-6);
        // Half volume should be -30 dB.
        let half = volume_taper(0.5);
        assert!((20.0 * half.log10() + 30.0).abs() < 0.01, "got {half}");
    }
}
