//! Media sources feeding symphonia's `MediaSourceStream`.
//!
//! Known-length streams (Navidrome `format=raw`) use `CachedHttpSource`: the
//! file downloads at full speed into an anonymous temp file while playback
//! reads from it. Seeks inside the downloaded region are local and instant —
//! crucial because symphonia locates seek targets by bisection, which would
//! otherwise cost dozens of HTTP round trips. Reads far ahead of the
//! download frontier are served by one bounded range request each.
//!
//! Unknown-length streams (server transcodes) use the plain pass-through
//! `HttpMediaSource`, which is not seekable.

use std::fs::File;
use std::io::{self, Read, Seek, SeekFrom};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::time::Duration;

use reqwest::blocking::{Client, Response};
use reqwest::header;
use reqwest::StatusCode;
use reqwest::Url;
use symphonia::core::io::MediaSource;

/// Forward seeks below this distance are serviced by reading and discarding
/// from the current connection instead of opening a new one.
const FORWARD_SKIP_LIMIT: u64 = 256 * 1024;

/// Reads this far beyond the download frontier use a direct range request
/// instead of waiting for the sequential download to catch up.
const FAR_READ_GAP: u64 = 2 * 1024 * 1024;

/// Open the right source for a stream URL.
pub fn open_stream_source(client: Client, url: Url) -> io::Result<Box<dyn MediaSource>> {
    let resp = client
        .get(url.clone())
        .send()
        .and_then(|r| r.error_for_status())
        .map_err(io_err)?;
    match resp.content_length() {
        Some(total) if total > 0 => {
            Ok(Box::new(CachedHttpSource::start(client, url, resp, total)?))
        }
        _ => Ok(Box::new(HttpMediaSource::from_response(client, url, resp))),
    }
}

// ---------------------------------------------------------------------------
// CachedHttpSource
// ---------------------------------------------------------------------------

struct CacheState {
    downloaded: u64,
    failed: Option<String>,
}

fn write_at(file: &File, offset: u64, data: &[u8]) -> io::Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::FileExt;
        file.write_all_at(data, offset)
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::FileExt;
        let mut written = 0usize;
        while written < data.len() {
            written += file.seek_write(&data[written..], offset + written as u64)?;
        }
        Ok(())
    }
}

fn read_at(file: &File, offset: u64, buf: &mut [u8]) -> io::Result<usize> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::FileExt;
        file.read_at(buf, offset)
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::FileExt;
        file.seek_read(buf, offset)
    }
}

pub struct CachedHttpSource {
    cache: File,
    state: Arc<(Mutex<CacheState>, Condvar)>,
    cancel: Arc<AtomicBool>,
    client: Client,
    url: Url,
    total: u64,
    pos: u64,
}

impl CachedHttpSource {
    fn start(client: Client, url: Url, mut resp: Response, total: u64) -> io::Result<Self> {
        // Anonymous temp file: vanishes when the last handle drops.
        let cache = tempfile::tempfile()?;
        let writer = cache.try_clone()?;
        let state = Arc::new((Mutex::new(CacheState { downloaded: 0, failed: None }), Condvar::new()));
        let cancel = Arc::new(AtomicBool::new(false));

        let thread_state = state.clone();
        let thread_cancel = cancel.clone();
        std::thread::Builder::new()
            .name("reverb-stream-cache".into())
            .spawn(move || {
                let mut offset = 0u64;
                let mut buf = vec![0u8; 256 * 1024];
                loop {
                    if thread_cancel.load(Ordering::Relaxed) {
                        return;
                    }
                    match resp.read(&mut buf) {
                        Ok(0) => {
                            // Mark complete even if the server under-delivered.
                            let (m, cv) = &*thread_state;
                            m.lock().unwrap().downloaded = offset.max(total);
                            cv.notify_all();
                            return;
                        }
                        Ok(n) => {
                            if let Err(e) = write_at(&writer, offset, &buf[..n]) {
                                let (m, cv) = &*thread_state;
                                m.lock().unwrap().failed = Some(e.to_string());
                                cv.notify_all();
                                return;
                            }
                            offset += n as u64;
                            let (m, cv) = &*thread_state;
                            m.lock().unwrap().downloaded = offset;
                            cv.notify_all();
                        }
                        Err(e) => {
                            let (m, cv) = &*thread_state;
                            m.lock().unwrap().failed = Some(e.to_string());
                            cv.notify_all();
                            return;
                        }
                    }
                }
            })
            .expect("spawning the stream cache thread cannot fail");

        Ok(Self { cache, state, cancel, client, url, total, pos: 0 })
    }

    fn downloaded(&self) -> Result<u64, io::Error> {
        let st = self.state.0.lock().unwrap();
        if let Some(e) = &st.failed {
            return Err(io::Error::other(format!("stream download failed: {e}")));
        }
        Ok(st.downloaded)
    }

    /// Block until at least one byte at `pos` is available.
    fn wait_for(&self, pos: u64) -> io::Result<u64> {
        let (m, cv) = &*self.state;
        let mut st = m.lock().unwrap();
        loop {
            if let Some(e) = &st.failed {
                return Err(io::Error::other(format!("stream download failed: {e}")));
            }
            if st.downloaded > pos {
                return Ok(st.downloaded);
            }
            st = cv.wait_timeout(st, Duration::from_millis(100)).unwrap().0;
        }
    }

    /// Serve a read far ahead of the download frontier with one bounded
    /// range request (does not touch the cache).
    fn far_read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let end = (self.pos + buf.len() as u64).min(self.total);
        let resp = self
            .client
            .get(self.url.clone())
            .header(header::RANGE, format!("bytes={}-{}", self.pos, end - 1))
            .send()
            .and_then(|r| r.error_for_status())
            .map_err(io_err)?;
        if resp.status() != StatusCode::PARTIAL_CONTENT {
            return Err(io::Error::new(
                io::ErrorKind::Unsupported,
                "server does not honor Range requests",
            ));
        }
        let mut body = resp;
        let n = body.read(buf)?;
        self.pos += n as u64;
        Ok(n)
    }
}

impl Read for CachedHttpSource {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        if self.pos >= self.total || buf.is_empty() {
            return Ok(0);
        }
        let downloaded = self.downloaded()?;
        // Far ahead of the frontier (e.g. a seek-bisection probe near the
        // file end): one direct range request instead of stalling.
        if self.pos > downloaded + FAR_READ_GAP {
            return self.far_read(buf);
        }
        let available = if downloaded > self.pos { downloaded } else { self.wait_for(self.pos)? };
        let want = (buf.len() as u64).min(available - self.pos).min(self.total - self.pos) as usize;
        let n = read_at(&self.cache, self.pos, &mut buf[..want])?;
        self.pos += n as u64;
        Ok(n)
    }
}

impl Seek for CachedHttpSource {
    fn seek(&mut self, from: SeekFrom) -> io::Result<u64> {
        let target = match from {
            SeekFrom::Start(offset) => offset,
            SeekFrom::Current(delta) => self
                .pos
                .checked_add_signed(delta)
                .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "seek out of range"))?,
            SeekFrom::End(delta) => self
                .total
                .checked_add_signed(delta)
                .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "seek out of range"))?,
        };
        self.pos = target.min(self.total);
        Ok(self.pos)
    }
}

impl MediaSource for CachedHttpSource {
    fn is_seekable(&self) -> bool {
        true
    }

    fn byte_len(&self) -> Option<u64> {
        Some(self.total)
    }
}

impl Drop for CachedHttpSource {
    fn drop(&mut self) {
        self.cancel.store(true, Ordering::Relaxed);
    }
}

// ---------------------------------------------------------------------------
// HttpMediaSource (pass-through for unknown-length transcoded streams)
// ---------------------------------------------------------------------------

pub fn stream_http_client() -> Client {
    Client::builder()
        .connect_timeout(Duration::from_secs(10))
        // No overall timeout: this connection streams for the whole track.
        .timeout(None::<Duration>)
        // Never let reqwest transparently decompress: byte offsets must map
        // 1:1 to file offsets for Range-based seeking.
        .no_gzip()
        .build()
        .expect("blocking client construction cannot fail with static config")
}

pub struct HttpMediaSource {
    client: Client,
    url: Url,
    body: Option<Response>,
    pos: u64,
    len: Option<u64>,
    seekable: bool,
}

impl HttpMediaSource {
    pub fn from_response(client: Client, url: Url, resp: Response) -> Self {
        let len = resp.content_length();
        let accepts_ranges = resp
            .headers()
            .get(header::ACCEPT_RANGES)
            .and_then(|v| v.to_str().ok())
            .map(|v| v.eq_ignore_ascii_case("bytes"))
            .unwrap_or(false);
        let seekable = accepts_ranges && len.is_some();

        Self { client, url, body: Some(resp), pos: 0, len, seekable }
    }

    pub fn is_transcoded_stream(&self) -> bool {
        !self.seekable
    }

    fn reopen_at(&mut self, target: u64) -> io::Result<()> {
        // Seeking to (or past) the end is a valid position — represent it as
        // an immediate EOF instead of asking the server for an empty range
        // (which would get a 416).
        if let Some(len) = self.len {
            if target >= len {
                self.body = None;
                self.pos = target.min(len);
                return Ok(());
            }
        }
        // Leave the current connection untouched until the new one succeeds,
        // so a failed seek doesn't kill ongoing playback.
        let resp = self
            .client
            .get(self.url.clone())
            .header(header::RANGE, format!("bytes={target}-"))
            .send()
            .and_then(|r| r.error_for_status())
            .map_err(io_err)?;
        if resp.status() != StatusCode::PARTIAL_CONTENT {
            // Server ignored the Range header; we're back at byte 0.
            if target == 0 {
                self.body = Some(resp);
                self.pos = 0;
                return Ok(());
            }
            return Err(io::Error::new(
                io::ErrorKind::Unsupported,
                "server does not honor Range requests",
            ));
        }
        self.body = Some(resp);
        self.pos = target;
        Ok(())
    }

    fn skip_forward(&mut self, mut remaining: u64) -> io::Result<()> {
        let mut scratch = [0u8; 8192];
        while remaining > 0 {
            let want = remaining.min(scratch.len() as u64) as usize;
            let n = self.read(&mut scratch[..want])?;
            if n == 0 {
                break;
            }
            remaining -= n as u64;
        }
        Ok(())
    }
}

impl Read for HttpMediaSource {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let Some(body) = self.body.as_mut() else {
            return Ok(0); // EOF
        };
        match body.read(buf) {
            Ok(0) => {
                self.body = None;
                Ok(0)
            }
            Ok(n) => {
                self.pos += n as u64;
                Ok(n)
            }
            Err(e) => Err(e),
        }
    }
}

impl Seek for HttpMediaSource {
    fn seek(&mut self, from: SeekFrom) -> io::Result<u64> {
        let target = match from {
            SeekFrom::Start(offset) => offset,
            SeekFrom::Current(delta) => self
                .pos
                .checked_add_signed(delta)
                .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "seek out of range"))?,
            SeekFrom::End(delta) => {
                let len = self.len.ok_or_else(|| {
                    io::Error::new(io::ErrorKind::Unsupported, "stream length unknown")
                })?;
                len.checked_add_signed(delta).ok_or_else(|| {
                    io::Error::new(io::ErrorKind::InvalidInput, "seek out of range")
                })?
            }
        };

        if target == self.pos {
            return Ok(self.pos);
        }
        if !self.seekable {
            return Err(io::Error::new(
                io::ErrorKind::Unsupported,
                "stream is not seekable (transcoded)",
            ));
        }
        if target > self.pos && target - self.pos <= FORWARD_SKIP_LIMIT && self.body.is_some() {
            let skip = target - self.pos;
            self.skip_forward(skip)?;
            // skip_forward may stop early at EOF; report the real position.
            return Ok(self.pos);
        }
        self.reopen_at(target)?;
        Ok(self.pos)
    }
}

impl MediaSource for HttpMediaSource {
    fn is_seekable(&self) -> bool {
        self.seekable
    }

    fn byte_len(&self) -> Option<u64> {
        self.len
    }
}

fn io_err(e: reqwest::Error) -> io::Error {
    let kind = if e.is_timeout() {
        io::ErrorKind::TimedOut
    } else if e.is_connect() {
        io::ErrorKind::ConnectionRefused
    } else {
        io::ErrorKind::Other
    };
    io::Error::new(kind, e)
}
