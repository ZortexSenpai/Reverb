//! `TrackDecoder` — wraps symphonia probe + format reader + audio decoder for
//! one track, yielding interleaved f32 chunks at the track's native rate.

use symphonia::core::audio::AudioSpec;
use symphonia::core::codecs::audio::{AudioDecoder, AudioDecoderOptions};
use symphonia::core::errors::Error as SymphoniaError;
use symphonia::core::formats::probe::Hint;
use symphonia::core::formats::{
    FormatOptions, FormatReader, SeekMode, SeekTo, TrackType,
};
use symphonia::core::io::{MediaSource, MediaSourceStream, MediaSourceStreamOptions};
use symphonia::core::meta::MetadataOptions;
use symphonia::core::units::Time;

#[derive(Debug, thiserror::Error)]
pub enum DecoderError {
    #[error("could not detect audio format: {0}")]
    Probe(String),
    #[error("no decodable audio track")]
    NoAudioTrack,
    #[error("codec unsupported: {0}")]
    Unsupported(String),
    #[error("seek failed: {0}")]
    Seek(String),
    #[error("decode failed: {0}")]
    Fatal(String),
}

/// Owned metadata about a decoded chunk (samples land in the caller's buffer).
#[derive(Debug, Clone, Copy)]
pub struct ChunkInfo {
    pub channels: usize,
    pub sample_rate: u32,
}

pub struct TrackDecoder {
    reader: Box<dyn FormatReader + 'static>,
    decoder: Box<dyn AudioDecoder>,
    track_id: u32,
    time_base: Option<symphonia::core::units::TimeBase>,
    sample_rate: u32,
    channels: usize,
    duration_ms: Option<u64>,
    seekable: bool,
    /// Frames still to discard after an accurate seek landed early.
    skip_frames: u64,
}

impl TrackDecoder {
    pub fn open(source: Box<dyn MediaSource>, ext_hint: Option<&str>) -> Result<Self, DecoderError> {
        let seekable = source.is_seekable();
        let mss = MediaSourceStream::new(
            source,
            MediaSourceStreamOptions { buffer_len: 128 * 1024 },
        );
        let mut hint = Hint::new();
        if let Some(ext) = ext_hint {
            hint.with_extension(ext);
        }

        let reader = symphonia::default::get_probe()
            .probe(&hint, mss, FormatOptions::default(), MetadataOptions::default())
            .map_err(|e| DecoderError::Probe(e.to_string()))?;

        let track = reader.default_track(TrackType::Audio).ok_or(DecoderError::NoAudioTrack)?;
        let track_id = track.id;
        let time_base = track.time_base;
        let params = track
            .codec_params
            .as_ref()
            .and_then(|p| p.audio())
            .ok_or(DecoderError::NoAudioTrack)?
            .clone();

        let duration_ms = time_base.zip(track
            .duration)
            .and_then(|(tb, dur)| {
                tb.calc_time(symphonia::core::units::Timestamp::new(0).checked_add(dur)?)
            })
            .map(|t| (t.as_secs_f64() * 1000.0) as u64);

        let decoder = symphonia::default::get_codecs()
            .make_audio_decoder(&params, &AudioDecoderOptions::default())
            .map_err(|e| DecoderError::Unsupported(e.to_string()))?;

        let sample_rate = params.sample_rate.unwrap_or(44100);
        let channels = params.channels.as_ref().map(|c| c.count()).unwrap_or(2).max(1);

        Ok(Self {
            reader,
            decoder,
            track_id,
            time_base,
            sample_rate,
            channels,
            duration_ms,
            seekable,
            skip_frames: 0,
        })
    }

    pub fn sample_rate(&self) -> u32 {
        self.sample_rate
    }

    pub fn duration_ms(&self) -> Option<u64> {
        self.duration_ms
    }

    pub fn is_seekable(&self) -> bool {
        self.seekable
    }

    /// Decode the next chunk into `out` (overwritten, interleaved).
    /// `Ok(None)` is clean end of stream. Corrupt packets are skipped rather
    /// than surfaced.
    pub fn next_chunk_into(
        &mut self,
        out: &mut Vec<f32>,
    ) -> Result<Option<ChunkInfo>, DecoderError> {
        loop {
            let packet = match self.reader.next_packet() {
                Ok(Some(p)) => p,
                Ok(None) => return Ok(None),
                Err(SymphoniaError::ResetRequired) => {
                    self.decoder.reset();
                    continue;
                }
                Err(SymphoniaError::IoError(e))
                    if e.kind() == std::io::ErrorKind::UnexpectedEof =>
                {
                    return Ok(None);
                }
                Err(e) => return Err(DecoderError::Fatal(e.to_string())),
            };
            if packet.track_id != self.track_id {
                continue;
            }

            match self.decoder.decode(&packet) {
                Ok(buf) => {
                    if buf.is_empty() {
                        continue;
                    }
                    let spec: &AudioSpec = buf.spec();
                    self.sample_rate = spec.rate();
                    self.channels = spec.channels().count().max(1);
                    buf.copy_to_vec_interleaved::<f32>(out);

                    if self.skip_frames > 0 {
                        let frames = out.len() / self.channels;
                        let skip = (self.skip_frames as usize).min(frames);
                        self.skip_frames -= skip as u64;
                        out.drain(..skip * self.channels);
                        if out.is_empty() {
                            continue;
                        }
                    }
                    return Ok(Some(ChunkInfo {
                        channels: self.channels,
                        sample_rate: self.sample_rate,
                    }));
                }
                // Undecodable packet: skip it and keep going.
                Err(SymphoniaError::DecodeError(_)) | Err(SymphoniaError::IoError(_)) => continue,
                Err(SymphoniaError::ResetRequired) => {
                    self.decoder.reset();
                    continue;
                }
                Err(e) => return Err(DecoderError::Fatal(e.to_string())),
            }
        }
    }

    /// Seek to `secs`, sample-accurately (decode-and-discard after the
    /// container seek). Returns the position playback will resume at.
    pub fn seek(&mut self, secs: f64) -> Result<f64, DecoderError> {
        if !self.seekable {
            return Err(DecoderError::Seek("stream is not seekable".into()));
        }
        let time = Time::try_from_secs_f64(secs.max(0.0))
            .ok_or_else(|| DecoderError::Seek(format!("bad position {secs}")))?;
        let seeked = self
            .reader
            .seek(SeekMode::Accurate, SeekTo::Time { time, track_id: Some(self.track_id) })
            .map_err(|e| DecoderError::Seek(e.to_string()))?;
        self.decoder.reset();

        let actual_secs = self
            .time_base
            .and_then(|tb| tb.calc_time(seeked.actual_ts))
            .map(|t| t.as_secs_f64())
            .unwrap_or(secs);

        if actual_secs < secs {
            self.skip_frames = ((secs - actual_secs) * f64::from(self.sample_rate)).round() as u64;
            Ok(secs)
        } else {
            self.skip_frames = 0;
            Ok(actual_secs)
        }
    }
}
