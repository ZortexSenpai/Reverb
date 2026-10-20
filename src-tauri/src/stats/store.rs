//! Scrobble storage for the stats page. Each source appends to its own
//! JSONL file (one scrobble per line, chronological), which makes
//! incremental imports a cheap append; sync cursors live in sources.json.

use std::fs;
use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, RwLock};

use serde::{Deserialize, Serialize};

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

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct LastfmConfig {
    pub username: String,
    pub api_key: String,
    /// Whether an authenticated session for *submitting* plays exists. The
    /// session key and shared secret themselves live in the keychain, not
    /// here — this is only the flag the UI reads. Importing needs neither, so
    /// a source connected before scrobbling existed stays valid with `false`.
    pub scrobbling: bool,
    /// Unix ts of the newest imported scrobble (sync cursor).
    pub last_sync_ts: i64,
    /// ISO timestamp of the last successful sync.
    pub last_sync_at: Option<String>,
    pub track_count: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct SourcesConfig {
    pub lastfm: Option<LastfmConfig>,
}

pub struct StatsStore {
    dir: PathBuf,
    config: RwLock<SourcesConfig>,
    /// In-memory scrobble cache, invalidated by imports.
    cache: Mutex<Option<Vec<StatScrobble>>>,
    pub importing: AtomicBool,
}

impl StatsStore {
    pub fn load(data_dir: PathBuf) -> Result<Self, ReverbError> {
        let dir = data_dir.join("stats");
        fs::create_dir_all(&dir)?;
        let config = match fs::read_to_string(dir.join("sources.json")) {
            Ok(raw) => serde_json::from_str(&raw).unwrap_or_default(),
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

    pub fn update_config(
        &self,
        mutate: impl FnOnce(&mut SourcesConfig),
    ) -> Result<SourcesConfig, ReverbError> {
        let snapshot = {
            let mut guard = self.config.write().unwrap();
            mutate(&mut guard);
            guard.clone()
        };
        let raw = serde_json::to_string_pretty(&snapshot)
            .map_err(|e| ReverbError::Io(e.to_string()))?;
        let tmp = self.dir.join("sources.json.tmp");
        fs::write(&tmp, raw)?;
        fs::rename(tmp, self.dir.join("sources.json"))?;
        Ok(snapshot)
    }

    fn source_file(&self, source: &str) -> PathBuf {
        self.dir.join(format!("{source}.jsonl"))
    }

    /// Append scrobbles (callers pass them in chronological order).
    pub fn append(&self, source: &str, scrobbles: &[StatScrobble]) -> Result<(), ReverbError> {
        let mut file = fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(self.source_file(source))?;
        let mut buf = String::new();
        for s in scrobbles {
            buf.push_str(&serde_json::to_string(s).map_err(|e| ReverbError::Io(e.to_string()))?);
            buf.push('\n');
        }
        file.write_all(buf.as_bytes())?;
        *self.cache.lock().unwrap() = None;
        Ok(())
    }

    pub fn clear_source(&self, source: &str) {
        let _ = fs::remove_file(self.source_file(source));
        *self.cache.lock().unwrap() = None;
    }

    /// All scrobbles from all sources, cached in memory between imports.
    pub fn all_scrobbles(&self) -> Result<Vec<StatScrobble>, ReverbError> {
        if let Some(cached) = self.cache.lock().unwrap().as_ref() {
            return Ok(cached.clone());
        }
        let mut all = Vec::new();
        for source in ["lastfm"] {
            let path = self.source_file(source);
            let Ok(file) = fs::File::open(&path) else { continue };
            for line in BufReader::new(file).lines() {
                let Ok(line) = line else { break };
                if let Ok(s) = serde_json::from_str::<StatScrobble>(&line) {
                    all.push(s);
                }
            }
        }
        all.sort_by_key(|s| s.ts);
        *self.cache.lock().unwrap() = Some(all.clone());
        Ok(all)
    }
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
