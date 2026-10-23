//! The Library index kept on disk with the listings it was built from, so a
//! restart on the same day doesn't read the whole library again.

use std::fs;
use std::io::{BufWriter, Write};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use reverb_core::subsonic::models::{ArtistId3, Child};
use serde::{Deserialize, Serialize};

use super::index::LibraryIndex;
use crate::error::ReverbError;

/// The index of one account, as of one day.
#[derive(Clone)]
pub struct Built {
    /// The server account it came from; ids mean nothing on another.
    pub account: String,
    /// Local date it was built on, `%Y-%m-%d`.
    pub date: String,
    pub index: Arc<LibraryIndex>,
}

/// What goes to disk: the listings, from which the index is rebuilt on load.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ListingsRef<'a> {
    account: &'a str,
    date: &'a str,
    songs: &'a [Child],
    artists: &'a [ArtistId3],
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Listings {
    account: String,
    date: String,
    songs: Vec<Child>,
    artists: Vec<ArtistId3>,
}

pub struct LibraryIndexStore {
    path: PathBuf,
    current: Mutex<Option<Built>>,
    /// The file is read on the first build rather than at startup: indexing
    /// a large library takes a moment.
    read: AtomicBool,
    /// Held while the index is read or built, so a second caller waits for
    /// that result instead of reading the library again.
    pub building: tauri::async_runtime::Mutex<()>,
    /// Indexes built since startup, so a caller can tell whether one was
    /// built while it waited.
    builds: AtomicU64,
    /// A scan started from Reverb is being watched, to rebuild after it.
    pub watching_scan: AtomicBool,
}

impl LibraryIndexStore {
    pub fn new(data_dir: PathBuf) -> Self {
        Self {
            path: data_dir.join("library_index.json"),
            current: Mutex::new(None),
            read: AtomicBool::new(false),
            building: Default::default(),
            builds: AtomicU64::new(0),
            watching_scan: AtomicBool::new(false),
        }
    }

    /// The index in memory, whichever account and day it is for.
    pub fn current(&self) -> Option<Built> {
        self.current.lock().unwrap().clone()
    }

    pub fn builds(&self) -> u64 {
        self.builds.load(Ordering::SeqCst)
    }

    /// Read the saved index the first time this is called, and make it
    /// current. Later calls, or a missing or unreadable file, read nothing.
    /// Blocking.
    pub fn read_once(&self) -> Option<Built> {
        if self.read.swap(true, Ordering::SeqCst) {
            return None;
        }
        let saved = fs::read(&self.path)
            .ok()
            .and_then(|raw| serde_json::from_slice::<Listings>(&raw).ok())?;
        let built = Built {
            account: saved.account,
            date: saved.date,
            index: Arc::new(LibraryIndex::new(saved.songs, saved.artists)),
        };
        *self.current.lock().unwrap() = Some(built.clone());
        Some(built)
    }

    /// Save the listings of `account` on `date`, index them and make that
    /// index current. Blocking.
    pub fn save(
        &self,
        account: String,
        date: String,
        songs: Vec<Child>,
        artists: Vec<ArtistId3>,
    ) -> Result<Built, ReverbError> {
        let listings =
            ListingsRef { account: &account, date: &date, songs: &songs, artists: &artists };
        if let Some(dir) = self.path.parent() {
            fs::create_dir_all(dir)?;
        }
        let tmp = self.path.with_extension("json.tmp");
        let mut out = BufWriter::new(fs::File::create(&tmp)?);
        serde_json::to_writer(&mut out, &listings).map_err(|e| ReverbError::Io(e.to_string()))?;
        out.flush()?;
        drop(out);
        fs::rename(tmp, &self.path)?;

        let built = Built { account, date, index: Arc::new(LibraryIndex::new(songs, artists)) };
        self.read.store(true, Ordering::SeqCst);
        *self.current.lock().unwrap() = Some(built.clone());
        self.builds.fetch_add(1, Ordering::SeqCst);
        Ok(built)
    }
}
