//! The Stats page: importing the history of the account it reads, and the
//! numbers computed over it. The accounts themselves are connected in
//! `commands::services`.

use std::future::Future;

use chrono::Utc;
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, State};

use crate::error::ReverbError;
use crate::state::AppState;
use crate::stats::compute::{compute, StatsQuery, StatsRange, StatsSummary};
use crate::stats::store::{Account, HistoryId};
use crate::stats::{lastfm, listenbrainz, listenbrainz_token_key, Source};

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct ImportProgress {
    source: Source,
    state: &'static str, // "running" | "done" | "error"
    imported: u64,
    total: Option<u64>,
    message: Option<String>,
}

#[tauri::command]
pub fn stats_import(app: AppHandle) {
    tauri::async_runtime::spawn(run_import(app));
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
    let scrobbles = state.stats.scrobbles()?;
    Ok(compute(&scrobbles, StatsQuery { range, from, to }))
}

/// One import, resolved from the account the Stats page reads.
enum Plan {
    Lastfm { from: HistoryId, username: String, api_key: String, cursor: i64 },
    Listenbrainz { from: HistoryId, root: String, token: String, username: String, cursor: i64 },
}

impl Plan {
    fn source(&self) -> Source {
        match self {
            Plan::Lastfm { .. } => Source::Lastfm,
            Plan::Listenbrainz { .. } => Source::Listenbrainz,
        }
    }
}

/// What to import, `Ok(None)` when the Stats page reads no account.
fn plan(state: &AppState) -> Result<Option<Plan>, (Source, String)> {
    let config = state.stats.config();
    let Some((role, account)) = config.stats_source() else { return Ok(None) };
    // `update` keeps the history pointed at the stats source.
    let Some(history) = config.active_history() else { return Ok(None) };
    let from = history.from.clone();
    let cursor = history.last_sync_ts;
    Ok(Some(match account.clone() {
        Account::Lastfm { username, api_key } => Plan::Lastfm { from, username, api_key, cursor },
        Account::Listenbrainz { username, api_url, .. } => {
            let token = state.keychain.retrieve_secret(listenbrainz_token_key(role)).ok_or((
                Source::Listenbrainz,
                "The ListenBrainz token is missing — connect ListenBrainz again".to_owned(),
            ))?;
            let root = api_url.unwrap_or_else(|| listenbrainz::DEFAULT_API_ROOT.to_owned());
            Plan::Listenbrainz { from, root, token, username, cursor }
        }
    }))
}

enum Outcome {
    Done { imported: u64 },
    Failed { imported: u64, total: Option<u64>, message: String },
    /// The Stats page moved to another account mid-import.
    Superseded,
}

/// Pull new scrobbles into the history of the account the Stats page reads.
/// Imports take turns: one already running makes this a no-op — and if the
/// account changes under a running import, that import starts over for the
/// new one rather than finishing the old.
pub async fn run_import(app: AppHandle) {
    let state = app.state::<AppState>();
    let Some(guard) = state.stats.try_begin_import() else { return };
    let finished = loop {
        let plan = match plan(&state) {
            Ok(Some(plan)) => plan,
            Ok(None) => break None,
            Err((source, message)) => {
                break Some((source, Outcome::Failed { imported: 0, total: None, message }))
            }
        };
        let source = plan.source();
        emit_progress(
            &app,
            ImportProgress { source, state: "running", imported: 0, total: None, message: None },
        );
        let outcome = match plan {
            Plan::Lastfm { from, username, api_key, cursor } => {
                import_lastfm(&app, &from, &username, &api_key, cursor).await
            }
            Plan::Listenbrainz { from, root, token, username, cursor } => {
                import_listenbrainz(&app, &from, &root, &token, &username, cursor).await
            }
        };
        match outcome {
            Outcome::Superseded => continue,
            outcome => break Some((source, outcome)),
        }
    };
    // Release the lock BEFORE announcing completion, so the refetch the
    // listeners fire cannot observe a stale importing=true.
    drop(guard);
    match finished {
        None => {}
        Some((source, Outcome::Done { imported })) => emit_progress(
            &app,
            ImportProgress {
                source,
                state: "done",
                imported,
                total: Some(imported),
                message: None,
            },
        ),
        Some((source, Outcome::Failed { imported, total, message })) => {
            let message = if imported > 0 {
                format!("{message} — kept {imported} scrobbles; refresh to resume")
            } else {
                message
            };
            emit_progress(
                &app,
                ImportProgress { source, state: "error", imported, total, message: Some(message) },
            );
        }
        Some((_, Outcome::Superseded)) => unreachable!("superseded imports start over"),
    }
}

fn emit_progress(app: &AppHandle, progress: ImportProgress) {
    let _ = app.emit("stats:import", progress);
}

/// Store one page and announce it. `Err` carries the outcome that ends the
/// import: a failed write, or the history having moved to another account.
fn save_page(
    app: &AppHandle,
    from: &HistoryId,
    scrobbles: &[crate::stats::store::StatScrobble],
    imported: &mut u64,
    total: Option<u64>,
) -> Result<(), Outcome> {
    let state = app.state::<AppState>();
    match state.stats.append_history(from, scrobbles) {
        Ok(true) => {}
        Ok(false) => return Err(Outcome::Superseded),
        Err(e) => {
            return Err(Outcome::Failed { imported: *imported, total, message: e.to_string() })
        }
    }
    *imported += scrobbles.len() as u64;
    emit_progress(
        app,
        ImportProgress {
            source: from.source,
            state: "running",
            imported: *imported,
            total,
            message: None,
        },
    );
    Ok(())
}

fn finish(app: &AppHandle, from: &HistoryId, imported: u64) -> Outcome {
    let state = app.state::<AppState>();
    match state.stats.finish_sync(from, Utc::now().to_rfc3339()) {
        Ok(()) => Outcome::Done { imported },
        Err(e) => Outcome::Failed { imported, total: None, message: e.to_string() },
    }
}

/// Incremental Last.fm import: fetch the [cursor, now] window oldest-first
/// (highest page number first), persisting one page at a time and advancing
/// the cursor after each — an interrupted import (flaky API, app quit) keeps
/// everything fetched so far and the next run resumes instead of restarting.
async fn import_lastfm(
    app: &AppHandle,
    from: &HistoryId,
    username: &str,
    api_key: &str,
    from_ts: i64,
) -> Outcome {
    let client = reqwest::Client::new();
    // Fixed upper bound: keeps page numbering stable during the walk; anything
    // scrobbled after this moment is picked up by the next sync.
    let to_ts = Utc::now().timestamp();
    let mut imported = 0u64;
    let mut newest_saved = from_ts;
    let fetch = |page: u32| {
        let client = &client;
        retrying(format!("last.fm page {page}"), move || {
            lastfm::fetch_recent(client, username, api_key, from_ts, Some(to_ts), page)
        })
    };

    // Page 1 reveals the window's page count; its content is reused when the
    // backwards walk arrives back at it.
    let first = match fetch(1).await {
        Ok(page) => page,
        Err(message) => return Outcome::Failed { imported, total: None, message },
    };
    let total = Some(first.total);
    let total_pages =
        if first.scrobbles.is_empty() { first.total_pages } else { first.total_pages.max(1) };
    let mut first = Some(first);

    for page in (1..=total_pages).rev() {
        let fetched = if page == 1 {
            first.take().expect("page 1 is visited exactly once")
        } else {
            match fetch(page).await {
                Ok(fetched) => fetched,
                Err(message) => return Outcome::Failed { imported, total, message },
            }
        };

        // Newest-first from the API; store chronologically. The ts filter
        // drops the cursor overlap (from is inclusive on some servers) and
        // anything already saved by this walk.
        let mut scrobbles = fetched.scrobbles;
        scrobbles.retain(|s| s.ts > newest_saved);
        scrobbles.sort_by_key(|s| s.ts);
        if let Some(newest) = scrobbles.last().map(|s| s.ts) {
            if let Err(outcome) = save_page(app, from, &scrobbles, &mut imported, total) {
                return outcome;
            }
            newest_saved = newest;
        }
        if page > 1 {
            // Stay well within Last.fm's rate limits.
            tokio_sleep(250).await;
        }
    }
    finish(app, from, imported)
}

/// Incremental ListenBrainz import. The API pages forward from a timestamp, so
/// this walks from the cursor towards now, persisting each page and advancing
/// the cursor after it — like the Last.fm import, an interrupted run keeps
/// what it fetched and the next one carries on from there.
async fn import_listenbrainz(
    app: &AppHandle,
    from: &HistoryId,
    root: &str,
    token: &str,
    username: &str,
    mut cursor: i64,
) -> Outcome {
    let client = reqwest::Client::new();
    let mut imported = 0u64;
    // The account's whole count is only the size of the job on a first import;
    // later ones fetch an unknown slice of it.
    let total = if cursor == 0 {
        listenbrainz::fetch_listen_count(&client, root, token, username).await.ok()
    } else {
        None
    };

    loop {
        let fetched = retrying("listenbrainz listens".to_owned(), || {
            listenbrainz::fetch_listens(&client, root, token, username, cursor)
        })
        .await;
        let page = match fetched {
            Ok(page) => page,
            Err(message) => return Outcome::Failed { imported, total, message },
        };

        // Newest-first from the API; store chronologically. The ts filter
        // guards against a server that ignores `min_ts` and sends the same
        // newest listens every time — nothing new means the walk is over.
        let mut scrobbles = page.scrobbles;
        scrobbles.retain(|s| s.ts > cursor);
        scrobbles.sort_by_key(|s| s.ts);
        let Some(newest) = scrobbles.last().map(|s| s.ts) else { break };
        if let Err(outcome) = save_page(app, from, &scrobbles, &mut imported, total) {
            return outcome;
        }
        cursor = newest;

        // A short page is the last one.
        if page.returned < listenbrainz::PAGE_SIZE {
            break;
        }
        // ListenBrainz rate-limits per token; this stays far below it.
        tokio_sleep(250).await;
    }
    finish(app, from, imported)
}

/// Run an import call, retrying transient failures with growing backoff.
/// Both services fail now and then on deep pagination (Last.fm's "backend
/// service failed", ListenBrainz's rate limit), and a retry is usually all it
/// takes.
async fn retrying<T, F, Fut>(label: String, mut call: F) -> Result<T, String>
where
    F: FnMut() -> Fut,
    Fut: Future<Output = Result<T, String>>,
{
    const ATTEMPTS: u32 = 4;
    let mut delay_ms = 1_000;
    for attempt in 1..=ATTEMPTS {
        match call().await {
            Ok(value) => return Ok(value),
            Err(message) if attempt < ATTEMPTS => {
                log::warn!("{label} failed (attempt {attempt}/{ATTEMPTS}): {message}");
                tokio_sleep(delay_ms).await;
                delay_ms *= 3;
            }
            Err(message) => {
                log::warn!("{label} failed, giving up: {message}");
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
