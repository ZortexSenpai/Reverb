use reverb_core::audio::{list_output_devices, EngineCommand, OutputDevice};
use serde::Serialize;
use tauri::State;

use crate::error::ReverbError;
use crate::persistence::settings::Preferences;
use crate::state::AppState;

/// Settings exposed to the frontend (no secrets live here).
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FrontendSettings {
    pub volume: f32,
    #[serde(flatten)]
    pub prefs: Preferences,
}

#[tauri::command]
pub fn settings_get(state: State<'_, AppState>) -> FrontendSettings {
    let s = state.settings.get();
    FrontendSettings { volume: s.volume, prefs: s.prefs }
}

/// Persist the full preference set and apply anything the engine cares about.
#[tauri::command]
pub fn settings_set_prefs(
    prefs: Preferences,
    state: State<'_, AppState>,
) -> Result<(), ReverbError> {
    let old = state.settings.get().prefs;
    if old.audio_device != prefs.audio_device {
        state.engine.send(EngineCommand::SetDevice(prefs.audio_device.clone()));
    }
    let rg_changed = serde_json::to_value(old.replay_gain).ok()
        != serde_json::to_value(prefs.replay_gain).ok();
    if rg_changed {
        state.engine.send(EngineCommand::SetReplayGain(prefs.replay_gain));
    }
    if old.scrobble_at_percent != prefs.scrobble_at_percent {
        state.engine.send(EngineCommand::SetScrobblePercent(prefs.scrobble_at_percent));
    }
    if old.crossfade_secs != prefs.crossfade_secs {
        state
            .engine
            .send(EngineCommand::SetCrossfade { ms: u64::from(prefs.crossfade_secs) * 1000 });
    }
    if old.match_source_rate != prefs.match_source_rate
        || old.exclusive_output != prefs.exclusive_output
    {
        state.engine.send(EngineCommand::SetOutputMode {
            auto_rate: prefs.match_source_rate,
            exclusive: prefs.exclusive_output,
        });
    }

    // Toggling scrobbling adjusts the server's now-playing display right away:
    // off clears the entry, on re-announces the current track. Switching
    // Last.fm-only mode does the same thing for the same reason — turning it
    // on stops reporting to the server, so the entry it left behind has to be
    // cleared rather than left to go stale.
    if old.scrobble_enabled != prefs.scrobble_enabled
        || old.lastfm_scrobble_only != prefs.lastfm_scrobble_only
    {
        let (track_id, position_ms, playback_state) = {
            let snapshot = state.player.lock().unwrap();
            (snapshot.track_id.clone(), snapshot.position_ms, snapshot.state)
        };
        if let Some(track_id) = track_id {
            let subsonic = state.subsonic.clone();
            let enabling = prefs.scrobble_enabled && !prefs.lastfm_scrobble_only;
            tauri::async_runtime::spawn(async move {
                if enabling {
                    let _ = subsonic.scrobble(&track_id, false, None).await;
                    let report_state = match playback_state {
                        reverb_core::audio::PlaybackState::Paused => "paused",
                        _ => "playing",
                    };
                    subsonic.report_playback(&track_id, position_ms, report_state).await;
                } else {
                    subsonic.report_playback(&track_id, position_ms, "stopped").await;
                }
            });
        }
    }

    state.settings.update(|s| {
        // Disabling queue persistence also discards what's already stored.
        if !prefs.save_queue {
            s.queue_snapshot = None;
        }
        s.prefs = prefs;
    })?;
    Ok(())
}

#[tauri::command]
pub fn audio_list_devices() -> Vec<OutputDevice> {
    list_output_devices()
}

/// The frontend owns the queue; it checkpoints an opaque snapshot here so a
/// restart can restore it.
#[tauri::command]
pub fn queue_save_snapshot(
    snapshot: Option<serde_json::Value>,
    state: State<'_, AppState>,
) -> Result<(), ReverbError> {
    state.settings.update(|s| s.queue_snapshot = snapshot)?;
    Ok(())
}

#[tauri::command]
pub fn queue_load_snapshot(state: State<'_, AppState>) -> Option<serde_json::Value> {
    state.settings.get().queue_snapshot
}
