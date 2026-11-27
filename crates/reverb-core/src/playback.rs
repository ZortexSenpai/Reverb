//! Where playback goes: the local audio engine, or an HQPlayer on the network.
//!
//! Both take the same [`EngineCommand`]s and report the same
//! [`EngineEvent`]s, so the rest of the app drives whichever is active
//! through one handle and never needs to know which. Switching between them
//! carries playback across: the Song that was playing continues on the new
//! output from where it was, as a device switch does within the local engine.
//!
//! The local engine is never shut down. While HQPlayer plays it sits in
//! standby with its device released — HQPlayer may be on this computer and
//! need that very device — and it keeps receiving the settings it will need
//! once it plays again.

use std::sync::{Arc, Mutex};
use std::time::Duration;

use crossbeam_channel::{unbounded, Receiver, Sender};

use crate::audio::{
    spawn_engine, EngineCommand, EngineEvent, EngineHandle, OutputSettings, PlaybackState,
    TrackChangeReason, TrackSource,
};
use crate::hqplayer::{self, Endpoint, HqPlayerHandle};

/// The output playback goes to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PlaybackTarget {
    /// The local audio engine, on this device setup.
    Local(OutputSettings),
    /// An HQPlayer, which fetches and plays the Songs itself.
    HqPlayer(Endpoint),
}

impl Default for PlaybackTarget {
    fn default() -> Self {
        Self::Local(OutputSettings::default())
    }
}

/// Which output's events reach the app. Ids only grow, so an HQPlayer output
/// replaced by another one can never pass for its successor.
type OutputId = u64;
const LOCAL: OutputId = 0;

/// How long [`Playback::shutdown`] waits for HQPlayer to hear the stop.
const SHUTDOWN_WAIT: Duration = Duration::from_millis(1500);

struct HqOutput {
    endpoint: Endpoint,
    handle: HqPlayerHandle,
}

struct Outputs {
    local: EngineHandle,
    hq: Option<HqOutput>,
    next_id: OutputId,
}

/// The one handle the app plays through.
pub struct Playback {
    outputs: Mutex<Outputs>,
    /// Shared with the event gates, which keep it current and consult it.
    mirror: Arc<Mutex<Mirror>>,
    event_tx: Sender<EngineEvent>,
}

impl Playback {
    /// Start the local engine — in standby when playback starts on HQPlayer —
    /// and connect to HQPlayer if that is the target. Events from whichever
    /// output is active go to `event_tx`.
    pub fn start(event_tx: Sender<EngineEvent>, target: PlaybackTarget) -> Self {
        let mirror = Arc::new(Mutex::new(Mirror::default()));
        let (local_tx, local_rx) = unbounded();
        let local = match &target {
            PlaybackTarget::Local(output) => spawn_engine(local_tx, output.clone(), false),
            PlaybackTarget::HqPlayer(_) => {
                spawn_engine(local_tx, OutputSettings::default(), true)
            }
        };
        spawn_gate(LOCAL, local_rx, event_tx.clone(), mirror.clone());

        let playback = Self {
            outputs: Mutex::new(Outputs { local, hq: None, next_id: LOCAL + 1 }),
            mirror,
            event_tx,
        };
        if let PlaybackTarget::HqPlayer(endpoint) = target {
            let mut outputs = playback.outputs.lock().unwrap();
            let id = outputs.allocate_id();
            playback.mirror.lock().unwrap().active = id;
            playback.connect_hqplayer(&mut outputs, id, endpoint);
        }
        playback
    }

    /// Route a command to the active output. Settings also reach the local
    /// engine while it is in standby, so it wakes up current.
    pub fn send(&self, cmd: EngineCommand) {
        self.mirror.lock().unwrap().observe_command(&cmd);
        let outputs = self.outputs.lock().unwrap();
        let hq = outputs.hq.as_ref().map(|hq| &hq.handle);
        match cmd {
            // Kept by both: the local engine needs them on waking, and
            // HQPlayer uses what applies to it and ignores the rest.
            EngineCommand::SetVolume { .. } | EngineCommand::SetScrobblePercent(_) => {
                if let Some(hq) = hq {
                    hq.send(clone_setting(&cmd));
                }
                outputs.local.send(cmd);
            }
            // The local engine's own processing; HQPlayer does its own.
            EngineCommand::SetEq(_)
            | EngineCommand::SetReplayGain(_)
            | EngineCommand::SetCrossfade { .. } => outputs.local.send(cmd),
            // Whether the engine sleeps is this router's business.
            EngineCommand::SetStandby(_) | EngineCommand::Shutdown => {
                log::warn!("standby and shutdown are not for callers to route");
            }
            // The device setup only means something to the local engine; a
            // switch to or from HQPlayer goes through `set_target`.
            EngineCommand::SetOutput(_) => outputs.local.send(cmd),
            _ => match hq {
                Some(hq) => hq.send(cmd),
                None => outputs.local.send(cmd),
            },
        }
    }

    /// Make `target` the output, carrying whatever was playing across when
    /// that means switching between the local engine and an HQPlayer.
    pub fn set_target(&self, target: PlaybackTarget) {
        let mut outputs = self.outputs.lock().unwrap();
        match (&outputs.hq, &target) {
            // Within the local engine: it reopens its own stream and keeps
            // the position.
            (None, PlaybackTarget::Local(output)) => {
                outputs.local.send(EngineCommand::SetOutput(output.clone()));
                return;
            }
            (Some(hq), PlaybackTarget::HqPlayer(endpoint)) if &hq.endpoint == endpoint => return,
            _ => {}
        }

        // Hand the events over to the new output before anything else, so the
        // old one's stop is not taken for the end of playback, and the new
        // one's first words (its volume) are not lost; then stop the old one.
        let active = match &target {
            PlaybackTarget::Local(_) => LOCAL,
            PlaybackTarget::HqPlayer(_) => outputs.allocate_id(),
        };
        let carry = {
            let mut mirror = self.mirror.lock().unwrap();
            mirror.active = active;
            mirror.carry_over()
        };
        match outputs.hq.take() {
            Some(old) => {
                old.handle.send(EngineCommand::Stop);
                // Not waited for: HQPlayer may be unreachable, and the next
                // output does not need anything from this one.
                old.handle.send(EngineCommand::Shutdown);
            }
            None => {
                outputs.local.send(EngineCommand::Stop);
                outputs.local.send(EngineCommand::SetStandby(true));
            }
        }

        let volume = self.mirror.lock().unwrap().volume;
        match target {
            PlaybackTarget::Local(output) => {
                outputs.local.send(EngineCommand::SetOutput(output));
                outputs.local.send(EngineCommand::SetStandby(false));
                // HQPlayer may have moved the volume since the engine last
                // heard it; the slider shows HQPlayer's.
                if let Some(volume) = volume {
                    outputs.local.send(EngineCommand::SetVolume { volume });
                }
            }
            PlaybackTarget::HqPlayer(endpoint) => {
                self.connect_hqplayer(&mut outputs, active, endpoint);
            }
        }

        if let Some(carry) = carry {
            let send = |cmd: EngineCommand| match &outputs.hq {
                Some(hq) => hq.handle.send(cmd),
                None => outputs.local.send(cmd),
            };
            send(EngineCommand::PlayTrack(carry.current));
            if carry.position_ms > 0 {
                send(EngineCommand::Seek { position_ms: carry.position_ms });
            }
            send(EngineCommand::SetNext(carry.next));
            if carry.paused {
                send(EngineCommand::Pause);
            }
        }
    }

    /// Stop HQPlayer on the way out: once Reverb is gone, so is the relay it
    /// streams from. Waits briefly for HQPlayer to hear it.
    pub fn shutdown(&self) {
        let mut outputs = self.outputs.lock().unwrap();
        if let Some(hq) = outputs.hq.take() {
            hq.handle.send(EngineCommand::Stop);
            hq.handle.shutdown(SHUTDOWN_WAIT);
        }
    }

    /// Start an HQPlayer output under `id`, and its event gate. Its events
    /// count once the mirror names `id` active, which callers see to first.
    fn connect_hqplayer(&self, outputs: &mut Outputs, id: OutputId, endpoint: Endpoint) {
        let (tx, rx) = unbounded();
        let handle = hqplayer::spawn(endpoint.clone(), tx);
        let scrobble_percent = self.mirror.lock().unwrap().scrobble_percent;
        if let Some(percent) = scrobble_percent {
            handle.send(EngineCommand::SetScrobblePercent(percent));
        }
        spawn_gate(id, rx, self.event_tx.clone(), self.mirror.clone());
        outputs.hq = Some(HqOutput { endpoint, handle });
    }
}

impl Outputs {
    fn allocate_id(&mut self) -> OutputId {
        let id = self.next_id;
        self.next_id += 1;
        id
    }
}

/// A copy of a settings command, for sending it to two outputs.
fn clone_setting(cmd: &EngineCommand) -> EngineCommand {
    match cmd {
        EngineCommand::SetVolume { volume } => EngineCommand::SetVolume { volume: *volume },
        EngineCommand::SetScrobblePercent(p) => EngineCommand::SetScrobblePercent(*p),
        _ => unreachable!("only settings are sent to both outputs"),
    }
}

/// Forward `id`'s events while it is the active output; drop them otherwise.
/// Ends with the output.
fn spawn_gate(
    id: OutputId,
    rx: Receiver<EngineEvent>,
    tx: Sender<EngineEvent>,
    mirror: Arc<Mutex<Mirror>>,
) {
    std::thread::Builder::new()
        .name(format!("reverb-output-events-{id}"))
        .spawn(move || {
            while let Ok(event) = rx.recv() {
                // Checked and forwarded under the lock that `set_target` flips
                // the active output under, so nothing from an output slips out
                // after it was switched away from.
                let mirror = &mut *mirror.lock().unwrap();
                if mirror.active == id && mirror.observe_event(&event) {
                    let _ = tx.send(event);
                }
            }
        })
        .expect("spawning an output event gate cannot fail");
}

/// Playback as the router has seen it go by — enough to carry it to another
/// output.
#[derive(Debug)]
struct Mirror {
    active: OutputId,
    current: Option<TrackSource>,
    next: Option<TrackSource>,
    state: PlaybackState,
    position_ms: u64,
    /// The Song whose Play has been submitted. A play carried to another
    /// output reaches the threshold there again and must not count twice.
    submitted: Option<String>,
    /// The last volume set or reported; `None` until there is one.
    volume: Option<f32>,
    scrobble_percent: Option<u32>,
}

impl Default for Mirror {
    fn default() -> Self {
        Self {
            active: LOCAL,
            current: None,
            next: None,
            state: PlaybackState::Stopped,
            position_ms: 0,
            submitted: None,
            volume: None,
            scrobble_percent: None,
        }
    }
}

/// What a switch hands the new output.
#[derive(Debug)]
struct CarryOver {
    current: TrackSource,
    next: Option<TrackSource>,
    position_ms: u64,
    paused: bool,
}

impl Mirror {
    fn observe_command(&mut self, cmd: &EngineCommand) {
        match cmd {
            EngineCommand::PlayTrack(src) => {
                self.current = Some(src.clone());
                self.position_ms = 0;
                // A Song started on purpose is a new Play, even the same one.
                self.submitted = None;
            }
            EngineCommand::SetNext(next) => self.next = next.clone(),
            EngineCommand::Stop => self.current = None,
            EngineCommand::SetVolume { volume } => self.volume = Some(*volume),
            EngineCommand::SetScrobblePercent(p) => self.scrobble_percent = Some(*p),
            _ => {}
        }
    }

    /// Note what `event` says, and whether to pass it on.
    fn observe_event(&mut self, event: &EngineEvent) -> bool {
        match event {
            EngineEvent::TrackChanged { track_id, reason: TrackChangeReason::Natural, .. } => {
                if self.next.as_ref().is_some_and(|next| &next.track_id == track_id) {
                    self.current = self.next.take();
                }
                self.position_ms = 0;
                self.submitted = None;
            }
            EngineEvent::Position { position_ms, .. } => self.position_ms = *position_ms,
            EngineEvent::State { state } => {
                self.state = *state;
                if *state == PlaybackState::Stopped {
                    self.current = None;
                    self.position_ms = 0;
                }
            }
            EngineEvent::Scrobble { track_id, submission: true, .. } => {
                if self.submitted.as_ref() == Some(track_id) {
                    return false;
                }
                self.submitted = Some(track_id.clone());
            }
            EngineEvent::Volume { volume } => self.volume = Some(*volume),
            _ => {}
        }
        true
    }

    /// What is playing, if anything is, for the next output to pick up.
    fn carry_over(&self) -> Option<CarryOver> {
        if self.state == PlaybackState::Stopped {
            return None;
        }
        Some(CarryOver {
            current: self.current.clone()?,
            next: self.next.clone(),
            position_ms: self.position_ms,
            paused: self.state == PlaybackState::Paused,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn song(id: &str) -> TrackSource {
        TrackSource {
            track_id: id.into(),
            url: format!("http://server/rest/stream?id={id}"),
            ext_hint: Some("flac".into()),
            duration_ms: Some(200_000),
            title: None,
            artist: None,
            first_artist: None,
            album: None,
            bit_rate: None,
            replay_gain: None,
        }
    }

    fn submission(id: &str) -> EngineEvent {
        EngineEvent::Scrobble {
            track_id: id.into(),
            submission: true,
            title: None,
            artist: None,
            first_artist: None,
            album: None,
            duration_ms: 200_000,
        }
    }

    fn natural(id: &str) -> EngineEvent {
        EngineEvent::TrackChanged {
            track_id: id.into(),
            reason: TrackChangeReason::Natural,
            title: None,
            artist: None,
            album: None,
            duration_ms: 200_000,
        }
    }

    fn playing(mirror: &mut Mirror) {
        mirror.observe_event(&EngineEvent::State { state: PlaybackState::Playing });
    }

    #[test]
    fn carries_the_song_its_position_and_the_next_one() {
        let mut m = Mirror::default();
        m.observe_command(&EngineCommand::PlayTrack(song("a")));
        m.observe_command(&EngineCommand::SetNext(Some(song("b"))));
        playing(&mut m);
        m.observe_event(&EngineEvent::Position { position_ms: 42_000, duration_ms: 200_000 });

        let carry = m.carry_over().expect("something is playing");
        assert_eq!(carry.current.track_id, "a");
        assert_eq!(carry.next.map(|n| n.track_id).as_deref(), Some("b"));
        assert_eq!(carry.position_ms, 42_000);
        assert!(!carry.paused);
    }

    #[test]
    fn follows_a_natural_advance_and_a_pause() {
        let mut m = Mirror::default();
        m.observe_command(&EngineCommand::PlayTrack(song("a")));
        m.observe_command(&EngineCommand::SetNext(Some(song("b"))));
        playing(&mut m);
        m.observe_event(&natural("b"));
        m.observe_event(&EngineEvent::State { state: PlaybackState::Paused });

        let carry = m.carry_over().unwrap();
        assert_eq!(carry.current.track_id, "b");
        assert!(carry.next.is_none(), "the frontend has not named the one after yet");
        assert_eq!(carry.position_ms, 0);
        assert!(carry.paused);
    }

    #[test]
    fn nothing_is_carried_once_playback_stopped() {
        let mut m = Mirror::default();
        m.observe_command(&EngineCommand::PlayTrack(song("a")));
        playing(&mut m);
        m.observe_event(&EngineEvent::State { state: PlaybackState::Stopped });
        assert!(m.carry_over().is_none());
    }

    #[test]
    fn a_carried_play_is_submitted_once() {
        let mut m = Mirror::default();
        m.observe_command(&EngineCommand::PlayTrack(song("a")));
        assert!(m.observe_event(&submission("a")), "the first submission counts");
        // Carried to another output, which plays past the threshold again.
        assert!(!m.observe_event(&submission("a")), "the carried play does not");

        // Repeat-one: the same Song again is a new Play.
        m.observe_command(&EngineCommand::SetNext(Some(song("a"))));
        m.observe_event(&natural("a"));
        assert!(m.observe_event(&submission("a")));

        // And so is starting it over on purpose.
        m.observe_command(&EngineCommand::PlayTrack(song("a")));
        assert!(m.observe_event(&submission("a")));
    }
}
