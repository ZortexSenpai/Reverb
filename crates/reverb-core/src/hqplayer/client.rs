//! One control connection to HQPlayer: XML requests out, one XML reply each
//! back, over TCP.

use std::io::{self, BufRead, BufReader, Write};
use std::net::{IpAddr, TcpStream, ToSocketAddrs};
use std::time::Duration;

use super::protocol::{self, Info};
use super::Endpoint;

/// How long connecting may take before HQPlayer counts as unreachable.
const CONNECT_TIMEOUT: Duration = Duration::from_secs(3);
/// How long a reply usually may take. The first one on a new connection has
/// been measured at over five seconds, and `Stop` at nearly four.
pub const REPLY_TIMEOUT: Duration = Duration::from_secs(10);
/// A reply larger than this is not one HQPlayer would send.
const MAX_REPLY_BYTES: usize = 1024 * 1024;
/// Documents to read past while looking for a reply, before giving up on it.
const MAX_STRAY_DOCUMENTS: usize = 16;

#[derive(Debug, thiserror::Error)]
pub enum ClientError {
    #[error("could not find {0}")]
    Resolve(String),
    #[error("could not reach HQPlayer at {address}: {source}{hint}")]
    Connect { address: String, source: io::Error, hint: &'static str },
    #[error("lost the connection to HQPlayer: {0}")]
    Io(#[from] io::Error),
    #[error("HQPlayer sent a reply Reverb could not read")]
    BadReply,
    #[error(
        "HQPlayer took the connection but did not answer. From another computer, it only \
         answers with “Allow control from network” turned on."
    )]
    NoAnswer,
    #[error("HQPlayer refused: {0}")]
    Refused(String),
}

pub struct Connection {
    reader: BufReader<TcpStream>,
    writer: TcpStream,
    /// This computer's address on the network HQPlayer is on: where HQPlayer
    /// can reach the song relay.
    pub local_ip: IpAddr,
}

impl Connection {
    pub fn open(endpoint: &Endpoint) -> Result<Self, ClientError> {
        let address = endpoint.address();
        let addrs: Vec<_> = (endpoint.host.as_str(), endpoint.port)
            .to_socket_addrs()
            .map_err(|_| ClientError::Resolve(endpoint.host.clone()))?
            .collect();
        let mut last_error = None;
        for addr in addrs {
            match TcpStream::connect_timeout(&addr, CONNECT_TIMEOUT) {
                Ok(stream) => {
                    stream.set_nodelay(true)?;
                    stream.set_write_timeout(Some(REPLY_TIMEOUT))?;
                    let local_ip = stream.local_addr()?.ip();
                    let writer = stream.try_clone()?;
                    return Ok(Self { reader: BufReader::new(stream), writer, local_ip });
                }
                Err(e) => last_error = Some(e),
            }
        }
        let source = last_error.unwrap_or_else(|| io::Error::other("no address to connect to"));
        // HQPlayer turns connections away while its settings dialog is open.
        let hint = if source.kind() == io::ErrorKind::ConnectionRefused {
            ". Is HQPlayer running, with its settings dialog closed?"
        } else {
            ""
        };
        Err(ClientError::Connect { address, source, hint })
    }

    /// Send one request and read its reply. A reply that says the request
    /// failed comes back as `Refused`.
    pub fn request(&mut self, xml: &str) -> Result<String, ClientError> {
        self.request_within(xml, REPLY_TIMEOUT)
    }

    /// `request`, waiting up to `timeout` for the reply.
    ///
    /// Replies carry no id; each is named after its command, so anything else
    /// that arrives first (a status push, a late reply to an earlier request
    /// that timed out) is read past. Only one request is ever in flight:
    /// HQPlayer loses track if they are pipelined.
    pub fn request_within(&mut self, xml: &str, timeout: Duration) -> Result<String, ClientError> {
        let command = protocol::command_name(xml).unwrap_or_default();
        self.reader.get_ref().set_read_timeout(Some(timeout))?;
        self.writer.write_all(xml.as_bytes())?;
        self.writer.write_all(b"\n")?;
        self.writer.flush()?;
        for _ in 0..MAX_STRAY_DOCUMENTS {
            let reply = self.read_document()?;
            if protocol::command_name(&reply).as_deref() != Some(command.as_str()) {
                log::debug!("HQPlayer: skipping a document that does not answer {command}");
                continue;
            }
            if let Some(reason) = protocol::failure(&reply) {
                return Err(ClientError::Refused(reason));
            }
            return Ok(reply);
        }
        Err(ClientError::BadReply)
    }

    /// Read lines until they add up to one whole XML document, however the
    /// reply happens to be broken into lines.
    fn read_document(&mut self) -> Result<String, ClientError> {
        let mut doc = String::new();
        loop {
            let read = match self.reader.read_line(&mut doc) {
                Ok(read) => read,
                Err(e) if matches!(e.kind(), io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut) => {
                    return Err(ClientError::NoAnswer)
                }
                Err(e) => return Err(e.into()),
            };
            if read == 0 {
                return Err(io::Error::new(io::ErrorKind::UnexpectedEof, "HQPlayer hung up").into());
            }
            if protocol::is_complete(&doc) {
                return Ok(doc.trim().to_owned());
            }
            if doc.len() > MAX_REPLY_BYTES {
                return Err(ClientError::BadReply);
            }
        }
    }
}

impl Connection {
    /// Ask what is at the other end.
    pub fn info(&mut self) -> Result<Info, ClientError> {
        let reply = self.request(&protocol::get_info())?;
        protocol::parse_info(&reply).ok_or(ClientError::BadReply)
    }
}

/// Connect to the HQPlayer at `endpoint` and ask what it is.
pub fn probe(endpoint: &Endpoint) -> Result<Info, ClientError> {
    Connection::open(endpoint)?.info()
}
