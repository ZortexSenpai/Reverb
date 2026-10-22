//! The Play History behind "Recently played": the newest Plays of one
//! account on this computer, kept on disk so they survive clearing the Queue
//! and restarting. Recorded locally rather than read from Navidrome (see
//! `docs/adr/0001-play-history-is-recorded-locally.md`).

use std::fs;
use std::path::PathBuf;
use std::sync::Mutex;

use serde::{Deserialize, Serialize};

use crate::error::ReverbError;

/// Past this, recording a Play drops the oldest.
const MAX_PLAYS: usize = 200;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Play {
    /// Unix ms of when the Song started.
    pub played_at: u64,
    /// The frontend's `Song`, kept as it was sent (like the queue snapshot).
    pub song: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PlayHistory {
    /// The server account the Plays belong to; song ids mean nothing on
    /// another.
    account: String,
    /// Newest first.
    plays: Vec<Play>,
}

pub struct PlayHistoryStore {
    path: PathBuf,
    current: Mutex<Option<PlayHistory>>,
}

impl PlayHistoryStore {
    pub fn load(data_dir: PathBuf) -> Self {
        let path = data_dir.join("play_history.json");
        // A missing or unreadable file only means no Plays yet.
        let current = fs::read_to_string(&path).ok().and_then(|raw| serde_json::from_str(&raw).ok());
        Self { path, current: Mutex::new(current) }
    }

    /// The Plays of `account`, newest first. Signed out (`None`), or signed in
    /// to an account other than the one recorded, there are none.
    pub fn plays(&self, account: Option<&str>) -> Vec<Play> {
        match (self.current.lock().unwrap().as_ref(), account) {
            (Some(history), Some(account)) if history.account == account => history.plays.clone(),
            _ => Vec::new(),
        }
    }

    /// Add a Play at the front. Under another account than the one recorded,
    /// it starts a new Play History in place of the old one.
    pub fn record(&self, account: &str, play: Play) -> Result<(), ReverbError> {
        let mut current = self.current.lock().unwrap();
        let mut plays = match current.as_ref() {
            Some(history) if history.account == account => history.plays.clone(),
            _ => Vec::new(),
        };
        plays.insert(0, play);
        plays.truncate(MAX_PLAYS);
        self.save(&mut current, PlayHistory { account: account.to_owned(), plays })
    }

    /// Empty `account`'s Play History. Like recording, it replaces whatever
    /// another account left behind.
    pub fn clear(&self, account: &str) -> Result<(), ReverbError> {
        let mut current = self.current.lock().unwrap();
        self.save(&mut current, PlayHistory { account: account.to_owned(), plays: Vec::new() })
    }

    /// Write `history` atomically (tmp file, then rename), then make it the
    /// current one. Called with the lock held, so writes never interleave.
    fn save(&self, current: &mut Option<PlayHistory>, history: PlayHistory) -> Result<(), ReverbError> {
        let raw = serde_json::to_string(&history).map_err(|e| ReverbError::Io(e.to_string()))?;
        if let Some(dir) = self.path.parent() {
            fs::create_dir_all(dir)?;
        }
        let tmp = self.path.with_extension("json.tmp");
        fs::write(&tmp, raw)?;
        fs::rename(tmp, &self.path)?;
        *current = Some(history);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    const ACCOUNT: &str = "listener@https://music.example.com/";

    fn play(id: &str, played_at: u64) -> Play {
        Play { played_at, song: json!({ "id": id, "title": format!("Song {id}") }) }
    }

    /// `(song id, playedAt)` of each Play, in the order the store gives them.
    fn listed(plays: &[Play]) -> Vec<(String, u64)> {
        plays.iter().map(|p| (p.song["id"].as_str().unwrap().to_owned(), p.played_at)).collect()
    }

    #[test]
    fn recording_keeps_plays_newest_first() {
        let dir = tempfile::tempdir().unwrap();
        let store = PlayHistoryStore::load(dir.path().to_path_buf());

        store.record(ACCOUNT, play("a", 1_000)).unwrap();
        store.record(ACCOUNT, play("b", 2_000)).unwrap();
        store.record(ACCOUNT, play("c", 3_000)).unwrap();

        assert_eq!(
            listed(&store.plays(Some(ACCOUNT))),
            [("c".into(), 3_000), ("b".into(), 2_000), ("a".into(), 1_000)]
        );
    }

    #[test]
    fn the_201st_play_drops_the_oldest() {
        let dir = tempfile::tempdir().unwrap();
        let store = PlayHistoryStore::load(dir.path().to_path_buf());

        for n in 1..=201 {
            store.record(ACCOUNT, play(&n.to_string(), n)).unwrap();
        }

        let plays = listed(&store.plays(Some(ACCOUNT)));
        assert_eq!(plays.len(), 200);
        assert_eq!(plays.first(), Some(&("201".into(), 201)), "the newest is kept");
        assert_eq!(plays.last(), Some(&("2".into(), 2)), "only the first Play is gone");
    }

    #[test]
    fn the_same_song_recorded_twice_gives_two_plays() {
        let dir = tempfile::tempdir().unwrap();
        let store = PlayHistoryStore::load(dir.path().to_path_buf());

        store.record(ACCOUNT, play("a", 1_000)).unwrap();
        store.record(ACCOUNT, play("a", 2_000)).unwrap();

        assert_eq!(listed(&store.plays(Some(ACCOUNT))), [("a".into(), 2_000), ("a".into(), 1_000)]);
    }

    #[test]
    fn reading_under_another_account_or_signed_out_returns_nothing() {
        let dir = tempfile::tempdir().unwrap();
        let store = PlayHistoryStore::load(dir.path().to_path_buf());
        store.record(ACCOUNT, play("a", 1_000)).unwrap();

        assert!(store.plays(Some("someone-else@https://music.example.com/")).is_empty());
        assert!(store.plays(Some("listener@https://other.example.com/")).is_empty());
        assert!(store.plays(None).is_empty());
        assert_eq!(store.plays(Some(ACCOUNT)).len(), 1, "still there for its own account");
    }

    #[test]
    fn recording_under_another_account_replaces_the_old_play_history() {
        let dir = tempfile::tempdir().unwrap();
        let store = PlayHistoryStore::load(dir.path().to_path_buf());
        let other = "someone-else@https://other.example.com/";
        store.record(ACCOUNT, play("a", 1_000)).unwrap();
        store.record(ACCOUNT, play("b", 2_000)).unwrap();

        store.record(other, play("x", 3_000)).unwrap();

        assert_eq!(listed(&store.plays(Some(other))), [("x".into(), 3_000)]);
        assert!(store.plays(Some(ACCOUNT)).is_empty(), "the old account's Plays are gone");
    }

    #[test]
    fn plays_survive_a_reload_from_disk() {
        let dir = tempfile::tempdir().unwrap();
        let store = PlayHistoryStore::load(dir.path().to_path_buf());
        let song = json!({ "id": "a", "title": "Song a", "coverArt": "al-1", "duration": 215 });
        store.record(ACCOUNT, Play { played_at: 1_000, song: song.clone() }).unwrap();
        store.record(ACCOUNT, play("b", 2_000)).unwrap();

        let reloaded = PlayHistoryStore::load(dir.path().to_path_buf());

        let plays = reloaded.plays(Some(ACCOUNT));
        assert_eq!(listed(&plays), [("b".into(), 2_000), ("a".into(), 1_000)]);
        assert_eq!(plays[1].song, song, "the Song comes back exactly as it was recorded");
    }

    #[test]
    fn clearing_leaves_an_empty_play_history_that_survives_a_reload() {
        let dir = tempfile::tempdir().unwrap();
        let store = PlayHistoryStore::load(dir.path().to_path_buf());
        store.record(ACCOUNT, play("a", 1_000)).unwrap();
        store.record(ACCOUNT, play("b", 2_000)).unwrap();

        store.clear(ACCOUNT).unwrap();

        assert!(store.plays(Some(ACCOUNT)).is_empty());
        let reloaded = PlayHistoryStore::load(dir.path().to_path_buf());
        assert!(reloaded.plays(Some(ACCOUNT)).is_empty(), "the Plays don't come back");
        reloaded.record(ACCOUNT, play("c", 3_000)).unwrap();
        assert_eq!(listed(&reloaded.plays(Some(ACCOUNT))), [("c".into(), 3_000)]);
    }

    #[test]
    fn a_missing_play_history_file_loads_as_empty() {
        let dir = tempfile::tempdir().unwrap();

        let store = PlayHistoryStore::load(dir.path().join("not-made-yet"));

        assert!(store.plays(Some(ACCOUNT)).is_empty());
        store.record(ACCOUNT, play("a", 1_000)).unwrap();
        assert_eq!(store.plays(Some(ACCOUNT)).len(), 1, "the data dir is made on first write");
    }

    #[test]
    fn an_unreadable_play_history_file_loads_as_empty() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("play_history.json"), br#"{"account": 42, "plays": "#).unwrap();

        let store = PlayHistoryStore::load(dir.path().to_path_buf());

        assert!(store.plays(Some(ACCOUNT)).is_empty());
        store.record(ACCOUNT, play("a", 1_000)).unwrap();
        let reloaded = PlayHistoryStore::load(dir.path().to_path_buf());
        assert_eq!(listed(&reloaded.plays(Some(ACCOUNT))), [("a".into(), 1_000)]);
    }
}
