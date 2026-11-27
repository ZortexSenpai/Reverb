//! Playing through HQPlayer: Reverb stays the remote — queue, transport,
//! scrobbling — while HQPlayer Desktop, on this computer or another, fetches
//! each Song and does the playing.
//!
//! HQPlayer is driven over its network control protocol (XML over TCP, see
//! [`protocol`]). It fetches the Songs from [`relay::SongRelay`], which
//! forwards to Navidrome. [`spawn`] starts an output that takes the same
//! [`EngineCommand`]s and reports the same [`EngineEvent`]s as the local
//! engine, so the app drives either one the same way (see
//! `crate::playback`).

mod client;
mod discovery;
mod output;
pub mod protocol;
pub mod relay;

use std::time::Duration;

use crossbeam_channel::{bounded, unbounded, Receiver, Sender};
use serde::{Deserialize, Serialize};

use crate::audio::{EngineCommand, EngineEvent};

pub use client::probe;
pub use discovery::{discover, Discovered};
pub use protocol::Info;

/// The port HQPlayer listens for control on.
pub const DEFAULT_PORT: u16 = 4321;

/// Where an HQPlayer listens for control.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Endpoint {
    /// Host name or IP address of the computer HQPlayer runs on.
    pub host: String,
    pub port: u16,
}

impl Endpoint {
    /// "host" for the usual port, "host:port" otherwise — what the listener
    /// typed, near enough, for messages and the signal path.
    pub fn address(&self) -> String {
        if self.port == DEFAULT_PORT {
            self.host.clone()
        } else if self.host.contains(':') {
            format!("[{}]:{}", self.host, self.port)
        } else {
            format!("{}:{}", self.host, self.port)
        }
    }
}

/// Control of a running HQPlayer output.
pub struct HqPlayerHandle {
    cmd_tx: Sender<EngineCommand>,
    /// Closed when the output's thread has finished.
    done_rx: Receiver<()>,
}

impl HqPlayerHandle {
    pub fn send(&self, cmd: EngineCommand) {
        let _ = self.cmd_tx.send(cmd);
    }

    /// End the output, waiting up to `wait` for it to finish what it was
    /// sent before — a stop that should reach HQPlayer before Reverb exits.
    pub fn shutdown(&self, wait: Duration) {
        let _ = self.cmd_tx.send(EngineCommand::Shutdown);
        let _ = self.done_rx.recv_timeout(wait);
    }
}

/// Start an output that plays through the HQPlayer at `endpoint`, reporting
/// to `event_tx`. It connects in the background and keeps trying; nothing
/// is reported as wrong until there is something to play.
pub fn spawn(endpoint: Endpoint, event_tx: Sender<EngineEvent>) -> HqPlayerHandle {
    let (cmd_tx, cmd_rx) = unbounded();
    let (done_tx, done_rx) = bounded::<()>(0);
    std::thread::Builder::new()
        .name("reverb-hqplayer".into())
        .spawn(move || {
            output::run(endpoint, cmd_rx, event_tx);
            drop(done_tx);
        })
        .expect("spawning the HQPlayer output thread cannot fail");
    HqPlayerHandle { cmd_tx, done_rx }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn addresses_leave_out_the_usual_port() {
        let at = |host: &str, port| Endpoint { host: host.into(), port }.address();
        assert_eq!(at("studio.local", 4321), "studio.local");
        assert_eq!(at("192.168.1.20", 4322), "192.168.1.20:4322");
        assert_eq!(at("fe80::1", 4322), "[fe80::1]:4322");
    }
}
