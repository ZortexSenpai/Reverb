//! OS media session integration (MPRIS on Linux, SMTC on Windows,
//! MPNowPlayingInfoCenter on macOS) via souvlaki. Hardware media keys and
//! desktop players control Reverb through this.

use std::time::Duration;

use reverb_core::audio::{EngineCommand, PlaybackState};
use souvlaki::{
    MediaControlEvent, MediaControls, MediaMetadata, MediaPlayback, MediaPosition, PlatformConfig,
};
use tauri::{AppHandle, Emitter, Manager};

use crate::state::AppState;

pub struct MediaSession {
    controls: Option<MediaControls>,
}

impl MediaSession {
    /// Best-effort: a missing D-Bus session (or SMTC failure) just means no
    /// media keys, never a startup failure.
    pub fn new(app: &AppHandle) -> Self {
        #[cfg(target_os = "windows")]
        let hwnd = app
            .get_webview_window("main")
            .and_then(|w| w.hwnd().ok())
            .map(|h| h.0 as *mut std::ffi::c_void);
        #[cfg(not(target_os = "windows"))]
        let hwnd = None;
        #[cfg(not(target_os = "windows"))]
        let _ = app.package_info(); // silence unused-variable lint paths

        let config = PlatformConfig {
            display_name: "Reverb",
            dbus_name: "reverb",
            hwnd,
        };

        let controls = match MediaControls::new(config) {
            Ok(mut controls) => {
                let app = app.clone();
                match controls.attach(move |event| handle_event(&app, event)) {
                    Ok(()) => Some(controls),
                    Err(e) => {
                        log::warn!("media session attach failed: {e:?}");
                        None
                    }
                }
            }
            Err(e) => {
                log::warn!("media session unavailable: {e:?}");
                None
            }
        };

        Self { controls }
    }

    pub fn on_track_changed(
        &mut self,
        title: Option<&str>,
        artist: Option<&str>,
        album: Option<&str>,
        duration_ms: u64,
    ) {
        if let Some(controls) = self.controls.as_mut() {
            let _ = controls.set_metadata(MediaMetadata {
                title,
                artist,
                album,
                duration: if duration_ms > 0 {
                    Some(Duration::from_millis(duration_ms))
                } else {
                    None
                },
                ..Default::default()
            });
        }
    }

    pub fn on_playback(&mut self, state: PlaybackState, position_ms: u64) {
        if let Some(controls) = self.controls.as_mut() {
            let progress = Some(MediaPosition(Duration::from_millis(position_ms)));
            let playback = match state {
                PlaybackState::Playing | PlaybackState::Buffering => {
                    MediaPlayback::Playing { progress }
                }
                PlaybackState::Paused => MediaPlayback::Paused { progress },
                PlaybackState::Stopped => MediaPlayback::Stopped,
            };
            let _ = controls.set_playback(playback);
        }
    }
}

fn handle_event(app: &AppHandle, event: MediaControlEvent) {
    let state = app.state::<AppState>();
    match event {
        MediaControlEvent::Play => state.engine.send(EngineCommand::Resume),
        MediaControlEvent::Pause => state.engine.send(EngineCommand::Pause),
        MediaControlEvent::Toggle => state.engine.send(EngineCommand::Toggle),
        MediaControlEvent::Stop | MediaControlEvent::Quit => {
            state.engine.send(EngineCommand::Stop)
        }
        // The queue lives in the frontend — route track changes there.
        MediaControlEvent::Next => {
            let _ = app.emit("player:media-key", "next");
        }
        MediaControlEvent::Previous => {
            let _ = app.emit("player:media-key", "previous");
        }
        MediaControlEvent::SetPosition(MediaPosition(position)) => {
            state
                .engine
                .send(EngineCommand::Seek { position_ms: position.as_millis() as u64 });
        }
        _ => {}
    }
}
