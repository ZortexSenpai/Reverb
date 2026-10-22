//! The Play History behind "Recently played" (see `crate::play_history`).
//! Only the frontend has the whole Song a row needs, so it records each Play
//! once it counts.

use reverb_core::subsonic::SubsonicError;
use tauri::State;

use crate::error::ReverbError;
use crate::play_history::store::Play;
use crate::state::AppState;

/// The signed-in account's Plays, newest first. None when signed out.
#[tauri::command]
pub fn history_get(state: State<'_, AppState>) -> Vec<Play> {
    state.play_history.plays(state.account().as_deref())
}

/// `played_at` is Unix ms of when the Song started.
#[tauri::command]
pub fn history_record(
    song: serde_json::Value,
    played_at: u64,
    state: State<'_, AppState>,
) -> Result<(), ReverbError> {
    let account = state.account().ok_or(SubsonicError::NotConfigured)?;
    state.play_history.record(&account, Play { played_at, song })
}

#[tauri::command]
pub fn history_clear(state: State<'_, AppState>) -> Result<(), ReverbError> {
    let account = state.account().ok_or(SubsonicError::NotConfigured)?;
    state.play_history.clear(&account)
}
