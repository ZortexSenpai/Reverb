//! The real-time render step: pops stereo f32 frames from the ring, runs the
//! EQ chain, applies the smoothed gain, and maps to the device's channel
//! count and sample format.
//!
//! Shared by both output backends (cpal everywhere, WASAPI exclusive on
//! Windows) so the audible path is identical whichever one is open.
//!
//! HARD RULE: nothing here allocates, locks, or does syscalls.

use std::sync::atomic::Ordering;
use std::sync::Arc;

use cpal::{FromSample, SizedSample};

use super::eq::{EqCoeffSet, EqProcessor};
use super::ring::SharedAudioState;

/// ~15 ms gain smoothing at 48 kHz (per-sample one-pole factor).
const GAIN_ALPHA: f64 = 1.0 / (0.015 * 48_000.0);
/// Below this gain while paused we stop consuming the ring entirely.
const SILENCE_EPSILON: f64 = 1e-4;

pub struct Renderer {
    shared: Arc<SharedAudioState>,
    consumer: rtrb::Consumer<f32>,
    eq_output: triple_buffer::Output<EqCoeffSet>,
    eq: EqProcessor,
    gain: f64,
    channels: usize,
}

impl Renderer {
    pub fn new(
        shared: Arc<SharedAudioState>,
        consumer: rtrb::Consumer<f32>,
        eq_output: triple_buffer::Output<EqCoeffSet>,
        sample_rate: u32,
        channels: usize,
    ) -> Self {
        Self {
            shared,
            consumer,
            eq_output,
            eq: EqProcessor::new(sample_rate),
            gain: 0.0,
            channels: channels.max(1),
        }
    }

    /// Fill one device buffer. `data` is interleaved, `data.len()` a whole
    /// number of frames.
    pub fn render<T>(&mut self, data: &mut [T])
    where
        T: SizedSample + FromSample<f32>,
    {
        let channels = self.channels;

        // 1. Flush handshake: drain the ring and ack, rendering silence.
        let flush_target = self.shared.flush_target_gen.load(Ordering::Acquire);
        if flush_target != self.shared.flush_ack_gen.load(Ordering::Relaxed) {
            while self.consumer.pop().is_ok() {}
            self.eq.reset_state();
            self.shared.flush_ack_gen.store(flush_target, Ordering::Release);
            silence(data);
            return;
        }

        // 2. Pick up EQ changes (wait-free).
        if self.eq_output.update() {
            let set = *self.eq_output.output_buffer();
            self.eq.set_target(&set);
        }
        self.eq.block_update(data.len() / channels);

        let paused = self.shared.paused.load(Ordering::Relaxed);
        let volume = f64::from(self.shared.volume_amp());
        let target_gain = if paused { 0.0 } else { volume };

        let mut frames_rendered: u64 = 0;
        let mut starved = false;

        for frame in data.chunks_mut(channels) {
            self.gain += (target_gain - self.gain) * GAIN_ALPHA;

            if paused && self.gain < SILENCE_EPSILON {
                for s in frame.iter_mut() {
                    *s = T::from_sample(0.0f32);
                }
                continue;
            }

            match (self.consumer.pop(), self.consumer.pop()) {
                (Ok(mut l), Ok(mut r)) => {
                    self.eq.process_frame(&mut l, &mut r);
                    let l = (f64::from(l) * self.gain).clamp(-1.0, 1.0) as f32;
                    let r = (f64::from(r) * self.gain).clamp(-1.0, 1.0) as f32;
                    write_frame(frame, l, r);
                    frames_rendered += 1;
                }
                _ => {
                    for s in frame.iter_mut() {
                        *s = T::from_sample(0.0f32);
                    }
                    if !paused && target_gain > 0.0 {
                        starved = true;
                    }
                }
            }
        }

        if frames_rendered > 0 {
            self.shared.frames_played.fetch_add(frames_rendered, Ordering::Relaxed);
        }
        if starved {
            self.shared.underrun.store(true, Ordering::Relaxed);
        }
    }
}

#[inline(always)]
fn write_frame<T: SizedSample + FromSample<f32>>(frame: &mut [T], l: f32, r: f32) {
    match frame.len() {
        0 => {}
        1 => frame[0] = T::from_sample(0.5 * (l + r)),
        n => {
            frame[0] = T::from_sample(l);
            frame[1] = T::from_sample(r);
            for s in frame[2..n].iter_mut() {
                *s = T::from_sample(0.0f32);
            }
        }
    }
}

fn silence<T: SizedSample + FromSample<f32>>(data: &mut [T]) {
    for s in data.iter_mut() {
        *s = T::from_sample(0.0f32);
    }
}
