//! Async Subsonic client used by the Tauri command layer, plus synchronous
//! URL building for the audio engine (which does its own blocking HTTP).

use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::RwLock;
use std::time::Duration;

use reqwest::Url;
use serde::Deserialize;

use super::auth;
use super::models::*;

#[derive(Debug, Clone)]
pub struct Credentials {
    /// Normalized to always end with a trailing slash.
    pub server_url: Url,
    pub username: String,
    pub password: String,
}

impl Credentials {
    pub fn new(server_url: &str, username: &str, password: &str) -> Result<Self, SubsonicError> {
        let mut raw = server_url.trim().to_owned();
        if !raw.starts_with("http://") && !raw.starts_with("https://") {
            raw = format!("https://{raw}");
        }
        if !raw.ends_with('/') {
            raw.push('/');
        }
        let server_url =
            Url::parse(&raw).map_err(|e| SubsonicError::BadUrl(format!("{raw}: {e}")))?;
        Ok(Self { server_url, username: username.to_owned(), password: password.to_owned() })
    }

    /// Full URL for an endpoint including fresh auth params.
    pub fn endpoint_url(&self, endpoint: &str, params: &[(&str, String)]) -> Url {
        let mut url = self
            .server_url
            .join(&format!("rest/{endpoint}"))
            .expect("endpoint join cannot fail on a normalized base");
        {
            let mut qp = url.query_pairs_mut();
            for (k, v) in auth::auth_params(&self.username, &self.password) {
                qp.append_pair(k, &v);
            }
            for (k, v) in params {
                qp.append_pair(k, v);
            }
        }
        url
    }
}

#[derive(Debug, thiserror::Error)]
pub enum SubsonicError {
    #[error("no server configured")]
    NotConfigured,
    #[error("invalid server URL: {0}")]
    BadUrl(String),
    #[error("network error: {0}")]
    Http(#[from] reqwest::Error),
    #[error("server error {code}: {message}")]
    Api { code: i32, message: String },
    #[error("unexpected response: {0}")]
    Parse(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum StarKind {
    Song,
    Album,
    Artist,
}

/// Parameters for a sorted/filtered song listing.
#[derive(Debug, Clone, Copy, Default)]
pub struct SongListRequest<'a> {
    pub query: Option<&'a str>,
    pub genre: Option<&'a str>,
    /// Native sort field ("title", "playCount", ...); None = title order.
    pub sort: Option<&'a str>,
    pub descending: bool,
    pub size: u32,
    pub offset: u32,
}

pub struct SubsonicClient {
    http: reqwest::Client,
    creds: RwLock<Option<Credentials>>,
    /// JWT for Navidrome's native REST API (used for sorted song listings).
    native_token: RwLock<Option<String>>,
    /// Genre name → native genre id cache.
    native_genres: RwLock<Option<HashMap<String, String>>>,
    /// Set after the first reportPlayback failure (extension unsupported).
    playback_report_unsupported: AtomicBool,
}

impl Default for SubsonicClient {
    fn default() -> Self {
        Self::new()
    }
}

impl SubsonicClient {
    pub fn new() -> Self {
        let http = reqwest::Client::builder()
            .connect_timeout(Duration::from_secs(10))
            .timeout(Duration::from_secs(30))
            .user_agent(format!("{}/{}", auth::CLIENT_NAME, env!("CARGO_PKG_VERSION")))
            .build()
            .expect("reqwest client construction cannot fail with static config");
        Self {
            http,
            creds: RwLock::new(None),
            native_token: RwLock::new(None),
            native_genres: RwLock::new(None),
            playback_report_unsupported: AtomicBool::new(false),
        }
    }

    pub fn set_credentials(&self, creds: Option<Credentials>) {
        *self.creds.write().unwrap() = creds;
        *self.native_token.write().unwrap() = None;
        *self.native_genres.write().unwrap() = None;
        self.playback_report_unsupported.store(false, Ordering::Relaxed);
    }

    pub fn credentials(&self) -> Option<Credentials> {
        self.creds.read().unwrap().clone()
    }

    fn require_creds(&self) -> Result<Credentials, SubsonicError> {
        self.credentials().ok_or(SubsonicError::NotConfigured)
    }

    async fn call_with(
        &self,
        creds: &Credentials,
        endpoint: &str,
        params: &[(&str, String)],
    ) -> Result<ResponseBody, SubsonicError> {
        let url = creds.endpoint_url(endpoint, params);
        let resp = self.http.get(url).send().await?.error_for_status()?;
        Self::unwrap_envelope(resp, endpoint).await
    }

    /// [`Self::call`] with the parameters in a form body (OpenSubsonic
    /// `formPost`) instead of the URL — for requests a reverse proxy would
    /// reject as too long, like a play queue of a few hundred song ids.
    async fn call_form(
        &self,
        endpoint: &str,
        params: &[(&str, String)],
    ) -> Result<ResponseBody, SubsonicError> {
        let creds = self.require_creds()?;
        let url = creds
            .server_url
            .join(&format!("rest/{endpoint}"))
            .expect("endpoint join cannot fail on a normalized base");
        let mut form = auth::auth_params(&creds.username, &creds.password);
        form.extend(params.iter().cloned());
        let resp = self.http.post(url).form(&form).send().await?.error_for_status()?;
        Self::unwrap_envelope(resp, endpoint).await
    }

    async fn unwrap_envelope(
        resp: reqwest::Response,
        endpoint: &str,
    ) -> Result<ResponseBody, SubsonicError> {
        let env: Envelope = resp
            .json()
            .await
            .map_err(|e| SubsonicError::Parse(format!("{endpoint}: {e}")))?;
        let body = env.inner;
        if body.status != "ok" {
            let err = body.error.unwrap_or(ApiError { code: -1, message: "unknown error".into() });
            return Err(SubsonicError::Api { code: err.code, message: err.message });
        }
        Ok(body)
    }

    async fn call(
        &self,
        endpoint: &str,
        params: &[(&str, String)],
    ) -> Result<ResponseBody, SubsonicError> {
        let creds = self.require_creds()?;
        self.call_with(&creds, endpoint, params).await
    }

    /// Ping with explicit credentials (used by "Test connection" before saving).
    pub async fn ping_with(&self, creds: &Credentials) -> Result<ServerInfo, SubsonicError> {
        let body = self.call_with(creds, "ping", &[]).await?;
        Ok(ServerInfo {
            version: body.version,
            server_type: body.server_type,
            server_version: body.server_version,
            open_subsonic: body.open_subsonic.unwrap_or(false),
        })
    }

    pub async fn ping(&self) -> Result<ServerInfo, SubsonicError> {
        let creds = self.require_creds()?;
        self.ping_with(&creds).await
    }

    /// `genre` is required by the server for `type=byGenre`; `from_year`/
    /// `to_year` for `type=byYear` (reversed range = newest first).
    pub async fn get_album_list2(
        &self,
        list_type: &str,
        size: u32,
        offset: u32,
        genre: Option<&str>,
        from_year: Option<i32>,
        to_year: Option<i32>,
    ) -> Result<Vec<AlbumId3>, SubsonicError> {
        let mut params = vec![
            ("type", list_type.to_owned()),
            ("size", size.to_string()),
            ("offset", offset.to_string()),
        ];
        if let Some(g) = genre {
            params.push(("genre", g.to_owned()));
        }
        if let Some(y) = from_year {
            params.push(("fromYear", y.to_string()));
        }
        if let Some(y) = to_year {
            params.push(("toYear", y.to_string()));
        }
        let body = self.call("getAlbumList2", &params).await?;
        Ok(body.album_list2.map(|l| l.album).unwrap_or_default())
    }

    pub async fn get_genres(&self) -> Result<Vec<Genre>, SubsonicError> {
        let body = self.call("getGenres", &[]).await?;
        Ok(body.genres.map(|g| g.genre).unwrap_or_default())
    }

    pub async fn get_album(&self, id: &str) -> Result<AlbumWithSongs, SubsonicError> {
        let body = self.call("getAlbum", &[("id", id.to_owned())]).await?;
        body.album.ok_or_else(|| SubsonicError::Parse("getAlbum: missing album".into()))
    }

    pub async fn get_song(&self, id: &str) -> Result<Child, SubsonicError> {
        let body = self.call("getSong", &[("id", id.to_owned())]).await?;
        body.song.ok_or_else(|| SubsonicError::Parse("getSong: missing song".into()))
    }

    pub async fn get_artists(&self) -> Result<ArtistsIndexes, SubsonicError> {
        let body = self.call("getArtists", &[]).await?;
        body.artists.ok_or_else(|| SubsonicError::Parse("getArtists: missing artists".into()))
    }

    pub async fn get_artist(&self, id: &str) -> Result<ArtistWithAlbums, SubsonicError> {
        let body = self.call("getArtist", &[("id", id.to_owned())]).await?;
        body.artist.ok_or_else(|| SubsonicError::Parse("getArtist: missing artist".into()))
    }

    /// Artist metadata (biography, similar artists) — Navidrome fills this
    /// from its Last.fm agent when configured.
    pub async fn get_artist_info2(&self, id: &str) -> Result<ArtistInfo2, SubsonicError> {
        let body = self
            .call("getArtistInfo2", &[("id", id.to_owned()), ("count", "12".to_owned())])
            .await?;
        Ok(body.artist_info2.unwrap_or_default())
    }

    pub async fn get_top_songs(
        &self,
        artist_name: &str,
        count: u32,
    ) -> Result<Vec<Child>, SubsonicError> {
        let body = self
            .call(
                "getTopSongs",
                &[("artist", artist_name.to_owned()), ("count", count.to_string())],
            )
            .await?;
        Ok(body.top_songs.map(|t| t.song).unwrap_or_default())
    }

    pub async fn get_playlists(&self) -> Result<Vec<Playlist>, SubsonicError> {
        let body = self.call("getPlaylists", &[]).await?;
        Ok(body.playlists.map(|p| p.playlist).unwrap_or_default())
    }

    pub async fn get_playlist(&self, id: &str) -> Result<PlaylistWithSongs, SubsonicError> {
        let body = self.call("getPlaylist", &[("id", id.to_owned())]).await?;
        body.playlist.ok_or_else(|| SubsonicError::Parse("getPlaylist: missing playlist".into()))
    }

    pub async fn create_playlist(
        &self,
        name: &str,
        song_ids: &[String],
    ) -> Result<PlaylistWithSongs, SubsonicError> {
        let mut params = vec![("name", name.to_owned())];
        for id in song_ids {
            params.push(("songId", id.clone()));
        }
        let body = self.call("createPlaylist", &params).await?;
        body.playlist
            .ok_or_else(|| SubsonicError::Parse("createPlaylist: missing playlist".into()))
    }

    pub async fn update_playlist(
        &self,
        playlist_id: &str,
        name: Option<&str>,
        comment: Option<&str>,
        add_song_ids: &[String],
        remove_indices: &[u32],
    ) -> Result<(), SubsonicError> {
        let mut params = vec![("playlistId", playlist_id.to_owned())];
        if let Some(name) = name {
            params.push(("name", name.to_owned()));
        }
        if let Some(comment) = comment {
            params.push(("comment", comment.to_owned()));
        }
        for id in add_song_ids {
            params.push(("songIdToAdd", id.clone()));
        }
        for index in remove_indices {
            params.push(("songIndexToRemove", index.to_string()));
        }
        self.call("updatePlaylist", &params).await?;
        Ok(())
    }

    pub async fn delete_playlist(&self, id: &str) -> Result<(), SubsonicError> {
        self.call("deletePlaylist", &[("id", id.to_owned())]).await?;
        Ok(())
    }

    /// OpenSubsonic structured (optionally time-synced) lyrics.
    pub async fn get_lyrics(&self, song_id: &str) -> Result<LyricsList, SubsonicError> {
        let body = self.call("getLyricsBySongId", &[("id", song_id.to_owned())]).await?;
        Ok(body.lyrics_list.unwrap_or(LyricsList { structured_lyrics: vec![] }))
    }

    /// Similar songs for radio mode (requires a Last.fm agent server-side).
    pub async fn get_similar_songs(
        &self,
        song_id: &str,
        count: u32,
    ) -> Result<Vec<Child>, SubsonicError> {
        let body = self
            .call(
                "getSimilarSongs2",
                &[("id", song_id.to_owned()), ("count", count.to_string())],
            )
            .await?;
        Ok(body.similar_songs2.map(|s| s.song).unwrap_or_default())
    }

    /// Random songs, optionally from one genre.
    pub async fn get_random_songs(
        &self,
        size: u32,
        genre: Option<&str>,
    ) -> Result<Vec<Child>, SubsonicError> {
        let mut params = vec![("size", size.to_string())];
        if let Some(genre) = genre.filter(|g| !g.is_empty()) {
            params.push(("genre", genre.to_owned()));
        }
        let body = self.call("getRandomSongs", &params).await?;
        Ok(body.random_songs.map(|r| r.song).unwrap_or_default())
    }

    /// The queue last saved to the server by any client; `None` when there is
    /// none.
    pub async fn get_play_queue(&self) -> Result<Option<PlayQueue>, SubsonicError> {
        match self.call("getPlayQueue", &[]).await {
            Ok(body) => Ok(body.play_queue),
            // Some servers report an unsaved queue as "not found".
            Err(SubsonicError::Api { code: 70, .. }) => Ok(None),
            Err(e) => Err(e),
        }
    }

    /// Save the queue for other clients to resume. An empty `ids` clears it.
    pub async fn save_play_queue(
        &self,
        ids: &[String],
        current: Option<&str>,
        position_ms: u64,
    ) -> Result<(), SubsonicError> {
        let params = |ids: &[String]| {
            let mut params: Vec<(&str, String)> = ids.iter().map(|id| ("id", id.clone())).collect();
            if let Some(current) = current {
                params.push(("current", current.to_owned()));
                params.push(("position", position_ms.to_string()));
            }
            params
        };
        let Err(e) = self.call_form("savePlayQueue", &params(ids)).await else {
            return Ok(());
        };
        // A server without form posts: fall back to the URL, with only the
        // stretch around the current song, so it stays short enough to pass.
        log::debug!("savePlayQueue as a form post failed ({e}); retrying in the URL");
        const URL_WINDOW: usize = 200;
        let at = current.and_then(|c| ids.iter().position(|id| id == c)).unwrap_or(0);
        let start = at.saturating_sub(URL_WINDOW / 4).min(ids.len().saturating_sub(URL_WINDOW));
        let window = &ids[start..(start + URL_WINDOW).min(ids.len())];
        self.call("savePlayQueue", &params(window)).await?;
        Ok(())
    }

    pub async fn get_starred2(&self) -> Result<Starred2, SubsonicError> {
        let body = self.call("getStarred2", &[]).await?;
        body.starred2.ok_or_else(|| SubsonicError::Parse("getStarred2: missing starred2".into()))
    }

    pub async fn set_star(
        &self,
        id: &str,
        kind: StarKind,
        starred: bool,
    ) -> Result<(), SubsonicError> {
        let endpoint = if starred { "star" } else { "unstar" };
        let key = match kind {
            StarKind::Song => "id",
            StarKind::Album => "albumId",
            StarKind::Artist => "artistId",
        };
        self.call(endpoint, &[(key, id.to_owned())]).await?;
        Ok(())
    }

    /// Set this user's 1–5 rating for any media id; `0` clears it.
    pub async fn set_rating(&self, id: &str, rating: u32) -> Result<(), SubsonicError> {
        self.call(
            "setRating",
            &[("id", id.to_owned()), ("rating", rating.min(5).to_string())],
        )
        .await?;
        Ok(())
    }

    pub async fn get_music_folders(&self) -> Result<Vec<MusicFolder>, SubsonicError> {
        let body = self.call("getMusicFolders", &[]).await?;
        Ok(body.music_folders.map(|f| f.music_folder).unwrap_or_default())
    }

    /// Top of the folder tree. Without a folder id the server merges every
    /// configured music folder.
    pub async fn get_indexes(
        &self,
        music_folder_id: Option<&str>,
    ) -> Result<Indexes, SubsonicError> {
        let mut params = Vec::new();
        if let Some(id) = music_folder_id.filter(|id| !id.is_empty()) {
            params.push(("musicFolderId", id.to_owned()));
        }
        let body = self.call("getIndexes", &params).await?;
        Ok(body.indexes.unwrap_or(Indexes {
            ignored_articles: None,
            index: vec![],
            child: vec![],
        }))
    }

    pub async fn get_music_directory(&self, id: &str) -> Result<Directory, SubsonicError> {
        let body = self.call("getMusicDirectory", &[("id", id.to_owned())]).await?;
        body.directory
            .ok_or_else(|| SubsonicError::Parse("getMusicDirectory: missing directory".into()))
    }

    /// Every song by an artist, in album order. Subsonic has no endpoint for
    /// this, so it walks the artist's albums (sequentially — artists have few
    /// enough albums that the extra round trips beat holding many in flight).
    pub async fn get_artist_songs(&self, artist_id: &str) -> Result<Vec<Child>, SubsonicError> {
        let artist = self.get_artist(artist_id).await?;
        let mut songs = Vec::new();
        for album in artist.album {
            songs.extend(self.get_album(&album.id).await?.song);
        }
        Ok(songs)
    }

    /// Albums an artist is credited on without being their album artist —
    /// features, compilations — which `getArtist` leaves out. Navidrome's
    /// native API can filter albums by track artist; other servers fall back
    /// to a search.
    pub async fn get_artist_appearances(
        &self,
        artist_id: &str,
    ) -> Result<Vec<AlbumId3>, SubsonicError> {
        let creds = self.require_creds()?;
        let artist = self.get_artist(artist_id).await?;
        // Subtracting what getArtist returned (rather than checking album
        // artists) keeps the two lists disjoint even when the server folds
        // participations into getArtist already.
        let own: HashSet<&str> = artist.album.iter().map(|a| a.id.as_str()).collect();
        let albums = match self.native_appearances(&creds, artist_id).await {
            Ok(albums) => albums,
            Err(e) => {
                log::debug!("native album listing unavailable, searching instead: {e}");
                self.searched_appearances(&artist.artist, &own).await?
            }
        };
        Ok(albums.into_iter().filter(|a| !own.contains(a.id.as_str())).collect())
    }

    /// Every album with a track crediting `artist_id`, their own included.
    async fn native_appearances(
        &self,
        creds: &Credentials,
        artist_id: &str,
    ) -> Result<Vec<AlbumId3>, SubsonicError> {
        let params = [
            ("role_artist_id", artist_id.to_owned()),
            ("missing", "false".to_owned()),
            // getArtist's order, so both sections on the page read alike.
            ("_sort", "max_year".to_owned()),
            ("_order", "ASC".to_owned()),
            ("_start", "0".to_owned()),
            ("_end", "500".to_owned()),
        ];
        let resp = self.native_get(creds, "api/album", &params).await?;
        let native: Vec<NativeAlbum> = resp
            .json()
            .await
            .map_err(|e| SubsonicError::Parse(format!("native albums: {e}")))?;
        Ok(native.into_iter().map(NativeAlbum::into_album_id3).collect())
    }

    /// Subsonic can't list albums by track artist, so search the artist's
    /// name and keep the albums of songs that credit them. Only the first
    /// credited artist is known without the OpenSubsonic `artists` list.
    async fn searched_appearances(
        &self,
        artist: &ArtistId3,
        own: &HashSet<&str>,
    ) -> Result<Vec<AlbumId3>, SubsonicError> {
        let found = self.search3(&artist.name, 0, 0, 500).await?;
        let mut seen = HashSet::new();
        let album_ids: Vec<String> = found
            .song
            .into_iter()
            .filter(|s| {
                s.artist_id.as_deref() == Some(artist.id.as_str())
                    || s.artists.iter().any(|a| a.id == artist.id)
            })
            .filter_map(|s| s.album_id)
            .filter(|id| !own.contains(id.as_str()) && seen.insert(id.clone()))
            .collect();
        let mut albums = Vec::new();
        for id in album_ids {
            albums.push(self.get_album(&id).await?.album);
        }
        Ok(albums)
    }

    pub async fn search3(
        &self,
        query: &str,
        artist_count: u32,
        album_count: u32,
        song_count: u32,
    ) -> Result<SearchResult3, SubsonicError> {
        let body = self
            .call(
                "search3",
                &[
                    ("query", query.to_owned()),
                    ("artistCount", artist_count.to_string()),
                    ("albumCount", album_count.to_string()),
                    ("songCount", song_count.to_string()),
                ],
            )
            .await?;
        Ok(body.search_result3.unwrap_or(SearchResult3 {
            song: vec![],
            album: vec![],
            artist: vec![],
        }))
    }

    /// Paginated album search. getAlbumList2 has no text filter, so the
    /// Albums view's filter routes here instead (search3 paginates albums
    /// with albumOffset, and orders by relevance rather than a list type).
    pub async fn search_albums(
        &self,
        query: &str,
        size: u32,
        offset: u32,
    ) -> Result<Vec<AlbumId3>, SubsonicError> {
        let body = self
            .call(
                "search3",
                &[
                    ("query", query.to_owned()),
                    ("artistCount", "0".to_owned()),
                    ("albumCount", size.to_string()),
                    ("albumOffset", offset.to_string()),
                    ("songCount", "0".to_owned()),
                ],
            )
            .await?;
        Ok(body.search_result3.map(|r| r.album).unwrap_or_default())
    }

    /// Paginated song listing. A genre filter routes to getSongsByGenre;
    /// otherwise search3 is used (OpenSubsonic servers like Navidrome treat
    /// an empty query as match-everything).
    pub async fn get_songs(
        &self,
        query: Option<&str>,
        genre: Option<&str>,
        size: u32,
        offset: u32,
    ) -> Result<Vec<Child>, SubsonicError> {
        if let Some(genre) = genre.filter(|g| !g.is_empty()) {
            let body = self
                .call(
                    "getSongsByGenre",
                    &[
                        ("genre", genre.to_owned()),
                        ("count", size.to_string()),
                        ("offset", offset.to_string()),
                    ],
                )
                .await?;
            return Ok(body.songs_by_genre.map(|r| r.song).unwrap_or_default());
        }
        let body = self
            .call(
                "search3",
                &[
                    ("query", query.unwrap_or_default().to_owned()),
                    ("artistCount", "0".to_owned()),
                    ("albumCount", "0".to_owned()),
                    ("songCount", size.to_string()),
                    ("songOffset", offset.to_string()),
                ],
            )
            .await?;
        Ok(body.search_result3.map(|r| r.song).unwrap_or_default())
    }

    /// `time` is the epoch-ms the song started playing (submissions only).
    pub async fn scrobble(
        &self,
        id: &str,
        submission: bool,
        time_ms: Option<u64>,
    ) -> Result<(), SubsonicError> {
        let mut params = vec![("id", id.to_owned()), ("submission", submission.to_string())];
        if let Some(t) = time_ms {
            params.push(("time", t.to_string()));
        }
        self.call("scrobble", &params).await?;
        Ok(())
    }

    /// OpenSubsonic playbackReport: live position/state for the server's
    /// now-playing display. `state` is "playing" | "paused" | "stopped".
    /// Silently disabled after the first failure (extension unsupported).
    pub async fn report_playback(&self, media_id: &str, position_ms: u64, state: &str) {
        if self.playback_report_unsupported.load(Ordering::Relaxed) {
            return;
        }
        let result = self
            .call(
                "reportPlayback",
                &[
                    ("mediaId", media_id.to_owned()),
                    ("mediaType", "song".to_owned()),
                    ("positionMs", position_ms.to_string()),
                    ("state", state.to_owned()),
                ],
            )
            .await;
        if let Err(e) = result {
            log::debug!("reportPlayback unsupported or failed, disabling: {e}");
            self.playback_report_unsupported.store(true, Ordering::Relaxed);
        }
    }

    /// Fetch cover art bytes (for the reverb-art protocol handler's cache).
    pub async fn get_cover_art(
        &self,
        id: &str,
        size: Option<u32>,
    ) -> Result<(Vec<u8>, String), SubsonicError> {
        let creds = self.require_creds()?;
        let mut params = vec![("id", id.to_owned())];
        if let Some(s) = size {
            params.push(("size", s.to_string()));
        }
        let url = creds.endpoint_url("getCoverArt", &params);
        let resp = self.http.get(url).send().await?.error_for_status()?;
        let content_type = resp
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .unwrap_or("image/jpeg")
            .to_owned();
        // A JSON content type means the server returned a Subsonic error envelope.
        if content_type.contains("json") || content_type.contains("xml") {
            return Err(SubsonicError::Parse(format!("getCoverArt {id}: no image returned")));
        }
        let bytes = resp.bytes().await?;
        Ok((bytes.to_vec(), content_type))
    }

    // ---- Navidrome native REST API ------------------------------------
    // The Subsonic API cannot sort song listings; Navidrome's own API can
    // (and reports totals). Used for the Songs view, with a Subsonic
    // fallback for other servers; also for an artist's appearances.

    async fn native_login(&self, creds: &Credentials) -> Result<String, SubsonicError> {
        #[derive(Deserialize)]
        struct LoginResponse {
            token: String,
        }
        let url = creds
            .server_url
            .join("auth/login")
            .map_err(|e| SubsonicError::BadUrl(e.to_string()))?;
        let resp = self
            .http
            .post(url)
            .json(&serde_json::json!({
                "username": creds.username,
                "password": creds.password,
            }))
            .send()
            .await?
            .error_for_status()?;
        let login: LoginResponse = resp
            .json()
            .await
            .map_err(|e| SubsonicError::Parse(format!("native login: {e}")))?;
        *self.native_token.write().unwrap() = Some(login.token.clone());
        Ok(login.token)
    }

    async fn native_get(
        &self,
        creds: &Credentials,
        path: &str,
        params: &[(&str, String)],
    ) -> Result<reqwest::Response, SubsonicError> {
        for attempt in 0..2 {
            let cached = self.native_token.read().unwrap().clone();
            let token = match cached {
                Some(t) => t,
                None => self.native_login(creds).await?,
            };
            let url = creds
                .server_url
                .join(path)
                .map_err(|e| SubsonicError::BadUrl(e.to_string()))?;
            let resp = self
                .http
                .get(url)
                .header("x-nd-authorization", format!("Bearer {token}"))
                .query(params)
                .send()
                .await?;
            // Expired token: re-login once and retry.
            if resp.status() == reqwest::StatusCode::UNAUTHORIZED && attempt == 0 {
                *self.native_token.write().unwrap() = None;
                continue;
            }
            return Ok(resp.error_for_status()?);
        }
        unreachable!("loop returns on second attempt");
    }

    async fn native_genre_id(
        &self,
        creds: &Credentials,
        name: &str,
    ) -> Result<Option<String>, SubsonicError> {
        #[derive(Deserialize)]
        struct NativeGenre {
            id: String,
            name: String,
        }
        let cache_empty = self.native_genres.read().unwrap().is_none();
        if cache_empty {
            let resp = self
                .native_get(
                    creds,
                    "api/genre",
                    &[("_start", "0".into()), ("_end", "5000".into()), ("_sort", "name".into())],
                )
                .await?;
            let genres: Vec<NativeGenre> = resp
                .json()
                .await
                .map_err(|e| SubsonicError::Parse(format!("native genres: {e}")))?;
            let map = genres.into_iter().map(|g| (g.name.to_lowercase(), g.id)).collect();
            *self.native_genres.write().unwrap() = Some(map);
        }
        Ok(self
            .native_genres
            .read()
            .unwrap()
            .as_ref()
            .and_then(|m| m.get(&name.to_lowercase()).cloned()))
    }

    /// Sorted, filtered, paginated song listing via the native API.
    pub async fn get_songs_sorted(&self, request: &SongListRequest<'_>) -> Result<SongPage, SubsonicError> {
        let creds = self.require_creds()?;
        match self.native_song_page(&creds, request).await {
            Ok(page) => Ok(page),
            Err(e) => {
                // Not Navidrome (or native API unreachable) — Subsonic can
                // still list and filter, just not sort.
                log::debug!("native song listing unavailable, using Subsonic fallback: {e}");
                let songs = self
                    .get_songs(request.query, request.genre, request.size, request.offset)
                    .await?;
                Ok(SongPage { songs, total: None, server_sorted: false })
            }
        }
    }

    async fn native_song_page(
        &self,
        creds: &Credentials,
        request: &SongListRequest<'_>,
    ) -> Result<SongPage, SubsonicError> {
        let mut params = vec![
            ("_start", request.offset.to_string()),
            ("_end", (request.offset + request.size).to_string()),
            ("_sort", request.sort.unwrap_or("title").to_owned()),
            ("_order", if request.descending { "DESC" } else { "ASC" }.to_owned()),
        ];
        if let Some(q) = request.query.filter(|q| !q.is_empty()) {
            params.push(("title", q.to_owned()));
        }
        if let Some(genre_name) = request.genre.filter(|g| !g.is_empty()) {
            let id = self.native_genre_id(creds, genre_name).await?.ok_or_else(|| {
                SubsonicError::Parse(format!("unknown genre {genre_name}"))
            })?;
            params.push(("genre_id", id));
        }

        let resp = self.native_get(creds, "api/song", &params).await?;
        let total = resp
            .headers()
            .get("x-total-count")
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.parse::<u64>().ok());
        let native: Vec<NativeSong> = resp
            .json()
            .await
            .map_err(|e| SubsonicError::Parse(format!("native songs: {e}")))?;
        let songs = native.into_iter().map(NativeSong::into_child).collect();
        Ok(SongPage { songs, total, server_sorted: true })
    }

    /// Build a stream URL for the audio engine. `format` of `Some("raw")`
    /// requests the original file (seekable via HTTP ranges); other formats
    /// with an optional `max_bit_rate` (kbps) request server transcoding.
    pub fn stream_url(
        &self,
        id: &str,
        format: Option<&str>,
        max_bit_rate: Option<u32>,
    ) -> Result<Url, SubsonicError> {
        let creds = self.require_creds()?;
        let mut params = vec![("id", id.to_owned())];
        if let Some(f) = format {
            params.push(("format", f.to_owned()));
        }
        if let Some(kbps) = max_bit_rate {
            params.push(("maxBitRate", kbps.to_string()));
        }
        Ok(creds.endpoint_url("stream", &params))
    }

    /// Ask the server to rescan its music folders.
    pub async fn start_scan(&self) -> Result<ScanStatus, SubsonicError> {
        let body = self.call("startScan", &[]).await?;
        Ok(body.scan_status.unwrap_or(ScanStatus { scanning: true, count: None }))
    }

    pub async fn get_scan_status(&self) -> Result<ScanStatus, SubsonicError> {
        let body = self.call("getScanStatus", &[]).await?;
        Ok(body.scan_status.unwrap_or(ScanStatus { scanning: false, count: None }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn credentials_normalize_url() {
        let c = Credentials::new("music.example.com", "u", "p").unwrap();
        assert_eq!(c.server_url.as_str(), "https://music.example.com/");
        let c = Credentials::new("http://192.168.1.5:4533", "u", "p").unwrap();
        assert_eq!(c.server_url.as_str(), "http://192.168.1.5:4533/");
        // Sub-path installs must keep their path segment.
        let c = Credentials::new("https://example.com/navidrome", "u", "p").unwrap();
        let url = c.endpoint_url("ping", &[]);
        assert!(url.path().starts_with("/navidrome/rest/ping"), "got {}", url.path());
    }

    #[test]
    fn endpoint_url_has_auth_params() {
        let c = Credentials::new("https://example.com", "alice", "secret").unwrap();
        let url = c.endpoint_url("getAlbum", &[("id", "42".into())]);
        let query: Vec<(String, String)> =
            url.query_pairs().map(|(k, v)| (k.into_owned(), v.into_owned())).collect();
        for key in ["u", "t", "s", "v", "c", "f", "id"] {
            assert!(query.iter().any(|(k, _)| k == key), "missing {key} in {url}");
        }
    }
}
