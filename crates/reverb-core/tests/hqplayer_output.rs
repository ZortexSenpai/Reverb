//! The HQPlayer output against a stand-in HQPlayer: one that speaks the
//! control protocol the way HQPlayer Desktop 5 does, and — as HQPlayer does
//! — fetches each Song it is handed before it says yes, here from a stand-in
//! Navidrome behind Reverb's song relay.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use crossbeam_channel::{unbounded, Receiver};
use reverb_core::audio::{EngineCommand, EngineEvent, PlaybackState, TrackChangeReason, TrackSource};
use reverb_core::hqplayer::{self, Endpoint};

/// What the stand-in HQPlayer holds, and what it was told.
#[derive(Default)]
struct Fake {
    playlist: Vec<String>,
    state: u32,
    /// Counted from 1; 0 for none.
    track: u32,
    position: f64,
    volume: f64,
    /// Every command, by name, in order.
    commands: Vec<String>,
    /// What each fetch of a playlist entry got back.
    fetched: Vec<String>,
}

fn attr(xml: &str, key: &str) -> Option<String> {
    let start = xml.find(&format!(" {key}=\""))? + key.len() + 3;
    let end = start + xml[start..].find('"')?;
    Some(xml[start..end].replace("&amp;", "&"))
}

/// GET `url` (plain http://host:port/path), returning the body.
fn fetch(url: &str) -> String {
    let rest = url.strip_prefix("http://").expect("an http URL");
    let (host, path) = rest.split_at(rest.find('/').unwrap());
    let mut stream = TcpStream::connect(host).unwrap();
    write!(stream, "GET {path} HTTP/1.1\r\nHost: {host}\r\n\r\n").unwrap();
    let mut answer = String::new();
    stream.read_to_string(&mut answer).unwrap();
    assert!(answer.starts_with("HTTP/1.1 200"), "{answer}");
    answer.split("\r\n\r\n").nth(1).unwrap_or_default().to_owned()
}

fn answer(fake: &Arc<Mutex<Fake>>, request: &str) -> String {
    let body = request.trim().trim_start_matches(r#"<?xml version="1.0" encoding="UTF-8"?>"#);
    let name: String =
        body.trim_start_matches('<').chars().take_while(|c| c.is_alphanumeric()).collect();
    let decl = r#"<?xml version="1.0" encoding="utf-8"?>"#;
    // Fetched outside the lock, as HQPlayer probes before it answers.
    if name == "PlaylistAdd" {
        let uri = attr(body, "uri").unwrap();
        let got = fetch(&uri);
        let mut f = fake.lock().unwrap();
        f.commands.push(name.clone());
        f.fetched.push(got);
        f.playlist.push(uri);
        return format!(r#"{decl}<PlaylistAdd result="OK"/>"#);
    }
    let mut f = fake.lock().unwrap();
    f.commands.push(name.clone());
    match name.as_str() {
        "GetInfo" => format!(
            r#"{decl}<GetInfo engine="5.35.10" name="fake" platform="Linux" product="Signalyst HQPlayer Desktop" version="5"/>"#
        ),
        "VolumeRange" => {
            format!(r#"{decl}<VolumeRange adaptive="0" enabled="1" max="0.0" min="-60.0"/>"#)
        }
        "Status" => {
            assert_eq!(attr(body, "subscribe").as_deref(), Some("0"), "polls, never subscribes");
            format!(
                r#"{decl}<Status active_mode="PCM" active_filter="poly-sinc-gauss-long" active_shaper="NS5" active_rate="384000" length="10.0" output_delay="0" position="{}" state="{}" track="{}" volume="{}"/>"#,
                f.position, f.state, f.track, f.volume
            )
        }
        "Stop" => {
            f.state = 0;
            format!(r#"{decl}<Stop result="OK"/>"#)
        }
        "PlaylistClear" => {
            f.playlist.clear();
            f.track = 0;
            format!(r#"{decl}<PlaylistClear result="OK"/>"#)
        }
        "Play" => {
            f.state = 2;
            f.track = 1;
            f.position = 0.0;
            format!(r#"{decl}<Play result="OK"/>"#)
        }
        "Pause" => {
            f.state = if f.state == 2 { 1 } else { 2 };
            format!(r#"{decl}<Pause result="OK"/>"#)
        }
        "Seek" => {
            f.position = attr(body, "position").unwrap().parse().unwrap();
            format!(r#"{decl}<Seek result="OK"/>"#)
        }
        "Volume" => {
            f.volume = attr(body, "value").unwrap().parse().unwrap();
            format!(r#"{decl}<Volume result="OK"/>"#)
        }
        "PlaylistRemove" => {
            let index: usize = attr(body, "index").unwrap().parse().unwrap();
            f.playlist.remove(index - 1);
            format!(r#"{decl}<PlaylistRemove result="OK"/>"#)
        }
        _ => format!(r#"{decl}<{name} result="Error">Unknown command</{name}>"#),
    }
}

/// A stand-in HQPlayer on a free port.
fn fake_hqplayer() -> (u16, Arc<Mutex<Fake>>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let fake = Arc::new(Mutex::new(Fake { volume: -20.0, ..Fake::default() }));
    let shared = fake.clone();
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let stream = stream.unwrap();
            let fake = shared.clone();
            std::thread::spawn(move || {
                let mut out = stream.try_clone().unwrap();
                for line in BufReader::new(stream).lines() {
                    let Ok(line) = line else { break };
                    if line.trim().is_empty() {
                        continue;
                    }
                    let reply = answer(&fake, &line);
                    if out.write_all(format!("{reply}\n").as_bytes()).is_err() {
                        break;
                    }
                }
            });
        }
    });
    (port, fake)
}

/// A stand-in Navidrome that serves every request a tiny FLAC "file".
fn fake_navidrome() -> u16 {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let mut stream = stream.unwrap();
            let mut reader = BufReader::new(stream.try_clone().unwrap());
            let mut line = String::new();
            while reader.read_line(&mut line).is_ok_and(|n| n > 2) {
                line.clear();
            }
            let _ = stream.write_all(
                b"HTTP/1.1 200 OK\r\nContent-Type: audio/flac\r\nContent-Length: 4\r\n\
                  Accept-Ranges: bytes\r\nConnection: close\r\n\r\nfLaC",
            );
        }
    });
    port
}

fn song(id: &str, navidrome: u16) -> TrackSource {
    TrackSource {
        track_id: id.into(),
        url: format!("http://127.0.0.1:{navidrome}/rest/stream?id={id}&t=token&s=salt&format=raw"),
        ext_hint: Some("flac".into()),
        duration_ms: Some(10_000),
        title: Some(format!("Song {id}")),
        artist: Some("Artist".into()),
        first_artist: None,
        album: Some("Album".into()),
        bit_rate: None,
        replay_gain: None,
    }
}

/// The first event that `wanted` picks out, within a few seconds.
fn expect<T>(rx: &Receiver<EngineEvent>, what: &str, wanted: impl Fn(&EngineEvent) -> Option<T>) -> T {
    let deadline = Instant::now() + Duration::from_secs(8);
    while let Some(left) = deadline.checked_duration_since(Instant::now()) {
        if let Ok(event) = rx.recv_timeout(left) {
            if let Some(found) = wanted(&event) {
                return found;
            }
        }
    }
    panic!("no {what} within 8 s");
}

#[test]
fn plays_songs_through_hqplayer_from_start_to_end() {
    let (port, fake) = fake_hqplayer();
    let navidrome = fake_navidrome();
    let (tx, rx) = unbounded();
    let output = hqplayer::spawn(Endpoint { host: "127.0.0.1".into(), port }, tx);

    // Connecting reads HQPlayer's volume, and the volume control adopts it.
    let volume = expect(&rx, "volume", |e| match e {
        EngineEvent::Volume { volume } => Some(*volume),
        _ => None,
    });
    assert!((volume - (1.0 - 20.0 / 60.0)).abs() < 1e-4, "-20 dB reads as {volume}");

    output.send(EngineCommand::SetScrobblePercent(50));
    output.send(EngineCommand::PlayTrack(song("a", navidrome)));
    expect(&rx, "the change to song a", |e| match e {
        EngineEvent::TrackChanged { track_id, reason: TrackChangeReason::Command, .. } => {
            (track_id == "a").then_some(())
        }
        _ => None,
    });
    expect(&rx, "playing", |e| match e {
        EngineEvent::State { state: PlaybackState::Playing } => Some(()),
        _ => None,
    });
    {
        let f = fake.lock().unwrap();
        let load: Vec<&str> = f
            .commands
            .iter()
            .map(String::as_str)
            .filter(|c| *c != "Status" && *c != "GetInfo" && *c != "VolumeRange")
            .collect();
        assert_eq!(load[..4], ["Stop", "PlaylistClear", "PlaylistAdd", "Play"]);
        assert_eq!(f.fetched[0], "fLaC", "HQPlayer got the song through the relay");
        assert!(
            !f.playlist[0].contains("token") && f.playlist[0].ends_with(".flac"),
            "HQPlayer never sees the server's URL or token: {}",
            f.playlist[0]
        );
    }

    // The next Song goes in behind it.
    output.send(EngineCommand::SetNext(Some(song("b", navidrome))));
    let deadline = Instant::now() + Duration::from_secs(5);
    while fake.lock().unwrap().playlist.len() < 2 {
        assert!(Instant::now() < deadline, "song b never went into the playlist");
        std::thread::sleep(Duration::from_millis(50));
    }

    // Past half way, the Play counts.
    fake.lock().unwrap().position = 6.0;
    expect(&rx, "a's submission", |e| match e {
        EngineEvent::Scrobble { track_id, submission: true, .. } => (track_id == "a").then_some(()),
        _ => None,
    });

    // HQPlayer moves on to the next entry by itself, without stopping.
    {
        let mut f = fake.lock().unwrap();
        f.track = 2;
        f.position = 0.5;
    }
    expect(&rx, "the gapless change to song b", |e| match e {
        EngineEvent::TrackChanged { track_id, reason: TrackChangeReason::Natural, .. } => {
            (track_id == "b").then_some(())
        }
        _ => None,
    });

    // Pause toggles HQPlayer; so does resume.
    output.send(EngineCommand::Pause);
    expect(&rx, "paused", |e| match e {
        EngineEvent::State { state: PlaybackState::Paused } => Some(()),
        _ => None,
    });
    assert_eq!(fake.lock().unwrap().state, 1);
    output.send(EngineCommand::Resume);
    expect(&rx, "playing again", |e| match e {
        EngineEvent::State { state: PlaybackState::Playing } => Some(()),
        _ => None,
    });
    assert_eq!(fake.lock().unwrap().state, 2);

    // The volume control sets HQPlayer's volume in dB.
    output.send(EngineCommand::SetVolume { volume: 0.5 });
    let deadline = Instant::now() + Duration::from_secs(5);
    while (fake.lock().unwrap().volume + 30.0).abs() > 0.01 {
        assert!(Instant::now() < deadline, "the volume never reached HQPlayer");
        std::thread::sleep(Duration::from_millis(50));
    }

    // The playlist runs out: playback is over.
    fake.lock().unwrap().state = 0;
    expect(&rx, "the end of song b", |e| match e {
        EngineEvent::TrackEnded { track_id } => (track_id == "b").then_some(()),
        _ => None,
    });

    output.shutdown(Duration::from_secs(2));
}

#[test]
fn an_unreachable_hqplayer_is_reported_when_there_is_something_to_play() {
    // A port nothing listens on.
    let port = TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap().port();
    let (tx, rx) = unbounded();
    let output = hqplayer::spawn(Endpoint { host: "127.0.0.1".into(), port }, tx);
    output.send(EngineCommand::PlayTrack(song("a", 9)));
    // Stopped, then why — the order the local engine reports a failed start in.
    expect(&rx, "stopped", |e| match e {
        EngineEvent::State { state: PlaybackState::Stopped } => Some(()),
        _ => None,
    });
    let message = expect(&rx, "an error", |e| match e {
        EngineEvent::Error { code, message, .. } if code == "hqplayer" => Some(message.clone()),
        _ => None,
    });
    assert!(message.contains("could not reach HQPlayer"), "{message}");
    output.shutdown(Duration::from_secs(2));
}

/// Switching outputs mid-Song carries it across, from where it was, and the
/// old output's stop is not taken for the end of playback. Two HQPlayers
/// rather than the local engine, which would need a sound card.
#[test]
fn switching_outputs_carries_the_song_across() {
    use reverb_core::playback::{Playback, PlaybackTarget};

    let (first_port, first) = fake_hqplayer();
    let (second_port, second) = fake_hqplayer();
    let navidrome = fake_navidrome();
    let (tx, rx) = unbounded();
    let at = |port| PlaybackTarget::HqPlayer(Endpoint { host: "127.0.0.1".into(), port });
    let playback = Playback::start(tx, at(first_port));

    playback.send(EngineCommand::PlayTrack(song("a", navidrome)));
    expect(&rx, "playing on the first", |e| match e {
        EngineEvent::State { state: PlaybackState::Playing } => Some(()),
        _ => None,
    });
    first.lock().unwrap().position = 4.2;
    expect(&rx, "the position", |e| match e {
        EngineEvent::Position { position_ms, .. } => (*position_ms >= 4000).then_some(()),
        _ => None,
    });

    playback.set_target(at(second_port));
    let deadline = Instant::now() + Duration::from_secs(8);
    loop {
        let event = rx.recv_timeout(deadline.saturating_duration_since(Instant::now()));
        match event.expect("song a never started on the second HQPlayer") {
            EngineEvent::State { state: PlaybackState::Stopped } => {
                panic!("the first HQPlayer's stop reached the app")
            }
            EngineEvent::TrackChanged { track_id, .. } if track_id == "a" => break,
            _ => {}
        }
    }
    expect(&rx, "playing on the second", |e| match e {
        EngineEvent::State { state: PlaybackState::Playing } => Some(()),
        _ => None,
    });
    let deadline = Instant::now() + Duration::from_secs(5);
    while second.lock().unwrap().position < 4.0 {
        assert!(Instant::now() < deadline, "the second HQPlayer never sought to where it was");
        std::thread::sleep(Duration::from_millis(50));
    }
    assert_eq!(first.lock().unwrap().state, 0, "the first HQPlayer was stopped");

    playback.shutdown();
    assert_eq!(second.lock().unwrap().state, 0, "quitting stops HQPlayer");
}
