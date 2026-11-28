//! Discord Rich Presence: while a Song plays, the listener's Discord profile
//! reads "Listening to Reverb", with the Song's title, artist and album and a
//! progress bar. Off unless asked for, since everyone who can see the profile
//! sees what plays.
//!
//! Reverb talks only to the Discord desktop app, over its local IPC socket.
//! With no Discord running there is no presence and a quiet look for it now
//! and then — never an error.
//!
//! The picture is the album cover from the Cover Art Archive, found by the
//! MusicBrainz release id the album is tagged with; without one it is Reverb's
//! icon. Never the Subsonic server's own cover art: its links carry the
//! account's credentials, and Discord shows image links to anyone who looks.
//!
//! Two halves. [`PresenceTracker`] lives on the event forwarder thread and
//! turns engine events into what Discord should show. The worker thread behind
//! [`DiscordPresence`] owns the socket and looks up covers, so a slow or absent
//! Discord never holds up playback events.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use crossbeam_channel::{Receiver, RecvTimeoutError, Sender};
use discord_rich_presence::activity::{Activity, ActivityType, Assets, Timestamps};
use discord_rich_presence::{DiscordIpc, DiscordIpcClient};
use reverb_core::audio::PlaybackState;
use reverb_core::subsonic::SubsonicClient;
use serde_json::json;

/// The Discord application the presence is shown under: its name is the
/// "Reverb" in "Listening to Reverb". Not a secret — every Discord client that
/// shows the presence sees it.
const APPLICATION_ID: &str = "1576020148932972554";

/// Reverb's icon: the picture when there is no cover, and the badge on the
/// cover when there is. Discord fetches it itself.
const ICON_URL: &str =
    "https://raw.githubusercontent.com/ZortexSenpai/Reverb/main/src-tauri/icons/icon.png";

/// How long to wait before looking for Discord again after not finding it.
const RETRY: Duration = Duration::from_secs(15);

/// Discord accepts five presence updates per 20 seconds and quietly drops the
/// rest — which could leave a skipped-past Song showing. One every four
/// seconds never hits that; the newest state goes once the time is up.
const MIN_UPDATE_INTERVAL: Duration = Duration::from_secs(4);

/// A start time that moves by less than this is the same playback drifting.
/// More is a seek (or a stall), and the progress bar has to jump with it.
const SEEK_TOLERANCE_MS: u64 = 2_000;

/// What Discord shows for the playing Song.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Listening {
    /// The Song's id on the server, which its cover is found by.
    pub song_id: String,
    pub title: String,
    pub artist: Option<String>,
    pub album: Option<String>,
    /// Unix ms the Song would have started at, played straight through to
    /// where it is now.
    pub started_at_ms: u64,
    /// 0 when unknown: then Discord counts up from the start instead of
    /// drawing a bar.
    pub duration_ms: u64,
}

impl Listening {
    /// Whether Discord shows the same thing for both: the same Song, and a
    /// progress bar that would not visibly move.
    fn looks_like(&self, other: &Listening) -> bool {
        self.song_id == other.song_id
            && self.title == other.title
            && self.artist == other.artist
            && self.album == other.album
            && self.duration_ms == other.duration_ms
            && self.started_at_ms.abs_diff(other.started_at_ms) <= SEEK_TOLERANCE_MS
    }
}

/// Discord turns down text under 2 or over 128 UTF-16 units, and with it the
/// whole presence. Pad the short and cut the long; nothing at all is `None`.
fn field(text: &str) -> Option<String> {
    const MAX: usize = 128;
    let text = text.trim();
    let units = text.encode_utf16().count();
    if units == 0 {
        return None;
    }
    if units < 2 {
        // A zero-width space: it counts, and shows as nothing.
        return Some(format!("{text}\u{200B}"));
    }
    if units <= MAX {
        return Some(text.to_owned());
    }
    let mut cut = String::new();
    let mut used = 0;
    for c in text.chars() {
        // Room for the ellipsis, which is one unit.
        if used + c.len_utf16() > MAX - 1 {
            break;
        }
        used += c.len_utf16();
        cut.push(c);
    }
    cut.push('…');
    Some(cut)
}

/// `cover` is the album cover's URL, when there is one.
fn activity(listening: &Listening, cover: Option<&str>) -> Activity<'static> {
    let start = listening.started_at_ms as i64;
    let mut timestamps = Timestamps::new().start(start);
    if listening.duration_ms > 0 {
        timestamps = timestamps.end(start + listening.duration_ms as i64);
    }
    let mut assets = match cover {
        Some(cover) => Assets::new()
            .large_image(cover.to_owned())
            .small_image(ICON_URL)
            .small_text("Reverb"),
        None => Assets::new().large_image(ICON_URL),
    };
    if let Some(album) = listening.album.as_deref().and_then(field) {
        assets = assets.large_text(album);
    }
    let mut activity = Activity::new()
        .activity_type(ActivityType::Listening)
        .timestamps(timestamps)
        .assets(assets);
    if let Some(title) = field(&listening.title) {
        activity = activity.details(title);
    }
    if let Some(artist) = listening.artist.as_deref().and_then(field) {
        activity = activity.state(artist);
    }
    activity
}

/// The Song the engine has, as far as the presence cares.
#[derive(Debug, Clone)]
struct Song {
    id: String,
    title: String,
    artist: Option<String>,
    album: Option<String>,
    duration_ms: u64,
}

/// Follows the engine's events and tells the worker what Discord should show:
/// the Song while it plays, nothing while paused or stopped. Only real changes
/// go through — not every position tick.
pub struct PresenceTracker {
    presence: DiscordPresence,
    song: Option<Song>,
    playing: bool,
    /// What the worker was last told.
    told: Option<Listening>,
}

impl PresenceTracker {
    pub fn new(presence: DiscordPresence) -> Self {
        Self {
            presence,
            song: None,
            playing: false,
            told: None,
        }
    }

    pub fn on_track_changed(
        &mut self,
        song_id: &str,
        title: Option<&str>,
        artist: Option<&str>,
        album: Option<&str>,
        duration_ms: u64,
        now_ms: u64,
    ) {
        // A Song without a title has nothing to show by.
        self.song = title.map(|title| Song {
            id: song_id.to_owned(),
            title: title.to_owned(),
            artist: artist.map(str::to_owned),
            album: album.map(str::to_owned),
            duration_ms,
        });
        self.update(0, now_ms);
    }

    pub fn on_state(&mut self, state: PlaybackState, position_ms: u64, now_ms: u64) {
        match state {
            PlaybackState::Playing => self.playing = true,
            PlaybackState::Paused => self.playing = false,
            PlaybackState::Stopped => {
                self.playing = false;
                self.song = None;
            }
            // Transient: whatever showed keeps showing.
            PlaybackState::Buffering => return,
        }
        self.update(position_ms, now_ms);
    }

    pub fn on_position(&mut self, position_ms: u64, duration_ms: u64, now_ms: u64) {
        // The duration can be unknown when the Song starts and turn up later.
        if let Some(song) = self.song.as_mut() {
            if duration_ms > 0 {
                song.duration_ms = duration_ms;
            }
        }
        self.update(position_ms, now_ms);
    }

    fn update(&mut self, position_ms: u64, now_ms: u64) {
        let wanted = self
            .song
            .as_ref()
            .filter(|_| self.playing)
            .map(|song| Listening {
                song_id: song.id.clone(),
                title: song.title.clone(),
                artist: song.artist.clone(),
                album: song.album.clone(),
                started_at_ms: now_ms.saturating_sub(position_ms),
                duration_ms: song.duration_ms,
            });
        let same = match (&self.told, &wanted) {
            (Some(told), Some(wanted)) => told.looks_like(wanted),
            (None, None) => true,
            _ => false,
        };
        if !same {
            self.presence.show(wanted.clone());
            self.told = wanted;
        }
    }
}

enum Msg {
    Enabled(bool),
    Show(Option<Listening>),
}

/// The handle to the presence worker. Cheap to clone; the worker stops when
/// the last handle goes, and Discord drops the presence with the connection.
#[derive(Clone)]
pub struct DiscordPresence {
    tx: Sender<Msg>,
}

impl DiscordPresence {
    /// Start the worker. It looks for Discord only once there is something to
    /// show and the preference is on, and for a cover only once Discord is
    /// there to show it.
    pub fn start(enabled: bool, subsonic: Arc<SubsonicClient>) -> Self {
        let (tx, rx) = crossbeam_channel::unbounded();
        let mut covers = CoverArt::new(subsonic);
        let worker = Worker::new(enabled, connect_to_discord, move |song_id: &str| {
            covers.lookup(song_id)
        });
        std::thread::Builder::new()
            .name("reverb-discord-presence".into())
            .spawn(move || run(rx, worker))
            .expect("spawning the Discord presence worker cannot fail");
        Self { tx }
    }

    /// Turning it off takes the presence down at once.
    pub fn set_enabled(&self, enabled: bool) {
        let _ = self.tx.send(Msg::Enabled(enabled));
    }

    fn show(&self, listening: Option<Listening>) {
        let _ = self.tx.send(Msg::Show(listening));
    }
}

/// A live connection to the Discord app. Dropping it closes it, which takes
/// the presence down with it.
trait Connection: Send {
    fn set(&mut self, listening: &Listening, cover: Option<&str>) -> Result<(), String>;
    fn clear(&mut self) -> Result<(), String>;
}

struct Ipc(DiscordIpcClient);

impl Ipc {
    /// Every command gets an answer. Reading it keeps the socket from filling
    /// up, and says why Discord turned a presence down.
    fn answer(&mut self) -> Result<(), String> {
        let (_, reply) = self.0.recv().map_err(|e| e.to_string())?;
        if reply["evt"] == "ERROR" {
            log::warn!(
                "Discord turned the presence down: {}",
                reply["data"]["message"]
            );
        }
        Ok(())
    }
}

impl Connection for Ipc {
    fn set(&mut self, listening: &Listening, cover: Option<&str>) -> Result<(), String> {
        self.0
            .set_activity(activity(listening, cover))
            .map_err(|e| e.to_string())?;
        self.answer()
    }

    fn clear(&mut self) -> Result<(), String> {
        self.0.clear_activity().map_err(|e| e.to_string())?;
        self.answer()
    }
}

impl Drop for Ipc {
    fn drop(&mut self) {
        let _ = self.0.close();
    }
}

fn connect_to_discord() -> Result<Box<dyn Connection>, String> {
    let mut client = DiscordIpcClient::new(APPLICATION_ID);
    client.connect_ipc().map_err(|e| e.to_string())?;
    // The handshake by hand rather than `connect()`, which ignores the answer:
    // an unknown application id is refused here, and the reason is worth a log.
    client
        .send(json!({ "v": 1, "client_id": APPLICATION_ID }), 0)
        .map_err(|e| e.to_string())?;
    let (_, reply) = client.recv().map_err(|e| e.to_string())?;
    if reply["evt"] != "READY" {
        return Err(format!("Discord refused the connection: {reply}"));
    }
    log::info!("connected to Discord for the presence");
    Ok(Box::new(Ipc(client)))
}

/// Finds album covers on the Cover Art Archive: the Song's album, the album's
/// MusicBrainz release id, and the release's front cover. What it learns is
/// kept for the session, so a Song is looked up once and an album's cover
/// once.
struct CoverArt {
    subsonic: Arc<SubsonicClient>,
    http: reqwest::Client,
    /// Song id → its cover, or `None` for a definite none.
    songs: HashMap<String, Option<String>>,
    /// Album id → its cover, the same way. Most Songs share theirs with the
    /// one before.
    albums: HashMap<String, Option<String>>,
}

impl CoverArt {
    fn new(subsonic: Arc<SubsonicClient>) -> Self {
        let http = reqwest::Client::builder()
            // The presence waits while it looks: better no cover than a late
            // presence.
            .timeout(Duration::from_secs(5))
            .user_agent(format!("Reverb/{}", env!("CARGO_PKG_VERSION")))
            .build()
            .expect("reqwest client construction cannot fail with static config");
        Self {
            subsonic,
            http,
            songs: HashMap::new(),
            albums: HashMap::new(),
        }
    }

    /// The cover's URL, or `None` when there is none — or none to be had right
    /// now, in which case the next look tries again.
    fn lookup(&mut self, song_id: &str) -> Option<String> {
        if let Some(known) = self.songs.get(song_id) {
            return known.clone();
        }
        match tauri::async_runtime::block_on(self.find(song_id)) {
            Ok(cover) => {
                self.songs.insert(song_id.to_owned(), cover.clone());
                cover
            }
            Err(e) => {
                log::debug!("no cover for {song_id} this time: {e}");
                None
            }
        }
    }

    /// `Ok(None)` is a definite none: no album, no release id, or no front
    /// cover for it. `Err` is worth asking again.
    async fn find(&mut self, song_id: &str) -> Result<Option<String>, String> {
        let song = self
            .subsonic
            .get_song(song_id)
            .await
            .map_err(|e| e.to_string())?;
        let Some(album_id) = song.album_id.filter(|id| !id.is_empty()) else {
            return Ok(None);
        };
        if let Some(known) = self.albums.get(&album_id) {
            return Ok(known.clone());
        }
        let album = self
            .subsonic
            .get_album(&album_id)
            .await
            .map_err(|e| e.to_string())?;
        let release = album.album.music_brainz_id.as_deref();
        let cover = match release.and_then(front_cover_url) {
            Some(url) => self.resolve(&url).await?,
            None => None,
        };
        self.albums.insert(album_id, cover.clone());
        Ok(cover)
    }

    /// Follow the archive's redirects to the image itself, so Discord is given
    /// a picture rather than a pointer to one.
    async fn resolve(&self, url: &str) -> Result<Option<String>, String> {
        let resp = self
            .http
            .head(url)
            .send()
            .await
            .map_err(|e| e.to_string())?;
        let status = resp.status();
        if status.is_success() {
            return Ok(Some(resp.url().to_string()));
        }
        // No such release, or none of its art is a front cover.
        if status == reqwest::StatusCode::NOT_FOUND {
            return Ok(None);
        }
        Err(format!("the Cover Art Archive answered {status}"))
    }
}

/// Where the Cover Art Archive keeps a release's front cover, at a size made
/// for a picture this small. `None` unless `mbid` is a release id at all — it
/// is a tag, so anything can be in it.
fn front_cover_url(mbid: &str) -> Option<String> {
    let id = uuid::Uuid::parse_str(mbid.trim()).ok()?;
    Some(format!(
        "https://coverartarchive.org/release/{}/front-250",
        id.hyphenated()
    ))
}

/// Keeps Discord showing what it should, through Discord starting, quitting
/// and restarting under it.
struct Worker<C, A> {
    connect: C,
    /// Song id → its cover's URL.
    cover: A,
    enabled: bool,
    wanted: Option<Listening>,
    conn: Option<Box<dyn Connection>>,
    /// What Discord shows from Reverb now. Nothing without a connection.
    shown: Option<Listening>,
    /// No looking for Discord before this.
    retry_at: Option<Instant>,
    /// No update before this (see [`MIN_UPDATE_INTERVAL`]).
    next_update_at: Option<Instant>,
}

impl<C, A> Worker<C, A>
where
    C: FnMut() -> Result<Box<dyn Connection>, String>,
    A: FnMut(&str) -> Option<String>,
{
    fn new(enabled: bool, connect: C, cover: A) -> Self {
        Self {
            connect,
            cover,
            enabled,
            wanted: None,
            conn: None,
            shown: None,
            retry_at: None,
            next_update_at: None,
        }
    }

    fn apply(&mut self, msg: Msg) {
        match msg {
            Msg::Enabled(enabled) => self.enabled = enabled,
            Msg::Show(listening) => self.wanted = listening,
        }
    }

    fn behind(&self) -> bool {
        self.enabled && self.wanted != self.shown
    }

    /// How long to sleep before trying again unprompted. `None` when nothing
    /// is owed, and only a message can change that.
    fn wait(&self, now: Instant) -> Option<Duration> {
        if !self.behind() {
            return None;
        }
        let at = if self.conn.is_some() {
            self.next_update_at
        } else {
            self.retry_at
        };
        Some(at.map_or(Duration::ZERO, |at| at.saturating_duration_since(now)))
    }

    fn sync(&mut self, now: Instant) {
        if !self.enabled {
            // Closing is enough: Discord drops a presence with its connection.
            self.conn = None;
            self.shown = None;
            return;
        }
        // A failed update most likely means Discord restarted since the
        // connection was made, so one fresh connection is worth trying at once.
        for _ in 0..2 {
            if !self.behind() {
                return;
            }
            if self.conn.is_none() {
                if self.retry_at.is_some_and(|at| now < at) {
                    return;
                }
                match (self.connect)() {
                    Ok(conn) => {
                        self.conn = Some(conn);
                        self.retry_at = None;
                    }
                    Err(e) => {
                        log::debug!("no Discord to show the presence on: {e}");
                        self.retry_at = Some(now + RETRY);
                        return;
                    }
                }
            }
            if self.next_update_at.is_some_and(|at| now < at) {
                return;
            }
            let sent = match &self.wanted {
                Some(listening) => {
                    // Looked for only now, with Discord there to show it.
                    let cover = (self.cover)(&listening.song_id);
                    let conn = self.conn.as_mut().expect("connected just above");
                    conn.set(listening, cover.as_deref())
                }
                None => self.conn.as_mut().expect("connected just above").clear(),
            };
            self.next_update_at = Some(now + MIN_UPDATE_INTERVAL);
            match sent {
                Ok(()) => {
                    self.shown = self.wanted.clone();
                    return;
                }
                Err(e) => {
                    log::debug!("lost the Discord connection: {e}");
                    self.conn = None;
                    self.shown = None;
                    // The interval paces one connection; a fresh one starts clean.
                    self.next_update_at = None;
                }
            }
        }
        self.retry_at = Some(now + RETRY);
    }
}

fn run<C, A>(rx: Receiver<Msg>, mut worker: Worker<C, A>)
where
    C: FnMut() -> Result<Box<dyn Connection>, String>,
    A: FnMut(&str) -> Option<String>,
{
    loop {
        let msg = match worker.wait(Instant::now()) {
            None => match rx.recv() {
                Ok(msg) => Some(msg),
                Err(_) => return,
            },
            Some(wait) => match rx.recv_timeout(wait) {
                Ok(msg) => Some(msg),
                Err(RecvTimeoutError::Timeout) => None,
                Err(RecvTimeoutError::Disconnected) => return,
            },
        };
        if let Some(msg) = msg {
            worker.apply(msg);
        }
        // Only the newest state matters: catch up before talking to Discord.
        while let Ok(msg) = rx.try_recv() {
            worker.apply(msg);
        }
        worker.sync(Instant::now());
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use super::*;

    fn song(title: &str, started_at_ms: u64) -> Listening {
        Listening {
            song_id: title.to_lowercase(),
            title: title.into(),
            artist: Some("Artist".into()),
            album: Some("Album".into()),
            started_at_ms,
            duration_ms: 180_000,
        }
    }

    // ---- tracker ----

    fn tracker() -> (PresenceTracker, Receiver<Msg>) {
        let (tx, rx) = crossbeam_channel::unbounded();
        (PresenceTracker::new(DiscordPresence { tx }), rx)
    }

    /// Everything the tracker told the worker, oldest first.
    fn told(rx: &Receiver<Msg>) -> Vec<Option<Listening>> {
        rx.try_iter()
            .map(|msg| match msg {
                Msg::Show(listening) => listening,
                Msg::Enabled(_) => panic!("the tracker never switches the presence"),
            })
            .collect()
    }

    #[test]
    fn a_playing_song_shows_and_a_paused_one_does_not() {
        let (mut t, rx) = tracker();
        t.on_track_changed(
            "song",
            Some("Song"),
            Some("Artist"),
            Some("Album"),
            180_000,
            1_000_000,
        );
        assert_eq!(told(&rx), vec![], "nothing until it plays");

        t.on_state(PlaybackState::Playing, 0, 1_000_000);
        assert_eq!(told(&rx), vec![Some(song("Song", 1_000_000))]);

        t.on_state(PlaybackState::Paused, 30_000, 1_030_000);
        assert_eq!(told(&rx), vec![None]);

        // Resumed after a minute: the bar picks up where the Song is.
        t.on_state(PlaybackState::Playing, 30_000, 1_090_000);
        assert_eq!(told(&rx), vec![Some(song("Song", 1_060_000))]);

        t.on_state(PlaybackState::Stopped, 0, 1_100_000);
        assert_eq!(told(&rx), vec![None]);
    }

    #[test]
    fn steady_playback_sends_nothing_but_a_seek_moves_the_bar() {
        let (mut t, rx) = tracker();
        t.on_state(PlaybackState::Playing, 0, 1_000_000);
        t.on_track_changed(
            "song",
            Some("Song"),
            Some("Artist"),
            Some("Album"),
            180_000,
            1_000_000,
        );
        assert_eq!(told(&rx).len(), 1);

        // Ticks that run a little late or early are the same playback.
        for (pos, now) in [(1_000, 1_001_000), (2_000, 1_002_500), (3_000, 1_003_200)] {
            t.on_position(pos, 180_000, now);
        }
        assert_eq!(told(&rx), vec![]);

        // Jumped to two minutes in.
        t.on_position(120_000, 180_000, 1_004_000);
        assert_eq!(told(&rx), vec![Some(song("Song", 884_000))]);
    }

    #[test]
    fn the_next_song_replaces_the_last_and_buffering_changes_nothing() {
        let (mut t, rx) = tracker();
        t.on_state(PlaybackState::Playing, 0, 1_000_000);
        t.on_track_changed(
            "one",
            Some("One"),
            Some("Artist"),
            Some("Album"),
            180_000,
            1_000_000,
        );
        t.on_state(PlaybackState::Buffering, 0, 1_000_500);
        t.on_track_changed(
            "two",
            Some("Two"),
            Some("Artist"),
            Some("Album"),
            180_000,
            1_180_000,
        );
        assert_eq!(
            told(&rx),
            vec![Some(song("One", 1_000_000)), Some(song("Two", 1_180_000))]
        );
    }

    #[test]
    fn a_duration_that_turns_up_late_is_sent_on() {
        let (mut t, rx) = tracker();
        t.on_state(PlaybackState::Playing, 0, 1_000_000);
        t.on_track_changed(
            "song",
            Some("Song"),
            Some("Artist"),
            Some("Album"),
            0,
            1_000_000,
        );
        t.on_position(500, 180_000, 1_000_500);
        let sent = told(&rx);
        assert_eq!(sent.len(), 2);
        assert_eq!(sent[0].as_ref().unwrap().duration_ms, 0);
        assert_eq!(sent[1].as_ref().unwrap().duration_ms, 180_000);
    }

    #[test]
    fn a_song_without_a_title_shows_nothing() {
        let (mut t, rx) = tracker();
        t.on_state(PlaybackState::Playing, 0, 1_000_000);
        t.on_track_changed("song", None, Some("Artist"), None, 180_000, 1_000_000);
        assert_eq!(told(&rx), vec![]);
    }

    // ---- what goes to Discord ----

    #[test]
    fn text_fits_what_discord_accepts() {
        assert_eq!(field("  "), None);
        assert_eq!(
            field("X").unwrap().encode_utf16().count(),
            2,
            "padded to the minimum"
        );
        assert_eq!(field(" Song ").as_deref(), Some("Song"));

        let long = "a".repeat(200);
        let cut = field(&long).unwrap();
        assert_eq!(cut.encode_utf16().count(), 128);
        assert!(cut.ends_with('…'));

        // Counted the way Discord counts: an emoji is two units, and is never
        // split in half.
        let emoji = "🎵".repeat(100);
        let cut = field(&emoji).unwrap();
        assert!(cut.encode_utf16().count() <= 128);
        assert!(cut.trim_end_matches('…').chars().all(|c| c == '🎵'));
    }

    #[test]
    fn the_activity_is_a_listening_one_with_a_progress_bar() {
        let value = serde_json::to_value(activity(&song("Song", 1_000_000), None)).unwrap();
        assert_eq!(value["type"], json!(2), "Listening");
        assert_eq!(value["details"], json!("Song"));
        assert_eq!(value["state"], json!("Artist"));
        assert_eq!(value["assets"]["large_text"], json!("Album"));
        assert_eq!(value["assets"]["large_image"], json!(ICON_URL));
        assert!(
            value["assets"].get("small_image").is_none(),
            "no badge on the icon itself"
        );
        assert_eq!(value["timestamps"]["start"], json!(1_000_000));
        assert_eq!(value["timestamps"]["end"], json!(1_180_000));

        let bare = Listening {
            artist: None,
            album: None,
            duration_ms: 0,
            ..song("Song", 5)
        };
        let value = serde_json::to_value(activity(&bare, None)).unwrap();
        assert!(value.get("state").is_none());
        assert!(value["assets"].get("large_text").is_none());
        assert!(
            value["timestamps"].get("end").is_none(),
            "no bar without a length"
        );
    }

    #[test]
    fn the_cover_is_the_picture_and_reverb_the_badge() {
        let cover = "https://archive.org/cover.jpg";
        let value = serde_json::to_value(activity(&song("Song", 0), Some(cover))).unwrap();
        assert_eq!(value["assets"]["large_image"], json!(cover));
        assert_eq!(value["assets"]["large_text"], json!("Album"));
        assert_eq!(value["assets"]["small_image"], json!(ICON_URL));
        assert_eq!(value["assets"]["small_text"], json!("Reverb"));
    }

    #[test]
    fn only_a_release_id_makes_a_cover_url() {
        let id = "76df3287-6cda-33eb-8e9a-044b5e15ffdd";
        let url = format!("https://coverartarchive.org/release/{id}/front-250");
        assert_eq!(front_cover_url(id), Some(url.clone()));
        // Tags come in all shapes; the same id is the same URL.
        assert_eq!(
            front_cover_url(&format!(" {} ", id.to_uppercase())),
            Some(url)
        );

        assert_eq!(front_cover_url(""), None);
        assert_eq!(front_cover_url("not an id"), None);
        assert_eq!(front_cover_url(&format!("{id}/../../other")), None);
    }

    // ---- worker ----

    #[derive(Debug, Clone, PartialEq)]
    enum Seen {
        Connected,
        /// Title, and cover if any.
        Set(String, Option<String>),
        Cleared,
        Closed,
    }

    /// A pretend Discord: logs what it is told, and goes away on request.
    #[derive(Clone, Default)]
    struct FakeDiscord {
        log: Arc<Mutex<Vec<Seen>>>,
        running: Arc<Mutex<bool>>,
    }

    struct FakeConn(FakeDiscord);

    impl Connection for FakeConn {
        fn set(&mut self, listening: &Listening, cover: Option<&str>) -> Result<(), String> {
            self.0
                .answer(Seen::Set(listening.title.clone(), cover.map(str::to_owned)))
        }
        fn clear(&mut self) -> Result<(), String> {
            self.0.answer(Seen::Cleared)
        }
    }

    impl Drop for FakeConn {
        fn drop(&mut self) {
            self.0.log.lock().unwrap().push(Seen::Closed);
        }
    }

    impl FakeDiscord {
        fn running() -> Self {
            let discord = Self::default();
            discord.set_running(true);
            discord
        }
        fn set_running(&self, running: bool) {
            *self.running.lock().unwrap() = running;
        }
        fn answer(&self, seen: Seen) -> Result<(), String> {
            if !*self.running.lock().unwrap() {
                return Err("broken pipe".into());
            }
            self.log.lock().unwrap().push(seen);
            Ok(())
        }
        fn connector(&self) -> impl FnMut() -> Result<Box<dyn Connection>, String> {
            let discord = self.clone();
            move || {
                discord.answer(Seen::Connected)?;
                Ok(Box::new(FakeConn(discord.clone())) as Box<dyn Connection>)
            }
        }
        fn seen(&self) -> Vec<Seen> {
            std::mem::take(&mut *self.log.lock().unwrap())
        }
    }

    fn set(title: &str) -> Seen {
        Seen::Set(title.into(), None)
    }

    fn no_cover(_: &str) -> Option<String> {
        None
    }

    #[test]
    fn nothing_connects_until_there_is_something_to_show() {
        let discord = FakeDiscord::running();
        let mut w = Worker::new(true, discord.connector(), no_cover);
        let now = Instant::now();
        w.sync(now);
        assert_eq!(w.wait(now), None);
        assert_eq!(discord.seen(), vec![]);

        w.apply(Msg::Show(Some(song("Song", 0))));
        w.sync(now);
        assert_eq!(discord.seen(), vec![Seen::Connected, set("Song")]);
        assert_eq!(w.wait(now), None, "caught up");
    }

    #[test]
    fn off_shows_nothing_and_turning_it_off_disconnects() {
        let discord = FakeDiscord::running();
        let mut w = Worker::new(false, discord.connector(), no_cover);
        let now = Instant::now();
        w.apply(Msg::Show(Some(song("Song", 0))));
        w.sync(now);
        assert_eq!(w.wait(now), None);
        assert_eq!(discord.seen(), vec![]);

        // Turned on mid-Song: it shows at once.
        w.apply(Msg::Enabled(true));
        w.sync(now);
        assert_eq!(discord.seen(), vec![Seen::Connected, set("Song")]);

        // And off again: closing takes it down, without waiting out the pace.
        w.apply(Msg::Enabled(false));
        w.sync(now + Duration::from_millis(10));
        assert_eq!(discord.seen(), vec![Seen::Closed]);
    }

    #[test]
    fn updates_are_paced_and_only_the_newest_goes() {
        let discord = FakeDiscord::running();
        let mut w = Worker::new(true, discord.connector(), no_cover);
        let t0 = Instant::now();
        w.apply(Msg::Show(Some(song("One", 0))));
        w.sync(t0);
        discord.seen();

        // Three skips in a second.
        let t1 = t0 + Duration::from_secs(1);
        for title in ["Two", "Three", "Four"] {
            w.apply(Msg::Show(Some(song(title, 0))));
            w.sync(t1);
        }
        assert_eq!(discord.seen(), vec![], "held back");
        assert_eq!(
            w.wait(t1),
            Some(MIN_UPDATE_INTERVAL - Duration::from_secs(1))
        );

        w.sync(t0 + MIN_UPDATE_INTERVAL);
        assert_eq!(discord.seen(), vec![set("Four")]);

        // Pausing clears, at the same pace.
        w.apply(Msg::Show(None));
        w.sync(t0 + MIN_UPDATE_INTERVAL * 2);
        assert_eq!(discord.seen(), vec![Seen::Cleared]);
    }

    #[test]
    fn discord_turning_up_later_is_found() {
        let discord = FakeDiscord::default();
        let mut w = Worker::new(true, discord.connector(), no_cover);
        let t0 = Instant::now();
        w.apply(Msg::Show(Some(song("Song", 0))));
        w.sync(t0);
        assert_eq!(w.wait(t0), Some(RETRY), "not running: look again later");

        discord.set_running(true);
        w.sync(t0 + Duration::from_secs(1));
        assert_eq!(discord.seen(), vec![], "not before the retry is due");

        w.sync(t0 + RETRY);
        assert_eq!(discord.seen(), vec![Seen::Connected, set("Song")]);
    }

    #[test]
    fn a_restarted_discord_gets_the_presence_back_at_once() {
        let discord = FakeDiscord::running();
        let mut w = Worker::new(true, discord.connector(), no_cover);
        let t0 = Instant::now();
        w.apply(Msg::Show(Some(song("One", 0))));
        w.sync(t0);
        discord.seen();

        // Discord quit and came back since: the connection Reverb holds is
        // dead, but the next update makes a new one rather than waiting out a
        // retry.
        let stale = FakeDiscord {
            log: discord.log.clone(),
            running: Arc::default(),
        };
        w.conn = Some(Box::new(FakeConn(stale)));
        discord.seen();

        w.apply(Msg::Show(Some(song("Two", 0))));
        w.sync(t0 + MIN_UPDATE_INTERVAL);
        assert_eq!(
            discord.seen(),
            vec![Seen::Closed, Seen::Connected, set("Two")]
        );
    }

    #[test]
    fn a_pause_with_no_discord_needs_no_connection() {
        let discord = FakeDiscord::default();
        let mut w = Worker::new(true, discord.connector(), no_cover);
        let t0 = Instant::now();
        w.apply(Msg::Show(Some(song("Song", 0))));
        w.sync(t0);
        w.apply(Msg::Show(None));
        w.sync(t0);
        assert_eq!(w.wait(t0), None, "nothing shows, so nothing is owed");
    }

    #[test]
    fn a_cover_is_looked_for_only_with_discord_there_to_show_it() {
        let discord = FakeDiscord::default();
        let looked_up = Arc::new(Mutex::new(Vec::<String>::new()));
        let cover = {
            let looked_up = looked_up.clone();
            move |song_id: &str| {
                looked_up.lock().unwrap().push(song_id.to_owned());
                Some(format!("https://archive.org/{song_id}.jpg"))
            }
        };
        let mut w = Worker::new(true, discord.connector(), cover);
        let t0 = Instant::now();
        w.apply(Msg::Show(Some(song("Song", 0))));
        w.sync(t0);
        assert!(
            looked_up.lock().unwrap().is_empty(),
            "no Discord, no lookup"
        );

        discord.set_running(true);
        w.sync(t0 + RETRY);
        assert_eq!(*looked_up.lock().unwrap(), vec!["song".to_owned()]);
        assert_eq!(
            discord.seen(),
            vec![
                Seen::Connected,
                Seen::Set("Song".into(), Some("https://archive.org/song.jpg".into()))
            ]
        );
    }

    #[test]
    fn the_worker_thread_stops_with_the_last_handle() {
        let discord = FakeDiscord::running();
        let (tx, rx) = crossbeam_channel::unbounded();
        let worker = Worker::new(true, discord.connector(), no_cover);
        let thread = std::thread::spawn(move || run(rx, worker));
        tx.send(Msg::Show(Some(song("Song", 0)))).unwrap();
        drop(tx);
        thread.join().unwrap();
        assert_eq!(
            discord.seen(),
            vec![Seen::Connected, set("Song"), Seen::Closed]
        );
    }
}
