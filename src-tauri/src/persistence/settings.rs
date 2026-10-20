//! Rust-owned settings store: `settings.json` in the app config dir, written
//! atomically (tmp + rename). Holds everything non-secret, including the EQ
//! state and presets (the engine needs them before the webview is up).

use std::fs;
use std::path::PathBuf;
use std::sync::RwLock;

use reverb_core::audio::{EqState, ReplayGainSettings};
use serde::{Deserialize, Serialize};

use crate::error::ReverbError;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EqPreset {
    pub id: String,
    pub name: String,
    pub state: EqState,
}

/// User preferences shared verbatim with the frontend (no secrets).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Preferences {
    pub theme: String,
    /// UI language: a locale the frontend ships ("en", "fr", "es", "de"), or
    /// "system" to follow the OS. Unknown values resolve like "system".
    pub language: String,
    /// Accent preset name ("coral", "blue", ...) — resolved to colors in CSS.
    pub accent: String,
    /// Shell layout: "classic" | "liquid".
    pub layout: String,
    /// Liquid layout glass strength, 0–100 (0 = solid panels, effect off).
    pub glass_intensity: u32,
    /// How see-through the panels are, 0–100 (0 = as designed), so the
    /// backdrop and theme art show through. Applies in both layouts.
    pub panel_transparency: u32,
    /// Full-screen player backdrop: "blurred" | "soft" | "clear" | "solid".
    pub now_playing_backdrop: String,
    /// Full-screen player cover size: "small" | "medium" | "large".
    pub now_playing_art_size: String,
    /// Full-screen player text alignment: "center" | "left".
    pub now_playing_align: String,
    /// Metadata fields shown under the title in the full-screen player
    /// (canonical ids, see frontend). An allowlist, so fields added in a later
    /// release stay hidden until asked for — the row is meant to stay short.
    pub now_playing_fields: Vec<String>,
    /// Show the track's genres as chips under the album line.
    pub now_playing_genres: bool,
    /// Route to open at startup ("/albums", "/songs", ...).
    pub startup_view: String,
    /// Library grid density: "small" | "medium" | "large".
    pub grid_size: String,
    /// Playlists view: "grid" of cover cards, or "list" of rows.
    pub playlist_layout: String,
    /// Preferred output device name; `None` = system default.
    pub audio_device: Option<String>,
    /// Switch the device to each track's native sample rate (bit-perfect).
    pub match_source_rate: bool,
    /// Take the output device exclusively (macOS hog mode / WASAPI exclusive).
    pub exclusive_output: bool,
    /// "raw" for the original file, or "mp3-320" / "mp3-192" / "mp3-128"
    /// for server-side transcoding.
    pub streaming_format: String,
    pub replay_gain: ReplayGainSettings,
    /// Crossfade between tracks, in seconds (0 = gapless).
    pub crossfade_secs: u32,
    /// How the playback position is drawn: "line" | "wave" | "bars".
    pub seek_bar_style: String,
    /// Animate the wave/bars while playing (the flat line ignores this).
    pub seek_bar_animated: bool,
    /// Seek bar weight: "thin" | "medium" | "thick".
    pub seek_bar_thickness: String,
    pub scrobble_enabled: bool,
    /// Percentage of a song to play before it counts as played (10–100).
    pub scrobble_at_percent: u32,
    /// Send plays to Last.fm directly and leave the Subsonic server out of it
    /// — no scrobble, no now-playing entry. Needs Last.fm scrobbling to be
    /// connected; without a session key it is ignored (see `events.rs`), so
    /// turning it on can never silently stop plays being recorded anywhere.
    pub lastfm_scrobble_only: bool,
    /// Clear `lastfm_scrobble_only` on the next launch, making it a
    /// this-session-only switch rather than a persistent mode.
    pub lastfm_scrobble_only_reset_on_start: bool,
    /// Show a `lastfm_scrobble_only` toggle in the sidebar.
    pub lastfm_scrobble_only_pinned: bool,
    /// Submit only the first credited artist to Last.fm, rather than the whole
    /// credit string ("A feat. B"). Last.fm matches a play by artist name, and
    /// a joined credit usually matches nothing — so this trades the featured
    /// artists for the play landing on the main one. Applies to the plays
    /// Reverb sends to Last.fm itself; what the Subsonic server forwards is the
    /// server's own business (Navidrome has the same option).
    pub lastfm_scrobble_first_artist_only: bool,
    /// Visible columns in the songs table (canonical ids, see frontend).
    pub song_columns: Vec<String>,
    /// Playlist ids pinned by the user, in pin order.
    pub pinned_playlists: Vec<String>,
    /// Whether pinned playlists are listed in the sidebar.
    pub show_pinned_in_sidebar: bool,
    /// Sidebar navigation the user has hidden, by route path. A denylist, so
    /// navigation added in a later release is visible by default.
    pub hidden_nav_items: Vec<String>,
    /// Radio: keep playing similar songs when the queue ends.
    pub radio_mode: bool,
    /// Persist the queue and restore it (paused) on the next launch.
    pub save_queue: bool,
    /// Keep the queue on the server too, and pick up a newer one another
    /// client saved there.
    pub sync_queue: bool,
}

impl Default for Preferences {
    fn default() -> Self {
        Self {
            theme: "dark".into(),
            language: "system".into(),
            accent: "coral".into(),
            layout: "classic".into(),
            glass_intensity: 100,
            panel_transparency: 0,
            now_playing_backdrop: "blurred".into(),
            now_playing_art_size: "medium".into(),
            now_playing_align: "center".into(),
            now_playing_fields: vec!["format".into(), "bitRate".into(), "sampleRate".into()],
            now_playing_genres: true,
            startup_view: "/home".into(),
            grid_size: "medium".into(),
            playlist_layout: "grid".into(),
            audio_device: None,
            match_source_rate: false,
            exclusive_output: false,
            streaming_format: "raw".into(),
            replay_gain: ReplayGainSettings::default(),
            crossfade_secs: 0,
            seek_bar_style: "line".into(),
            seek_bar_animated: true,
            seek_bar_thickness: "medium".into(),
            scrobble_enabled: true,
            scrobble_at_percent: 80,
            lastfm_scrobble_only: false,
            lastfm_scrobble_only_reset_on_start: false,
            lastfm_scrobble_only_pinned: false,
            lastfm_scrobble_first_artist_only: false,
            song_columns: vec!["artist".into(), "album".into(), "duration".into()],
            pinned_playlists: Vec::new(),
            show_pinned_in_sidebar: true,
            hidden_nav_items: Vec::new(),
            radio_mode: false,
            save_queue: true,
            sync_queue: true,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Settings {
    pub server_url: Option<String>,
    pub username: Option<String>,
    pub volume: f32,
    #[serde(flatten)]
    pub prefs: Preferences,
    pub eq: EqState,
    pub eq_presets: Vec<EqPreset>,
    /// Serialized queue snapshot owned by the frontend (restored on boot).
    pub queue_snapshot: Option<serde_json::Value>,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            server_url: None,
            username: None,
            volume: 1.0,
            prefs: Preferences::default(),
            eq: EqState::default(),
            eq_presets: Vec::new(),
            queue_snapshot: None,
        }
    }
}

pub struct SettingsStore {
    path: PathBuf,
    data: RwLock<Settings>,
}

impl SettingsStore {
    pub fn load(config_dir: PathBuf) -> Result<Self, ReverbError> {
        fs::create_dir_all(&config_dir)?;
        let path = config_dir.join("settings.json");
        let data = match fs::read_to_string(&path) {
            Ok(raw) => serde_json::from_str(&raw).unwrap_or_else(|e| {
                log::warn!("settings.json unreadable ({e}); starting fresh");
                Settings::default()
            }),
            Err(_) => Settings::default(),
        };
        Ok(Self { path, data: RwLock::new(data) })
    }

    pub fn get(&self) -> Settings {
        self.data.read().unwrap().clone()
    }

    pub fn update(&self, mutate: impl FnOnce(&mut Settings)) -> Result<Settings, ReverbError> {
        let snapshot = {
            let mut guard = self.data.write().unwrap();
            mutate(&mut guard);
            guard.clone()
        };
        let raw = serde_json::to_string_pretty(&snapshot)
            .map_err(|e| ReverbError::Io(format!("serialize settings: {e}")))?;
        let tmp = self.path.with_extension("json.tmp");
        fs::write(&tmp, raw)?;
        fs::rename(&tmp, &self.path)?;
        Ok(snapshot)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The frontend sends the whole preference object as camelCase JSON and
    /// serde silently drops anything this struct does not declare, so a field
    /// that exists only on the TS side fails to persist without any error.
    /// Pin the names of the fields most recently added.
    #[test]
    fn lastfm_only_prefs_survive_a_camel_case_round_trip() {
        let raw = r#"{
            "lastfmScrobbleOnly": true,
            "lastfmScrobbleOnlyResetOnStart": true,
            "lastfmScrobbleOnlyPinned": true,
            "lastfmScrobbleFirstArtistOnly": true,
            "seekBarStyle": "wave",
            "seekBarAnimated": false,
            "seekBarThickness": "thick",
            "syncQueue": false
        }"#;
        let prefs: Preferences = serde_json::from_str(raw).expect("camelCase names must match");
        assert!(prefs.lastfm_scrobble_only);
        assert!(prefs.lastfm_scrobble_only_reset_on_start);
        assert!(prefs.lastfm_scrobble_only_pinned);
        assert!(prefs.lastfm_scrobble_first_artist_only);
        assert_eq!(prefs.seek_bar_style, "wave");
        assert!(!prefs.seek_bar_animated);
        assert_eq!(prefs.seek_bar_thickness, "thick");
        assert!(!prefs.sync_queue);

        // ...and come back out under the same names.
        let back = serde_json::to_value(&prefs).unwrap();
        assert_eq!(back["lastfmScrobbleOnly"], serde_json::json!(true));
        assert_eq!(back["lastfmScrobbleOnlyResetOnStart"], serde_json::json!(true));
        assert_eq!(back["lastfmScrobbleOnlyPinned"], serde_json::json!(true));
        assert_eq!(back["lastfmScrobbleFirstArtistOnly"], serde_json::json!(true));
        assert_eq!(back["seekBarStyle"], serde_json::json!("wave"));
        assert_eq!(back["seekBarAnimated"], serde_json::json!(false));
        assert_eq!(back["seekBarThickness"], serde_json::json!("thick"));
        assert_eq!(back["syncQueue"], serde_json::json!(false));
    }

    #[test]
    fn old_settings_files_parse_with_pref_defaults() {
        // A settings.json written before the preferences existed.
        let old = r#"{
            "serverUrl": "https://music.example.com/",
            "username": "zortexsenpai",
            "volume": 0.8,
            "theme": "dark",
            "eq": {"enabled": false, "bands": [], "preamp": {"mode": "auto"}},
            "eqPresets": [],
            "queueSnapshot": null
        }"#;
        let s: Settings = serde_json::from_str(old).expect("old format must parse");
        assert_eq!(s.username.as_deref(), Some("zortexsenpai"));
        assert_eq!(s.prefs.theme, "dark");
        assert_eq!(s.prefs.accent, "coral");
        assert_eq!(s.prefs.streaming_format, "raw");
        assert!(s.prefs.scrobble_enabled);
        assert!(!s.prefs.lastfm_scrobble_only);
        assert!(!s.prefs.lastfm_scrobble_only_reset_on_start);
        assert!(!s.prefs.lastfm_scrobble_only_pinned);
        assert!(s.prefs.audio_device.is_none());
        // And the new shape round-trips.
        let back = serde_json::to_string(&s).unwrap();
        let s2: Settings = serde_json::from_str(&back).unwrap();
        assert_eq!(s2.prefs.startup_view, "/home");
    }
}
