//! Manual smoke test: play an audio URL through the full engine pipeline
//! (HTTP → symphonia → resample → ring → EQ → cpal) and print engine events.
//!
//!     cargo run -p reverb-core --example play_url -- <url> [seconds] [volume] [seek_to_secs] [eq_cut_hz]
//!     cargo run -p reverb-core --example play_url -- --devices
//!
//! When `seek_to_secs` is non-zero, a seek is issued after 2 seconds.
//! When `eq_cut_hz` is given, a -18 dB peaking cut at that frequency is
//! applied after 2 seconds (for A/B-ing the DSP).
//!
//! The output path is set by environment, so the device-specific behaviour
//! (direct ALSA cards, rate switching) can be exercised without the app:
//! `REVERB_DEVICE` is a device id from `--devices`, and `REVERB_AUTO_RATE` /
//! `REVERB_EXCLUSIVE` turn on rate matching and exclusive access.

use std::time::{Duration, Instant};

use reverb_core::audio::{
    spawn_engine, EngineCommand, EqBand, EqState, FilterType, PreampMode, TrackSource,
};

struct Stderr;
impl log::Log for Stderr {
    fn enabled(&self, _: &log::Metadata) -> bool {
        true
    }
    fn log(&self, record: &log::Record) {
        eprintln!("[{}] {}", record.level(), record.args());
    }
    fn flush(&self) {}
}

fn main() {
    log::set_logger(&Stderr).ok();
    log::set_max_level(log::LevelFilter::Debug);
    let mut args = std::env::args().skip(1);
    let url = args.next().expect("usage: play_url <url> [seconds] [volume] [seek_to_secs]");
    if url == "--devices" {
        for d in reverb_core::audio::list_output_devices() {
            println!("{}\t{}\tdirect={}", d.id, d.label, d.direct);
        }
        return;
    }
    let device = std::env::var("REVERB_DEVICE").ok();
    let secs: u64 = args.next().and_then(|s| s.parse().ok()).unwrap_or(5);
    let volume: f32 = args.next().and_then(|s| s.parse().ok()).unwrap_or(0.3);
    let seek_to: Option<u64> = args.next().and_then(|s| s.parse().ok()).filter(|&s| s > 0);
    let eq_cut_hz: Option<f32> = args.next().and_then(|s| s.parse().ok());

    let (event_tx, event_rx) = crossbeam_channel::unbounded();
    let engine = spawn_engine(event_tx, device);
    engine.send(EngineCommand::SetOutputMode {
        auto_rate: std::env::var("REVERB_AUTO_RATE").is_ok(),
        exclusive: std::env::var("REVERB_EXCLUSIVE").is_ok(),
    });
    engine.send(EngineCommand::SetVolume { volume });
    engine.send(EngineCommand::PlayTrack(TrackSource {
        track_id: "smoke-test".into(),
        url,
        ext_hint: None,
        duration_ms: None,
        title: Some("Smoke test".into()),
        artist: None,
        first_artist: None,
        album: None,
        bit_rate: None,
        replay_gain: None,
    }));

    let start = Instant::now();
    let deadline = start + Duration::from_secs(secs);
    let mut seek_sent = false;
    let mut eq_sent = false;
    while Instant::now() < deadline {
        if let Some(to) = seek_to {
            if !seek_sent && start.elapsed() >= Duration::from_secs(2) {
                seek_sent = true;
                println!(">>> seeking to {to}s");
                engine.send(EngineCommand::Seek { position_ms: to * 1000 });
            }
        }
        if let Some(freq) = eq_cut_hz {
            if !eq_sent && start.elapsed() >= Duration::from_secs(2) {
                eq_sent = true;
                println!(">>> applying -18 dB peaking cut at {freq} Hz");
                engine.send(EngineCommand::SetEq(EqState {
                    enabled: true,
                    bands: vec![EqBand {
                        enabled: true,
                        filter_type: FilterType::Peaking,
                        frequency: freq,
                        gain_db: -18.0,
                        q: 2.0,
                    }],
                    preamp: PreampMode::Manual { db: 0.0 },
                }));
            }
        }
        match event_rx.recv_timeout(Duration::from_millis(200)) {
            Ok(event) => println!("{event:?}"),
            Err(crossbeam_channel::RecvTimeoutError::Timeout) => {}
            Err(crossbeam_channel::RecvTimeoutError::Disconnected) => break,
        }
    }
    engine.send(EngineCommand::Shutdown);
    std::thread::sleep(Duration::from_millis(100));
}
