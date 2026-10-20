//! "Made for you" mixes (see `crate::mixes`). Making them reads the whole
//! library listing, so a set is cached and only rebuilt for a new day, a
//! different account, or on request.

use std::collections::HashMap;
use std::sync::Arc;

use chrono::Local;
use reverb_core::subsonic::client::SongListRequest;
use reverb_core::subsonic::models::Child;
use reverb_core::subsonic::{SubsonicClient, SubsonicError};
use tauri::{AppHandle, Emitter, Manager};

use crate::error::ReverbError;
use crate::mixes::compute::{build_profile, generate, Mix, MixKind};
use crate::mixes::store::MixSet;
use crate::state::AppState;

const LIBRARY_PAGE: u32 = 1000;
/// Past this, the library listing stops (and mixes come from what was read).
const MAX_LIBRARY_SONGS: u32 = 250_000;
/// Favourite artists whose similar artists get looked up.
const SIMILAR_SEEDS: usize = 12;

#[tauri::command]
pub async fn mixes_get(refresh: bool, app: AppHandle) -> Result<Vec<Mix>, ReverbError> {
    Ok(ensure_mixes(&app, refresh).await?.0)
}

/// Build the day's mixes in the background, before Home asks for them, and
/// tell the UI when a new set replaced the one it has.
pub async fn warm_up(app: AppHandle) {
    match ensure_mixes(&app, false).await {
        Ok((_, true)) => {
            let _ = app.emit("mixes:updated", ());
        }
        Ok(_) => {}
        Err(e) => log::debug!("mixes not generated: {e}"),
    }
}

/// The current mixes, generated first when the cached set is out of date or
/// `refresh` asks for new picks. The flag says whether a new set was made.
async fn ensure_mixes(app: &AppHandle, refresh: bool) -> Result<(Vec<Mix>, bool), ReverbError> {
    let state = app.state::<AppState>();
    let _generating = state.mixes.generation.lock().await;

    let creds = state.subsonic.credentials().ok_or(SubsonicError::NotConfigured)?;
    let account = format!("{}@{}", creds.username, creds.server_url);
    let now = Local::now();
    let date = now.format("%Y-%m-%d").to_string();
    let week = now.format("%G-W%V").to_string();
    let has_history = state.stats.config().lastfm.is_some_and(|c| c.track_count > 0);

    let cached = state
        .mixes
        .current()
        .filter(|set| set.account == account && set.has_history == has_history);
    if let Some(set) = cached.as_ref().filter(|set| !refresh && set.date == date) {
        return Ok((set.mixes.clone(), false));
    }

    let songs = fetch_library(&state.subsonic).await?;
    let scrobbles = state.stats.all_scrobbles()?;
    let profile = build_profile(songs, &scrobbles, now.timestamp());
    let (library, unplayed) = profile.coverage();
    log::info!(
        "mixes: {library} songs ({unplayed} never played), {} of {} scrobbles matched",
        profile.matched_scrobbles,
        scrobbles.len()
    );
    let similar = fetch_similar(&state.subsonic, profile.top_artist_ids(SIMILAR_SEEDS)).await;
    let seed = now.timestamp_nanos_opt().unwrap_or_else(|| now.timestamp()) as u64;
    let mut mixes = generate(&profile, &similar, now.timestamp(), seed);

    // Discover holds for the week. Rebuilt daily, the songs played from it
    // would drop out the next morning for no longer being new.
    let kept = cached
        .filter(|set| !refresh && set.week == week)
        .and_then(|set| set.mixes.into_iter().find(|m| m.kind == MixKind::Discover));
    if let Some(discover) = kept {
        mixes.retain(|m| m.kind != MixKind::Discover);
        let at = mixes.iter().take_while(|m| m.kind == MixKind::Daily).count();
        mixes.insert(at, discover);
    }

    state.mixes.save(MixSet { account, date, week, has_history, mixes: mixes.clone() })?;
    Ok((mixes, true))
}

/// Every song with its play stats, page by page through the song listing
/// (Navidrome's native API, or search3 on other servers).
async fn fetch_library(client: &SubsonicClient) -> Result<Vec<Child>, ReverbError> {
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

/// Similar artists for each seed, by name (from Navidrome's getArtistInfo2,
/// which needs its Last.fm agent). The lookups run side by side; a failed one
/// just means no similar artists for that seed.
async fn fetch_similar(
    client: &Arc<SubsonicClient>,
    seeds: Vec<(String, String)>,
) -> HashMap<String, Vec<String>> {
    let lookups: Vec<_> = seeds
        .into_iter()
        .map(|(artist, id)| {
            let client = client.clone();
            tauri::async_runtime::spawn(async move {
                let info = client.get_artist_info2(&id).await.ok()?;
                Some((artist, info.similar_artist.into_iter().map(|a| a.name).collect()))
            })
        })
        .collect();
    let mut similar = HashMap::new();
    for lookup in lookups {
        if let Ok(Some((artist, names))) = lookup.await {
            similar.insert(artist, names);
        }
    }
    similar
}
