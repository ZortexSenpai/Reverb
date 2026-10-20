//! Bridges `EngineEvent`s from the audio thread to Tauri events for the
//! frontend, keeps the `PlayerSnapshot` cache current, and fires scrobbles.

use std::sync::{Arc, Mutex, OnceLock};
use std::time::Instant;

use crossbeam_channel::Receiver;
use reverb_core::audio::{EngineEvent, PlaybackState, TrackChangeReason};
use reverb_core::subsonic::SubsonicClient;
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};

use crate::commands::stats::{LASTFM_SECRET_KEY, LASTFM_SESSION_KEY};
use crate::state::PlayerSnapshot;
use crate::stats::lastfm::{self, ScrobbleTrack};

/// One connection pool for the scrobble traffic, rather than a fresh client
/// (and TLS handshake) per play.
fn http_client() -> &'static reqwest::Client {
    static CLIENT: OnceLock<reqwest::Client> = OnceLock::new();
    CLIENT.get_or_init(reqwest::Client::new)
}

/// Credentials for submitting plays straight to Last.fm.
struct LastfmTarget {
    api_key: String,
    secret: String,
    session_key: String,
}

/// Resolve the Last.fm-only target, or `None` to fall back to the Subsonic
/// server. Deliberately strict: the mode applies only when the preference is
/// on *and* a full set of submission credentials exists. A half-configured
/// Last.fm would otherwise route plays away from Navidrome and then drop
/// them, which is worse than ignoring the preference.
fn lastfm_target(app: &AppHandle) -> Option<LastfmTarget> {
    let state = app.state::<crate::state::AppState>();
    if !state.settings.get().prefs.lastfm_scrobble_only {
        return None;
    }
    let config = state.stats.config().lastfm?;
    if !config.scrobbling {
        return None;
    }
    Some(LastfmTarget {
        api_key: config.api_key,
        secret: state.keychain.retrieve_secret(LASTFM_SECRET_KEY)?,
        session_key: state.keychain.retrieve_secret(LASTFM_SESSION_KEY)?,
    })
}

/// Feeds the server's live now-playing display (OpenSubsonic playbackReport):
/// periodic updates while playing, immediate ones on seeks, pause/resume,
/// track changes, and stop.
struct PlaybackReporter {
    track_id: Option<String>,
    last_sent: Option<Instant>,
    last_position_ms: u64,
    last_position_at: Instant,
}

const REPORT_INTERVAL_SECS: u64 = 10;
/// A position that deviates from expected linear progress by more than this
/// is a seek and gets reported immediately.
const SEEK_JUMP_MS: u64 = 2_000;

impl PlaybackReporter {
    fn new() -> Self {
        Self {
            track_id: None,
            last_sent: None,
            last_position_ms: 0,
            last_position_at: Instant::now(),
        }
    }

    fn send(&mut self, app: &AppHandle, subsonic: &Arc<SubsonicClient>, position_ms: u64, state: &'static str) {
        let Some(track_id) = self.track_id.clone() else { return };
        let enabled =
            app.state::<crate::state::AppState>().settings.get().prefs.scrobble_enabled;
        if !enabled {
            return;
        }
        // Last.fm-only mode means the server hears nothing at all — the
        // now-playing entry is part of what the user asked us not to send.
        if lastfm_target(app).is_some() {
            return;
        }
        self.last_sent = Some(Instant::now());
        let subsonic = subsonic.clone();
        tauri::async_runtime::spawn(async move {
            subsonic.report_playback(&track_id, position_ms, state).await;
        });
    }

    fn on_track_changed(&mut self, app: &AppHandle, subsonic: &Arc<SubsonicClient>, track_id: &str) {
        self.track_id = Some(track_id.to_owned());
        self.last_position_ms = 0;
        self.last_position_at = Instant::now();
        self.send(app, subsonic, 0, "playing");
    }

    fn on_position(&mut self, app: &AppHandle, subsonic: &Arc<SubsonicClient>, position_ms: u64) {
        let expected =
            self.last_position_ms + self.last_position_at.elapsed().as_millis() as u64;
        let jumped = position_ms.abs_diff(expected) > SEEK_JUMP_MS;
        let due = self
            .last_sent
            .is_none_or(|at| at.elapsed().as_secs() >= REPORT_INTERVAL_SECS);
        self.last_position_ms = position_ms;
        self.last_position_at = Instant::now();
        if jumped || due {
            self.send(app, subsonic, position_ms, "playing");
        }
    }

    fn on_state(&mut self, app: &AppHandle, subsonic: &Arc<SubsonicClient>, state: PlaybackState, position_ms: u64) {
        match state {
            PlaybackState::Playing => self.send(app, subsonic, position_ms, "playing"),
            PlaybackState::Paused => self.send(app, subsonic, position_ms, "paused"),
            PlaybackState::Stopped => {
                self.send(app, subsonic, position_ms, "stopped");
                self.track_id = None;
            }
            // Transient; keep the last reported state.
            PlaybackState::Buffering => {}
        }
    }
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct StatePayload {
    state: PlaybackState,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct PositionPayload {
    position_ms: u64,
    duration_ms: u64,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct TrackChangedPayload {
    track_id: String,
    reason: TrackChangeReason,
    title: Option<String>,
    artist: Option<String>,
    album: Option<String>,
    duration_ms: u64,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct TrackEndedPayload {
    track_id: String,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct ErrorPayload {
    code: String,
    message: String,
    track_id: Option<String>,
    recoverable: bool,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct FormatPayload {
    sample_rate: u32,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct DeviceLostPayload {
    /// The device id that was given up. Its label is unknowable — the device is
    /// gone from the enumeration the label would have come from.
    device: String,
}

pub fn spawn_forwarder(
    app: AppHandle,
    rx: Receiver<EngineEvent>,
    player: Arc<Mutex<PlayerSnapshot>>,
    subsonic: Arc<SubsonicClient>,
) {
    std::thread::Builder::new()
        .name("reverb-event-forwarder".into())
        .spawn(move || {
            let mut media = crate::media_keys::MediaSession::new(&app);
            let mut reporter = PlaybackReporter::new();
            while let Ok(event) = rx.recv() {
                forward(&app, &player, &subsonic, &mut media, &mut reporter, event);
            }
        })
        .expect("spawning the event forwarder cannot fail");
}

fn forward(
    app: &AppHandle,
    player: &Arc<Mutex<PlayerSnapshot>>,
    subsonic: &Arc<SubsonicClient>,
    media: &mut crate::media_keys::MediaSession,
    reporter: &mut PlaybackReporter,
    event: EngineEvent,
) {
    match event {
        EngineEvent::State { state } => {
            let position_ms;
            {
                let mut snapshot = player.lock().unwrap();
                snapshot.state = state;
                if state == PlaybackState::Stopped {
                    snapshot.track_id = None;
                    snapshot.position_ms = 0;
                    snapshot.duration_ms = 0;
                }
                position_ms = snapshot.position_ms;
            }
            media.on_playback(state, position_ms);
            reporter.on_state(app, subsonic, state, position_ms);
            let _ = app.emit("player:state", StatePayload { state });
        }
        EngineEvent::Position { position_ms, duration_ms } => {
            let state;
            {
                let mut snapshot = player.lock().unwrap();
                snapshot.position_ms = position_ms;
                snapshot.duration_ms = duration_ms;
                state = snapshot.state;
            }
            media.on_playback(state, position_ms);
            if state == PlaybackState::Playing {
                reporter.on_position(app, subsonic, position_ms);
            }
            let _ = app.emit("player:position", PositionPayload { position_ms, duration_ms });
        }
        EngineEvent::TrackChanged { track_id, reason, title, artist, album, duration_ms } => {
            {
                let mut snapshot = player.lock().unwrap();
                snapshot.track_id = Some(track_id.clone());
                snapshot.position_ms = 0;
            }
            media.on_track_changed(title.as_deref(), artist.as_deref(), album.as_deref(), duration_ms);
            reporter.on_track_changed(app, subsonic, &track_id);
            let _ = app.emit(
                "player:track-changed",
                TrackChangedPayload { track_id, reason, title, artist, album, duration_ms },
            );
        }
        EngineEvent::TrackEnded { track_id } => {
            let _ = app.emit("player:track-ended", TrackEndedPayload { track_id });
        }
        EngineEvent::Error { code, message, track_id, recoverable } => {
            log::warn!("engine error [{code}]: {message}");
            let _ = app
                .emit("player:error", ErrorPayload { code, message, track_id, recoverable });
        }
        EngineEvent::Scrobble {
            track_id,
            submission,
            title,
            artist,
            first_artist,
            album,
            duration_ms,
        } => {
            use tauri::Manager;
            let scrobble_enabled =
                app.state::<crate::state::AppState>().settings.get().prefs.scrobble_enabled;
            if scrobble_enabled {
                // Submissions carry the time the song *started* playing.
                let time_ms = submission.then(|| {
                    let position_ms = player.lock().unwrap().position_ms;
                    let now_ms = std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .map(|d| d.as_millis() as u64)
                        .unwrap_or(0);
                    now_ms.saturating_sub(position_ms)
                });
                match lastfm_target(app) {
                    Some(target) => {
                        // Last.fm-only: the server is skipped entirely.
                        //
                        // Last.fm identifies a play by artist *name*, and a
                        // joined credit ("A feat. B") matches no artist there,
                        // so the preference trades the featured names for the
                        // play landing on the main one. Only possible when the
                        // server enumerated the credit; otherwise the full
                        // string is all there is and it goes as-is.
                        let first_artist_only = app
                            .state::<crate::state::AppState>()
                            .settings
                            .get()
                            .prefs
                            .lastfm_scrobble_first_artist_only;
                        let artist =
                            lastfm::scrobble_artist(artist, first_artist, first_artist_only);
                        let Some(track) =
                            ScrobbleTrack::from_parts(artist, title, album, duration_ms)
                        else {
                            log::debug!("no artist/title for {track_id}; nothing to scrobble");
                            return;
                        };
                        // `time_ms` is only computed for submissions; for a
                        // now-playing update Last.fm wants no timestamp.
                        let started_secs = time_ms.map(|ms| (ms / 1000) as i64);
                        tauri::async_runtime::spawn(async move {
                            let client = http_client();
                            let result = match started_secs {
                                Some(ts) => {
                                    lastfm::scrobble(
                                        client,
                                        &target.api_key,
                                        &target.secret,
                                        &target.session_key,
                                        &track,
                                        ts,
                                    )
                                    .await
                                }
                                None => {
                                    lastfm::update_now_playing(
                                        client,
                                        &target.api_key,
                                        &target.secret,
                                        &target.session_key,
                                        &track,
                                    )
                                    .await
                                }
                            };
                            if let Err(e) = result {
                                log::debug!("last.fm scrobble failed for {track_id}: {e}");
                            }
                        });
                    }
                    None => {
                        let subsonic = subsonic.clone();
                        tauri::async_runtime::spawn(async move {
                            if let Err(e) = subsonic.scrobble(&track_id, submission, time_ms).await
                            {
                                log::debug!("scrobble failed for {track_id}: {e}");
                            }
                        });
                    }
                }
            }
        }
        EngineEvent::Format { sample_rate } => {
            player.lock().unwrap().sample_rate = sample_rate;
            let _ = app.emit("player:format", FormatPayload { sample_rate });
        }
        EngineEvent::OutputDeviceLost { device } => {
            log::warn!("output device {device} disconnected; reverting to the system default");
            // The engine has already reopened on the default and forgotten the
            // device; persist that so a restart does not resurrect it, and tell
            // the frontend so the picker follows.
            let state = app.state::<crate::state::AppState>();
            if let Err(e) = state.settings.update(|s| s.prefs.audio_device = None) {
                log::warn!("could not clear the saved output device: {e}");
            }
            let _ = app.emit("player:device-lost", DeviceLostPayload { device });
        }
        EngineEvent::SignalPath(info) => {
            let _ = app.emit("player:signal-path", info);
        }
    }
}
