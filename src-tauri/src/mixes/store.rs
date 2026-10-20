//! The current mixes, kept on disk so they hold still across restarts the
//! way Spotify's do: rebuilt once a day, Discover once a week.

use std::fs;
use std::path::PathBuf;
use std::sync::Mutex;

use serde::{Deserialize, Serialize};

use super::compute::Mix;
use crate::error::ReverbError;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MixSet {
    /// The server account the mixes came from; song ids mean nothing on
    /// another.
    pub account: String,
    /// Local date they were made for, `%Y-%m-%d`.
    pub date: String,
    /// ISO week Discover was made for, `%G-W%V`.
    pub week: String,
    /// Whether scrobbles fed them. Connecting Last.fm changes the picture
    /// enough to rebuild before tomorrow.
    pub has_history: bool,
    pub mixes: Vec<Mix>,
}

pub struct MixStore {
    path: PathBuf,
    current: Mutex<Option<MixSet>>,
    /// Held while mixes are generated, so a second caller waits for that
    /// result instead of fetching the whole library again.
    pub generation: tauri::async_runtime::Mutex<()>,
}

impl MixStore {
    pub fn load(data_dir: PathBuf) -> Self {
        let path = data_dir.join("mixes.json");
        // A missing or unreadable file only means generating anew.
        let current = fs::read_to_string(&path).ok().and_then(|raw| serde_json::from_str(&raw).ok());
        Self { path, current: Mutex::new(current), generation: Default::default() }
    }

    pub fn current(&self) -> Option<MixSet> {
        self.current.lock().unwrap().clone()
    }

    pub fn save(&self, set: MixSet) -> Result<(), ReverbError> {
        let raw = serde_json::to_string(&set).map_err(|e| ReverbError::Io(e.to_string()))?;
        if let Some(dir) = self.path.parent() {
            fs::create_dir_all(dir)?;
        }
        let tmp = self.path.with_extension("json.tmp");
        fs::write(&tmp, raw)?;
        fs::rename(tmp, &self.path)?;
        *self.current.lock().unwrap() = Some(set);
        Ok(())
    }
}
