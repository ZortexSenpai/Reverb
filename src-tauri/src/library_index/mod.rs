//! The Library index: the signed-in account's library Songs, Albums and
//! Artists, indexed so Scrobbles find their Library match by name. Mixes
//! read it, and so do stats.
//!
//! It's built from the account's song listing and artist listing, kept on
//! disk, and rebuilt once a day, after a scan started from Reverb finishes,
//! and for another account. Each new index is announced to the UI.

pub mod index;
pub mod matcher;
pub mod store;

use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::Duration;

use chrono::Local;
use reverb_core::subsonic::client::SongListRequest;
use reverb_core::subsonic::models::{ArtistId3, Child};
use reverb_core::subsonic::{SubsonicClient, SubsonicError};
use tauri::{AppHandle, Emitter, Manager};

use self::index::LibraryIndex;
use crate::error::ReverbError;
use crate::state::AppState;

/// Told to the UI whenever a new index is current.
pub const READY_EVENT: &str = "library:index-ready";

const LIBRARY_PAGE: u32 = 1000;
/// Past this, the song listing stops (and the index holds what was read).
const MAX_LIBRARY_SONGS: u32 = 250_000;
/// How often a scan started from Reverb is checked on.
const SCAN_POLL: Duration = Duration::from_secs(2);

/// How fresh an index a caller needs.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Need {
    /// Any built today.
    Today,
    /// A new one, though one already being built when asked will do.
    Fresh,
    /// One built from listings read after asking.
    Rebuilt,
}

/// The signed-in account's index for today, built first when there's none.
/// A caller arriving during a build waits for that build.
pub async fn get(app: &AppHandle) -> Result<Arc<LibraryIndex>, ReverbError> {
    ensure(app, Need::Today).await
}

/// A newly built index of the signed-in account, so it holds the stars,
/// ratings and play counts of the moment. A caller arriving during a build
/// shares that build.
pub async fn refresh(app: &AppHandle) -> Result<Arc<LibraryIndex>, ReverbError> {
    ensure(app, Need::Fresh).await
}

/// The signed-in account's index as it stands, without waiting: `None`
/// until its first one is ready. When it's missing or from an earlier day, a
/// build starts in the background and [`READY_EVENT`] tells when it's done.
pub fn current(app: &AppHandle) -> Option<Arc<LibraryIndex>> {
    let state = app.state::<AppState>();
    let account = state.account()?;
    let built = state.library_index.current().filter(|built| built.account == account);
    let stale = built.as_ref().is_none_or(|built| built.date != today());
    if stale && state.library_index.building.try_lock().is_ok() {
        tauri::async_runtime::spawn(warm_up(app.clone()));
    }
    built.map(|built| built.index)
}

/// Make sure the index is current, in the background: at startup, and when
/// the account changes.
pub async fn warm_up(app: AppHandle) {
    if let Err(e) = ensure(&app, Need::Today).await {
        log::debug!("library index not built: {e}");
    }
}

/// Rebuild the index once the scan just started from Reverb finishes.
pub fn rebuild_after_scan(app: AppHandle) {
    if app.state::<AppState>().library_index.watching_scan.swap(true, Ordering::SeqCst) {
        return;
    }
    tauri::async_runtime::spawn(async move {
        let finished = loop {
            pause(SCAN_POLL).await;
            match app.state::<AppState>().subsonic.get_scan_status().await {
                Ok(status) if status.scanning => {}
                Ok(_) => break true,
                Err(e) => {
                    log::debug!("lost track of the library scan: {e}");
                    break false;
                }
            }
        };
        app.state::<AppState>().library_index.watching_scan.store(false, Ordering::SeqCst);
        if finished {
            // A build already under way may have read the library mid-scan.
            if let Err(e) = ensure(&app, Need::Rebuilt).await {
                log::debug!("library index not rebuilt after the scan: {e}");
            }
        }
    });
}

/// The current index, built anew when the one at hand is for another
/// account, from an earlier day, or not as fresh as `need` asks.
async fn ensure(app: &AppHandle, need: Need) -> Result<Arc<LibraryIndex>, ReverbError> {
    let state = app.state::<AppState>();
    let builds_before = state.library_index.builds();
    let _building = state.library_index.building.lock().await;
    let account = state.account().ok_or(SubsonicError::NotConfigured)?;
    let date = today();

    let handle = app.clone();
    let saved = blocking(move || handle.state::<AppState>().library_index.read_once()).await?;
    if saved.is_some_and(|saved| saved.account == account) {
        announce(app);
    }
    // A build that finished while this call waited was under way when it
    // arrived.
    let shared_build = state.library_index.builds() != builds_before;
    let fresh_enough = match need {
        Need::Today => true,
        Need::Fresh => shared_build,
        Need::Rebuilt => false,
    };
    let current = state.library_index.current().filter(|built| built.account == account);
    if let Some(built) = current.filter(|built| fresh_enough && built.date == date) {
        return Ok(built.index);
    }

    let songs = fetch_songs(&state.subsonic).await?;
    let artists = fetch_artists(&state.subsonic).await?;
    let handle = app.clone();
    let built = blocking(move || {
        handle.state::<AppState>().library_index.save(account, date, songs, artists)
    })
    .await??;
    log::info!("library index: {} songs", built.index.songs().len());
    announce(app);
    Ok(built.index)
}

fn announce(app: &AppHandle) {
    let _ = app.emit(READY_EVENT, ());
}

fn today() -> String {
    Local::now().format("%Y-%m-%d").to_string()
}

/// Run `work` off the async threads.
async fn blocking<T: Send + 'static>(
    work: impl FnOnce() -> T + Send + 'static,
) -> Result<T, ReverbError> {
    tauri::async_runtime::spawn_blocking(work).await.map_err(|e| ReverbError::State(e.to_string()))
}

async fn pause(duration: Duration) {
    let _ = blocking(move || std::thread::sleep(duration)).await;
}

/// Every song with its play stats, page by page through the song listing
/// (Navidrome's native API, or search3 on other servers).
async fn fetch_songs(client: &SubsonicClient) -> Result<Vec<Child>, ReverbError> {
    let mut songs = Vec::new();
    let mut offset = 0;
    while offset < MAX_LIBRARY_SONGS {
        let page = client
            .get_songs_sorted(&SongListRequest { size: LIBRARY_PAGE, offset, ..Default::default() })
            .await?;
        let count = page.songs.len() as u32;
        songs.extend(page.songs);
        offset += LIBRARY_PAGE;
        if count < LIBRARY_PAGE || page.total.is_some_and(|total| u64::from(offset) >= total) {
            break;
        }
    }
    Ok(songs)
}

/// Every artist in the artist listing, with its art.
async fn fetch_artists(client: &SubsonicClient) -> Result<Vec<ArtistId3>, ReverbError> {
    let listing = client.get_artists().await?;
    Ok(listing.index.into_iter().flat_map(|group| group.artist).collect())
}

/// Library Songs and Artists built by hand, as the listings send them.
#[cfg(test)]
pub mod testing {
    use reverb_core::subsonic::models::{ArtistId3, Child};

    /// A library Song credited to `artist` (id `ar-<artist>`), with nothing
    /// else set; fill in any other field on the result.
    pub fn song(id: &str, artist: &str, title: &str) -> Child {
        serde_json::from_value(serde_json::json!({
            "id": id,
            "title": title,
            "artist": artist,
            "artistId": format!("ar-{artist}"),
        }))
        .unwrap()
    }

    /// `song` on album `id`, with that album's cover, as Navidrome lists it.
    pub fn on_album(mut song: Child, id: &str, name: &str) -> Child {
        song.album_id = Some(id.to_owned());
        song.album = Some(name.to_owned());
        song.cover_art = Some(format!("al-{id}"));
        song
    }

    /// An artist as the artist listing sends it, with art.
    pub fn artist(id: &str, name: &str) -> ArtistId3 {
        ArtistId3 {
            id: id.to_owned(),
            name: name.to_owned(),
            cover_art: Some(format!("ar-{id}")),
            album_count: None,
            artist_image_url: None,
            starred: None,
        }
    }
}
