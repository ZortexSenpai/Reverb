//! Setting up an HQPlayer device config: testing an address, and finding the
//! HQPlayers on the local network. Playing through one goes through
//! `Playback` like any other output.

use std::time::Duration;

use reverb_core::hqplayer::{self, Discovered, Endpoint, Info};

use crate::error::ReverbError;

/// How long a search listens for answers.
const DISCOVERY_WAIT: Duration = Duration::from_secs(2);

/// Connect to the HQPlayer at `host:port` and ask what it is — the check that
/// it is there and takes control from the network.
#[tauri::command]
pub async fn hqplayer_probe(host: String, port: u16) -> Result<Info, ReverbError> {
    let endpoint = Endpoint { host: host.trim().to_owned(), port };
    tauri::async_runtime::spawn_blocking(move || hqplayer::probe(&endpoint))
        .await
        .map_err(|e| ReverbError::State(e.to_string()))?
        .map_err(|e| ReverbError::HqPlayer(e.to_string()))
}

/// The HQPlayers that answer a search of the local network.
#[tauri::command]
pub async fn hqplayer_discover() -> Result<Vec<Discovered>, ReverbError> {
    tauri::async_runtime::spawn_blocking(|| hqplayer::discover(DISCOVERY_WAIT))
        .await
        .map_err(|e| ReverbError::State(e.to_string()))?
        .map_err(|e| ReverbError::HqPlayer(e.to_string()))
}
