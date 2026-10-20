//! Serde models for the Subsonic / OpenSubsonic REST API (Navidrome).
//!
//! Field names are camelCase on the wire; structs are Serialize as well so the
//! Tauri layer can pass them straight through to the frontend unchanged.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize)]
pub struct Envelope {
    #[serde(rename = "subsonic-response")]
    pub inner: ResponseBody,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResponseBody {
    pub status: String,
    #[serde(default)]
    pub version: Option<String>,
    /// OpenSubsonic extensions (Navidrome sets these on every response).
    #[serde(default, rename = "type")]
    pub server_type: Option<String>,
    #[serde(default)]
    pub server_version: Option<String>,
    #[serde(default)]
    pub open_subsonic: Option<bool>,
    #[serde(default)]
    pub error: Option<ApiError>,

    // Payload variants — exactly one is present depending on the endpoint.
    #[serde(default)]
    pub album_list2: Option<AlbumList2>,
    #[serde(default)]
    pub album: Option<AlbumWithSongs>,
    #[serde(default)]
    pub artists: Option<ArtistsIndexes>,
    #[serde(default)]
    pub artist: Option<ArtistWithAlbums>,
    #[serde(default)]
    pub playlists: Option<Playlists>,
    #[serde(default)]
    pub playlist: Option<PlaylistWithSongs>,
    #[serde(default)]
    pub starred2: Option<Starred2>,
    #[serde(default)]
    pub search_result3: Option<SearchResult3>,
    #[serde(default)]
    pub top_songs: Option<TopSongs>,
    #[serde(default)]
    pub song: Option<Child>,
    #[serde(default)]
    pub scan_status: Option<ScanStatus>,
    #[serde(default)]
    pub genres: Option<Genres>,
    #[serde(default)]
    pub songs_by_genre: Option<SongsByGenre>,
    #[serde(default)]
    pub random_songs: Option<SongsByGenre>,
    #[serde(default)]
    pub play_queue: Option<PlayQueue>,
    #[serde(default)]
    pub lyrics_list: Option<LyricsList>,
    #[serde(default)]
    pub similar_songs2: Option<SimilarSongs2>,
    #[serde(default)]
    pub artist_info2: Option<ArtistInfo2>,
    #[serde(default)]
    pub music_folders: Option<MusicFolders>,
    #[serde(default)]
    pub indexes: Option<Indexes>,
    #[serde(default)]
    pub directory: Option<Directory>,
}

/// Music-folder ids are the one place Subsonic uses numbers rather than
/// strings, and servers disagree on which they send — accept both.
fn de_id_as_string<'de, D>(deserializer: D) -> Result<String, D::Error>
where
    D: serde::Deserializer<'de>,
{
    match serde_json::Value::deserialize(deserializer)? {
        serde_json::Value::String(s) => Ok(s),
        serde_json::Value::Number(n) => Ok(n.to_string()),
        other => Err(serde::de::Error::custom(format!("expected id, got {other}"))),
    }
}

/// [`de_id_as_string`] for an id that may be absent.
fn de_opt_id_as_string<'de, D>(deserializer: D) -> Result<Option<String>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    match serde_json::Value::deserialize(deserializer)? {
        serde_json::Value::Null => Ok(None),
        serde_json::Value::String(s) => Ok(Some(s)),
        serde_json::Value::Number(n) => Ok(Some(n.to_string())),
        other => Err(serde::de::Error::custom(format!("expected id, got {other}"))),
    }
}

/// The play queue the server keeps per user (`getPlayQueue`), so one client
/// can pick up where another left off.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlayQueue {
    #[serde(default)]
    pub entry: Vec<Child>,
    /// Id of the song that was playing.
    #[serde(default, deserialize_with = "de_opt_id_as_string")]
    pub current: Option<String>,
    /// Position within that song, in ms.
    #[serde(default)]
    pub position: Option<u64>,
    /// ISO time of the last save.
    #[serde(default)]
    pub changed: Option<String>,
    /// Name of the client that saved it.
    #[serde(default)]
    pub changed_by: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MusicFolders {
    #[serde(default)]
    pub music_folder: Vec<MusicFolder>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MusicFolder {
    #[serde(deserialize_with = "de_id_as_string")]
    pub id: String,
    #[serde(default)]
    pub name: Option<String>,
}

/// Top level of the folder tree (`getIndexes`): artist-level directories
/// grouped by initial, plus any loose files sitting at the root.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Indexes {
    #[serde(default)]
    pub ignored_articles: Option<String>,
    #[serde(default)]
    pub index: Vec<IndexId3>,
    #[serde(default)]
    pub child: Vec<Child>,
}

/// One directory listing (`getMusicDirectory`): subdirectories and songs
/// arrive interleaved in `child`, told apart by `isDir`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Directory {
    pub id: String,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub parent: Option<String>,
    #[serde(default)]
    pub child: Vec<Child>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ArtistInfo2 {
    pub biography: Option<String>,
    pub music_brainz_id: Option<String>,
    pub last_fm_url: Option<String>,
    pub small_image_url: Option<String>,
    pub medium_image_url: Option<String>,
    pub large_image_url: Option<String>,
    pub similar_artist: Vec<ArtistId3>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LyricsList {
    #[serde(default)]
    pub structured_lyrics: Vec<StructuredLyrics>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StructuredLyrics {
    #[serde(default)]
    pub lang: Option<String>,
    #[serde(default)]
    pub synced: bool,
    /// Global timing offset in milliseconds.
    #[serde(default)]
    pub offset: Option<i64>,
    #[serde(default)]
    pub display_artist: Option<String>,
    #[serde(default)]
    pub display_title: Option<String>,
    #[serde(default)]
    pub line: Vec<LyricsLine>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LyricsLine {
    /// Start time in milliseconds (synced lyrics only).
    #[serde(default)]
    pub start: Option<i64>,
    pub value: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SimilarSongs2 {
    #[serde(default)]
    pub song: Vec<Child>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Genres {
    #[serde(default)]
    pub genre: Vec<Genre>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Genre {
    pub value: String,
    #[serde(default)]
    pub song_count: u32,
    #[serde(default)]
    pub album_count: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SongsByGenre {
    #[serde(default)]
    pub song: Vec<Child>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScanStatus {
    pub scanning: bool,
    #[serde(default)]
    pub count: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ApiError {
    pub code: i32,
    #[serde(default)]
    pub message: String,
}

/// One credited artist. OpenSubsonic sends these as `ArtistID3`; only the id
/// and name matter for linking, so the rest of that shape is ignored.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArtistRef {
    pub id: String,
    pub name: String,
}

/// One genre. Both OpenSubsonic and Navidrome's native API wrap genre names in
/// objects; the native one adds an id that is not needed here (the genre views
/// address genres by name).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GenreRef {
    pub name: String,
}

/// A song (Subsonic calls this a "Child").
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Child {
    pub id: String,
    pub title: String,
    #[serde(default)]
    pub album: Option<String>,
    #[serde(default)]
    pub album_id: Option<String>,
    #[serde(default)]
    pub artist: Option<String>,
    #[serde(default)]
    pub artist_id: Option<String>,
    /// OpenSubsonic extension: every credited artist, in tag order. The legacy
    /// `artist`/`artist_id` pair above only ever carries the *first* of them,
    /// so per-artist navigation needs this list.
    #[serde(default)]
    pub artists: Vec<ArtistRef>,
    /// OpenSubsonic extension: the server's formatted credit for all of
    /// `artists`, e.g. "A feat. B".
    #[serde(default)]
    pub display_artist: Option<String>,
    #[serde(default)]
    pub track: Option<u32>,
    #[serde(default)]
    pub disc_number: Option<u32>,
    #[serde(default)]
    pub year: Option<i32>,
    #[serde(default)]
    pub genre: Option<String>,
    /// OpenSubsonic extension: every genre, where `genre` holds only the first.
    #[serde(default)]
    pub genres: Vec<GenreRef>,
    #[serde(default)]
    pub cover_art: Option<String>,
    #[serde(default)]
    pub size: Option<u64>,
    #[serde(default)]
    pub content_type: Option<String>,
    #[serde(default)]
    pub suffix: Option<String>,
    /// Duration in seconds.
    #[serde(default)]
    pub duration: Option<u32>,
    #[serde(default)]
    pub bit_rate: Option<u32>,
    #[serde(default)]
    pub sampling_rate: Option<u32>,
    #[serde(default)]
    pub bit_depth: Option<u32>,
    /// ISO date when starred; present only if starred.
    #[serde(default)]
    pub starred: Option<String>,
    /// This user's 1–5 rating; absent or 0 when unrated.
    #[serde(default)]
    pub user_rating: Option<u32>,
    #[serde(default)]
    pub play_count: Option<u64>,
    /// ISO date of the last play.
    #[serde(default)]
    pub played: Option<String>,
    /// ISO date the file was added to the library.
    #[serde(default)]
    pub created: Option<String>,
    /// OpenSubsonic extension: loudness normalization data.
    #[serde(default)]
    pub replay_gain: Option<ReplayGain>,
    /// Folder browsing (`getIndexes` / `getMusicDirectory`): a `Child` is a
    /// subdirectory rather than a song when this is set.
    #[serde(default)]
    pub is_dir: bool,
    /// Id of the containing directory, in folder listings.
    #[serde(default)]
    pub parent: Option<String>,
    #[serde(default)]
    pub path: Option<String>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReplayGain {
    /// dB.
    #[serde(default)]
    pub track_gain: Option<f32>,
    #[serde(default)]
    pub album_gain: Option<f32>,
    /// Linear peak amplitude (1.0 = full scale).
    #[serde(default)]
    pub track_peak: Option<f32>,
    #[serde(default)]
    pub album_peak: Option<f32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AlbumId3 {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub artist: Option<String>,
    #[serde(default)]
    pub artist_id: Option<String>,
    /// OpenSubsonic extension: every credited album artist, in tag order —
    /// `artist`/`artist_id` above only carry the first, same as on [`Child`].
    #[serde(default)]
    pub artists: Vec<ArtistRef>,
    /// OpenSubsonic extension: the server's formatted album-artist credit.
    #[serde(default)]
    pub display_artist: Option<String>,
    #[serde(default)]
    pub cover_art: Option<String>,
    #[serde(default)]
    pub song_count: u32,
    /// Total duration in seconds.
    #[serde(default)]
    pub duration: u32,
    #[serde(default)]
    pub created: Option<String>,
    #[serde(default)]
    pub year: Option<i32>,
    #[serde(default)]
    pub genre: Option<String>,
    /// OpenSubsonic extension: every genre, where `genre` holds only the first.
    #[serde(default)]
    pub genres: Vec<GenreRef>,
    #[serde(default)]
    pub starred: Option<String>,
    /// This user's 1–5 rating; absent or 0 when unrated.
    #[serde(default)]
    pub user_rating: Option<u32>,
    #[serde(default)]
    pub played: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AlbumWithSongs {
    #[serde(flatten)]
    pub album: AlbumId3,
    #[serde(default)]
    pub song: Vec<Child>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ArtistId3 {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub cover_art: Option<String>,
    #[serde(default)]
    pub album_count: Option<u32>,
    #[serde(default)]
    pub artist_image_url: Option<String>,
    #[serde(default)]
    pub starred: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ArtistWithAlbums {
    #[serde(flatten)]
    pub artist: ArtistId3,
    #[serde(default)]
    pub album: Vec<AlbumId3>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ArtistsIndexes {
    #[serde(default)]
    pub ignored_articles: Option<String>,
    #[serde(default)]
    pub index: Vec<IndexId3>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IndexId3 {
    pub name: String,
    #[serde(default)]
    pub artist: Vec<ArtistId3>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AlbumList2 {
    #[serde(default)]
    pub album: Vec<AlbumId3>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Playlists {
    #[serde(default)]
    pub playlist: Vec<Playlist>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Playlist {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub comment: Option<String>,
    #[serde(default)]
    pub owner: Option<String>,
    #[serde(default)]
    pub song_count: u32,
    /// Total duration in seconds.
    #[serde(default)]
    pub duration: u32,
    #[serde(default)]
    pub created: Option<String>,
    #[serde(default)]
    pub changed: Option<String>,
    #[serde(default)]
    pub cover_art: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlaylistWithSongs {
    #[serde(flatten)]
    pub playlist: Playlist,
    #[serde(default)]
    pub entry: Vec<Child>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Starred2 {
    #[serde(default)]
    pub song: Vec<Child>,
    #[serde(default)]
    pub album: Vec<AlbumId3>,
    #[serde(default)]
    pub artist: Vec<ArtistId3>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchResult3 {
    #[serde(default)]
    pub song: Vec<Child>,
    #[serde(default)]
    pub album: Vec<AlbumId3>,
    #[serde(default)]
    pub artist: Vec<ArtistId3>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TopSongs {
    #[serde(default)]
    pub song: Vec<Child>,
}

/// One page of a sorted song listing (native Navidrome API when available).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SongPage {
    pub songs: Vec<Child>,
    /// Total matching songs, when the server reports it.
    pub total: Option<u64>,
    /// False when the fallback (unsorted Subsonic listing) was used.
    pub server_sorted: bool,
}

/// A song as Navidrome's native REST API returns it.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NativeSong {
    pub id: String,
    pub title: String,
    #[serde(default)]
    pub album: Option<String>,
    #[serde(default)]
    pub album_id: Option<String>,
    #[serde(default)]
    pub artist: Option<String>,
    #[serde(default)]
    pub artist_id: Option<String>,
    /// Navidrome's native shape for the same data as OpenSubsonic `artists`,
    /// keyed by role ("artist", "albumartist", …).
    #[serde(default)]
    pub participants: HashMap<String, Vec<ArtistRef>>,
    #[serde(default)]
    pub track_number: Option<u32>,
    #[serde(default)]
    pub disc_number: Option<u32>,
    #[serde(default)]
    pub year: Option<i32>,
    #[serde(default)]
    pub genre: Option<String>,
    #[serde(default)]
    pub genres: Vec<GenreRef>,
    #[serde(default)]
    pub duration: Option<f64>,
    #[serde(default)]
    pub bit_rate: Option<u32>,
    #[serde(default)]
    pub sample_rate: Option<u32>,
    #[serde(default)]
    pub suffix: Option<String>,
    #[serde(default)]
    pub size: Option<u64>,
    #[serde(default)]
    pub play_count: Option<u64>,
    #[serde(default)]
    pub play_date: Option<String>,
    #[serde(default)]
    pub created_at: Option<String>,
    #[serde(default)]
    pub starred: Option<bool>,
    #[serde(default)]
    pub starred_at: Option<String>,
    /// This user's 1–5 rating (native API spells it without the `user` prefix).
    #[serde(default)]
    pub rating: Option<u32>,
    #[serde(default)]
    pub path: Option<String>,
    #[serde(default)]
    pub rg_track_gain: Option<f32>,
    #[serde(default)]
    pub rg_track_peak: Option<f32>,
    #[serde(default)]
    pub rg_album_gain: Option<f32>,
    #[serde(default)]
    pub rg_album_peak: Option<f32>,
}

/// Go encodes an empty slice or map as `null`, which `#[serde(default)]` alone
/// rejects — read it as the empty value instead.
fn null_as_default<'de, D, T>(deserializer: D) -> Result<T, D::Error>
where
    D: serde::Deserializer<'de>,
    T: Default + Deserialize<'de>,
{
    Ok(Option::<T>::deserialize(deserializer)?.unwrap_or_default())
}

/// An album as Navidrome's native REST API returns it.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NativeAlbum {
    pub id: String,
    pub name: String,
    /// The formatted album-artist credit.
    #[serde(default)]
    pub album_artist: Option<String>,
    #[serde(default)]
    pub album_artist_id: Option<String>,
    /// Keyed by role, as on [`NativeSong`]; an album's roles gather those of
    /// all its tracks.
    #[serde(default, deserialize_with = "null_as_default")]
    pub participants: HashMap<String, Vec<ArtistRef>>,
    #[serde(default)]
    pub song_count: u32,
    #[serde(default)]
    pub duration: Option<f64>,
    #[serde(default)]
    pub max_year: Option<i32>,
    #[serde(default)]
    pub genre: Option<String>,
    #[serde(default, deserialize_with = "null_as_default")]
    pub genres: Vec<GenreRef>,
    #[serde(default)]
    pub starred: Option<bool>,
    #[serde(default)]
    pub starred_at: Option<String>,
    #[serde(default)]
    pub rating: Option<u32>,
    #[serde(default)]
    pub play_date: Option<String>,
    #[serde(default)]
    pub created_at: Option<String>,
}

impl NativeAlbum {
    pub fn into_album_id3(mut self) -> AlbumId3 {
        AlbumId3 {
            artists: self.participants.remove("albumartist").unwrap_or_default(),
            // `album_artist` is already the formatted credit, as on songs.
            display_artist: None,
            cover_art: Some(format!("al-{}", self.id)),
            id: self.id,
            name: self.name,
            artist: self.album_artist.filter(|a| !a.is_empty()),
            artist_id: self.album_artist_id.filter(|a| !a.is_empty()),
            song_count: self.song_count,
            duration: self.duration.map_or(0, |d| d.round() as u32),
            created: real_date(self.created_at),
            year: self.max_year.filter(|y| *y > 0),
            genre: self.genre.filter(|g| !g.is_empty()),
            genres: self.genres,
            starred: if self.starred == Some(true) {
                real_date(self.starred_at).or_else(|| Some("1970-01-01T00:00:00Z".into()))
            } else {
                None
            },
            user_rating: self.rating.filter(|r| *r > 0),
            played: real_date(self.play_date),
        }
    }
}

/// Navidrome serializes "never" timestamps as year 0001 — treat as absent.
fn real_date(date: Option<String>) -> Option<String> {
    date.filter(|d| !d.is_empty() && !d.starts_with("0001-"))
}

impl NativeSong {
    pub fn into_child(mut self) -> Child {
        let has_rg = self.rg_track_gain.is_some() || self.rg_album_gain.is_some();
        // `artist` is already the formatted credit here, so the display string
        // needs no separate field.
        let artists = self.participants.remove("artist").unwrap_or_default();
        Child {
            artists,
            display_artist: None,
            // Subsonic cover art ids accept the bare `al-<albumId>` form.
            cover_art: self.album_id.as_ref().map(|id| format!("al-{id}")),
            id: self.id,
            title: self.title,
            album: self.album,
            album_id: self.album_id,
            artist: self.artist,
            artist_id: self.artist_id,
            // The native API uses 0 / "" for "unset".
            track: self.track_number.filter(|t| *t > 0),
            disc_number: self.disc_number.filter(|d| *d > 0),
            year: self.year.filter(|y| *y > 0),
            genre: self.genre.filter(|g| !g.is_empty()),
            genres: self.genres,
            size: self.size,
            content_type: None,
            suffix: self.suffix,
            duration: self.duration.map(|d| d.round() as u32),
            bit_rate: self.bit_rate,
            sampling_rate: self.sample_rate,
            bit_depth: None,
            starred: if self.starred == Some(true) {
                real_date(self.starred_at).or_else(|| Some("1970-01-01T00:00:00Z".into()))
            } else {
                None
            },
            user_rating: self.rating.filter(|r| *r > 0),
            play_count: self.play_count,
            played: real_date(self.play_date),
            created: real_date(self.created_at),
            replay_gain: has_rg.then_some(ReplayGain {
                track_gain: self.rg_track_gain,
                album_gain: self.rg_album_gain,
                track_peak: self.rg_track_peak,
                album_peak: self.rg_album_peak,
            }),
            is_dir: false,
            parent: None,
            path: self.path,
        }
    }
}

/// Returned by a successful ping — what the setup screen shows after
/// "Test connection".
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ServerInfo {
    pub version: Option<String>,
    pub server_type: Option<String>,
    pub server_version: Option<String>,
    pub open_subsonic: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_navidrome_ping() {
        let json = r#"{"subsonic-response":{"status":"ok","version":"1.16.1","type":"navidrome","serverVersion":"0.54.1 (734eb30a)","openSubsonic":true}}"#;
        let env: Envelope = serde_json::from_str(json).unwrap();
        assert_eq!(env.inner.status, "ok");
        assert_eq!(env.inner.server_type.as_deref(), Some("navidrome"));
        assert_eq!(env.inner.open_subsonic, Some(true));
    }

    #[test]
    fn parses_error_response() {
        let json = r#"{"subsonic-response":{"status":"failed","version":"1.16.1","error":{"code":40,"message":"Wrong username or password"}}}"#;
        let env: Envelope = serde_json::from_str(json).unwrap();
        let err = env.inner.error.unwrap();
        assert_eq!(err.code, 40);
    }

    /// Verbatim from Navidrome 0.63 `search3`: `artistId` is only the first
    /// credited artist, so `artists` is what per-artist links must use.
    #[test]
    fn parses_multi_artist_song() {
        let json = r#"{"id":"s1","title":"Feat Style","artist":"Alpha Artist feat. Epsilon Artist","artistId":"a-alpha","displayArtist":"Alpha Artist feat. Epsilon Artist","artists":[{"id":"a-alpha","name":"Alpha Artist"},{"id":"a-eps","name":"Epsilon Artist"}],"albumArtists":[{"id":"a-alpha","name":"Alpha Artist"}]}"#;
        let song: Child = serde_json::from_str(json).unwrap();
        assert_eq!(song.artist_id.as_deref(), Some("a-alpha"));
        assert_eq!(song.display_artist.as_deref(), Some("Alpha Artist feat. Epsilon Artist"));
        let names: Vec<&str> = song.artists.iter().map(|a| a.name.as_str()).collect();
        assert_eq!(names, ["Alpha Artist", "Epsilon Artist"]);
        assert_eq!(song.artists[1].id, "a-eps");
    }

    /// Servers without the OpenSubsonic extension leave the list empty rather
    /// than failing to parse.
    #[test]
    fn parses_song_without_artists_extension() {
        let json = r#"{"id":"s1","title":"Single","artist":"Alpha Artist","artistId":"a-alpha"}"#;
        let song: Child = serde_json::from_str(json).unwrap();
        assert!(song.artists.is_empty());
        assert!(song.display_artist.is_none());
        assert!(song.genres.is_empty());
    }

    /// Genres arrive as objects on all three paths, and `genre` is only the
    /// first of them.
    #[test]
    fn parses_multi_genre_entities() {
        let song: Child = serde_json::from_str(
            r#"{"id":"s1","title":"Multi","genre":"Electronic","genres":[{"name":"Electronic"},{"name":"Ambient"}]}"#,
        )
        .unwrap();
        let names: Vec<&str> = song.genres.iter().map(|g| g.name.as_str()).collect();
        assert_eq!(names, ["Electronic", "Ambient"]);

        let album: AlbumId3 = serde_json::from_str(
            r#"{"id":"al1","name":"Test Album","genre":"Ambient","genres":[{"name":"Ambient"},{"name":"Electronic"}],"songCount":5,"duration":10}"#,
        )
        .unwrap();
        assert_eq!(album.genres.len(), 2);

        // The native API adds an id to each genre, which is ignored.
        let native: NativeSong = serde_json::from_str(
            r#"{"id":"s1","title":"Multi","genre":"Electronic","genres":[{"id":"g1","name":"Electronic"},{"id":"g2","name":"Ambient"}]}"#,
        )
        .unwrap();
        let child = native.into_child();
        assert_eq!(child.genres[1].name, "Ambient");
    }

    /// The native API spells the same list `participants.artist`.
    #[test]
    fn native_song_carries_participant_artists() {
        let json = r#"{"id":"s1","title":"Feat Style","artist":"Alpha Artist feat. Epsilon Artist","artistId":"a-alpha","participants":{"albumartist":[{"id":"a-alpha","name":"Alpha Artist","missing":false}],"artist":[{"id":"a-alpha","name":"Alpha Artist","missing":false},{"id":"a-eps","name":"Epsilon Artist","missing":false}]}}"#;
        let child = serde_json::from_str::<NativeSong>(json).unwrap().into_child();
        let names: Vec<&str> = child.artists.iter().map(|a| a.name.as_str()).collect();
        assert_eq!(names, ["Alpha Artist", "Epsilon Artist"]);
        // The credit string doubles as the display value on this path.
        assert_eq!(child.artist.as_deref(), Some("Alpha Artist feat. Epsilon Artist"));
    }

    /// A compilation another artist appears on: the album artists come from
    /// `participants.albumartist`, and unset values arrive as 0 / "" / null.
    #[test]
    fn native_album_converts_to_album_id3() {
        let json = r#"{"id":"al9","name":"Summer Hits","albumArtist":"Various Artists","albumArtistId":"a-va","participants":{"albumartist":[{"id":"a-va","name":"Various Artists","missing":false}],"artist":[{"id":"a-alpha","name":"Alpha Artist","missing":false},{"id":"a-eps","name":"Epsilon Artist","missing":false}]},"songCount":18,"duration":3901.4,"maxYear":0,"genre":"","genres":null,"starred":true,"starredAt":"2026-01-02T03:04:05Z","playDate":"0001-01-01T00:00:00Z","createdAt":"2025-06-01T12:00:00Z"}"#;
        let album = serde_json::from_str::<NativeAlbum>(json).unwrap().into_album_id3();
        assert_eq!(album.artist.as_deref(), Some("Various Artists"));
        let names: Vec<&str> = album.artists.iter().map(|a| a.name.as_str()).collect();
        assert_eq!(names, ["Various Artists"]);
        assert_eq!(album.cover_art.as_deref(), Some("al-al9"));
        assert_eq!(album.duration, 3901);
        assert_eq!(album.year, None);
        assert_eq!(album.genre, None);
        assert!(album.genres.is_empty());
        assert_eq!(album.starred.as_deref(), Some("2026-01-02T03:04:05Z"));
        assert_eq!(album.played, None);
    }

    #[test]
    fn parses_album_list() {
        let json = r#"{"subsonic-response":{"status":"ok","version":"1.16.1","albumList2":{"album":[{"id":"1","name":"Abbey Road","artist":"The Beatles","artistId":"a1","coverArt":"al-1","songCount":17,"duration":2832,"year":1969}]}}}"#;
        let env: Envelope = serde_json::from_str(json).unwrap();
        let list = env.inner.album_list2.unwrap();
        assert_eq!(list.album.len(), 1);
        assert_eq!(list.album[0].name, "Abbey Road");
        assert_eq!(list.album[0].song_count, 17);
        // No OpenSubsonic extension in this response.
        assert!(list.album[0].artists.is_empty());
    }

    /// Verbatim from Navidrome 0.63 `getAlbum`: album artists arrive in the
    /// same shape as a song's, `artistId` again being only the first.
    #[test]
    fn parses_multi_artist_album() {
        let json = r#"{"id":"al1","name":"Collab Album","artist":"Alpha Artist / Gamma Artist","artistId":"a-alpha","displayArtist":"Alpha Artist / Gamma Artist","artists":[{"id":"a-alpha","name":"Alpha Artist"},{"id":"a-gamma","name":"Gamma Artist"}],"songCount":2,"duration":4}"#;
        let album: AlbumId3 = serde_json::from_str(json).unwrap();
        assert_eq!(album.artist_id.as_deref(), Some("a-alpha"));
        let names: Vec<&str> = album.artists.iter().map(|a| a.name.as_str()).collect();
        assert_eq!(names, ["Alpha Artist", "Gamma Artist"]);
        assert_eq!(album.display_artist.as_deref(), Some("Alpha Artist / Gamma Artist"));
    }

    #[test]
    fn parses_play_queue() {
        let json = r#"{"subsonic-response":{"status":"ok","version":"1.16.1","playQueue":{
            "entry":[{"id":"a","title":"A"},{"id":"b","title":"B"}],
            "current":"b","position":61234,"username":"u",
            "changed":"2026-10-20T21:00:00.123Z","changedBy":"Symfonium"}}}"#;
        let env: Envelope = serde_json::from_str(json).unwrap();
        let queue = env.inner.play_queue.unwrap();
        assert_eq!(queue.entry.len(), 2);
        assert_eq!(queue.current.as_deref(), Some("b"));
        assert_eq!(queue.position, Some(61234));
        assert_eq!(queue.changed_by.as_deref(), Some("Symfonium"));

        // Older servers send the current id as a number, and may omit it.
        let numeric = r#"{"entry":[],"current":42}"#;
        let queue: PlayQueue = serde_json::from_str(numeric).unwrap();
        assert_eq!(queue.current.as_deref(), Some("42"));
        let bare: PlayQueue = serde_json::from_str(r#"{"entry":[]}"#).unwrap();
        assert_eq!(bare.current, None);
    }
}
