//! Events flowing from the engine thread out to the app layer (which forwards
//! them to the frontend as Tauri events).

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PlaybackState {
    Playing,
    Paused,
    Stopped,
    Buffering,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TrackChangeReason {
    /// The engine flowed gaplessly into the preloaded next track.
    Natural,
    /// An explicit play command switched tracks.
    Command,
}

#[derive(Debug, Clone)]
pub enum EngineEvent {
    State {
        state: PlaybackState,
    },
    Position {
        position_ms: u64,
        duration_ms: u64,
    },
    TrackChanged {
        track_id: String,
        reason: TrackChangeReason,
        title: Option<String>,
        artist: Option<String>,
        album: Option<String>,
        duration_ms: u64,
    },
    /// The queue ran dry — playback stopped after this track.
    TrackEnded {
        track_id: String,
    },
    Error {
        code: String,
        message: String,
        track_id: Option<String>,
        recoverable: bool,
    },
    /// Scrobble hook: `submission == false` is "now playing".
    ///
    /// Carries the track's metadata as well as its id: a Subsonic scrobble
    /// only needs the id, but Last.fm identifies a play by artist and title,
    /// and by the time this is handled the engine may already have moved on to
    /// the next track (gapless transitions run ahead of the display).
    Scrobble {
        track_id: String,
        submission: bool,
        title: Option<String>,
        artist: Option<String>,
        /// The first credited artist alone, when the server enumerated the
        /// credit. Carried beside `artist` rather than replacing it because
        /// only Last.fm submissions may want it — the OS media session and the
        /// server's own scrobble keep the full credit.
        first_artist: Option<String>,
        album: Option<String>,
        duration_ms: u64,
    },
    /// Emitted once at startup — the device rate the EQ runs at.
    Format {
        sample_rate: u32,
    },
    /// The configured output device is not there any more (a USB DAC unplugged,
    /// say). The engine has already reopened on the system default and dropped
    /// its own preference; carries the id it gave up so the app layer can clear
    /// the saved one to match.
    OutputDeviceLost {
        device: String,
    },
    /// The current audio chain (Roon-style signal path).
    SignalPath(SignalPathInfo),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SignalPathInfo {
    /// Source codec, e.g. "FLAC" (from the stream's format hint).
    pub codec: Option<String>,
    pub source_sample_rate: u32,
    /// kbps, from server metadata when known.
    pub bit_rate: Option<u32>,
    pub lossless_source: bool,
    /// Applied ReplayGain in dB, when active.
    pub replay_gain_db: Option<f32>,
    /// Output rate when the resampler is engaged.
    pub resample_to: Option<u32>,
    /// Enabled EQ bands (0 = bypassed).
    pub eq_bands: u32,
    pub output_sample_rate: u32,
    /// Display name of the device the stream is open on; `None` for the system
    /// default. Not the saved preference — after a fallback the two differ.
    pub output_device: Option<String>,
    /// The rate the hardware actually runs at, when that is knowable —
    /// `None` means we could not confirm it (see `output::hardware_rate`).
    pub device_rate: Option<u32>,
    /// The OS is resampling below us: our stream opened at one rate and the
    /// device is confirmed to be running at another. Bit-perfect playback is
    /// impossible while this is true, whatever the stream rate says.
    pub os_resampling: bool,
    /// "lossless" (bit-perfect), "high" (lossy source, untouched),
    /// or "enhanced" (DSP active).
    pub quality: String,
}
