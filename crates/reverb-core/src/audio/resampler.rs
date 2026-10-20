//! Stereo resampler: track rate → device rate, interleaved f32 in and out.
//! Wraps rubato's async sinc resampler; pass-through when rates match.

use rubato::audioadapter_buffers::direct::InterleavedSlice;
use rubato::{
    Async, FixedAsync, Resampler, SincInterpolationParameters, SincInterpolationType,
    WindowFunction,
};

const CHUNK_FRAMES: usize = 1024;

#[derive(Debug, thiserror::Error)]
#[error("resampler error: {0}")]
pub struct ResamplerError(String);

pub struct StereoResampler {
    inner: Option<Async<f32>>,
    staging: Vec<f32>,
    out_buf: Vec<f32>,
    max_out_frames: usize,
}

impl StereoResampler {
    pub fn new(in_rate: u32, out_rate: u32) -> Result<Self, ResamplerError> {
        if in_rate == out_rate {
            return Ok(Self { inner: None, staging: Vec::new(), out_buf: Vec::new(), max_out_frames: 0 });
        }
        let params = SincInterpolationParameters {
            sinc_len: 128,
            f_cutoff: None,
            oversampling_factor: 128,
            interpolation: SincInterpolationType::Cubic,
            window: WindowFunction::Blackman2,
        };
        let inner = Async::<f32>::new_sinc(
            f64::from(out_rate) / f64::from(in_rate),
            1.1,
            &params,
            CHUNK_FRAMES,
            2,
            FixedAsync::Input,
        )
        .map_err(|e| ResamplerError(e.to_string()))?;
        let max_out_frames = inner.output_frames_max();
        Ok(Self {
            inner: Some(inner),
            staging: Vec::with_capacity(CHUNK_FRAMES * 4),
            out_buf: vec![0.0; max_out_frames * 2],
            max_out_frames,
        })
    }

    pub fn is_passthrough(&self) -> bool {
        self.inner.is_none()
    }

    /// Feed interleaved stereo input; `emit` receives interleaved stereo
    /// output (possibly multiple times, possibly not at all).
    pub fn process(
        &mut self,
        input: &[f32],
        mut emit: impl FnMut(&[f32]),
    ) -> Result<(), ResamplerError> {
        let Some(resampler) = self.inner.as_mut() else {
            if !input.is_empty() {
                emit(input);
            }
            return Ok(());
        };
        self.staging.extend_from_slice(input);
        while self.staging.len() >= CHUNK_FRAMES * 2 {
            let in_adapter = InterleavedSlice::new(&self.staging[..CHUNK_FRAMES * 2], 2, CHUNK_FRAMES)
                .map_err(|e| ResamplerError(e.to_string()))?;
            let mut out_adapter =
                InterleavedSlice::new_mut(&mut self.out_buf[..], 2, self.max_out_frames)
                    .map_err(|e| ResamplerError(e.to_string()))?;
            let (frames_in, frames_out) = resampler
                .process_into_buffer(&in_adapter, &mut out_adapter, None)
                .map_err(|e| ResamplerError(e.to_string()))?;
            if frames_out > 0 {
                emit(&self.out_buf[..frames_out * 2]);
            }
            self.staging.drain(..frames_in * 2);
        }
        Ok(())
    }

    /// Flush whatever is buffered (end of track): pad to a full chunk with
    /// silence and run it through.
    pub fn flush(&mut self, mut emit: impl FnMut(&[f32])) -> Result<(), ResamplerError> {
        if self.inner.is_none() {
            return Ok(());
        }
        if !self.staging.is_empty() {
            let deficit = CHUNK_FRAMES * 2 - self.staging.len() % (CHUNK_FRAMES * 2);
            if deficit != CHUNK_FRAMES * 2 {
                self.staging.extend(std::iter::repeat_n(0.0, deficit));
            }
            self.process(&[], &mut emit)?;
            // process() consumed everything since staging is now chunk-aligned.
        }
        Ok(())
    }

    /// Drop any buffered input and internal filter history (used on seek).
    pub fn reset(&mut self) {
        self.staging.clear();
        if let Some(r) = self.inner.as_mut() {
            r.reset();
        }
    }
}
