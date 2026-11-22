//! Connecting the accounts Reverb scrobbles to and imports stats from — one
//! per role, Last.fm or ListenBrainz each (see `stats::store`).

use chrono::Utc;
use serde::Serialize;
use tauri::{AppHandle, State};

use crate::error::ReverbError;
use crate::state::AppState;
use crate::stats::store::{Account, SourcesConfig};
use crate::stats::{
    lastfm, listenbrainz, listenbrainz_token_key, Role, Source, LASTFM_SECRET_KEY,
    LASTFM_SESSION_KEY,
};

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct AccountStatus {
    service: Source,
    username: String,
    /// The server's API root when it is not listenbrainz.org.
    api_url: Option<String>,
    /// The Stats page can import from it (always so for Last.fm).
    history: bool,
}

impl From<&Account> for AccountStatus {
    fn from(account: &Account) -> Self {
        Self {
            service: account.source(),
            username: account.username().to_owned(),
            api_url: account.api_url().map(str::to_owned),
            history: account.has_history(),
        }
    }
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ServicesStatus {
    scrobble: Option<AccountStatus>,
    stats_same_as_scrobble: bool,
    stats_account: Option<AccountStatus>,
    /// The service the Stats page reads, after following the scrobbling
    /// account; `None` while no account can supply history.
    stats_source: Option<Source>,
    track_count: u64,
    last_sync_at: Option<String>,
    importing: bool,
}

#[tauri::command]
pub fn services_get(state: State<'_, AppState>) -> ServicesStatus {
    let config = state.stats.config();
    let history = config.active_history();
    ServicesStatus {
        scrobble: config.scrobble.as_ref().map(Into::into),
        stats_same_as_scrobble: config.stats_same_as_scrobble,
        stats_account: config.stats_account.as_ref().map(Into::into),
        stats_source: config.stats_source().map(|(_, a)| a.source()),
        track_count: history.map_or(0, |h| h.track_count),
        last_sync_at: history.and_then(|h| h.last_sync_at.clone()),
        importing: state.stats.importing.load(std::sync::atomic::Ordering::Relaxed),
    }
}

/// Forget the secrets `account` keeps under `role`.
fn clear_secrets(state: &AppState, role: Role, account: &Account) {
    match account {
        Account::Lastfm { .. } if role == Role::Scrobble => {
            state.keychain.clear_secret(LASTFM_SECRET_KEY);
            state.keychain.clear_secret(LASTFM_SESSION_KEY);
        }
        // Last.fm's import needs only the API key, kept with the account.
        Account::Lastfm { .. } => {}
        Account::Listenbrainz { .. } => state.keychain.clear_secret(listenbrainz_token_key(role)),
    }
}

fn set_account(config: &mut SourcesConfig, role: Role, account: Account) {
    match role {
        Role::Scrobble => config.scrobble = Some(account),
        Role::Stats => {
            config.stats_account = Some(account);
            config.stats_same_as_scrobble = false;
        }
    }
}

/// Apply a change to the accounts, and sync the history when the Stats page
/// now reads another account than before.
fn apply(
    app: &AppHandle,
    state: &AppState,
    mutate: impl FnOnce(&mut SourcesConfig),
) -> Result<(), ReverbError> {
    let change = state.stats.update(mutate)?;
    if change.stats_source_changed && change.config.stats_source().is_some() {
        tauri::async_runtime::spawn(crate::commands::stats::run_import(app.clone()));
    }
    Ok(())
}

/// Connect a Last.fm account for `role`. Scrobbling needs the API account's
/// shared secret and the user's password, which is traded once for a session
/// key and never stored; the Stats page's import needs only the username and
/// API key. Everything is checked against Last.fm before anything is saved.
#[tauri::command]
pub async fn services_connect_lastfm(
    role: Role,
    username: String,
    api_key: String,
    shared_secret: Option<String>,
    password: Option<String>,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<(), ReverbError> {
    let mut username = username.trim().to_owned();
    let api_key = api_key.trim().to_owned();
    if username.is_empty() || api_key.is_empty() {
        return Err(ReverbError::State("Enter your Last.fm username and API key.".into()));
    }
    lastfm::check_credential_shape("API key", &api_key).map_err(ReverbError::State)?;
    let client = reqwest::Client::new();
    let previous = state.stats.config().account(role).cloned();

    match role {
        Role::Scrobble => {
            let secret = shared_secret.map(|s| s.trim().to_owned()).filter(|s| !s.is_empty());
            let password = password.filter(|p| !p.is_empty());
            let (Some(secret), Some(password)) = (secret, password) else {
                return Err(ReverbError::State(
                    "Scrobbling needs the shared secret and your Last.fm password.".into(),
                ));
            };
            lastfm::check_credential_shape("shared secret", &secret)
                .map_err(ReverbError::State)?;
            let session =
                lastfm::get_mobile_session(&client, &api_key, &secret, &username, &password)
                    .await
                    .map_err(ReverbError::State)?;
            // The sign-in takes other spellings of the name; the import
            // (should stats follow this account) wants Last.fm's own.
            if let Some(canonical) = session.username.filter(|n| !n.is_empty()) {
                username = canonical;
            }
            if let Some(previous) = &previous {
                clear_secrets(&state, role, previous);
            }
            state.keychain.store_secret(LASTFM_SECRET_KEY, &secret)?;
            state.keychain.store_secret(LASTFM_SESSION_KEY, &session.key)?;
        }
        Role::Stats => {
            // One scrobble is enough to prove the name and key.
            lastfm::fetch_recent(&client, &username, &api_key, 0, None, 1)
                .await
                .map_err(ReverbError::State)?;
            if let Some(previous) = &previous {
                clear_secrets(&state, role, previous);
            }
        }
    }

    let account = Account::Lastfm { username, api_key };
    apply(&app, &state, |c| set_account(c, role, account))
}

/// Connect ListenBrainz, or a server that speaks its API, for `role`.
/// `api_url` is blank for listenbrainz.org; a blank `token` keeps the role's
/// current one, so the server can be changed without pasting it again. The
/// token is checked before anything is saved.
#[tauri::command]
pub async fn services_connect_listenbrainz(
    role: Role,
    token: Option<String>,
    api_url: Option<String>,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<(), ReverbError> {
    let key = listenbrainz_token_key(role);
    let previous = state.stats.config().account(role).cloned();
    let stored = || match previous {
        Some(Account::Listenbrainz { .. }) => state.keychain.retrieve_secret(key),
        _ => None,
    };
    let token = token
        .map(|t| t.trim().to_owned())
        .filter(|t| !t.is_empty())
        .or_else(stored)
        .ok_or_else(|| ReverbError::State("Enter your ListenBrainz user token.".into()))?;
    let api_root = listenbrainz::normalize_api_root(api_url.as_deref().unwrap_or_default())
        .map_err(ReverbError::State)?;
    let client = reqwest::Client::new();
    let username = listenbrainz::validate_token(&client, &api_root, &token)
        .await
        .map_err(ReverbError::State)?;

    // Does this server hand listen history back? listenbrainz.org always does.
    // For any other, asking for the listens after "now" costs nothing and
    // answers it: an empty page means yes, an error means a server that only
    // takes submissions. Imports also need to know whose listens to ask for.
    let api_url = (api_root != listenbrainz::DEFAULT_API_ROOT).then(|| api_root.clone());
    let history = !username.is_empty()
        && (api_url.is_none()
            || listenbrainz::fetch_listens(
                &client,
                &api_root,
                &token,
                &username,
                Utc::now().timestamp(),
            )
            .await
            .inspect_err(|e| log::info!("listenbrainz: no history to import from {api_root}: {e}"))
            .is_ok());
    if role == Role::Stats && !history {
        return Err(ReverbError::State(
            "This server does not share listening history, so the Stats page cannot import \
             from it. It can still be scrobbled to."
                .into(),
        ));
    }

    if let Some(previous @ Account::Lastfm { .. }) = &previous {
        clear_secrets(&state, role, previous);
    }
    state.keychain.store_secret(key, &token)?;
    let account = Account::Listenbrainz { username, api_url, history };
    apply(&app, &state, |c| set_account(c, role, account))
}

/// Disconnect `role`'s account.
///
/// Dropping the scrobbling account never takes the Stats page's history with
/// it: when stats follow that account, it moves over to stats instead and
/// keeps importing. Disconnecting the stats account deletes its history.
#[tauri::command]
pub fn services_disconnect(role: Role, state: State<'_, AppState>) -> Result<(), ReverbError> {
    let config = state.stats.config();
    match role {
        Role::Scrobble => {
            let Some(account) = config.scrobble.clone() else { return Ok(()) };
            let stats_follow = config.stats_same_as_scrobble && account.has_history();
            if stats_follow {
                if let Some(old) = &config.stats_account {
                    clear_secrets(&state, Role::Stats, old);
                }
                if let Account::Listenbrainz { .. } = account {
                    let token = state.keychain.retrieve_secret(listenbrainz_token_key(role));
                    if let Some(token) = token {
                        state.keychain.store_secret(listenbrainz_token_key(Role::Stats), &token)?;
                    }
                }
            }
            clear_secrets(&state, role, &account);
            state.stats.update(|c| {
                let account = c.scrobble.take();
                if stats_follow {
                    c.stats_account = account;
                    c.stats_same_as_scrobble = false;
                }
            })?;
        }
        Role::Stats => {
            if let Some(account) = &config.stats_account {
                clear_secrets(&state, role, account);
            }
            state.stats.update(|c| {
                c.stats_account = None;
                // Its history goes too, unless the Stats page reads the
                // scrobbling account's.
                if !c.stats_same_as_scrobble {
                    c.history = None;
                }
            })?;
        }
    }
    Ok(())
}

/// Whether the Stats page reads the scrobbling account or one of its own.
#[tauri::command]
pub fn services_set_stats_same_as_scrobble(
    same: bool,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<(), ReverbError> {
    apply(&app, &state, |c| c.stats_same_as_scrobble = same)
}
