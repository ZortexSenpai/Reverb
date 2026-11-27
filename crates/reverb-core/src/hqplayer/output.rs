//! The HQPlayer output: a thread that turns `EngineCommand`s into HQPlayer
//! requests, and HQPlayer's status into `EngineEvent`s.
//!
//! A Song started on purpose replaces HQPlayer's playlist; the one the
//! frontend wants next is appended behind it. HQPlayer plays its playlist
//! through without a gap, and Reverb sees the move to the next entry in the
//! status it polls. As with the local engine, the frontend owns the Queue and
//! only ever says what plays now and what plays next.

use std::time::{Duration, Instant};

use crossbeam_channel::{Receiver, RecvTimeoutError, Sender};

use super::client::{ClientError, Connection};
use super::protocol::{self, Metadata, Status, TransportState, VolumeRange};
use super::relay::SongRelay;
use super::Endpoint;
use crate::audio::{
    EngineCommand, EngineEvent, HqPlayerPath, PlaybackState, SignalPathInfo, TrackChangeReason,
    TrackSource,
};

/// How often HQPlayer is asked how it is doing while a Song is on.
const POLL_PLAYING: Duration = Duration::from_millis(500);
/// ...and while nothing is, to follow its volume and notice it going away.
/// Also keeps the connection from idling out (HQPlayer drops it after about
/// two and a half minutes of silence).
const POLL_IDLE: Duration = Duration::from_secs(2);
/// How soon to try again after HQPlayer could not be reached.
const RECONNECT_AFTER: Duration = Duration::from_secs(3);
/// How long a stop may take to finish before the next Song goes in anyway.
const STOP_SETTLES: Duration = Duration::from_secs(6);
/// HQPlayer answers a playlist addition only once it has fetched the start of
/// the Song, through the relay, from Navidrome.
const ADD_TIMEOUT: Duration = Duration::from_secs(30);
/// A freshly started HQPlayer refuses additions until its reader is ready.
const ADD_ATTEMPTS: u32 = 4;
const ADD_RETRY_AFTER: Duration = Duration::from_millis(1500);
/// HQPlayer does not always take the first `Play`; ask again after this.
const PLAY_AGAIN_AFTER: Duration = Duration::from_secs(6);
/// How long HQPlayer may take to start a Song (DSD output can take seconds)
/// before Reverb gives up on it.
const START_TIMEOUT: Duration = Duration::from_secs(30);
/// Navidrome expires now-playing entries; re-announce this often.
const NOW_PLAYING_EVERY: Duration = Duration::from_secs(30);
/// HQPlayer's volume readings this soon after Reverb set it are Reverb's own
/// change arriving (rounded to HQPlayer's steps), not someone else's.
const OWN_VOLUME_SETTLES: Duration = Duration::from_millis(1500);
/// The span of the volume control, in dB below HQPlayer's maximum — the same
/// taper as the local engine's.
const VOLUME_SPAN_DB: f32 = 60.0;

/// File types HQPlayer decodes from a stream. Anything else (m4a, ALAC, AAC,
/// APE, WMA...) it refuses, so Navidrome is asked for MP3 instead.
const HQPLAYER_FORMATS: &[&str] =
    &["flac", "wav", "aif", "aiff", "aifc", "dsf", "dff", "wv", "mp3", "mp2", "ogg", "oga"];
/// The bit rate of that MP3, kbps.
const TRANSCODE_KBPS: &str = "320";

/// The Song HQPlayer has (or is about to have) on.
struct Current {
    src: TrackSource,
    /// Its entry in HQPlayer's playlist, counted from 1.
    index: u32,
    duration_ms: u64,
    position_ms: u64,
    /// HQPlayer has been seen playing it. Until then a stopped HQPlayer is
    /// one still opening it, not one that finished it.
    started: bool,
    requested_at: Instant,
    /// `Play` was sent a second time.
    asked_again: bool,
    scrobbled: bool,
}

/// The Song placed after `Current` in HQPlayer's playlist.
struct Queued {
    src: TrackSource,
    index: u32,
}

struct Output {
    endpoint: Endpoint,
    event_tx: Sender<EngineEvent>,
    conn: Option<Connection>,
    last_connect: Option<Instant>,
    /// HQPlayer 4, which takes playlist entries without metadata.
    legacy: bool,
    /// Started on the first Song; it goes with the output.
    relay: Option<SongRelay>,
    volume_range: Option<VolumeRange>,
    /// What HQPlayer's volume last read, in dB, and when Reverb last set it.
    volume_db: Option<f32>,
    volume_set_at: Option<Instant>,
    state: PlaybackState,
    current: Option<Current>,
    /// What the frontend wants after the current Song.
    next: Option<TrackSource>,
    queued: Option<Queued>,
    /// Asked for before HQPlayer had the Song going; done once it has.
    pending_seek_ms: Option<u64>,
    pending_pause: bool,
    scrobble_percent: u32,
    last_poll: Instant,
    last_now_playing: Instant,
    /// HQPlayer's processing as last reported, to tell when it changes.
    path: HqPlayerPath,
    /// The current Song's own rate, once HQPlayer has said.
    source_rate: Option<u32>,
}

pub(super) fn run(
    endpoint: Endpoint,
    cmd_rx: Receiver<EngineCommand>,
    event_tx: Sender<EngineEvent>,
) {
    let mut output = Output {
        path: HqPlayerPath { address: endpoint.address(), ..HqPlayerPath::default() },
        endpoint,
        event_tx,
        conn: None,
        last_connect: None,
        legacy: false,
        relay: None,
        volume_range: None,
        volume_db: None,
        volume_set_at: None,
        state: PlaybackState::Stopped,
        current: None,
        next: None,
        queued: None,
        pending_seek_ms: None,
        pending_pause: false,
        scrobble_percent: 80,
        last_poll: Instant::now(),
        last_now_playing: Instant::now(),
        source_rate: None,
    };
    // Connect straight away: the volume control should show HQPlayer's
    // volume from the moment it is picked, before anything plays.
    output.poll();
    loop {
        let interval = if output.current.is_some() { POLL_PLAYING } else { POLL_IDLE };
        let wait = interval.saturating_sub(output.last_poll.elapsed());
        match cmd_rx.recv_timeout(wait) {
            Ok(EngineCommand::Shutdown) | Err(RecvTimeoutError::Disconnected) => break,
            Ok(cmd) => {
                // Starting a Song waits on HQPlayer for a second or more, so
                // skipping through several stacks them up: only the last one
                // the listener landed on is worth starting.
                let mut batch = vec![cmd];
                batch.extend(cmd_rx.try_iter());
                let shutdown = batch.iter().any(|c| matches!(c, EngineCommand::Shutdown));
                for cmd in coalesce(batch) {
                    output.handle(cmd);
                }
                if shutdown {
                    break;
                }
            }
            Err(RecvTimeoutError::Timeout) => {}
        }
        if output.last_poll.elapsed() >= interval {
            output.poll();
        }
    }
    log::info!("HQPlayer output for {} ended", output.endpoint.address());
}

/// Drop what a later `PlayTrack` in the same batch makes moot: earlier Songs
/// and anything aimed at them. Settings stay, in order; `Shutdown` goes (the
/// caller acts on it once the rest is done).
fn coalesce(batch: Vec<EngineCommand>) -> Vec<EngineCommand> {
    let last_play = batch.iter().rposition(|c| matches!(c, EngineCommand::PlayTrack(_)));
    batch
        .into_iter()
        .enumerate()
        .filter(|(i, cmd)| match cmd {
            EngineCommand::Shutdown => false,
            EngineCommand::SetVolume { .. } | EngineCommand::SetScrobblePercent(_) => true,
            _ => last_play.is_none_or(|last| *i >= last),
        })
        .map(|(_, cmd)| cmd)
        .collect()
}

impl Output {
    fn emit(&self, event: EngineEvent) {
        let _ = self.event_tx.send(event);
    }

    fn set_state(&mut self, state: PlaybackState) {
        if self.state != state {
            self.state = state;
            self.emit(EngineEvent::State { state });
        }
    }

    // ---- connection -------------------------------------------------------

    /// The open connection, opening one if it is time to try.
    fn connection(&mut self) -> Option<&mut Connection> {
        if self.conn.is_none() {
            let due = self.last_connect.is_none_or(|at| at.elapsed() >= RECONNECT_AFTER);
            if !due {
                return None;
            }
            if let Err(e) = self.connect_now() {
                log::debug!("{e}");
                return None;
            }
        }
        self.conn.as_mut()
    }

    /// Open a connection now, whatever the retry pacing says — for a request
    /// the listener is waiting on.
    fn connect_now(&mut self) -> Result<(), ClientError> {
        if self.conn.is_some() {
            return Ok(());
        }
        self.last_connect = Some(Instant::now());
        let mut conn = Connection::open(&self.endpoint)?;
        // Also the proof that HQPlayer is listening, not just accepting.
        let info = conn.info()?;
        log::info!(
            "connected to {} {} at {}",
            info.name,
            info.version.as_deref().unwrap_or("?"),
            self.endpoint.address()
        );
        self.legacy = info.major_version().is_some_and(|major| major < 5);
        self.conn = Some(conn);
        self.volume_range = self
            .request(&protocol::volume_range())
            .ok()
            .and_then(|reply| protocol::parse_volume_range(&reply));
        // Forgetting the last volume makes the next status report it, which
        // the volume control then adopts.
        self.volume_db = None;
        Ok(())
    }

    /// One request on the open connection. A connection that fails is
    /// dropped — after a timeout a late reply could still be on its way, and
    /// it must not be taken for the answer to something else — while one that
    /// merely refused the request is kept.
    fn request_within(&mut self, xml: &str, timeout: Duration) -> Result<String, ClientError> {
        let Some(conn) = self.conn.as_mut() else {
            return Err(ClientError::Io(std::io::ErrorKind::NotConnected.into()));
        };
        let result = conn.request_within(xml, timeout);
        if let Err(e) = &result {
            if !matches!(e, ClientError::Refused(_)) {
                log::warn!("HQPlayer at {}: {e}", self.endpoint.address());
                self.conn = None;
            }
        }
        result
    }

    fn request(&mut self, xml: &str) -> Result<String, ClientError> {
        self.request_within(xml, super::client::REPLY_TIMEOUT)
    }

    /// HQPlayer's status, read now.
    fn status(&mut self) -> Result<Status, ClientError> {
        let reply = self.request(&protocol::status())?;
        protocol::parse_status(&reply).ok_or(ClientError::BadReply)
    }

    /// The connection went while a Song was on: say so, and stop.
    fn lost(&mut self, error: &ClientError) {
        let track_id = self.current.take().map(|c| c.src.track_id);
        self.queued = None;
        if track_id.is_some() {
            self.emit(EngineEvent::Error {
                code: "hqplayer".into(),
                message: error.to_string(),
                track_id,
                recoverable: true,
            });
        }
        self.set_state(PlaybackState::Stopped);
    }

    // ---- commands ---------------------------------------------------------

    fn handle(&mut self, cmd: EngineCommand) {
        match cmd {
            EngineCommand::PlayTrack(src) => self.play_track(src),
            EngineCommand::SetNext(next) => {
                self.next = next;
                self.sync_next();
            }
            EngineCommand::Pause => self.pause(),
            EngineCommand::Resume => self.resume(),
            EngineCommand::Toggle => {
                if self.state == PlaybackState::Paused {
                    self.resume();
                } else {
                    self.pause();
                }
            }
            EngineCommand::Stop => self.stop(),
            EngineCommand::Seek { position_ms } => self.seek(position_ms),
            EngineCommand::SetVolume { volume } => self.set_volume(volume),
            EngineCommand::SetScrobblePercent(percent) => {
                self.scrobble_percent = percent.clamp(10, 100);
            }
            // HQPlayer has its own EQ (convolution), gain and output setup.
            EngineCommand::SetEq(_)
            | EngineCommand::SetReplayGain(_)
            | EngineCommand::SetCrossfade { .. }
            | EngineCommand::SetOutput(_)
            | EngineCommand::SetStandby(_)
            | EngineCommand::Shutdown => {}
        }
    }

    /// Where HQPlayer can fetch `src`: a path on the relay, at this
    /// computer's address on HQPlayer's network. Songs in a format HQPlayer
    /// cannot decode are fetched as MP3.
    fn song_url(&mut self, src: &TrackSource) -> Result<String, String> {
        let url = reqwest::Url::parse(&src.url).map_err(|e| format!("bad stream URL: {e}"))?;
        let ext = src.ext_hint.as_deref().unwrap_or("").to_ascii_lowercase();
        let (url, ext) = if HQPLAYER_FORMATS.contains(&ext.as_str()) {
            (url, ext)
        } else {
            log::info!("HQPlayer cannot decode {ext:?}; asking the server for MP3");
            (transcoded(&url), "mp3".to_owned())
        };
        if self.relay.is_none() {
            let relay =
                SongRelay::start().map_err(|e| format!("could not serve songs to HQPlayer: {e}"))?;
            self.relay = Some(relay);
        }
        let relay = self.relay.as_ref().expect("started above");
        let local_ip = self.conn.as_ref().map(|c| c.local_ip).ok_or("not connected")?;
        let path = relay.register(url, &ext);
        Ok(relay.url(local_ip, &path))
    }

    /// Append `src` to HQPlayer's playlist. A freshly started HQPlayer turns
    /// additions down until it is ready, so a refusal is tried again a few
    /// times before it counts.
    fn add_to_playlist(&mut self, src: &TrackSource) -> Result<(), String> {
        let url = self.song_url(src)?;
        let xml = protocol::playlist_add(&url, &metadata(src), self.legacy);
        let mut attempt = 1;
        loop {
            match self.request_within(&xml, ADD_TIMEOUT) {
                Ok(_) => return Ok(()),
                Err(ClientError::Refused(reason)) if attempt < ADD_ATTEMPTS => {
                    log::debug!("HQPlayer turned the song down ({reason}); trying again");
                    attempt += 1;
                    std::thread::sleep(ADD_RETRY_AFTER);
                }
                Err(ClientError::Refused(reason)) => {
                    return Err(format!("HQPlayer could not open the song: {reason}"));
                }
                Err(e) => return Err(e.to_string()),
            }
        }
    }

    /// Stop, and wait for HQPlayer to actually have stopped: it answers
    /// first, and an entry still playing cannot be cleared away.
    fn stop_and_settle(&mut self) -> Result<(), ClientError> {
        self.request(&protocol::stop())?;
        let deadline = Instant::now() + STOP_SETTLES;
        while Instant::now() < deadline {
            if self.status()?.state == TransportState::Stopped {
                return Ok(());
            }
            std::thread::sleep(Duration::from_millis(150));
        }
        log::warn!("HQPlayer was still stopping after {STOP_SETTLES:?}; carrying on");
        Ok(())
    }

    fn play_track(&mut self, src: TrackSource) {
        self.current = None;
        self.queued = None;
        self.pending_seek_ms = None;
        self.pending_pause = false;
        self.set_state(PlaybackState::Buffering);

        if let Err(e) = self.connect_now() {
            self.fail_track(src.track_id, e.to_string());
            return;
        }
        // A fresh playlist with just this Song in it, then play it.
        let loaded = self
            .stop_and_settle()
            .and_then(|()| self.request(&protocol::playlist_clear()))
            .map_err(|e| e.to_string())
            .and_then(|_| self.add_to_playlist(&src))
            .and_then(|()| self.request(&protocol::play()).map_err(|e| e.to_string()));
        if let Err(message) = loaded {
            self.fail_track(src.track_id, message);
            return;
        }

        let duration_ms = src.duration_ms.unwrap_or(0);
        self.emit(EngineEvent::TrackChanged {
            track_id: src.track_id.clone(),
            reason: TrackChangeReason::Command,
            title: src.title.clone(),
            artist: src.artist.clone(),
            album: src.album.clone(),
            duration_ms,
        });
        self.announce_now_playing(&src, duration_ms);
        self.emit(EngineEvent::Position { position_ms: 0, duration_ms });
        self.current = Some(Current {
            src,
            index: 1,
            duration_ms,
            position_ms: 0,
            started: false,
            requested_at: Instant::now(),
            asked_again: false,
            scrobbled: false,
        });
        self.emit_signal_path();
        self.sync_next();
    }

    fn fail_track(&mut self, track_id: String, message: String) {
        self.current = None;
        self.queued = None;
        self.set_state(PlaybackState::Stopped);
        self.emit(EngineEvent::Error {
            code: "hqplayer".into(),
            message,
            track_id: Some(track_id),
            recoverable: true,
        });
    }

    /// Make the entry after the current Song in HQPlayer's playlist the one
    /// the frontend wants next — or none.
    fn sync_next(&mut self) {
        let Some(current_index) = self.current.as_ref().map(|c| c.index) else { return };
        let wanted = self.next.clone();
        let same = match (&self.queued, &wanted) {
            (Some(queued), Some(next)) => queued.src.track_id == next.track_id,
            (None, None) => true,
            _ => false,
        };
        if same {
            return;
        }
        if let Some(queued) = self.queued.take() {
            if let Err(e) = self.request(&protocol::playlist_remove(queued.index)) {
                // Left in, HQPlayer plays it after the current Song, and the
                // status poll reports it as what came next — which it is.
                log::warn!("could not take the next song out of HQPlayer's playlist: {e}");
                self.queued = Some(queued);
                return;
            }
        }
        let Some(next) = wanted else { return };
        match self.add_to_playlist(&next) {
            Ok(()) => self.queued = Some(Queued { src: next, index: current_index + 1 }),
            Err(message) => log::warn!("could not line up the next song in HQPlayer: {message}"),
        }
    }

    fn pause(&mut self) {
        if !matches!(self.state, PlaybackState::Playing | PlaybackState::Buffering) {
            return;
        }
        if self.current.as_ref().is_some_and(|c| !c.started) {
            // Still opening: pause the moment it plays.
            self.pending_pause = true;
            self.set_state(PlaybackState::Paused);
            return;
        }
        // `Pause` toggles, so only send it to an HQPlayer that is playing.
        match self.status() {
            Ok(status) if status.state == TransportState::Playing => {
                if let Err(e) = self.request(&protocol::pause()) {
                    log::warn!("HQPlayer would not pause: {e}");
                    return;
                }
            }
            Ok(_) => {}
            Err(e) => {
                log::warn!("HQPlayer would not pause: {e}");
                return;
            }
        }
        self.set_state(PlaybackState::Paused);
    }

    fn resume(&mut self) {
        if self.state != PlaybackState::Paused {
            return;
        }
        if self.pending_pause {
            self.pending_pause = false;
            self.set_state(PlaybackState::Buffering);
            return;
        }
        // Resuming is a second `Pause`, which picks up where it left off.
        match self.status() {
            Ok(status) if status.state == TransportState::Paused => {
                if let Err(e) = self.request(&protocol::pause()) {
                    log::warn!("HQPlayer would not resume: {e}");
                    return;
                }
            }
            // Stopped meanwhile: the next poll says so.
            Ok(_) => {}
            Err(e) => {
                log::warn!("HQPlayer would not resume: {e}");
                return;
            }
        }
        self.set_state(PlaybackState::Playing);
        // The server's now-playing entry likely expired while paused.
        if let Some((src, duration_ms)) =
            self.current.as_ref().map(|c| (c.src.clone(), c.duration_ms))
        {
            self.announce_now_playing(&src, duration_ms);
        }
    }

    fn stop(&mut self) {
        self.current = None;
        self.queued = None;
        self.pending_seek_ms = None;
        self.pending_pause = false;
        if self.conn.is_some() {
            if let Err(e) = self.request(&protocol::stop()) {
                log::warn!("HQPlayer would not stop: {e}");
            }
        }
        self.set_state(PlaybackState::Stopped);
    }

    fn seek(&mut self, position_ms: u64) {
        let Some(current) = self.current.as_mut() else { return };
        let (started, duration_ms, track_id) =
            (current.started, current.duration_ms, current.src.track_id.clone());
        if !started {
            current.position_ms = position_ms;
            self.pending_seek_ms = Some(position_ms);
            self.emit(EngineEvent::Position { position_ms, duration_ms });
            return;
        }
        match self.request(&protocol::seek(position_ms / 1000)) {
            Ok(_) => {
                if let Some(current) = self.current.as_mut() {
                    current.position_ms = position_ms;
                }
                self.emit(EngineEvent::Position { position_ms, duration_ms });
            }
            // A stream without byte ranges (a transcode) cannot be sought.
            Err(ClientError::Refused(reason)) => self.emit(EngineEvent::Error {
                code: "seek-unsupported".into(),
                message: format!("HQPlayer cannot seek in this song: {reason}"),
                track_id: Some(track_id),
                recoverable: true,
            }),
            Err(e) => log::warn!("HQPlayer would not seek: {e}"),
        }
    }

    fn set_volume(&mut self, volume: f32) {
        let Some(range) = self.volume_range.filter(|r| r.adjustable) else { return };
        if self.conn.is_none() {
            return;
        }
        let db = volume_to_db(volume, range);
        match self.request(&protocol::set_volume(db)) {
            // With nothing in its playlist HQPlayer complains, and sets the
            // volume anyway.
            Ok(_) | Err(ClientError::Refused(_)) => {
                self.volume_db = Some(db);
                self.volume_set_at = Some(Instant::now());
            }
            Err(e) => log::warn!("HQPlayer would not take the volume: {e}"),
        }
    }

    // ---- status -----------------------------------------------------------

    fn poll(&mut self) {
        self.last_poll = Instant::now();
        if self.current.is_some() {
            // With a Song on, a connection that went is a Song that went: try
            // once more now rather than on the idle pacing, and say so if
            // HQPlayer is gone — the UI would otherwise show it playing on.
            if let Err(e) = self.connect_now() {
                self.lost(&e);
                return;
            }
        } else if self.connection().is_none() {
            return;
        }
        let status = match self.status() {
            Ok(status) => status,
            Err(e) => {
                if self.conn.is_none() {
                    self.lost(&e);
                } else {
                    log::warn!("HQPlayer sent a status Reverb could not read: {e}");
                }
                return;
            }
        };
        self.follow_volume(&status);
        self.follow_path(&status);
        self.follow_playback(&status);
    }

    /// Adopt a volume that changed in HQPlayer (or on connecting).
    fn follow_volume(&mut self, status: &Status) {
        let (Some(db), Some(range)) = (status.volume_db, self.volume_range) else { return };
        if self.volume_set_at.is_some_and(|at| at.elapsed() < OWN_VOLUME_SETTLES) {
            self.volume_db = Some(db);
            return;
        }
        let moved = self.volume_db.is_none_or(|last| (last - db).abs() > 0.05);
        self.volume_db = Some(db);
        if moved && range.adjustable {
            self.emit(EngineEvent::Volume { volume: db_to_volume(db, range) });
        }
    }

    fn follow_path(&mut self, status: &Status) {
        let path = HqPlayerPath {
            address: self.path.address.clone(),
            mode: status.mode.clone().or_else(|| self.path.mode.clone()),
            filter: status.filter.clone().or_else(|| self.path.filter.clone()),
            shaper: status.shaper.clone().or_else(|| self.path.shaper.clone()),
            output_rate: status.output_rate.or(self.path.output_rate),
        };
        // Read off the status of whichever Song is on; it follows the advance.
        let source_rate = if self.current.as_ref().is_some_and(|c| c.started) {
            status.source_rate
        } else {
            None
        };
        if path != self.path || source_rate != self.source_rate {
            self.path = path;
            self.source_rate = source_rate;
            self.emit_signal_path();
        }
    }

    fn follow_playback(&mut self, status: &Status) {
        let Some(current) = self.current.as_ref() else { return };
        let (was_started, index) = (current.started, current.index);

        // HQPlayer moved on to the entry after the current one: the next
        // Song is playing, gaplessly.
        let advanced = self.queued.as_ref().is_some_and(|q| status.track == Some(q.index));
        if advanced {
            self.advance();
        } else if was_started
            && status.state != TransportState::Stopped
            && status.track.is_some_and(|track| track != index)
        {
            // Someone else picked another entry (HQPlayer's own controls,
            // another app): it is no longer the Queue's Song playing.
            log::info!("HQPlayer moved off Reverb's song; leaving it be");
            self.current = None;
            self.queued = None;
            self.set_state(PlaybackState::Stopped);
            return;
        }

        let current = self.current.as_mut().expect("checked above");
        let was_started = current.started;
        if matches!(status.state, TransportState::Playing | TransportState::Paused) {
            if let Some(position_ms) = status.position_ms {
                current.position_ms = position_ms;
            }
            if let Some(length_ms) = status.length_ms {
                current.duration_ms = length_ms;
            }
        }
        match status.state {
            TransportState::Playing => {
                current.started = true;
                if !was_started {
                    // The entry HQPlayer put it at, should the playlist have
                    // held more than Reverb thought.
                    if let Some(track) = status.track {
                        current.index = track;
                        if let Some(queued) = self.queued.as_mut() {
                            queued.index = track + 1;
                        }
                    }
                    // Whatever was asked for while it was still opening.
                    if let Some(position_ms) = self.pending_seek_ms.take() {
                        self.seek(position_ms);
                    }
                    if std::mem::take(&mut self.pending_pause) {
                        if let Err(e) = self.request(&protocol::pause()) {
                            log::warn!("HQPlayer would not pause: {e}");
                        }
                        self.set_state(PlaybackState::Paused);
                        return;
                    }
                }
                // Also when it was resumed in HQPlayer itself.
                self.set_state(PlaybackState::Playing);
                self.report_progress();
            }
            TransportState::Paused => {
                if was_started {
                    self.set_state(PlaybackState::Paused);
                }
            }
            // On its way to stopped; the next poll sees where it lands.
            TransportState::Stopping => {}
            TransportState::Stopped => {
                let track_id = current.src.track_id.clone();
                let waited = current.requested_at.elapsed();
                let asked_again = current.asked_again;
                if was_started {
                    // Played out — or stopped in HQPlayer itself. Either way
                    // the Queue's Song is over.
                    self.current = None;
                    self.queued = None;
                    self.set_state(PlaybackState::Stopped);
                    self.emit(EngineEvent::TrackEnded { track_id });
                } else if waited >= START_TIMEOUT {
                    let message = format!(
                        "HQPlayer at {} did not start the song. It fetches songs from Reverb: \
                         is this computer's firewall letting it in?",
                        self.endpoint.address()
                    );
                    self.fail_track(track_id, message);
                } else if waited >= PLAY_AGAIN_AFTER && !asked_again {
                    current.asked_again = true;
                    log::debug!("HQPlayer has not started the song yet; asking again");
                    let _ = self.request(&protocol::play());
                }
            }
        }
    }

    /// The queued Song became the current one.
    fn advance(&mut self) {
        let Some(queued) = self.queued.take() else { return };
        // The frontend names the Song after this one once it hears of the change.
        self.next = None;
        let src = queued.src;
        let duration_ms = src.duration_ms.unwrap_or(0);
        self.emit(EngineEvent::TrackChanged {
            track_id: src.track_id.clone(),
            reason: TrackChangeReason::Natural,
            title: src.title.clone(),
            artist: src.artist.clone(),
            album: src.album.clone(),
            duration_ms,
        });
        self.announce_now_playing(&src, duration_ms);
        self.current = Some(Current {
            src,
            index: queued.index,
            duration_ms,
            position_ms: 0,
            started: true,
            requested_at: Instant::now(),
            asked_again: false,
            scrobbled: false,
        });
        self.emit_signal_path();
    }

    /// Position, scrobble, and the now-playing keepalive, while playing.
    fn report_progress(&mut self) {
        let Some(current) = self.current.as_mut() else { return };
        let (position_ms, duration_ms) = (current.position_ms, current.duration_ms);
        let percent = u64::from(self.scrobble_percent);
        let threshold = if duration_ms == 0 { 240_000 } else { duration_ms * percent / 100 };
        let submit = (!current.scrobbled && position_ms >= threshold).then(|| {
            current.scrobbled = true;
            current.src.clone()
        });
        self.emit(EngineEvent::Position { position_ms, duration_ms });
        if let Some(src) = submit {
            self.emit(scrobble(&src, true, duration_ms));
        }
        if self.last_now_playing.elapsed() >= NOW_PLAYING_EVERY {
            if let Some(src) = self.current.as_ref().map(|c| c.src.clone()) {
                self.announce_now_playing(&src, duration_ms);
            }
        }
    }

    fn announce_now_playing(&mut self, src: &TrackSource, duration_ms: u64) {
        self.emit(scrobble(src, false, duration_ms));
        self.last_now_playing = Instant::now();
    }

    fn emit_signal_path(&self) {
        let Some(current) = self.current.as_ref() else { return };
        let ext = current.src.ext_hint.as_deref().unwrap_or("").to_lowercase();
        let lossless_source = matches!(
            ext.as_str(),
            "flac" | "wav" | "alac" | "aiff" | "aif" | "ape" | "dsf" | "dff" | "wv"
        );
        self.emit(EngineEvent::SignalPath(SignalPathInfo {
            codec: (!ext.is_empty()).then(|| ext.to_uppercase()),
            // 0 until HQPlayer has opened the Song and said.
            source_sample_rate: self.source_rate.unwrap_or(0),
            bit_rate: current.src.bit_rate,
            lossless_source,
            replay_gain_db: None,
            resample_to: None,
            eq_bands: 0,
            output_sample_rate: self.path.output_rate.unwrap_or(0),
            output_device: Some(format!("HQPlayer ({})", self.path.address)),
            device_rate: None,
            os_resampling: false,
            // HQPlayer always processes: that is what it is for.
            quality: "enhanced".into(),
            hqplayer: Some(self.path.clone()),
        }));
    }
}

/// The same Subsonic stream request, asking the server for MP3 instead of the
/// original file.
fn transcoded(url: &reqwest::Url) -> reqwest::Url {
    let pairs: Vec<(String, String)> = url
        .query_pairs()
        .filter(|(key, _)| key != "format" && key != "maxBitRate")
        .map(|(k, v)| (k.into_owned(), v.into_owned()))
        .collect();
    let mut url = url.clone();
    url.query_pairs_mut()
        .clear()
        .extend_pairs(pairs)
        .append_pair("format", "mp3")
        .append_pair("maxBitRate", TRANSCODE_KBPS);
    url
}

fn metadata(src: &TrackSource) -> Metadata<'_> {
    Metadata {
        title: src.title.as_deref(),
        artist: src.artist.as_deref(),
        album: src.album.as_deref(),
        length_ms: src.duration_ms,
    }
}

fn scrobble(src: &TrackSource, submission: bool, duration_ms: u64) -> EngineEvent {
    EngineEvent::Scrobble {
        track_id: src.track_id.clone(),
        submission,
        title: src.title.clone(),
        artist: src.artist.clone(),
        first_artist: src.first_artist.clone(),
        album: src.album.clone(),
        duration_ms,
    }
}

/// Reverb's 0–1 volume as HQPlayer dB: the local engine's taper (60 dB from
/// silence to full), hung from HQPlayer's maximum and kept within its range.
fn volume_to_db(volume: f32, range: VolumeRange) -> f32 {
    if volume < 0.001 {
        return range.min_db;
    }
    let db = range.max_db - (1.0 - volume.clamp(0.0, 1.0)) * VOLUME_SPAN_DB;
    db.clamp(range.min_db, range.max_db)
}

/// HQPlayer dB back to Reverb's 0–1 volume; the inverse of `volume_to_db`
/// inside the range both cover.
fn db_to_volume(db: f32, range: VolumeRange) -> f32 {
    (1.0 - (range.max_db - db) / VOLUME_SPAN_DB).clamp(0.0, 1.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    const RANGE: VolumeRange = VolumeRange { min_db: -60.0, max_db: 0.0, adjustable: true };

    #[test]
    fn volume_follows_the_local_taper() {
        assert_eq!(volume_to_db(1.0, RANGE), 0.0);
        assert_eq!(volume_to_db(0.5, RANGE), -30.0);
        assert_eq!(volume_to_db(0.0, RANGE), -60.0);
    }

    #[test]
    fn volume_stays_inside_a_narrower_range() {
        // HQPlayer Desktop 5 on a Mac ships with -3 dB at the top.
        let narrow = VolumeRange { min_db: -60.0, max_db: -3.0, adjustable: true };
        assert_eq!(volume_to_db(1.0, narrow), -3.0);
        assert_eq!(volume_to_db(0.5, narrow), -33.0);
        assert_eq!(volume_to_db(0.02, narrow), -60.0);
    }

    #[test]
    fn volume_round_trips() {
        for volume in [0.25f32, 0.5, 0.8, 1.0] {
            let back = db_to_volume(volume_to_db(volume, RANGE), RANGE);
            assert!((back - volume).abs() < 1e-5, "{volume} came back as {back}");
        }
        assert_eq!(db_to_volume(-90.0, RANGE), 0.0);
    }

    #[test]
    fn formats_hqplayer_cannot_decode_are_asked_for_as_mp3() {
        let url = reqwest::Url::parse(
            "https://music.example.com/rest/stream?u=me&t=tok&s=salt&v=1.16.1&c=Reverb&id=7&format=raw",
        )
        .unwrap();
        let mp3 = transcoded(&url);
        let pairs: Vec<(String, String)> = mp3.query_pairs().into_owned().collect();
        assert!(pairs.contains(&("format".into(), "mp3".into())), "{mp3}");
        assert!(pairs.contains(&("maxBitRate".into(), "320".into())), "{mp3}");
        assert!(!pairs.contains(&("format".into(), "raw".into())), "{mp3}");
        assert!(pairs.contains(&("id".into(), "7".into())), "the rest is kept: {mp3}");
        assert!(pairs.contains(&("t".into(), "tok".into())), "the rest is kept: {mp3}");
        assert_eq!(mp3.path(), "/rest/stream");
    }

    fn song(id: &str) -> EngineCommand {
        EngineCommand::PlayTrack(TrackSource {
            track_id: id.into(),
            url: "http://server/rest/stream".into(),
            ext_hint: Some("flac".into()),
            duration_ms: None,
            title: None,
            artist: None,
            first_artist: None,
            album: None,
            bit_rate: None,
            replay_gain: None,
        })
    }

    fn played(cmds: &[EngineCommand]) -> Vec<String> {
        cmds.iter()
            .map(|c| match c {
                EngineCommand::PlayTrack(src) => format!("play {}", src.track_id),
                EngineCommand::Seek { position_ms } => format!("seek {position_ms}"),
                EngineCommand::SetNext(_) => "next".into(),
                EngineCommand::SetVolume { .. } => "volume".into(),
                EngineCommand::Pause => "pause".into(),
                other => format!("{other:?}"),
            })
            .collect()
    }

    #[test]
    fn a_run_of_skips_starts_only_the_last_song() {
        let batch = vec![
            song("a"),
            EngineCommand::SetNext(None),
            EngineCommand::SetVolume { volume: 0.5 },
            song("b"),
            EngineCommand::Seek { position_ms: 1000 },
            song("c"),
            EngineCommand::SetNext(None),
            EngineCommand::Pause,
        ];
        assert_eq!(played(&coalesce(batch)), ["volume", "play c", "next", "pause"]);
    }

    #[test]
    fn a_batch_without_a_new_song_is_kept_whole() {
        let batch = vec![EngineCommand::Seek { position_ms: 5 }, EngineCommand::Pause];
        assert_eq!(played(&coalesce(batch)), ["seek 5", "pause"]);
    }
}
