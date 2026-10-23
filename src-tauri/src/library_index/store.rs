//! The Library index kept on disk, one file per account holding the listings
//! it was built from, so a restart on the same day, or signing back in to an
//! account, doesn't read the whole library again.

use std::fs;
use std::io::{BufWriter, Write};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Instant;

use reverb_core::subsonic::models::{ArtistId3, Child};
use serde::{Deserialize, Serialize};

use super::index::LibraryIndex;
use crate::error::ReverbError;

/// One account's index, as of one day.
#[derive(Clone)]
pub struct Built {
    /// Local date it was built on, `%Y-%m-%d`.
    pub date: String,
    pub index: Arc<LibraryIndex>,
}

/// One account's song and artist listings as read on one day: what goes to
/// disk, and what its index is built from.
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Listings {
    /// The server account they came from; ids mean nothing on another.
    pub account: String,
    /// Local date they were read on, `%Y-%m-%d`.
    pub date: String,
    pub songs: Vec<Child>,
    pub artists: Vec<ArtistId3>,
}

impl Listings {
    fn index(self) -> Built {
        Built { date: self.date, index: Arc::new(LibraryIndex::new(self.songs, self.artists)) }
    }
}

/// What's in memory: the index of the account last asked for.
struct Held {
    account: String,
    /// None while that account has neither a saved index nor a new one.
    built: Option<Built>,
}

pub struct LibraryIndexStore {
    dir: PathBuf,
    held: Mutex<Option<Held>>,
    /// Held while the index is read or built, so a second caller waits for
    /// that result instead of reading the library again.
    pub building: tauri::async_runtime::Mutex<()>,
    /// Indexes built since startup, so a caller can tell whether one was
    /// built while it waited.
    builds: AtomicU64,
    /// A scan started from Reverb is being watched, to rebuild after it.
    pub watching_scan: AtomicBool,
    failures: Mutex<Option<Failures>>,
}

/// Builds that failed in a row.
#[derive(Clone, Copy)]
pub struct Failures {
    pub count: u32,
    /// When the last one did.
    pub last: Instant,
}

impl LibraryIndexStore {
    pub fn new(data_dir: PathBuf) -> Self {
        Self {
            dir: data_dir.join("library_index"),
            held: Mutex::new(None),
            building: Default::default(),
            builds: AtomicU64::new(0),
            watching_scan: AtomicBool::new(false),
            failures: Mutex::new(None),
        }
    }

    pub fn builds(&self) -> u64 {
        self.builds.load(Ordering::SeqCst)
    }

    /// The builds that failed since the last one that didn't.
    pub fn failures(&self) -> Option<Failures> {
        *self.failures.lock().unwrap()
    }

    /// Count a failed build. The next saved one clears the count.
    pub fn failed(&self) {
        let mut failures = self.failures.lock().unwrap();
        let count = failures.map_or(0, |f| f.count) + 1;
        *failures = Some(Failures { count, last: Instant::now() });
    }

    /// `account`'s index: the one in memory, else its saved one, read from
    /// disk the first time it's asked for. A caller asking during that read
    /// waits for it. Blocking.
    pub fn get(&self, account: &str) -> Option<Built> {
        let mut held = self.held.lock().unwrap();
        if let Some(held) = held.as_ref().filter(|held| held.account == account) {
            return held.built.clone();
        }
        let built = self.read(account);
        *held = Some(Held { account: account.to_owned(), built: built.clone() });
        built
    }

    /// `account`'s saved index. A missing or unreadable file reads nothing.
    fn read(&self, account: &str) -> Option<Built> {
        let raw = fs::read(self.path(account)).ok()?;
        let saved: Listings = serde_json::from_slice(&raw).ok()?;
        // Another account's name can come out as the same file name.
        (saved.account == account).then(|| saved.index())
    }

    /// Save `listings`, index them and make that their account's index.
    /// Blocking.
    pub fn save(&self, listings: Listings) -> Result<Built, ReverbError> {
        let path = self.path(&listings.account);
        fs::create_dir_all(&self.dir)?;
        let tmp = path.with_extension("json.tmp");
        let mut out = BufWriter::new(fs::File::create(&tmp)?);
        serde_json::to_writer(&mut out, &listings).map_err(|e| ReverbError::Io(e.to_string()))?;
        out.flush()?;
        drop(out);
        fs::rename(tmp, &path)?;

        let account = listings.account.clone();
        let built = listings.index();
        *self.held.lock().unwrap() = Some(Held { account, built: Some(built.clone()) });
        self.builds.fetch_add(1, Ordering::SeqCst);
        *self.failures.lock().unwrap() = None;
        Ok(built)
    }

    /// Where `account`'s index is saved.
    fn path(&self, account: &str) -> PathBuf {
        // An account is `user@server_url`, slashes and all.
        let name: String =
            account.chars().map(|c| if c.is_ascii_alphanumeric() { c } else { '_' }).collect();
        self.dir.join(format!("{name}.json"))
    }
}
