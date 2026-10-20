//! Thin typed proxies over the Subsonic client — the frontend never talks
//! HTTP to Navidrome directly (no CORS, no credentials in the webview).

use std::collections::HashSet;

use reverb_core::subsonic::client::{SongListRequest, StarKind};
use reverb_core::subsonic::models::*;
use tauri::State;

use crate::error::ReverbError;
use crate::state::AppState;

// The Subsonic album list genuinely has this many independent filters.
#[allow(clippy::too_many_arguments)]
#[tauri::command]
pub async fn library_get_albums(
    list_type: String,
    size: u32,
    offset: u32,
    query: Option<String>,
    genre: Option<String>,
    from_year: Option<i32>,
    to_year: Option<i32>,
    state: State<'_, AppState>,
) -> Result<Vec<AlbumId3>, ReverbError> {
    // A text filter can only be expressed by search3, which ignores the list
    // type, genre and year range — the frontend disables those while filtering.
    if let Some(query) = query.as_deref().filter(|q| !q.is_empty()) {
        return Ok(state.subsonic.search_albums(query, size, offset).await?);
    }
    Ok(state
        .subsonic
        .get_album_list2(&list_type, size, offset, genre.as_deref(), from_year, to_year)
        .await?)
}

#[tauri::command]
pub async fn library_get_genres(state: State<'_, AppState>) -> Result<Vec<Genre>, ReverbError> {
    Ok(state.subsonic.get_genres().await?)
}

#[tauri::command]
pub async fn library_get_album(
    id: String,
    state: State<'_, AppState>,
) -> Result<AlbumWithSongs, ReverbError> {
    Ok(state.subsonic.get_album(&id).await?)
}

#[tauri::command]
pub async fn library_get_artists(
    state: State<'_, AppState>,
) -> Result<ArtistsIndexes, ReverbError> {
    Ok(state.subsonic.get_artists().await?)
}

#[tauri::command]
pub async fn library_get_artist(
    id: String,
    state: State<'_, AppState>,
) -> Result<ArtistWithAlbums, ReverbError> {
    Ok(state.subsonic.get_artist(&id).await?)
}

#[tauri::command]
pub async fn library_get_artist_appearances(
    id: String,
    state: State<'_, AppState>,
) -> Result<Vec<AlbumId3>, ReverbError> {
    Ok(state.subsonic.get_artist_appearances(&id).await?)
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ArtistInfo {
    pub biography: Option<String>,
    pub last_fm_url: Option<String>,
    pub similar_artists: Vec<ArtistId3>,
}

/// Strip HTML tags and Last.fm boilerplate from a biography.
fn clean_biography(raw: &str) -> Option<String> {
    let mut text = String::with_capacity(raw.len());
    let mut in_tag = false;
    for c in raw.chars() {
        match c {
            '<' => in_tag = true,
            '>' => in_tag = false,
            c if !in_tag => text.push(c),
            _ => {}
        }
    }
    let text = text
        .replace("&amp;", "&")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&lt;", "<")
        .replace("&gt;", ">");
    let mut text = text.trim().to_owned();
    for marker in ["Read more on Last.fm", "User-contributed text"] {
        if let Some(at) = text.find(marker) {
            text.truncate(at);
        }
    }
    let text = text.trim().trim_end_matches(['.', ' ']).to_owned();
    (!text.is_empty()).then(|| format!("{text}."))
}

#[tauri::command]
pub async fn library_get_artist_songs(
    id: String,
    state: State<'_, AppState>,
) -> Result<Vec<Child>, ReverbError> {
    Ok(state.subsonic.get_artist_songs(&id).await?)
}

#[tauri::command]
pub async fn library_set_rating(
    id: String,
    rating: u32,
    state: State<'_, AppState>,
) -> Result<(), ReverbError> {
    Ok(state.subsonic.set_rating(&id, rating).await?)
}

#[tauri::command]
pub async fn library_get_music_folders(
    state: State<'_, AppState>,
) -> Result<Vec<MusicFolder>, ReverbError> {
    Ok(state.subsonic.get_music_folders().await?)
}

#[tauri::command]
pub async fn library_get_indexes(
    music_folder_id: Option<String>,
    state: State<'_, AppState>,
) -> Result<Indexes, ReverbError> {
    Ok(state.subsonic.get_indexes(music_folder_id.as_deref()).await?)
}

#[tauri::command]
pub async fn library_get_music_directory(
    id: String,
    state: State<'_, AppState>,
) -> Result<Directory, ReverbError> {
    Ok(state.subsonic.get_music_directory(&id).await?)
}

#[tauri::command]
pub async fn library_get_artist_info(
    id: String,
    name: String,
    state: State<'_, AppState>,
) -> Result<ArtistInfo, ReverbError> {
    // Navidrome first (its Last.fm agent supplies biographies).
    let nav = state.subsonic.get_artist_info2(&id).await.unwrap_or_default();
    let mut biography = nav.biography.as_deref().and_then(clean_biography);
    let mut last_fm_url = nav.last_fm_url.clone();

    // Fall back to the user's Last.fm key (from stats sources) when the
    // server has no biography.
    if biography.is_none() {
        if let Some(config) = state.stats.config().lastfm {
            let client = reqwest::Client::new();
            if let Ok(Some((raw, url))) =
                crate::stats::lastfm::fetch_artist_bio(&client, &name, &config.api_key).await
            {
                biography = clean_biography(&raw);
                if last_fm_url.is_none() {
                    last_fm_url = url;
                }
            }
        }
    }

    Ok(ArtistInfo { biography, last_fm_url, similar_artists: nav.similar_artist })
}

#[tauri::command]
pub async fn library_get_top_songs(
    artist_name: String,
    count: u32,
    state: State<'_, AppState>,
) -> Result<Vec<Child>, ReverbError> {
    Ok(state.subsonic.get_top_songs(&artist_name, count).await?)
}

#[tauri::command]
pub async fn library_get_playlists(
    state: State<'_, AppState>,
) -> Result<Vec<Playlist>, ReverbError> {
    Ok(state.subsonic.get_playlists().await?)
}

#[tauri::command]
pub async fn library_get_playlist(
    id: String,
    state: State<'_, AppState>,
) -> Result<PlaylistWithSongs, ReverbError> {
    Ok(state.subsonic.get_playlist(&id).await?)
}

#[tauri::command]
pub async fn library_get_starred(state: State<'_, AppState>) -> Result<Starred2, ReverbError> {
    Ok(state.subsonic.get_starred2().await?)
}

#[tauri::command]
pub async fn library_set_star(
    id: String,
    kind: StarKind,
    starred: bool,
    state: State<'_, AppState>,
) -> Result<(), ReverbError> {
    Ok(state.subsonic.set_star(&id, kind, starred).await?)
}

#[tauri::command]
pub async fn library_get_songs(
    size: u32,
    offset: u32,
    query: Option<String>,
    genre: Option<String>,
    sort: Option<String>,
    descending: Option<bool>,
    state: State<'_, AppState>,
) -> Result<SongPage, ReverbError> {
    Ok(state
        .subsonic
        .get_songs_sorted(&SongListRequest {
            query: query.as_deref(),
            genre: genre.as_deref(),
            sort: sort.as_deref(),
            descending: descending.unwrap_or(false),
            size,
            offset,
        })
        .await?)
}

#[tauri::command]
pub async fn library_create_playlist(
    name: String,
    song_ids: Vec<String>,
    state: State<'_, AppState>,
) -> Result<PlaylistWithSongs, ReverbError> {
    Ok(state.subsonic.create_playlist(&name, &song_ids).await?)
}

#[tauri::command]
pub async fn library_update_playlist(
    playlist_id: String,
    name: Option<String>,
    comment: Option<String>,
    add_song_ids: Option<Vec<String>>,
    remove_indices: Option<Vec<u32>>,
    state: State<'_, AppState>,
) -> Result<(), ReverbError> {
    Ok(state
        .subsonic
        .update_playlist(
            &playlist_id,
            name.as_deref(),
            comment.as_deref(),
            &add_song_ids.unwrap_or_default(),
            &remove_indices.unwrap_or_default(),
        )
        .await?)
}

#[tauri::command]
pub async fn library_delete_playlist(
    id: String,
    state: State<'_, AppState>,
) -> Result<(), ReverbError> {
    Ok(state.subsonic.delete_playlist(&id).await?)
}

#[tauri::command]
pub async fn library_get_lyrics(
    id: String,
    state: State<'_, AppState>,
) -> Result<LyricsList, ReverbError> {
    Ok(state.subsonic.get_lyrics(&id).await?)
}

#[tauri::command]
pub async fn library_get_similar_songs(
    id: String,
    count: u32,
    state: State<'_, AppState>,
) -> Result<Vec<Child>, ReverbError> {
    Ok(state.subsonic.get_similar_songs(&id, count).await?)
}

/// Songs to follow `id` in a radio: the server's similar songs (Navidrome
/// needs its Last.fm agent for those), topped up with random songs from
/// `genre` so a radio still plays without one.
#[tauri::command]
pub async fn library_get_radio(
    id: String,
    genre: Option<String>,
    count: u32,
    state: State<'_, AppState>,
) -> Result<Vec<Child>, ReverbError> {
    let similar = state.subsonic.get_similar_songs(&id, count).await.unwrap_or_else(|e| {
        log::debug!("no similar songs for {id}: {e}");
        Vec::new()
    });
    let mut seen = HashSet::from([id]);
    // Nothing rated one star, and nothing as short as an intro or a skit.
    let fits = |song: &Child| {
        song.user_rating != Some(1) && song.duration.is_none_or(|secs| secs >= 60)
    };
    let mut songs: Vec<Child> =
        similar.into_iter().filter(|s| fits(s) && seen.insert(s.id.clone())).collect();
    let missing = (count as usize).saturating_sub(songs.len());
    if missing > 0 {
        let random = state.subsonic.get_random_songs(count, genre.as_deref()).await?;
        songs.extend(
            random.into_iter().filter(|s| fits(s) && seen.insert(s.id.clone())).take(missing),
        );
    }
    Ok(songs)
}

#[tauri::command]
pub async fn library_get_play_queue(
    state: State<'_, AppState>,
) -> Result<Option<PlayQueue>, ReverbError> {
    Ok(state.subsonic.get_play_queue().await?)
}

#[tauri::command]
pub async fn library_save_play_queue(
    ids: Vec<String>,
    current: Option<String>,
    position_ms: u64,
    state: State<'_, AppState>,
) -> Result<(), ReverbError> {
    Ok(state.subsonic.save_play_queue(&ids, current.as_deref(), position_ms).await?)
}

#[tauri::command]
pub async fn library_start_scan(state: State<'_, AppState>) -> Result<ScanStatus, ReverbError> {
    Ok(state.subsonic.start_scan().await?)
}

#[tauri::command]
pub async fn library_get_scan_status(
    state: State<'_, AppState>,
) -> Result<ScanStatus, ReverbError> {
    Ok(state.subsonic.get_scan_status().await?)
}

#[tauri::command]
pub async fn library_search(
    query: String,
    artist_count: u32,
    album_count: u32,
    song_count: u32,
    state: State<'_, AppState>,
) -> Result<SearchResult3, ReverbError> {
    Ok(state.subsonic.search3(&query, artist_count, album_count, song_count).await?)
}
