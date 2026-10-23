mod commands;
mod error;
mod events;
mod library_index;
mod media_keys;
mod mixes;
mod persistence;
mod play_history;
mod protocol;
mod state;
mod stats;

use std::sync::{Arc, Mutex};

use reverb_core::audio::{spawn_engine, EngineCommand};
use reverb_core::subsonic::{Credentials, SubsonicClient};
use tauri::Manager;

use persistence::keychain::Keychain;
use persistence::settings::SettingsStore;
use state::{AppState, PlayerSnapshot};

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(
            tauri_plugin_log::Builder::new()
                .level(log::LevelFilter::Info)
                .level_for("reverb_core", log::LevelFilter::Debug)
                .build(),
        )
        // Self-update from GitHub releases, and the relaunch into the new
        // version. The frontend's update controller decides when.
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_process::init())
        // External links (release notes, the release page) open in the browser.
        .plugin(tauri_plugin_opener::init())
        .register_asynchronous_uri_scheme_protocol("reverb-art", protocol::cover_art::handle)
        .setup(|app| {
            let config_dir = app.path().app_config_dir()?;
            let settings = SettingsStore::load(config_dir.clone())?;

            // "Last.fm scrobble only" can be asked to last for one session
            // only. Clear it here, before anything else reads preferences, so
            // the engine and the webview both start from the reset value.
            {
                let prefs = settings.get().prefs;
                if prefs.lastfm_scrobble_only && prefs.lastfm_scrobble_only_reset_on_start {
                    if let Err(e) = settings.update(|s| s.prefs.lastfm_scrobble_only = false) {
                        log::warn!("could not reset Last.fm-only scrobbling: {e}");
                    }
                }
            }

            let keychain = Keychain::new(config_dir);
            let subsonic = Arc::new(SubsonicClient::new());

            // Restore the session: credentials from the keychain, volume and
            // EQ into the engine, before the webview asks for anything.
            let persisted = settings.get();
            if let (Some(url), Some(user)) = (&persisted.server_url, &persisted.username) {
                match keychain.retrieve(url, user) {
                    Ok(password) => match Credentials::new(url, user, &password) {
                        Ok(creds) => subsonic.set_credentials(Some(creds)),
                        Err(e) => log::warn!("stored server URL invalid: {e}"),
                    },
                    Err(e) => log::warn!("could not restore credentials: {e}"),
                }
            }

            let (event_tx, event_rx) = crossbeam_channel::unbounded();
            let engine = spawn_engine(event_tx, persisted.prefs.audio_device.clone());
            engine.send(EngineCommand::SetVolume { volume: persisted.volume });
            engine.send(EngineCommand::SetEq(persisted.eq.clone()));
            engine.send(EngineCommand::SetReplayGain(persisted.prefs.replay_gain));
            engine.send(EngineCommand::SetScrobblePercent(persisted.prefs.scrobble_at_percent));
            engine.send(EngineCommand::SetCrossfade {
                ms: u64::from(persisted.prefs.crossfade_secs) * 1000,
            });
            engine.send(EngineCommand::SetOutputMode {
                auto_rate: persisted.prefs.match_source_rate,
                exclusive: persisted.prefs.exclusive_output,
            });

            let player = Arc::new(Mutex::new(PlayerSnapshot {
                volume: persisted.volume,
                ..PlayerSnapshot::default()
            }));

            let data_dir = app.path().app_data_dir()?;
            let stats = stats::store::StatsStore::load(data_dir.clone())?;
            let library_index = library_index::store::LibraryIndexStore::new(data_dir.clone());
            let mixes = mixes::store::MixStore::load(data_dir.clone());
            let play_history = play_history::store::PlayHistoryStore::load(data_dir);

            // Manage state before the forwarder spawns: the media-session
            // callback resolves AppState through the app handle.
            app.manage(AppState {
                subsonic: subsonic.clone(),
                engine,
                settings,
                keychain,
                player: player.clone(),
                stats,
                library_index,
                mixes,
                play_history,
            });

            events::spawn_forwarder(app.handle().clone(), event_rx, player, subsonic);

            // Shortly after startup, ready the Library index and pull fresh
            // scrobbles from connected stats sources side by side, then build
            // the day's mixes on top of both.
            {
                let app_handle = app.handle().clone();
                tauri::async_runtime::spawn(async move {
                    tauri::async_runtime::spawn_blocking(|| {
                        std::thread::sleep(std::time::Duration::from_secs(3));
                    })
                    .await
                    .ok();
                    tauri::async_runtime::spawn(library_index::warm_up(app_handle.clone()));
                    let configured = {
                        use tauri::Manager;
                        app_handle.state::<AppState>().stats.config().lastfm.is_some()
                    };
                    if configured {
                        commands::stats::run_lastfm_import(app_handle.clone()).await;
                    }
                    commands::mixes::warm_up(app_handle).await;
                });
            }

            // Trim the cover art cache in the background (LRU by mtime).
            if let Ok(cache_dir) = app.path().app_cache_dir() {
                tauri::async_runtime::spawn_blocking(move || {
                    prune_cover_cache(&cache_dir.join("covers"), 200 * 1024 * 1024);
                });
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::auth::auth_get_status,
            commands::auth::auth_test,
            commands::auth::auth_save,
            commands::auth::auth_sign_out,
            commands::library::library_get_albums,
            commands::library::library_get_album,
            commands::library::library_get_song,
            commands::library::library_get_artists,
            commands::library::library_get_artist,
            commands::library::library_get_artist_appearances,
            commands::library::library_get_artist_info,
            commands::library::library_get_top_songs,
            commands::library::library_get_playlists,
            commands::library::library_get_playlist,
            commands::library::library_get_starred,
            commands::library::library_set_star,
            commands::library::library_set_rating,
            commands::library::library_get_artist_songs,
            commands::library::library_get_music_folders,
            commands::library::library_get_indexes,
            commands::library::library_get_music_directory,
            commands::library::library_get_songs,
            commands::library::library_get_genres,
            commands::library::library_search,
            commands::library::library_create_playlist,
            commands::library::library_update_playlist,
            commands::library::library_delete_playlist,
            commands::library::library_get_lyrics,
            commands::library::library_get_similar_songs,
            commands::library::library_get_radio,
            commands::library::library_get_play_queue,
            commands::library::library_save_play_queue,
            commands::library::library_start_scan,
            commands::library::library_get_scan_status,
            commands::mixes::mixes_get,
            commands::play_history::history_get,
            commands::play_history::history_record,
            commands::play_history::history_clear,
            commands::player::player_get_state,
            commands::player::player_play_track,
            commands::player::player_set_next,
            commands::player::player_pause,
            commands::player::player_resume,
            commands::player::player_toggle,
            commands::player::player_stop,
            commands::player::player_seek,
            commands::player::player_set_volume,
            commands::eq::eq_get_state,
            commands::eq::eq_set_all,
            commands::eq::eq_set_band,
            commands::eq::eq_set_enabled,
            commands::eq::eq_set_preamp,
            commands::eq::eq_list_presets,
            commands::eq::eq_save_preset,
            commands::eq::eq_load_preset,
            commands::eq::eq_delete_preset,
            commands::eq::eq_rename_preset,
            commands::stats::stats_get_sources,
            commands::stats::stats_configure_lastfm,
            commands::stats::stats_enable_lastfm_scrobbling,
            commands::stats::stats_disable_lastfm_scrobbling,
            commands::stats::stats_disconnect,
            commands::stats::stats_import,
            commands::stats::stats_get,
            commands::settings::settings_get,
            commands::settings::settings_set_prefs,
            commands::settings::audio_list_devices,
            commands::settings::queue_save_snapshot,
            commands::settings::queue_load_snapshot,
        ])
        .run(tauri::generate_context!())
        .expect("error while running Reverb");
}

/// Delete oldest cover files (by mtime) until the cache is under the limit.
fn prune_cover_cache(dir: &std::path::Path, max_bytes: u64) {
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    let mut files: Vec<(std::path::PathBuf, u64, std::time::SystemTime)> = entries
        .filter_map(|e| {
            let e = e.ok()?;
            let meta = e.metadata().ok()?;
            if !meta.is_file() {
                return None;
            }
            Some((e.path(), meta.len(), meta.modified().ok()?))
        })
        .collect();
    let total: u64 = files.iter().map(|(_, len, _)| len).sum();
    if total <= max_bytes {
        return;
    }
    files.sort_by_key(|(_, _, mtime)| *mtime);
    let mut remaining = total;
    // Trim to 75% of the limit so we don't prune on every launch.
    let target = max_bytes * 3 / 4;
    for (path, len, _) in files {
        if remaining <= target {
            break;
        }
        if std::fs::remove_file(&path).is_ok() {
            remaining = remaining.saturating_sub(len);
        }
    }
    log::info!("cover cache pruned to {} MiB", remaining / (1024 * 1024));
}
