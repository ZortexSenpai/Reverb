use reverb_core::audio::{EngineCommand, TrackSource};
use reverb_core::subsonic::models::ReplayGain;
use serde::Deserialize;
use tauri::State;

use crate::error::ReverbError;
use crate::state::{AppState, PlayerSnapshot};

/// The frontend passes the track metadata it already has, so playback never
/// needs a metadata round-trip to the server.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlayableTrack {
    pub track_id: String,
    pub suffix: Option<String>,
    pub duration_ms: Option<u64>,
    pub title: Option<String>,
    pub artist: Option<String>,
    /// First credited artist alone; `None` unless the server enumerated them.
    pub first_artist: Option<String>,
    pub album: Option<String>,
    pub bit_rate: Option<u32>,
    pub replay_gain: Option<ReplayGain>,
}

fn to_track_source(state: &AppState, track: &PlayableTrack) -> Result<TrackSource, ReverbError> {
    // Default is the original file: seekable via HTTP ranges and bit-perfect.
    // Server-side transcoding kicks in when the user chose a bitrate cap, or
    // for Opus files (symphonia has no Opus decoder; Navidrome ships an MP3
    // transcoding profile by default).
    let is_opus = track.suffix.as_deref().is_some_and(|s| s.eq_ignore_ascii_case("opus"));
    let streaming = state.settings.get().prefs.streaming_format;
    let transcode_kbps = match streaming.as_str() {
        "mp3-320" => Some(320),
        "mp3-192" => Some(192),
        "mp3-128" => Some(128),
        _ => None,
    };

    let (format, max_bit_rate, ext_hint) = if let Some(kbps) = transcode_kbps {
        (Some("mp3"), Some(kbps), Some("mp3".to_owned()))
    } else if is_opus {
        (Some("mp3"), None, Some("mp3".to_owned()))
    } else {
        (Some("raw"), None, track.suffix.clone())
    };

    let url = state.subsonic.stream_url(&track.track_id, format, max_bit_rate)?;
    Ok(TrackSource {
        track_id: track.track_id.clone(),
        url: url.to_string(),
        ext_hint,
        duration_ms: track.duration_ms,
        title: track.title.clone(),
        artist: track.artist.clone(),
        first_artist: track.first_artist.clone(),
        album: track.album.clone(),
        bit_rate: track.bit_rate,
        replay_gain: track.replay_gain,
    })
}

#[tauri::command]
pub fn player_get_state(state: State<'_, AppState>) -> PlayerSnapshot {
    state.player.lock().unwrap().clone()
}

#[tauri::command]
pub fn player_play_track(
    track: PlayableTrack,
    state: State<'_, AppState>,
) -> Result<(), ReverbError> {
    let src = to_track_source(&state, &track)?;
    state.engine.send(EngineCommand::PlayTrack(src));
    Ok(())
}

#[tauri::command]
pub fn player_set_next(
    track: Option<PlayableTrack>,
    state: State<'_, AppState>,
) -> Result<(), ReverbError> {
    let next = match &track {
        Some(t) => Some(to_track_source(&state, t)?),
        None => None,
    };
    state.engine.send(EngineCommand::SetNext(next));
    Ok(())
}

#[tauri::command]
pub fn player_pause(state: State<'_, AppState>) {
    state.engine.send(EngineCommand::Pause);
}

#[tauri::command]
pub fn player_resume(state: State<'_, AppState>) {
    state.engine.send(EngineCommand::Resume);
}

#[tauri::command]
pub fn player_toggle(state: State<'_, AppState>) {
    state.engine.send(EngineCommand::Toggle);
}

#[tauri::command]
pub fn player_stop(state: State<'_, AppState>) {
    state.engine.send(EngineCommand::Stop);
}

#[tauri::command]
pub fn player_seek(position_ms: u64, state: State<'_, AppState>) {
    state.engine.send(EngineCommand::Seek { position_ms });
}

#[tauri::command]
pub fn player_set_volume(volume: f32, state: State<'_, AppState>) -> Result<(), ReverbError> {
    let volume = volume.clamp(0.0, 1.0);
    state.engine.send(EngineCommand::SetVolume { volume });
    state.player.lock().unwrap().volume = volume;
    state.settings.update(|s| s.volume = volume)?;
    Ok(())
}
