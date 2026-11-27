//! Rust-owned settings store: `settings.json` in the app config dir, written
//! atomically (tmp + rename). Holds everything non-secret, including the EQ
//! state and presets (the engine needs them before the webview is up).
//!
//! Compatibility rule: Reverb updates itself, so every release has to read the
//! file the one before it wrote. New versions only *add* fields, each with a
//! default (the `#[serde(default)]` on the structs fills them in for old
//! files). Changing an existing field's type or meaning needs a deliberate
//! migration. There is no migration system yet, so that means writing one.
//! A file that still fails to parse is moved aside as
//! `settings.json.bak-<timestamp>` rather than overwritten (see
//! [`SettingsStore::load`]).

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::RwLock;

use reverb_core::audio::{list_output_devices, EqState, OutputSettings, ReplayGainSettings};
use reverb_core::hqplayer::Endpoint as HqPlayerEndpoint;
use reverb_core::playback::PlaybackTarget;
use serde::{Deserialize, Serialize};

use crate::error::ReverbError;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EqPreset {
    pub id: String,
    pub name: String,
    pub state: EqState,
}

/// A named output setup the listener switches to in one go: a device and how
/// Reverb plays through it, or an HQPlayer to hand playback to. See
/// `GLOSSARY.md`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct DeviceConfig {
    pub id: String,
    pub name: String,
    /// Output device id (see `OutputDevice::id`); `None` = system default.
    /// Kept when the device is unplugged, so the config works again the
    /// moment it is back.
    pub device: Option<String>,
    /// Switch the device to each track's native sample rate (bit-perfect).
    pub match_source_rate: bool,
    /// Take the output device exclusively (macOS hog mode / WASAPI exclusive).
    pub exclusive_output: bool,
    /// Play through this HQPlayer instead of a device here. When set, the
    /// three fields above mean nothing: HQPlayer has its own output setup.
    pub hqplayer: Option<HqPlayerEndpoint>,
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
    /// The listener's device configs, in the order they were made.
    pub device_configs: Vec<DeviceConfig>,
    /// The device config playing now, by id. `None` — or an id no config has —
    /// plays on the system default, shared, at the device's own rate.
    pub active_device_config: Option<String>,
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
    /// Send plays straight to the scrobbling account (Last.fm or
    /// ListenBrainz) and leave the Subsonic server out of it — no scrobble, no
    /// now-playing entry. Without a scrobbling account it is ignored (see
    /// `stats::scrobble::direct_target`), so turning it on can never silently
    /// stop plays being recorded anywhere. Was Last.fm-only before ListenBrainz
    /// existed, hence the alias.
    #[serde(alias = "lastfmScrobbleOnly")]
    pub direct_scrobble: bool,
    /// Clear `direct_scrobble` on the next launch, making it a
    /// this-session-only switch rather than a persistent mode.
    #[serde(alias = "lastfmScrobbleOnlyResetOnStart")]
    pub direct_scrobble_reset_on_start: bool,
    /// Show a `direct_scrobble` toggle in the sidebar.
    #[serde(alias = "lastfmScrobbleOnlyPinned")]
    pub direct_scrobble_pinned: bool,
    /// Submit only the first credited artist rather than the whole credit
    /// string ("A feat. B"). The services match a play by artist name, and a
    /// joined credit usually matches nothing — so this trades the featured
    /// artists for the play landing on the main one. Applies to the plays
    /// Reverb sends to a service itself; what the Subsonic server forwards is
    /// the server's own business (Navidrome has the same option).
    #[serde(alias = "lastfmScrobbleFirstArtistOnly")]
    pub scrobble_first_artist_only: bool,
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
    /// Look for a new release at launch and every 6 hours. Off means Reverb
    /// never contacts GitHub unless asked to ("Check now").
    pub check_for_updates: bool,
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
            device_configs: Vec::new(),
            active_device_config: None,
            streaming_format: "raw".into(),
            replay_gain: ReplayGainSettings::default(),
            crossfade_secs: 0,
            seek_bar_style: "line".into(),
            seek_bar_animated: true,
            seek_bar_thickness: "medium".into(),
            scrobble_enabled: true,
            scrobble_at_percent: 80,
            direct_scrobble: false,
            direct_scrobble_reset_on_start: false,
            direct_scrobble_pinned: false,
            scrobble_first_artist_only: false,
            song_columns: vec!["artist".into(), "album".into(), "duration".into()],
            pinned_playlists: Vec::new(),
            show_pinned_in_sidebar: true,
            hidden_nav_items: Vec::new(),
            radio_mode: false,
            save_queue: true,
            sync_queue: true,
            check_for_updates: true,
        }
    }
}

impl Preferences {
    /// The device config playing now, if there is one.
    pub fn active_device_config(&self) -> Option<&DeviceConfig> {
        let id = self.active_device_config.as_deref()?;
        self.device_configs.iter().find(|config| config.id == id)
    }

    /// What the local engine should be playing through. An HQPlayer config
    /// has no device here, so with one active this is the system default —
    /// which the engine, in standby, does not open.
    pub fn output(&self) -> OutputSettings {
        self.active_device_config().filter(|config| config.hqplayer.is_none()).map_or_else(
            OutputSettings::default,
            |config| OutputSettings {
                device: config.device.clone(),
                auto_rate: config.match_source_rate,
                exclusive: config.exclusive_output,
            },
        )
    }

    /// Where playback goes: the local engine, or the active config's HQPlayer.
    pub fn target(&self) -> PlaybackTarget {
        match self.active_device_config().and_then(|config| config.hqplayer.clone()) {
            Some(endpoint) => PlaybackTarget::HqPlayer(endpoint),
            None => PlaybackTarget::Local(self.output()),
        }
    }
}

/// How output was set up before device configs: one device and two switches,
/// straight on the preferences. Read from old files only, to carry over into a
/// config (see [`Settings::adopt_legacy_output`]); never written.
#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
struct LegacyOutput {
    audio_device: Option<String>,
    match_source_rate: bool,
    exclusive_output: bool,
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

impl Settings {
    /// Carry output settings from before device configs into a config, made
    /// active so playback starts exactly where it left off. All-default old
    /// settings carry nothing: that is the system default, which needs no
    /// config. `device_label` names the device, when it can.
    fn adopt_legacy_output(
        &mut self,
        legacy: LegacyOutput,
        device_label: impl FnOnce(&str) -> Option<String>,
    ) {
        let LegacyOutput { audio_device, match_source_rate, exclusive_output } = legacy;
        let nothing_set = audio_device.is_none() && !match_source_rate && !exclusive_output;
        if nothing_set || !self.prefs.device_configs.is_empty() {
            return;
        }
        let name = match audio_device.as_deref() {
            None => "System default".to_owned(),
            Some(device) => device_label(device)
                // Settings older than device ids hold the display name itself;
                // cpal ids read "<host>:<device>".
                .or_else(|| (!device.contains(':')).then(|| device.to_owned()))
                .unwrap_or_else(|| "Output device".to_owned()),
        };
        let id = uuid::Uuid::new_v4().to_string();
        self.prefs.device_configs.push(DeviceConfig {
            id: id.clone(),
            name,
            device: audio_device,
            match_source_rate,
            exclusive_output,
            hqplayer: None,
        });
        self.prefs.active_device_config = Some(id);
    }
}

/// Parse a settings file, carrying over anything an older version kept in a
/// shape this one no longer uses.
fn parse(
    raw: &[u8],
    device_label: impl FnOnce(&str) -> Option<String>,
) -> Result<Settings, serde_json::Error> {
    let mut settings: Settings = serde_json::from_slice(raw)?;
    // Anything that parses as `Settings` parses as this too: every field has a
    // default and unknown ones are ignored.
    let legacy: LegacyOutput = serde_json::from_slice(raw).unwrap_or_default();
    settings.adopt_legacy_output(legacy, device_label);
    Ok(settings)
}

/// A device's display name, looked up among the devices plugged in now.
fn current_device_label(id: &str) -> Option<String> {
    list_output_devices().into_iter().find(|d| d.id == id || d.label == id).map(|d| d.label)
}

pub struct SettingsStore {
    path: PathBuf,
    data: RwLock<Settings>,
}

/// Move a settings file this version can't parse out of the way, so the next
/// save starts a fresh one beside it instead of overwriting the listener's
/// preferences, EQ and presets. The rename keeps the original bytes.
fn back_up_unreadable(path: &Path, error: &serde_json::Error) {
    let stamp = chrono::Utc::now().format("%Y%m%dT%H%M%SZ");
    let backup = path.with_file_name(format!("settings.json.bak-{stamp}"));
    match fs::rename(path, &backup) {
        Ok(()) => log::warn!(
            "settings.json unreadable ({error}); moved it to {} and starting fresh",
            backup.display()
        ),
        Err(e) => log::warn!(
            "settings.json unreadable ({error}) and could not be backed up ({e}); starting fresh"
        ),
    }
}

impl SettingsStore {
    /// Read `settings.json` from `config_dir`. A missing file gives defaults;
    /// one that can't be parsed is backed up first, then defaults.
    pub fn load(config_dir: PathBuf) -> Result<Self, ReverbError> {
        fs::create_dir_all(&config_dir)?;
        let path = config_dir.join("settings.json");
        let data = match fs::read(&path) {
            Ok(raw) => parse(&raw, current_device_label).unwrap_or_else(|e| {
                back_up_unreadable(&path, &e);
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
    fn scrobbling_prefs_survive_a_camel_case_round_trip() {
        let raw = r#"{
            "directScrobble": true,
            "directScrobbleResetOnStart": true,
            "directScrobblePinned": true,
            "scrobbleFirstArtistOnly": true,
            "seekBarStyle": "wave",
            "seekBarAnimated": false,
            "seekBarThickness": "thick",
            "syncQueue": false,
            "checkForUpdates": false
        }"#;
        let prefs: Preferences = serde_json::from_str(raw).expect("camelCase names must match");
        assert!(prefs.direct_scrobble);
        assert!(prefs.direct_scrobble_reset_on_start);
        assert!(prefs.direct_scrobble_pinned);
        assert!(prefs.scrobble_first_artist_only);
        assert_eq!(prefs.seek_bar_style, "wave");
        assert!(!prefs.seek_bar_animated);
        assert_eq!(prefs.seek_bar_thickness, "thick");
        assert!(!prefs.sync_queue);
        assert!(!prefs.check_for_updates);

        // ...and come back out under the same names.
        let back = serde_json::to_value(&prefs).unwrap();
        assert_eq!(back["directScrobble"], serde_json::json!(true));
        assert_eq!(back["directScrobbleResetOnStart"], serde_json::json!(true));
        assert_eq!(back["directScrobblePinned"], serde_json::json!(true));
        assert_eq!(back["scrobbleFirstArtistOnly"], serde_json::json!(true));
        assert_eq!(back["seekBarStyle"], serde_json::json!("wave"));
        assert_eq!(back["seekBarAnimated"], serde_json::json!(false));
        assert_eq!(back["seekBarThickness"], serde_json::json!("thick"));
        assert_eq!(back["syncQueue"], serde_json::json!(false));
        assert_eq!(back["checkForUpdates"], serde_json::json!(false));
    }

    /// Before ListenBrainz, direct scrobbling was "Last.fm only". A settings
    /// file from then has to come through with the same switches set (the
    /// Last.fm connection itself carries over in `stats::store`) — read
    /// through the whole `Settings`, since prefs are flattened into it and
    /// that is the path a real file takes.
    #[test]
    fn last_fm_only_settings_become_direct_scrobbling() {
        let raw = r#"{
            "serverUrl": "https://music.example.com/",
            "lastfmScrobbleOnly": true,
            "lastfmScrobbleOnlyResetOnStart": true,
            "lastfmScrobbleOnlyPinned": true,
            "lastfmScrobbleFirstArtistOnly": true
        }"#;
        let settings: Settings = serde_json::from_str(raw).expect("old names must still parse");
        let prefs = settings.prefs;
        assert!(prefs.direct_scrobble);
        assert!(prefs.direct_scrobble_reset_on_start);
        assert!(prefs.direct_scrobble_pinned);
        assert!(prefs.scrobble_first_artist_only);

        // Written back under the new names only.
        let back = serde_json::to_value(Settings { prefs, ..Settings::default() }).unwrap();
        assert_eq!(back["directScrobble"], serde_json::json!(true));
        assert!(back.get("lastfmScrobbleOnly").is_none());
    }

    /// Every `settings.json.bak-*` file in `dir`.
    fn backups(dir: &Path) -> Vec<PathBuf> {
        let mut found: Vec<_> = fs::read_dir(dir)
            .unwrap()
            .map(|e| e.unwrap().path())
            .filter(|p| {
                p.file_name()
                    .and_then(|n| n.to_str())
                    .is_some_and(|n| n.starts_with("settings.json.bak-"))
            })
            .collect();
        found.sort();
        found
    }

    #[test]
    fn an_unreadable_settings_file_is_backed_up_before_starting_fresh() {
        let dir = tempfile::tempdir().unwrap();
        // A later release changed `volume` into an object this one can't read.
        let original = br#"{"serverUrl": "https://music.example.com/", "volume": {"db": -3}}"#;
        fs::write(dir.path().join("settings.json"), original).unwrap();

        let store = SettingsStore::load(dir.path().to_path_buf()).unwrap();

        assert_eq!(store.get().server_url, None, "defaults, not a half-read file");
        let backups = backups(dir.path());
        assert_eq!(backups.len(), 1, "exactly one backup: {backups:?}");
        assert_eq!(fs::read(&backups[0]).unwrap(), original, "byte-for-byte");
        assert!(
            !dir.path().join("settings.json").exists(),
            "the original path is free for the next save"
        );
    }

    #[test]
    fn a_readable_settings_file_loads_and_is_left_alone() {
        let dir = tempfile::tempdir().unwrap();
        let original = br#"{"serverUrl": "https://music.example.com/", "volume": 0.5}"#;
        fs::write(dir.path().join("settings.json"), original).unwrap();

        let store = SettingsStore::load(dir.path().to_path_buf()).unwrap();

        assert_eq!(store.get().server_url.as_deref(), Some("https://music.example.com/"));
        assert_eq!(store.get().volume, 0.5);
        assert!(backups(dir.path()).is_empty());
        assert_eq!(fs::read(dir.path().join("settings.json")).unwrap(), original);
    }

    #[test]
    fn a_missing_settings_file_gives_defaults_and_no_backup() {
        let dir = tempfile::tempdir().unwrap();

        let store = SettingsStore::load(dir.path().to_path_buf()).unwrap();

        assert_eq!(store.get().server_url, None);
        assert!(store.get().prefs.save_queue);
        assert!(backups(dir.path()).is_empty());
    }

    fn no_label(_: &str) -> Option<String> {
        None
    }

    #[test]
    fn old_output_settings_become_the_active_device_config() {
        let raw = br#"{
            "audioDevice": "coreaudio:BTR17-uid",
            "matchSourceRate": true,
            "exclusiveOutput": true
        }"#;
        let s = parse(raw, |id| (id == "coreaudio:BTR17-uid").then(|| "FiiO BTR17".into()))
            .unwrap();

        let [config] = s.prefs.device_configs.as_slice() else {
            panic!("one config: {:?}", s.prefs.device_configs)
        };
        assert_eq!(config.name, "FiiO BTR17");
        assert_eq!(s.prefs.active_device_config.as_deref(), Some(config.id.as_str()));
        assert_eq!(
            s.prefs.output(),
            OutputSettings {
                device: Some("coreaudio:BTR17-uid".into()),
                auto_rate: true,
                exclusive: true,
            },
            "playback starts where it left off"
        );

        // Written back as a config only.
        let back = serde_json::to_value(&s).unwrap();
        assert!(back.get("audioDevice").is_none());
        assert!(back.get("exclusiveOutput").is_none());
        assert_eq!(back["deviceConfigs"][0]["matchSourceRate"], serde_json::json!(true));
    }

    #[test]
    fn an_old_device_that_cannot_be_named_still_gets_a_config() {
        // Unplugged: the id cannot be looked up.
        let s = parse(br#"{"audioDevice": "alsa:hw:CARD=1,DEV=0"}"#, no_label).unwrap();
        assert_eq!(s.prefs.device_configs[0].name, "Output device");
        assert_eq!(s.prefs.output().device.as_deref(), Some("alsa:hw:CARD=1,DEV=0"));

        // Saved before device ids, when the preference was the display name.
        let s = parse(br#"{"audioDevice": "MacBook Pro Speakers"}"#, no_label).unwrap();
        assert_eq!(s.prefs.device_configs[0].name, "MacBook Pro Speakers");

        // The system default with a switch on.
        let s = parse(br#"{"matchSourceRate": true}"#, no_label).unwrap();
        assert_eq!(s.prefs.device_configs[0].name, "System default");
        assert_eq!(
            s.prefs.output(),
            OutputSettings { device: None, auto_rate: true, exclusive: false }
        );
    }

    #[test]
    fn default_old_output_settings_need_no_config() {
        let raw = br#"{"audioDevice": null, "matchSourceRate": false, "exclusiveOutput": false}"#;
        let s = parse(raw, |_| panic!("nothing to name")).unwrap();
        assert!(s.prefs.device_configs.is_empty());
        assert_eq!(s.prefs.active_device_config, None);
    }

    #[test]
    fn existing_device_configs_are_not_migrated_over() {
        // A file this version wrote never has the old fields; one that has both
        // keeps the configs it has.
        let raw = br#"{
            "audioDevice": "coreaudio:other",
            "deviceConfigs": [{"id": "a", "name": "Desk", "device": null}],
            "activeDeviceConfig": "a"
        }"#;
        let s = parse(raw, no_label).unwrap();
        assert_eq!(s.prefs.device_configs.len(), 1);
        assert_eq!(s.prefs.device_configs[0].name, "Desk");
    }

    #[test]
    fn output_follows_the_active_device_config() {
        let mut prefs = Preferences {
            device_configs: vec![
                DeviceConfig {
                    id: "desk".into(),
                    name: "Desk DAC".into(),
                    device: Some("coreaudio:dac".into()),
                    match_source_rate: true,
                    exclusive_output: false,
                    hqplayer: None,
                },
                DeviceConfig {
                    id: "speakers".into(),
                    name: "Speakers".into(),
                    device: None,
                    match_source_rate: false,
                    exclusive_output: true,
                    hqplayer: None,
                },
            ],
            ..Preferences::default()
        };
        assert_eq!(prefs.output(), OutputSettings::default(), "none active: system default");

        prefs.active_device_config = Some("speakers".into());
        assert_eq!(prefs.output(), OutputSettings { device: None, auto_rate: false, exclusive: true });

        prefs.active_device_config = Some("desk".into());
        assert_eq!(prefs.output().device.as_deref(), Some("coreaudio:dac"));
        assert!(prefs.output().auto_rate);

        // A config deleted out from under the pointer.
        prefs.active_device_config = Some("gone".into());
        assert_eq!(prefs.output(), OutputSettings::default());
    }

    #[test]
    fn an_hqplayer_config_plays_through_hqplayer() {
        let raw = r#"{
            "deviceConfigs": [
                {"id": "desk", "name": "Desk DAC", "device": "coreaudio:dac", "exclusiveOutput": true},
                {"id": "hq", "name": "HQPlayer", "hqplayer": {"host": "studio.local", "port": 4321}}
            ],
            "activeDeviceConfig": "hq"
        }"#;
        let mut prefs: Preferences = serde_json::from_str(raw).unwrap();
        assert_eq!(
            prefs.target(),
            PlaybackTarget::HqPlayer(HqPlayerEndpoint { host: "studio.local".into(), port: 4321 })
        );
        // The local engine is left on nothing in particular: it is in standby.
        assert_eq!(prefs.output(), OutputSettings::default());

        // A config from before HQPlayer has none, and plays here.
        prefs.active_device_config = Some("desk".into());
        assert_eq!(prefs.device_configs[0].hqplayer, None);
        assert_eq!(
            prefs.target(),
            PlaybackTarget::Local(OutputSettings {
                device: Some("coreaudio:dac".into()),
                auto_rate: false,
                exclusive: true,
            })
        );

        // And the field round-trips under its name.
        let back = serde_json::to_value(&prefs).unwrap();
        assert_eq!(back["deviceConfigs"][1]["hqplayer"]["host"], serde_json::json!("studio.local"));
        assert_eq!(back["deviceConfigs"][0]["hqplayer"], serde_json::Value::Null);
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
        assert!(!s.prefs.direct_scrobble);
        assert!(!s.prefs.direct_scrobble_reset_on_start);
        assert!(!s.prefs.direct_scrobble_pinned);
        assert!(s.prefs.device_configs.is_empty());
        assert_eq!(s.prefs.output(), OutputSettings::default());
        assert!(s.prefs.check_for_updates, "update checks default to on");
        // And the new shape round-trips.
        let back = serde_json::to_string(&s).unwrap();
        let s2: Settings = serde_json::from_str(&back).unwrap();
        assert_eq!(s2.prefs.startup_view, "/home");
    }
}
