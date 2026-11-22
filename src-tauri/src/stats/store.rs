//! The connected accounts and the imported history behind the Stats page.
//!
//! There are two accounts, one per [`Role`]: the scrobbling account takes Plays
//! when direct scrobbling is on, and the stats account supplies the history —
//! or, by default, the Stats page follows the scrobbling account instead of
//! having one of its own. Secrets live in the keychain; the rest is
//! `sources.json`. The imported history is a single JSONL file (one scrobble
//! per line, chronological), which makes incremental imports a cheap append.

use std::fs;
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, RwLock};

use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::{Role, Source};
use crate::error::ReverbError;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StatScrobble {
    /// Unix seconds.
    pub ts: i64,
    pub artist: String,
    pub track: String,
    #[serde(default)]
    pub album: Option<String>,
}

/// A connected account, minus its secrets (those are in the keychain).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "service", rename_all = "lowercase", rename_all_fields = "camelCase")]
pub enum Account {
    Lastfm {
        username: String,
        api_key: String,
    },
    Listenbrainz {
        /// As the server reported it. Can be empty for a server that only
        /// mimics the API, which then cannot supply history.
        username: String,
        /// API root of a server other than listenbrainz.org (normalized, no
        /// trailing slash); `None` means listenbrainz.org.
        api_url: Option<String>,
        /// Whether the server hands listening history back. listenbrainz.org
        /// does; scrobble servers that merely accept ListenBrainz submissions
        /// often do not, and can then only be scrobbled to.
        history: bool,
    },
}

impl Account {
    pub fn source(&self) -> Source {
        match self {
            Account::Lastfm { .. } => Source::Lastfm,
            Account::Listenbrainz { .. } => Source::Listenbrainz,
        }
    }

    pub fn username(&self) -> &str {
        match self {
            Account::Lastfm { username, .. } | Account::Listenbrainz { username, .. } => username,
        }
    }

    pub fn api_url(&self) -> Option<&str> {
        match self {
            Account::Lastfm { .. } => None,
            Account::Listenbrainz { api_url, .. } => api_url.as_deref(),
        }
    }

    /// Whether the Stats page can import from this account.
    pub fn has_history(&self) -> bool {
        match self {
            Account::Lastfm { .. } => true,
            Account::Listenbrainz { history, .. } => *history,
        }
    }

    pub fn history_id(&self) -> HistoryId {
        HistoryId {
            source: self.source(),
            username: self.username().to_owned(),
            api_url: self.api_url().map(str::to_owned),
        }
    }
}

/// Whose history it is. The same service, account and server is the same
/// history, however the account got connected — which is what lets "use the
/// scrobbling account" be switched off and on without importing again.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HistoryId {
    pub source: Source,
    pub username: String,
    pub api_url: Option<String>,
}

/// The imported history and how far it has been synced.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct History {
    pub from: HistoryId,
    /// Unix ts of the newest imported scrobble (sync cursor).
    #[serde(default)]
    pub last_sync_ts: i64,
    /// ISO timestamp of the last completed sync.
    #[serde(default)]
    pub last_sync_at: Option<String>,
    #[serde(default)]
    pub track_count: u64,
}

impl History {
    fn fresh(from: HistoryId) -> Self {
        Self { from, last_sync_ts: 0, last_sync_at: None, track_count: 0 }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct SourcesConfig {
    /// Where direct scrobbling sends Plays.
    pub scrobble: Option<Account>,
    /// Read the scrobbling account's history rather than `stats_account`'s.
    pub stats_same_as_scrobble: bool,
    /// The Stats page's own account, used while `stats_same_as_scrobble` is
    /// off — another service, account or server than the one scrobbled to.
    pub stats_account: Option<Account>,
    /// The imported history. It belongs to whichever account the Stats page
    /// reads; while that is none it is kept as it was, so it picks up where it
    /// left off when that account comes back.
    pub history: Option<History>,
}

impl Default for SourcesConfig {
    fn default() -> Self {
        Self { scrobble: None, stats_same_as_scrobble: true, stats_account: None, history: None }
    }
}

impl SourcesConfig {
    pub fn account(&self, role: Role) -> Option<&Account> {
        match role {
            Role::Scrobble => self.scrobble.as_ref(),
            Role::Stats => self.stats_account.as_ref(),
        }
    }

    /// The account the Stats page reads, and the role whose secrets it uses.
    /// `None` when the chosen account is missing or cannot supply history.
    pub fn stats_source(&self) -> Option<(Role, &Account)> {
        let role = if self.stats_same_as_scrobble { Role::Scrobble } else { Role::Stats };
        self.account(role).filter(|a| a.has_history()).map(|a| (role, a))
    }

    /// The history of the account the Stats page reads, if any.
    pub fn active_history(&self) -> Option<&History> {
        let (_, account) = self.stats_source()?;
        let id = account.history_id();
        self.history.as_ref().filter(|h| h.from == id)
    }

    /// An API key for Last.fm's read-only calls (artist biographies), from
    /// whichever account has one.
    pub fn lastfm_api_key(&self) -> Option<String> {
        [&self.scrobble, &self.stats_account].into_iter().flatten().find_map(|a| match a {
            Account::Lastfm { api_key, .. } => Some(api_key.clone()),
            Account::Listenbrainz { .. } => None,
        })
    }

    /// Point the history at the account the Stats page now reads. Another
    /// account's history is replaced by an empty one; with no account to
    /// read, the history is left alone.
    fn reconcile(&mut self) {
        let Some(id) = self.stats_source().map(|(_, a)| a.history_id()) else { return };
        if self.history.as_ref().map(|h| &h.from) != Some(&id) {
            self.history = Some(History::fresh(id));
        }
    }
}

/// `sources.json` as Reverb 1.0 wrote it: one Last.fm connection that
/// imported history and, with `scrobbling`, also took Plays.
#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
struct LegacyLastfm {
    username: String,
    api_key: String,
    scrobbling: bool,
    last_sync_ts: i64,
    last_sync_at: Option<String>,
    track_count: u64,
}

/// Read `sources.json`, carrying a 1.0 file over: a Last.fm connection that
/// scrobbled becomes the scrobbling account with stats following it, an
/// import-only one becomes the stats account. Either way its history (still
/// `lastfm.jsonl`) and sync cursor stay, so nothing is imported again.
fn parse_sources(raw: &str) -> SourcesConfig {
    let Ok(value) = serde_json::from_str::<Value>(raw) else { return SourcesConfig::default() };
    let legacy = value["lastfm"].is_object()
        && ["scrobble", "statsAccount", "history"].iter().all(|key| value.get(key).is_none());
    if !legacy {
        return serde_json::from_value(value).unwrap_or_default();
    }
    let old: LegacyLastfm = serde_json::from_value(value["lastfm"].clone()).unwrap_or_default();
    let account = Account::Lastfm { username: old.username, api_key: old.api_key };
    let history = Some(History {
        from: account.history_id(),
        last_sync_ts: old.last_sync_ts,
        last_sync_at: old.last_sync_at,
        track_count: old.track_count,
    });
    if old.scrobbling {
        SourcesConfig {
            scrobble: Some(account),
            stats_same_as_scrobble: true,
            stats_account: None,
            history,
        }
    } else {
        SourcesConfig {
            scrobble: None,
            stats_same_as_scrobble: false,
            stats_account: Some(account),
            history,
        }
    }
}

/// What a configuration change did.
pub struct Change {
    pub config: SourcesConfig,
    /// The Stats page reads a different account than before, or now reads one
    /// at all — its history wants a sync.
    pub stats_source_changed: bool,
}

pub struct StatsStore {
    dir: PathBuf,
    config: RwLock<SourcesConfig>,
    /// The active history in memory, invalidated by imports.
    cache: Mutex<Option<(HistoryId, Vec<StatScrobble>)>>,
    pub importing: AtomicBool,
}

impl StatsStore {
    pub fn load(data_dir: PathBuf) -> Result<Self, ReverbError> {
        let dir = data_dir.join("stats");
        fs::create_dir_all(&dir)?;
        let config = match fs::read_to_string(dir.join("sources.json")) {
            Ok(raw) => parse_sources(&raw),
            Err(_) => SourcesConfig::default(),
        };
        Ok(Self {
            dir,
            config: RwLock::new(config),
            cache: Mutex::new(None),
            importing: AtomicBool::new(false),
        })
    }

    pub fn config(&self) -> SourcesConfig {
        self.config.read().unwrap().clone()
    }

    fn history_file(&self, source: Source) -> PathBuf {
        history_file(&self.dir, source)
    }

    /// Written while the config lock is held, so writes land in the order the
    /// changes were made.
    fn persist(&self, config: &SourcesConfig) -> Result<(), ReverbError> {
        let raw =
            serde_json::to_string_pretty(config).map_err(|e| ReverbError::Io(e.to_string()))?;
        let tmp = self.dir.join("sources.json.tmp");
        fs::write(&tmp, raw)?;
        fs::rename(tmp, self.dir.join("sources.json"))?;
        Ok(())
    }

    /// Change the accounts. The history follows: when the Stats page ends up
    /// reading another account, the old history's file is deleted and an
    /// empty one takes its place. Setting `history` to `None` in `mutate`
    /// deletes it outright.
    pub fn update(&self, mutate: impl FnOnce(&mut SourcesConfig)) -> Result<Change, ReverbError> {
        let mut config = self.config.write().unwrap();
        let source_before = config.stats_source().map(|(_, a)| a.history_id());
        let history_before = config.history.as_ref().map(|h| h.from.clone());
        mutate(&mut config);
        config.reconcile();
        let history_after = config.history.as_ref().map(|h| h.from.clone());
        if history_before != history_after {
            // The old history's file goes with it, and a new one starts from
            // an empty file rather than from whatever an earlier one left.
            for id in [&history_before, &history_after].into_iter().flatten() {
                let _ = fs::remove_file(self.history_file(id.source));
            }
            *self.cache.lock().unwrap() = None;
        }
        self.persist(&config)?;
        let source_after = config.stats_source().map(|(_, a)| a.history_id());
        Ok(Change {
            config: config.clone(),
            stats_source_changed: source_before != source_after,
        })
    }

    /// Append scrobbles (chronological) to the history of `from` and move its
    /// cursor past them. Returns `false`, appending nothing, when the history
    /// now belongs to another account — the import fetching them has been
    /// overtaken by a change of account.
    pub fn append_history(
        &self,
        from: &HistoryId,
        scrobbles: &[StatScrobble],
    ) -> Result<bool, ReverbError> {
        let mut config = self.config.write().unwrap();
        let Some(history) = config.history.as_mut().filter(|h| &h.from == from) else {
            return Ok(false);
        };
        let Some(newest) = scrobbles.last().map(|s| s.ts) else { return Ok(true) };
        let mut buf = String::new();
        for s in scrobbles {
            buf.push_str(&serde_json::to_string(s).map_err(|e| ReverbError::Io(e.to_string()))?);
            buf.push('\n');
        }
        fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(self.history_file(from.source))?
            .write_all(buf.as_bytes())?;
        history.last_sync_ts = newest;
        history.track_count += scrobbles.len() as u64;
        *self.cache.lock().unwrap() = None;
        self.persist(&config)?;
        Ok(true)
    }

    /// Stamp a completed sync of `from`'s history.
    pub fn finish_sync(&self, from: &HistoryId, at: String) -> Result<(), ReverbError> {
        let mut config = self.config.write().unwrap();
        if let Some(history) = config.history.as_mut().filter(|h| &h.from == from) {
            history.last_sync_at = Some(at);
            self.persist(&config)?;
        }
        Ok(())
    }

    /// The scrobbles of the account the Stats page reads, chronological,
    /// cached in memory between imports. Empty while it reads none.
    pub fn scrobbles(&self) -> Result<Vec<StatScrobble>, ReverbError> {
        let Some(id) = self.config().active_history().map(|h| h.from.clone()) else {
            return Ok(Vec::new());
        };
        if let Some((cached_id, cached)) = self.cache.lock().unwrap().as_ref() {
            if *cached_id == id {
                return Ok(cached.clone());
            }
        }
        let mut all = Vec::new();
        if let Ok(file) = fs::File::open(self.history_file(id.source)) {
            for line in BufReader::new(file).lines() {
                let Ok(line) = line else { break };
                if let Ok(s) = serde_json::from_str::<StatScrobble>(&line) {
                    all.push(s);
                }
            }
        }
        all.sort_by_key(|s| s.ts);
        *self.cache.lock().unwrap() = Some((id, all.clone()));
        Ok(all)
    }
}

/// Named by service, as in 1.0, so its Last.fm history file carries over.
fn history_file(dir: &Path, source: Source) -> PathBuf {
    dir.join(format!("{}.jsonl", source.id()))
}

/// RAII guard for the single-import lock.
pub struct ImportGuard<'a>(&'a AtomicBool);

impl StatsStore {
    pub fn try_begin_import(&self) -> Option<ImportGuard<'_>> {
        if self.importing.swap(true, Ordering::SeqCst) {
            None
        } else {
            Some(ImportGuard(&self.importing))
        }
    }
}

impl Drop for ImportGuard<'_> {
    fn drop(&mut self) {
        self.0.store(false, Ordering::SeqCst);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lastfm(username: &str) -> Account {
        Account::Lastfm { username: username.into(), api_key: "k".into() }
    }

    fn listenbrainz(api_url: Option<&str>, history: bool) -> Account {
        Account::Listenbrainz {
            username: "lb".into(),
            api_url: api_url.map(str::to_owned),
            history,
        }
    }

    fn scrobble(ts: i64) -> StatScrobble {
        StatScrobble { ts, artist: "A".into(), track: "T".into(), album: None }
    }

    #[test]
    fn a_last_fm_connection_that_scrobbled_becomes_the_scrobbling_account() {
        let config = parse_sources(
            r#"{"lastfm": {"username": "u", "apiKey": "k", "scrobbling": true,
                "lastSyncTs": 1700000000, "lastSyncAt": "2026-01-01T00:00:00Z", "trackCount": 42}}"#,
        );
        assert_eq!(config.scrobble, Some(lastfm("u")));
        assert!(config.stats_same_as_scrobble);
        assert_eq!(config.stats_account, None);
        let history = config.active_history().expect("stats read the scrobbling account");
        assert_eq!(history.last_sync_ts, 1_700_000_000);
        assert_eq!(history.track_count, 42);
    }

    #[test]
    fn an_import_only_last_fm_connection_becomes_the_stats_account() {
        let config = parse_sources(
            r#"{"lastfm": {"username": "u", "apiKey": "k", "scrobbling": false, "trackCount": 7}}"#,
        );
        assert_eq!(config.scrobble, None);
        assert!(!config.stats_same_as_scrobble);
        assert_eq!(config.stats_account, Some(lastfm("u")));
        assert_eq!(config.active_history().map(|h| h.track_count), Some(7));
    }

    #[test]
    fn a_file_without_accounts_reads_as_defaults() {
        for raw in ["", "{}", r#"{"lastfm": null}"#] {
            assert_eq!(parse_sources(raw), SourcesConfig::default(), "{raw:?}");
        }
    }

    #[test]
    fn the_current_layout_round_trips() {
        let config = SourcesConfig {
            scrobble: Some(listenbrainz(Some("https://maloja.example/apis/listenbrainz"), false)),
            stats_same_as_scrobble: false,
            stats_account: Some(lastfm("u")),
            history: Some(History::fresh(lastfm("u").history_id())),
        };
        let raw = serde_json::to_string(&config).unwrap();
        assert!(raw.contains(r#""service":"listenbrainz""#), "{raw}");
        assert!(raw.contains(r#""apiUrl":"https://maloja.example/apis/listenbrainz""#), "{raw}");
        assert_eq!(parse_sources(&raw), config);
    }

    #[test]
    fn stats_follow_the_scrobbling_account_only_where_it_has_history() {
        let mut config = SourcesConfig { scrobble: Some(lastfm("u")), ..SourcesConfig::default() };
        assert_eq!(config.stats_source().map(|(role, _)| role), Some(Role::Scrobble));

        // A scrobble-only server has nothing to show.
        config.scrobble = Some(listenbrainz(Some("https://maloja.example"), false));
        assert!(config.stats_source().is_none());

        // Its own account is read once stats stop following.
        config.stats_same_as_scrobble = false;
        config.stats_account = Some(listenbrainz(None, true));
        assert_eq!(config.stats_source().map(|(role, _)| role), Some(Role::Stats));
    }

    #[test]
    fn the_history_belongs_to_whichever_account_stats_read() {
        let mut config = SourcesConfig { scrobble: Some(lastfm("u")), ..SourcesConfig::default() };
        config.reconcile();
        config.history.as_mut().unwrap().track_count = 10;

        // No account to read: the history waits rather than going.
        config.stats_same_as_scrobble = false;
        config.reconcile();
        assert_eq!(config.history.as_ref().map(|h| h.track_count), Some(10));
        assert!(config.active_history().is_none());

        // The same account again, under the other role: same history.
        config.stats_account = Some(lastfm("u"));
        config.reconcile();
        assert_eq!(config.active_history().map(|h| h.track_count), Some(10));

        // Another server is another history.
        config.stats_account = Some(listenbrainz(Some("https://lb.example"), true));
        config.reconcile();
        assert_eq!(config.active_history().map(|h| h.track_count), Some(0));
    }

    #[test]
    fn switching_accounts_deletes_the_old_history_and_overtakes_its_import() {
        let dir = tempfile::tempdir().unwrap();
        let store = StatsStore::load(dir.path().to_path_buf()).unwrap();
        let change = store.update(|c| c.scrobble = Some(lastfm("u"))).unwrap();
        assert!(change.stats_source_changed, "stats now read an account");

        let from = lastfm("u").history_id();
        assert!(store.append_history(&from, &[scrobble(1), scrobble(2)]).unwrap());
        assert_eq!(store.scrobbles().unwrap().len(), 2);
        assert_eq!(store.config().active_history().unwrap().last_sync_ts, 2);
        let file = dir.path().join("stats/lastfm.jsonl");
        assert!(file.exists());

        let change = store.update(|c| c.scrobble = Some(listenbrainz(None, true))).unwrap();
        assert!(change.stats_source_changed);
        assert!(!file.exists(), "the Last.fm history went with the account");
        assert!(store.scrobbles().unwrap().is_empty());
        // An import still fetching for Last.fm finds its history gone.
        assert!(!store.append_history(&from, &[scrobble(3)]).unwrap());
        assert!(store.scrobbles().unwrap().is_empty());
    }

    #[test]
    fn toggling_without_another_account_keeps_the_history() {
        let dir = tempfile::tempdir().unwrap();
        let store = StatsStore::load(dir.path().to_path_buf()).unwrap();
        store.update(|c| c.scrobble = Some(lastfm("u"))).unwrap();
        let from = lastfm("u").history_id();
        store.append_history(&from, &[scrobble(1)]).unwrap();

        store.update(|c| c.stats_same_as_scrobble = false).unwrap();
        assert!(store.scrobbles().unwrap().is_empty(), "nothing to read meanwhile");
        let change = store.update(|c| c.stats_same_as_scrobble = true).unwrap();
        assert!(change.stats_source_changed);
        assert_eq!(store.scrobbles().unwrap().len(), 1, "back without importing again");

        // Reloading from disk gives the same picture.
        let reloaded = StatsStore::load(dir.path().to_path_buf()).unwrap();
        assert_eq!(reloaded.scrobbles().unwrap().len(), 1);
    }
}
