use chrono::Utc;
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, State};

use crate::error::ReverbError;
use crate::state::AppState;
use crate::stats::compute::{compute, StatsQuery, StatsRange, StatsSummary};
use crate::stats::lastfm;
use crate::stats::store::LastfmConfig;

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct LastfmStatus {
    pub username: String,
    pub track_count: u64,
    pub last_sync_at: Option<String>,
    /// An authenticated session for submitting plays exists. Importing works
    /// without one, so this is reported separately from being connected.
    pub scrobbling: bool,
}

/// Keychain entry names for the Last.fm scrobbling credentials.
pub const LASTFM_SECRET_KEY: &str = "lastfm-shared-secret";
pub const LASTFM_SESSION_KEY: &str = "lastfm-session-key";

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct SourcesStatus {
    pub lastfm: Option<LastfmStatus>,
    pub importing: bool,
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct ImportProgress {
    source: &'static str,
    state: &'static str, // "running" | "done" | "error"
    imported: u64,
    total: Option<u64>,
    message: Option<String>,
}

#[tauri::command]
pub fn stats_get_sources(state: State<'_, AppState>) -> SourcesStatus {
    let config = state.stats.config();
    SourcesStatus {
        lastfm: config.lastfm.map(|c| LastfmStatus {
            username: c.username,
            track_count: c.track_count,
            last_sync_at: c.last_sync_at,
            scrobbling: c.scrobbling,
        }),
        importing: state.stats.importing.load(std::sync::atomic::Ordering::Relaxed),
    }
}

/// Connect Last.fm. `shared_secret` and `password` are optional and only
/// needed to enable *submitting* plays: with both, the password is traded once
/// for a session key (`auth.getMobileSession`) and then discarded — only the
/// session key and shared secret are kept, in the keychain. Without them the
/// source is import-only, exactly as before.
#[tauri::command]
pub async fn stats_configure_lastfm(
    username: String,
    api_key: String,
    shared_secret: Option<String>,
    password: Option<String>,
    state: State<'_, AppState>,
) -> Result<(), ReverbError> {
    // Validate credentials with a one-item probe before saving.
    let client = reqwest::Client::new();
    lastfm::fetch_recent(&client, &username, &api_key, 0, None, 1)
        .await
        .map_err(ReverbError::State)?;

    // Authenticate for scrobbling when a secret and password were supplied.
    // Done before anything is persisted, so a rejected password leaves the
    // existing configuration untouched.
    let secret = shared_secret.map(|s| s.trim().to_owned()).filter(|s| !s.is_empty());
    let password = password.filter(|p| !p.is_empty());
    let session = match (&secret, &password) {
        (Some(secret), Some(password)) => {
            lastfm::check_credential_shape("shared secret", secret)
                .map_err(ReverbError::State)?;
            Some(
                lastfm::get_mobile_session(&client, &api_key, secret, &username, password)
                    .await
                    .map_err(ReverbError::State)?,
            )
        }
        _ => None,
    };

    let previous = state.stats.config().lastfm;
    let changed_user = previous.as_ref().is_some_and(|c| c.username != username);
    if changed_user {
        state.stats.clear_source("lastfm");
    }

    // A new session replaces any stored one; a re-connect without a password
    // keeps whatever was there, unless the account changed — those credentials
    // belong to the old account.
    let scrobbling = match (&session, &secret) {
        (Some(session), Some(secret)) => {
            state.keychain.store_secret(LASTFM_SECRET_KEY, secret)?;
            state.keychain.store_secret(LASTFM_SESSION_KEY, session)?;
            true
        }
        _ if changed_user => {
            state.keychain.clear_secret(LASTFM_SECRET_KEY);
            state.keychain.clear_secret(LASTFM_SESSION_KEY);
            false
        }
        _ => previous.as_ref().is_some_and(|p| p.scrobbling),
    };

    state.stats.update_config(|c| {
        let keep = previous.filter(|_| !changed_user);
        c.lastfm = Some(LastfmConfig {
            username,
            api_key,
            scrobbling,
            last_sync_ts: keep.as_ref().map_or(0, |k| k.last_sync_ts),
            last_sync_at: keep.as_ref().and_then(|k| k.last_sync_at.clone()),
            track_count: keep.as_ref().map_or(0, |k| k.track_count),
        });
    })?;
    Ok(())
}

/// Add scrobbling to an already-connected Last.fm source. `api_key` overrides
/// the stored one: a signature is only valid when the key and the shared secret
/// come from the *same* Last.fm application, so the pair has to be enterable
/// together — reusing a stored key the user cannot see invites a mismatch that
/// surfaces as an opaque "invalid method signature". The password is traded for
/// a session key and dropped.
#[tauri::command]
pub async fn stats_enable_lastfm_scrobbling(
    shared_secret: String,
    password: String,
    api_key: Option<String>,
    state: State<'_, AppState>,
) -> Result<(), ReverbError> {
    let config = state
        .stats
        .config()
        .lastfm
        .ok_or_else(|| ReverbError::State("Last.fm is not connected".into()))?;
    let secret = shared_secret.trim();
    let api_key = api_key
        .map(|k| k.trim().to_owned())
        .filter(|k| !k.is_empty())
        .unwrap_or(config.api_key);
    lastfm::check_credential_shape("API key", &api_key).map_err(ReverbError::State)?;
    lastfm::check_credential_shape("shared secret", secret).map_err(ReverbError::State)?;
    let client = reqwest::Client::new();
    let session =
        lastfm::get_mobile_session(&client, &api_key, secret, &config.username, &password)
            .await
            .map_err(ReverbError::State)?;
    state.keychain.store_secret(LASTFM_SECRET_KEY, secret)?;
    state.keychain.store_secret(LASTFM_SESSION_KEY, &session)?;
    state.stats.update_config(|c| {
        if let Some(lastfm) = c.lastfm.as_mut() {
            lastfm.scrobbling = true;
            // A replacement key has just proven itself against Last.fm; keep
            // it so imports and scrobbles stay on one application.
            lastfm.api_key = api_key;
        }
    })?;
    Ok(())
}

/// Drop the submission credentials, leaving the source connected for imports.
/// The counterpart to `stats_enable_lastfm_scrobbling` — without it the only
/// way to stop scrobbling would be a full disconnect, which also deletes the
/// imported history.
#[tauri::command]
pub fn stats_disable_lastfm_scrobbling(state: State<'_, AppState>) -> Result<(), ReverbError> {
    state.keychain.clear_secret(LASTFM_SECRET_KEY);
    state.keychain.clear_secret(LASTFM_SESSION_KEY);
    state.stats.update_config(|c| {
        if let Some(lastfm) = c.lastfm.as_mut() {
            lastfm.scrobbling = false;
        }
    })?;
    Ok(())
}

#[tauri::command]
pub fn stats_disconnect(source: String, state: State<'_, AppState>) -> Result<(), ReverbError> {
    if source == "lastfm" {
        state.stats.clear_source("lastfm");
        state.keychain.clear_secret(LASTFM_SECRET_KEY);
        state.keychain.clear_secret(LASTFM_SESSION_KEY);
        state.stats.update_config(|c| c.lastfm = None)?;
    }
    Ok(())
}

#[tauri::command]
pub fn stats_import(source: String, app: AppHandle) {
    if source == "lastfm" {
        tauri::async_runtime::spawn(run_lastfm_import(app));
    }
}

/// `from`/`to` are unix seconds and only used for the custom range; `to` is
/// exclusive.
#[tauri::command]
pub fn stats_get(
    range: StatsRange,
    from: Option<i64>,
    to: Option<i64>,
    state: State<'_, AppState>,
) -> Result<StatsSummary, ReverbError> {
    let scrobbles = state.stats.all_scrobbles()?;
    Ok(compute(&scrobbles, StatsQuery { range, from, to }))
}

fn emit_progress(app: &AppHandle, progress: ImportProgress) {
    let _ = app.emit("stats:import", progress);
}

/// Terminal error event. Callers drop the import guard first: listeners
/// refetch the importing flag immediately on receiving the event.
fn emit_import_error(app: &AppHandle, imported: u64, total: Option<u64>, message: String) {
    let message = if imported > 0 {
        format!("{message} — kept {imported} scrobbles; refresh to resume")
    } else {
        message
    };
    emit_progress(
        app,
        ImportProgress {
            source: "lastfm",
            state: "error",
            imported,
            total,
            message: Some(message),
        },
    );
}

/// Incremental Last.fm import: fetch the [cursor, now] window oldest-first
/// (highest page number first), persisting one page at a time and advancing
/// the cursor after each — an interrupted import (flaky API, app quit) keeps
/// everything fetched so far and the next run resumes instead of restarting.
pub async fn run_lastfm_import(app: AppHandle) {
    let state = app.state::<AppState>();
    let Some(guard) = state.stats.try_begin_import() else { return };
    let Some(config) = state.stats.config().lastfm else { return };

    let client = reqwest::Client::new();
    let from_ts = config.last_sync_ts;
    // Fixed upper bound: keeps page numbering stable during the walk; anything
    // scrobbled after this moment is picked up by the next sync.
    let to_ts = Utc::now().timestamp();
    let mut imported = 0u64;
    let mut newest_saved = from_ts;

    // Page 1 reveals the window's page count; its content is reused when the
    // backwards walk arrives back at it.
    let first = match fetch_recent_retrying(&client, &config, from_ts, to_ts, 1).await {
        Ok(page) => page,
        Err(message) => {
            drop(guard);
            return emit_import_error(&app, imported, None, message);
        }
    };
    let total = Some(first.total);
    let total_pages =
        if first.scrobbles.is_empty() { first.total_pages } else { first.total_pages.max(1) };
    let mut first = Some(first);

    for page in (1..=total_pages).rev() {
        let fetched = if page == 1 {
            first.take().expect("page 1 is visited exactly once")
        } else {
            match fetch_recent_retrying(&client, &config, from_ts, to_ts, page).await {
                Ok(fetched) => fetched,
                Err(message) => {
                    drop(guard);
                    return emit_import_error(&app, imported, total, message);
                }
            }
        };

        // Newest-first from the API; store chronologically. The ts filter
        // drops the cursor overlap (from is inclusive on some servers) and
        // anything already saved by this walk.
        let mut scrobbles = fetched.scrobbles;
        scrobbles.retain(|s| s.ts > newest_saved);
        scrobbles.sort_by_key(|s| s.ts);

        if let Some(newest) = scrobbles.last().map(|s| s.ts) {
            if let Err(e) = state.stats.append("lastfm", &scrobbles) {
                drop(guard);
                return emit_import_error(&app, imported, total, e.to_string());
            }
            newest_saved = newest;
            let count = scrobbles.len() as u64;
            imported += count;
            let _ = state.stats.update_config(|c| {
                if let Some(lastfm) = c.lastfm.as_mut() {
                    lastfm.last_sync_ts = newest_saved;
                    lastfm.track_count += count;
                }
            });
        }

        emit_progress(
            &app,
            ImportProgress { source: "lastfm", state: "running", imported, total, message: None },
        );
        if page > 1 {
            // Stay well within Last.fm's rate limits.
            tokio_sleep(250).await;
        }
    }

    let _ = state.stats.update_config(|c| {
        if let Some(lastfm) = c.lastfm.as_mut() {
            lastfm.last_sync_at = Some(Utc::now().to_rfc3339());
        }
    });
    // Release the lock BEFORE announcing completion, so the refetch the
    // listeners fire cannot observe a stale importing=true.
    drop(guard);
    emit_progress(
        &app,
        ImportProgress {
            source: "lastfm",
            state: "done",
            imported,
            total: Some(imported),
            message: None,
        },
    );
}

/// Fetch one page, retrying transient failures (Last.fm's "backend service
/// failed" errors are routine on deep pagination) with growing backoff.
async fn fetch_recent_retrying(
    client: &reqwest::Client,
    config: &LastfmConfig,
    from_ts: i64,
    to_ts: i64,
    page: u32,
) -> Result<lastfm::RecentPage, String> {
    const ATTEMPTS: u32 = 4;
    let mut delay_ms = 1_000;
    for attempt in 1..=ATTEMPTS {
        match lastfm::fetch_recent(client, &config.username, &config.api_key, from_ts, Some(to_ts), page)
            .await
        {
            Ok(fetched) => return Ok(fetched),
            Err(message) if attempt < ATTEMPTS => {
                log::warn!("last.fm page {page} failed (attempt {attempt}/{ATTEMPTS}): {message}");
                tokio_sleep(delay_ms).await;
                delay_ms *= 3;
            }
            Err(message) => {
                log::warn!("last.fm import failed on page {page}: {message}");
                return Err(message);
            }
        }
    }
    unreachable!("loop returns on success or final attempt")
}

async fn tokio_sleep(ms: u64) {
    tauri::async_runtime::spawn_blocking(move || {
        std::thread::sleep(std::time::Duration::from_millis(ms));
    })
    .await
    .ok();
}
