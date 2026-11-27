//! The song relay: a small HTTP server HQPlayer fetches songs from.
//!
//! HQPlayer plays a URL by fetching it itself, so it needs one it can reach
//! and read. Navidrome's own stream URL is a poor fit on all three counts: it
//! carries the account's token in its query string (which HQPlayer would
//! show and keep in its playlist), it ends in `/rest/stream` rather than in
//! the file type, and it may point at `localhost` or a VPN address that only
//! this computer can reach. So each Song is registered here under an
//! unguessable path ending in its file extension, and the relay forwards
//! requests for it to Navidrome — Range requests included, so HQPlayer can
//! seek. Nothing that was not registered is served.

use std::collections::VecDeque;
use std::io::{self, BufRead, BufReader, Read, Write};
use std::net::{IpAddr, Ipv4Addr, Shutdown, SocketAddr, TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use reqwest::blocking::Client;
use reqwest::header;
use reqwest::Url;

/// How many registered Songs stay servable. HQPlayer only ever needs the
/// current and the next one; a few more cover a playlist it is still reading.
const ROUTES_KEPT: usize = 32;
/// Connections served at once. HQPlayer uses one or two.
const MAX_CONNECTIONS: usize = 16;
/// Longest request head accepted.
const MAX_HEAD_BYTES: usize = 16 * 1024;

struct Route {
    token: String,
    upstream: Url,
    ext: String,
}

struct Inner {
    routes: Mutex<VecDeque<Route>>,
    http: Client,
    stopped: AtomicBool,
    connections: AtomicUsize,
}

/// A running relay. Dropping it stops listening.
pub struct SongRelay {
    inner: Arc<Inner>,
    port: u16,
}

impl SongRelay {
    /// Start listening on every interface, on a port the OS picks: HQPlayer
    /// may run on another computer.
    pub fn start() -> io::Result<Self> {
        let listener = TcpListener::bind(SocketAddr::from((Ipv4Addr::UNSPECIFIED, 0)))?;
        let port = listener.local_addr()?.port();
        let inner = Arc::new(Inner {
            routes: Mutex::new(VecDeque::new()),
            http: crate::audio::source::stream_http_client(),
            stopped: AtomicBool::new(false),
            connections: AtomicUsize::new(0),
        });
        let accept_inner = inner.clone();
        std::thread::Builder::new()
            .name("reverb-song-relay".into())
            .spawn(move || accept_loop(listener, accept_inner))?;
        log::info!("song relay listening on port {port}");
        Ok(Self { inner, port })
    }

    /// Make `upstream` servable and return its path on the relay, e.g.
    /// `/songs/<token>.flac`. `ext` is the Song's file type.
    pub fn register(&self, upstream: Url, ext: &str) -> String {
        let token = uuid::Uuid::new_v4().simple().to_string();
        let ext = sanitize_ext(ext);
        let path = format!("/songs/{token}.{ext}");
        let mut routes = self.inner.routes.lock().unwrap();
        routes.push_back(Route { token, upstream, ext });
        while routes.len() > ROUTES_KEPT {
            routes.pop_front();
        }
        path
    }

    /// The URL a player at the other end of a connection made from `local_ip`
    /// reaches `path` under. `local_ip` is this computer's address on the
    /// network HQPlayer is on — the local end of the control connection.
    pub fn url(&self, local_ip: IpAddr, path: &str) -> String {
        // Only IPv4 is listened on; a v4-mapped v6 address is still v4.
        let ip = match local_ip {
            IpAddr::V6(v6) => v6.to_ipv4_mapped().map_or(local_ip, IpAddr::V4),
            v4 => v4,
        };
        match ip {
            IpAddr::V4(v4) => format!("http://{v4}:{}{path}", self.port),
            IpAddr::V6(v6) => format!("http://[{v6}]:{}{path}", self.port),
        }
    }

    /// Forget every registered Song.
    pub fn clear(&self) {
        self.inner.routes.lock().unwrap().clear();
    }
}

impl Drop for SongRelay {
    fn drop(&mut self) {
        self.inner.stopped.store(true, Ordering::SeqCst);
        // `accept` blocks; a connection of our own wakes it to see the flag.
        let _ = TcpStream::connect_timeout(
            &SocketAddr::from((Ipv4Addr::LOCALHOST, self.port)),
            Duration::from_millis(200),
        );
    }
}

fn accept_loop(listener: TcpListener, inner: Arc<Inner>) {
    for stream in listener.incoming() {
        if inner.stopped.load(Ordering::SeqCst) {
            break;
        }
        let Ok(stream) = stream else { continue };
        if inner.connections.fetch_add(1, Ordering::SeqCst) >= MAX_CONNECTIONS {
            inner.connections.fetch_sub(1, Ordering::SeqCst);
            let _ = respond_status(&stream, 503, "Service Unavailable");
            continue;
        }
        let conn_inner = inner.clone();
        let spawned = std::thread::Builder::new().name("reverb-song-relay-conn".into()).spawn(
            move || {
                if let Err(e) = serve(&stream, &conn_inner) {
                    // Players drop connections mid-body all the time (seeks).
                    log::debug!("song relay connection ended: {e}");
                }
                let _ = stream.shutdown(Shutdown::Both);
                conn_inner.connections.fetch_sub(1, Ordering::SeqCst);
            },
        );
        if spawned.is_err() {
            inner.connections.fetch_sub(1, Ordering::SeqCst);
        }
    }
    log::info!("song relay stopped");
}

/// A parsed request head: method, path, and the one header that matters.
#[derive(Debug, PartialEq, Eq)]
struct Request {
    method: String,
    path: String,
    range: Option<String>,
}

fn read_request(stream: &TcpStream) -> io::Result<Request> {
    stream.set_read_timeout(Some(Duration::from_secs(10)))?;
    let mut reader = BufReader::new(stream.take(MAX_HEAD_BYTES as u64));
    let mut request_line = String::new();
    reader.read_line(&mut request_line)?;
    let mut headers = Vec::new();
    loop {
        let mut line = String::new();
        if reader.read_line(&mut line)? == 0 {
            return Err(io::Error::new(io::ErrorKind::UnexpectedEof, "request head cut short"));
        }
        let line = line.trim_end();
        if line.is_empty() {
            break;
        }
        headers.push(line.to_owned());
    }
    parse_request(&request_line, &headers)
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "malformed request"))
}

fn parse_request(request_line: &str, headers: &[String]) -> Option<Request> {
    let mut parts = request_line.split_whitespace();
    let method = parts.next()?.to_owned();
    let path = parts.next()?.to_owned();
    parts.next().filter(|version| version.starts_with("HTTP/"))?;
    let range = headers.iter().find_map(|line| {
        let (name, value) = line.split_once(':')?;
        name.trim().eq_ignore_ascii_case("range").then(|| value.trim().to_owned())
    });
    Some(Request { method, path, range })
}

/// The token in a `/songs/<token>.<ext>` path.
fn token_of(path: &str) -> Option<&str> {
    let file = path.strip_prefix("/songs/")?;
    let (token, _ext) = file.split_once('.')?;
    (!token.is_empty() && token.chars().all(|c| c.is_ascii_alphanumeric())).then_some(token)
}

fn serve(stream: &TcpStream, inner: &Inner) -> io::Result<()> {
    let request = read_request(stream)?;
    let head_only = match request.method.as_str() {
        "GET" => false,
        "HEAD" => true,
        _ => return respond_status(stream, 405, "Method Not Allowed"),
    };
    let route = token_of(&request.path).and_then(|token| {
        let routes = inner.routes.lock().unwrap();
        routes.iter().find(|r| r.token == token).map(|r| (r.upstream.clone(), r.ext.clone()))
    });
    let Some((upstream, ext)) = route else {
        return respond_status(stream, 404, "Not Found");
    };

    let mut upstream_req = inner.http.get(upstream);
    if let Some(range) = &request.range {
        upstream_req = upstream_req.header(header::RANGE, range);
    }
    let mut response = match upstream_req.send() {
        Ok(response) => response,
        Err(e) => {
            log::warn!("song relay: server request failed: {e}");
            return respond_status(stream, 502, "Bad Gateway");
        }
    };
    let status = response.status();
    if !status.is_success() {
        log::warn!("song relay: server answered {status}");
        // 416 is the one a player needs to hear as-is (a seek past the end).
        return match status.as_u16() {
            416 => respond_status(stream, 416, "Range Not Satisfiable"),
            _ => respond_status(stream, 502, "Bad Gateway"),
        };
    }

    let headers = response.headers();
    let header_str = |name: header::HeaderName| {
        headers.get(name).and_then(|v| v.to_str().ok()).map(str::to_owned)
    };
    // Navidrome names the type; trust it unless it is a shrug.
    let content_type = header_str(header::CONTENT_TYPE)
        .filter(|t| !t.starts_with("application/octet-stream"))
        .unwrap_or_else(|| content_type_for(&ext).to_owned());
    let mut head = format!(
        "HTTP/1.1 {} {}\r\nContent-Type: {content_type}\r\nConnection: close\r\n",
        status.as_u16(),
        status.canonical_reason().unwrap_or("OK"),
    );
    if let Some(len) = header_str(header::CONTENT_LENGTH) {
        head.push_str(&format!("Content-Length: {len}\r\n"));
    }
    if let Some(range) = header_str(header::CONTENT_RANGE) {
        head.push_str(&format!("Content-Range: {range}\r\n"));
    }
    if let Some(ranges) = header_str(header::ACCEPT_RANGES) {
        head.push_str(&format!("Accept-Ranges: {ranges}\r\n"));
    }
    head.push_str("\r\n");

    let mut out = stream;
    out.write_all(head.as_bytes())?;
    if !head_only {
        io::copy(&mut response, &mut out)?;
    }
    out.flush()
}

fn respond_status(mut stream: &TcpStream, code: u16, reason: &str) -> io::Result<()> {
    write!(stream, "HTTP/1.1 {code} {reason}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n")?;
    stream.flush()
}

/// File extensions kept to what a path can carry without escaping.
fn sanitize_ext(ext: &str) -> String {
    let ext: String =
        ext.chars().filter(|c| c.is_ascii_alphanumeric()).take(8).collect::<String>();
    if ext.is_empty() {
        "bin".into()
    } else {
        ext.to_ascii_lowercase()
    }
}

/// A type to announce when the server does not name one.
fn content_type_for(ext: &str) -> &'static str {
    match ext {
        "flac" => "audio/flac",
        "mp3" => "audio/mpeg",
        "wav" => "audio/wav",
        "aif" | "aiff" => "audio/aiff",
        "m4a" | "alac" | "aac" | "mp4" => "audio/mp4",
        "ogg" | "oga" | "opus" => "audio/ogg",
        "dsf" => "audio/x-dsf",
        "dff" => "audio/x-dff",
        "wv" => "audio/x-wavpack",
        "ape" => "audio/x-ape",
        _ => "application/octet-stream",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_a_range_request() {
        let headers = vec!["Host: 10.0.0.2:5000".to_owned(), "range: bytes=100-".to_owned()];
        assert_eq!(
            parse_request("GET /songs/abc.flac HTTP/1.1\r\n", &headers),
            Some(Request {
                method: "GET".into(),
                path: "/songs/abc.flac".into(),
                range: Some("bytes=100-".into()),
            })
        );
        assert_eq!(parse_request("garbage\r\n", &[]), None);
    }

    #[test]
    fn only_song_paths_have_a_token() {
        assert_eq!(token_of("/songs/0a1b2c.flac"), Some("0a1b2c"));
        assert_eq!(token_of("/songs/.flac"), None);
        assert_eq!(token_of("/songs/../etc.passwd"), None);
        assert_eq!(token_of("/other/0a1b2c.flac"), None);
        assert_eq!(token_of("/songs/0a1b2c"), None);
    }

    #[test]
    fn extensions_are_kept_path_safe() {
        assert_eq!(sanitize_ext("FLAC"), "flac");
        assert_eq!(sanitize_ext("m4a/../x"), "m4ax");
        assert_eq!(sanitize_ext(""), "bin");
    }

    #[test]
    fn urls_point_at_the_relay_on_the_given_address() {
        let relay = SongRelay::start().unwrap();
        let path = relay.register(Url::parse("http://server/rest/stream?id=1").unwrap(), "flac");
        assert!(path.starts_with("/songs/") && path.ends_with(".flac"), "{path}");
        let url = relay.url("192.168.1.20".parse().unwrap(), &path);
        assert_eq!(url, format!("http://192.168.1.20:{}{path}", relay.port));
        // A v4-mapped address is still the v4 listener.
        let mapped = relay.url("::ffff:192.168.1.20".parse().unwrap(), &path);
        assert_eq!(mapped, url);
    }

    /// End to end: a fake server behind the relay, and a request through it
    /// that has to carry its Range header there and the answer back.
    #[test]
    fn forwards_registered_songs_with_their_range() {
        let upstream = TcpListener::bind("127.0.0.1:0").unwrap();
        let upstream_addr = upstream.local_addr().unwrap();
        let server = std::thread::spawn(move || {
            let (stream, _) = upstream.accept().unwrap();
            let request = read_request(&stream).unwrap();
            let mut out = &stream;
            write!(
                out,
                "HTTP/1.1 206 Partial Content\r\nContent-Type: audio/flac\r\n\
                 Content-Length: 4\r\nContent-Range: bytes 2-5/10\r\nAccept-Ranges: bytes\r\n\
                 Connection: close\r\n\r\nfLaC"
            )
            .unwrap();
            request
        });

        let relay = SongRelay::start().unwrap();
        let upstream_url = Url::parse(&format!("http://{upstream_addr}/rest/stream?id=7")).unwrap();
        let path = relay.register(upstream_url, "flac");

        let mut client =
            TcpStream::connect(SocketAddr::from((Ipv4Addr::LOCALHOST, relay.port))).unwrap();
        write!(client, "GET {path} HTTP/1.1\r\nHost: x\r\nRange: bytes=2-5\r\n\r\n").unwrap();
        let mut answer = String::new();
        client.read_to_string(&mut answer).unwrap();

        let seen = server.join().unwrap();
        assert_eq!(seen.path, "/rest/stream?id=7");
        assert_eq!(seen.range.as_deref(), Some("bytes=2-5"));
        assert!(answer.starts_with("HTTP/1.1 206 Partial Content\r\n"), "{answer}");
        assert!(answer.contains("Content-Range: bytes 2-5/10\r\n"), "{answer}");
        assert!(answer.contains("Content-Type: audio/flac\r\n"), "{answer}");
        assert!(answer.ends_with("\r\n\r\nfLaC"), "{answer}");
    }

    #[test]
    fn unregistered_paths_are_not_found() {
        let relay = SongRelay::start().unwrap();
        let mut client =
            TcpStream::connect(SocketAddr::from((Ipv4Addr::LOCALHOST, relay.port))).unwrap();
        write!(client, "GET /songs/deadbeef.flac HTTP/1.1\r\n\r\n").unwrap();
        let mut answer = String::new();
        client.read_to_string(&mut answer).unwrap();
        assert!(answer.starts_with("HTTP/1.1 404"), "{answer}");
    }
}
