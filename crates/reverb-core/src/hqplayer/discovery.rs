//! Finding the HQPlayers on the local network: a discovery request sent over
//! UDP multicast, and every answer that comes back within the wait.

use std::collections::HashSet;
use std::io;
use std::net::{Ipv4Addr, SocketAddr, UdpSocket};
use std::time::{Duration, Instant};

use serde::Serialize;

use super::DEFAULT_PORT;

/// Where HQPlayer listens for discovery requests: this group, on its control
/// port. Only an HQPlayer that allows control from the network answers, and
/// multicast stays inside one network — anything else is found by address.
const DISCOVERY_GROUP: Ipv4Addr = Ipv4Addr::new(239, 192, 0, 199);
const DISCOVERY_REQUEST: &str =
    "<?xml version=\"1.0\" encoding=\"UTF-8\"?><discover>hqplayer</discover>\n";

/// An HQPlayer that answered.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Discovered {
    /// The address it answered from.
    pub host: String,
    pub port: u16,
    /// The name of the computer it runs on.
    pub name: Option<String>,
    /// The product and major version: "Signalyst HQPlayer Desktop 5".
    pub version: Option<String>,
}

/// Ask the local network for HQPlayers, and gather the answers that arrive
/// within `wait`.
pub fn discover(wait: Duration) -> io::Result<Vec<Discovered>> {
    let socket = UdpSocket::bind(SocketAddr::from((Ipv4Addr::UNSPECIFIED, 0)))?;
    socket.set_multicast_ttl_v4(1)?;
    socket.send_to(DISCOVERY_REQUEST.as_bytes(), (DISCOVERY_GROUP, DEFAULT_PORT))?;

    let deadline = Instant::now() + wait;
    let mut found = Vec::new();
    let mut seen = HashSet::new();
    let mut buf = [0u8; 4096];
    while let Some(left) = deadline.checked_duration_since(Instant::now()) {
        if left.is_zero() {
            break;
        }
        socket.set_read_timeout(Some(left))?;
        let (len, from) = match socket.recv_from(&mut buf) {
            Ok(received) => received,
            Err(e) if matches!(e.kind(), io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut) => {
                break
            }
            Err(e) => return Err(e),
        };
        let Ok(reply) = std::str::from_utf8(&buf[..len]) else { continue };
        let Some(player) = parse_answer(reply, from) else { continue };
        if seen.insert(player.host.clone()) {
            found.push(player);
        }
    }
    found.sort_by(|a, b| a.name.cmp(&b.name).then_with(|| a.host.cmp(&b.host)));
    Ok(found)
}

/// One discovery answer, from `from`. Anything that is not one is `None`.
fn parse_answer(reply: &str, from: SocketAddr) -> Option<Discovered> {
    let root = super::protocol::root_element(reply)?;
    if root.name != "discover" || root.text.trim() != "hqplayer" {
        return None;
    }
    // An answer says how it went; a request (ours, looped back) says nothing.
    if !root.attributes.get("result").is_some_and(|r| r.eq_ignore_ascii_case("ok")) {
        return None;
    }
    let attr = |key: &str| root.attributes.get(key).filter(|v| !v.is_empty()).cloned();
    Some(Discovered {
        host: from.ip().to_string(),
        port: DEFAULT_PORT,
        name: attr("name"),
        version: attr("version"),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_an_answer() {
        let from: SocketAddr = "192.168.1.20:4321".parse().unwrap();
        // As HQPlayer Desktop 5 answers.
        let answer = parse_answer(
            r#"<?xml version="1.0" encoding="utf-8"?><discover name="studio" result="OK" version="Signalyst HQPlayer Desktop 5">hqplayer</discover>"#,
            from,
        );
        assert_eq!(
            answer,
            Some(Discovered {
                host: "192.168.1.20".into(),
                port: 4321,
                name: Some("studio".into()),
                version: Some("Signalyst HQPlayer Desktop 5".into()),
            })
        );
        // Our own request, looped back, is not an answer.
        assert_eq!(parse_answer(DISCOVERY_REQUEST, from), None);
    }
}
