use std::sync::{Arc, Mutex};

use reverb_core::audio::{EngineHandle, PlaybackState};
use reverb_core::subsonic::SubsonicClient;
use serde::Serialize;

use crate::mixes::store::MixStore;
use crate::persistence::keychain::Keychain;
use crate::persistence::settings::SettingsStore;
use crate::stats::store::StatsStore;

/// Snapshot handed to the frontend on boot/reload — mirrors what the event
/// stream would have told it. Kept current by the event forwarder.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlayerSnapshot {
    pub state: PlaybackState,
    pub track_id: Option<String>,
    pub position_ms: u64,
    pub duration_ms: u64,
    pub volume: f32,
    pub sample_rate: u32,
}

impl Default for PlayerSnapshot {
    fn default() -> Self {
        Self {
            state: PlaybackState::Stopped,
            track_id: None,
            position_ms: 0,
            duration_ms: 0,
            volume: 1.0,
            sample_rate: 48_000,
        }
    }
}

pub struct AppState {
    pub subsonic: Arc<SubsonicClient>,
    pub engine: EngineHandle,
    pub settings: SettingsStore,
    pub keychain: Keychain,
    pub player: Arc<Mutex<PlayerSnapshot>>,
    pub stats: StatsStore,
    pub mixes: MixStore,
}
