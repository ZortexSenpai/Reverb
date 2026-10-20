//! Last.fm scrobble import via user.getRecentTracks. Parsed defensively
//! through serde_json::Value — the Last.fm API returns objects instead of
//! arrays for single items and other quirks.

use serde_json::Value;

use super::store::StatScrobble;

const API_ROOT: &str = "https://ws.audioscrobbler.com/2.0/";
pub const PAGE_SIZE: u32 = 200;

#[derive(Debug)]
pub struct RecentPage {
    pub scrobbles: Vec<StatScrobble>,
    pub total_pages: u32,
    pub total: u64,
}

fn as_array(value: &Value) -> Vec<&Value> {
    match value {
        Value::Array(items) => items.iter().collect(),
        Value::Null => vec![],
        other => vec![other],
    }
}

fn text_field(value: &Value, key: &str) -> Option<String> {
    let field = &value[key];
    field["#text"]
        .as_str()
        .or_else(|| field["name"].as_str())
        .or_else(|| field.as_str())
        .map(str::to_owned)
        .filter(|s| !s.is_empty())
}

/// Fetch one page of scrobbles newer than `from_ts` (exclusive). A `to_ts`
/// bound freezes the window so page numbers stay stable across requests.
pub async fn fetch_recent(
    client: &reqwest::Client,
    username: &str,
    api_key: &str,
    from_ts: i64,
    to_ts: Option<i64>,
    page: u32,
) -> Result<RecentPage, String> {
    let mut query = vec![
        ("method", "user.getrecenttracks".to_string()),
        ("user", username.to_string()),
        ("api_key", api_key.to_string()),
        ("format", "json".to_string()),
        ("limit", PAGE_SIZE.to_string()),
        ("from", from_ts.max(0).to_string()),
        ("page", page.to_string()),
    ];
    if let Some(to_ts) = to_ts {
        query.push(("to", to_ts.to_string()));
    }
    let resp = client
        .get(API_ROOT)
        .query(&query)
        .send()
        .await
        .map_err(|e| format!("Last.fm request failed: {e}"))?;

    let body: Value = resp.json().await.map_err(|e| format!("Last.fm response invalid: {e}"))?;
    parse_recent(&body)
}

fn parse_recent(body: &Value) -> Result<RecentPage, String> {
    if let Some(message) = body["message"].as_str() {
        // API-level error ({"error": 6, "message": "User not found"} etc.)
        return Err(format!("Last.fm: {message}"));
    }

    let recent = &body["recenttracks"];
    let attr = &recent["@attr"];
    let parse_num = |v: &Value| -> u64 {
        v.as_str().and_then(|s| s.parse().ok()).or_else(|| v.as_u64()).unwrap_or(0)
    };

    let mut scrobbles = Vec::new();
    for track in as_array(&recent["track"]) {
        // Skip the currently-playing entry (no timestamp).
        if track["@attr"]["nowplaying"].as_str() == Some("true") {
            continue;
        }
        let Some(ts) = track["date"]["uts"]
            .as_str()
            .and_then(|s| s.parse::<i64>().ok())
            .or_else(|| track["date"]["uts"].as_i64())
        else {
            continue;
        };
        let Some(name) = track["name"].as_str().map(str::to_owned) else { continue };
        let Some(artist) = text_field(track, "artist") else { continue };
        scrobbles.push(StatScrobble {
            ts,
            artist,
            track: name,
            album: text_field(track, "album"),
        });
    }

    Ok(RecentPage {
        scrobbles,
        total_pages: parse_num(&attr["totalPages"]) as u32,
        total: parse_num(&attr["total"]),
    })
}

/// Artist biography via artist.getInfo (fallback when the server has none).
pub async fn fetch_artist_bio(
    client: &reqwest::Client,
    artist: &str,
    api_key: &str,
) -> Result<Option<(String, Option<String>)>, String> {
    let resp = client
        .get(API_ROOT)
        .query(&[
            ("method", "artist.getinfo"),
            ("artist", artist),
            ("api_key", api_key),
            ("format", "json"),
            ("autocorrect", "1"),
        ])
        .send()
        .await
        .map_err(|e| format!("Last.fm request failed: {e}"))?;
    let body: Value = resp.json().await.map_err(|e| format!("Last.fm response invalid: {e}"))?;
    if let Some(message) = body["message"].as_str() {
        return Err(format!("Last.fm: {message}"));
    }
    let bio = body["artist"]["bio"]["content"]
        .as_str()
        .filter(|s| !s.trim().is_empty())
        .or_else(|| body["artist"]["bio"]["summary"].as_str())
        .map(str::to_owned);
    let url = body["artist"]["url"].as_str().map(str::to_owned);
    Ok(bio.map(|b| (b, url)))
}

// ---------------------------------------------------------------------------
// Scrobbling (the write half of the API)
//
// Reading recent tracks needs nothing but an API key, but submitting a play
// is a signed, authenticated call: every request carries an `api_sig` — the
// md5 of the request's own parameters plus the account's shared secret — and
// a session key standing in for the user's password.
// ---------------------------------------------------------------------------

use std::fmt::Write as _;

use md5::{Digest, Md5};

/// One play, as Last.fm identifies it. `track` and `artist` are the only
/// fields it requires; a submission missing either is rejected outright.
#[derive(Debug, Clone)]
pub struct ScrobbleTrack {
    pub artist: String,
    pub track: String,
    pub album: Option<String>,
    /// Track length in seconds; omitted when the engine never learned it.
    pub duration_secs: Option<u64>,
}

/// The artist name to submit for a play.
///
/// Last.fm identifies a play by artist *name*, so a joined credit like
/// "A feat. B" usually matches no artist there and the play is filed under
/// nothing useful. `first_artist_only` trades the featured names for a play
/// that lands on the main artist — which is only possible where the server
/// enumerated the credit, so without a first artist the full string still goes.
pub fn scrobble_artist(
    artist: Option<String>,
    first_artist: Option<String>,
    first_artist_only: bool,
) -> Option<String> {
    if !first_artist_only {
        return artist;
    }
    // A blank first artist would otherwise beat a perfectly good credit and
    // lose the scrobble entirely (`from_parts` drops empty artists).
    first_artist.filter(|name| !name.trim().is_empty()).or(artist)
}

impl ScrobbleTrack {
    /// Build a submission from the engine's (all-optional) track metadata.
    /// Returns `None` when artist or title is missing — Last.fm would only
    /// reject it, and a play it cannot attribute is not worth a round trip.
    pub fn from_parts(
        artist: Option<String>,
        title: Option<String>,
        album: Option<String>,
        duration_ms: u64,
    ) -> Option<Self> {
        let artist = artist.filter(|s| !s.trim().is_empty())?;
        let track = title.filter(|s| !s.trim().is_empty())?;
        Some(Self {
            artist,
            track,
            album: album.filter(|s| !s.trim().is_empty()),
            duration_secs: (duration_ms > 0).then_some(duration_ms / 1000),
        })
    }
}

fn md5_hex(input: &str) -> String {
    let digest = Md5::digest(input.as_bytes());
    let mut out = String::with_capacity(32);
    for byte in digest {
        let _ = write!(out, "{byte:02x}");
    }
    out
}

/// Last.fm's request signature: every parameter except `format` and `callback`
/// sorted by name, concatenated as `name || value` with no separators, the
/// shared secret appended, md5'd.
fn sign(params: &[(&str, String)], secret: &str) -> String {
    let mut signed: Vec<&(&str, String)> =
        params.iter().filter(|(k, _)| *k != "format" && *k != "callback").collect();
    signed.sort_by(|a, b| a.0.cmp(b.0));
    let mut input = String::new();
    for (key, value) in signed {
        input.push_str(key);
        input.push_str(value);
    }
    input.push_str(secret);
    md5_hex(&input)
}

/// POST a signed call and hand back the parsed body, mapping Last.fm's
/// in-band `{"error": N, "message": ...}` onto `Err`.
async fn post_signed(
    client: &reqwest::Client,
    mut params: Vec<(&str, String)>,
    secret: &str,
) -> Result<Value, String> {
    params.push(("api_sig", sign(&params, secret)));
    params.push(("format", "json".to_string()));
    let resp = client
        .post(API_ROOT)
        .form(&params)
        .send()
        .await
        .map_err(|e| format!("Last.fm request failed: {e}"))?;
    let body: Value = resp.json().await.map_err(|e| format!("Last.fm response invalid: {e}"))?;
    if let Some(message) = body["message"].as_str() {
        return Err(format!("Last.fm: {message}"));
    }
    Ok(body)
}

/// Last.fm issues API keys and shared secrets as 32-character hex strings.
/// Checking the shape before sending anything turns the most common setup
/// mistake — a credential that got truncated or half-pasted — into a precise
/// message, instead of the server's opaque "invalid method signature", which
/// says nothing about *which* value is wrong.
pub fn check_credential_shape(label: &str, value: &str) -> Result<(), String> {
    if value.len() == 32 && value.chars().all(|c| c.is_ascii_hexdigit()) {
        return Ok(());
    }
    Err(format!(
        "That {label} does not look right: Last.fm issues 32 hexadecimal \
         characters, but this is {} character{} long. Copy it again from the \
         application's page on last.fm/api/accounts.",
        value.len(),
        if value.len() == 1 { "" } else { "s" },
    ))
}

/// Check that an API key and shared secret belong to the same Last.fm
/// application. `auth.getToken` is signed but takes no user credentials, so it
/// isolates a bad key/secret pair from a bad password — the two produce
/// different failures that are otherwise easy to confuse.
pub async fn verify_key_pair(
    client: &reqwest::Client,
    api_key: &str,
    secret: &str,
) -> Result<(), String> {
    let params = vec![
        ("method", "auth.getToken".to_string()),
        ("api_key", api_key.to_string()),
    ];
    post_signed(client, params, secret).await.map(|_| ())
}

/// Trade a username and password for a long-lived session key
/// (`auth.getMobileSession`). The password is used for this one call and never
/// stored — the session key it returns is what gets kept.
pub async fn get_mobile_session(
    client: &reqwest::Client,
    api_key: &str,
    secret: &str,
    username: &str,
    password: &str,
) -> Result<String, String> {
    let params = vec![
        ("method", "auth.getMobileSession".to_string()),
        ("username", username.to_string()),
        ("password", password.to_string()),
        ("api_key", api_key.to_string()),
    ];
    let body = match post_signed(client, params, secret).await {
        Ok(body) => body,
        // "Invalid method signature" points at the key/secret pair rather than
        // at anything the user typed as a password, but the message alone
        // reads like a generic auth failure. Ask Last.fm which one is wrong so
        // the error can say so.
        Err(message) if message.contains("signature") => {
            return Err(match verify_key_pair(client, api_key, secret).await {
                // The pair is fine, so the signature failure was about this
                // call's other parameters — nothing the user can fix by
                // re-copying credentials.
                Ok(()) => message,
                Err(_) => "Last.fm rejected the signature: this API key and \
                     shared secret are not from the same application. Copy both \
                     from the same entry on last.fm/api/accounts."
                    .to_string(),
            });
        }
        Err(message) => return Err(message),
    };
    body["session"]["key"]
        .as_str()
        .map(str::to_owned)
        .ok_or_else(|| "Last.fm did not return a session key".to_string())
}

/// Announce the track as now playing. Last.fm expires the entry on its own
/// after roughly the track's length, so this is re-sent periodically.
pub async fn update_now_playing(
    client: &reqwest::Client,
    api_key: &str,
    secret: &str,
    session_key: &str,
    track: &ScrobbleTrack,
) -> Result<(), String> {
    let mut params = vec![
        ("method", "track.updateNowPlaying".to_string()),
        ("artist", track.artist.clone()),
        ("track", track.track.clone()),
        ("api_key", api_key.to_string()),
        ("sk", session_key.to_string()),
    ];
    if let Some(album) = &track.album {
        params.push(("album", album.clone()));
    }
    if let Some(duration) = track.duration_secs {
        params.push(("duration", duration.to_string()));
    }
    post_signed(client, params, secret).await.map(|_| ())
}

/// Submit a finished play. `timestamp` is unix seconds at which the track
/// *started*, which is what Last.fm records as the play time.
pub async fn scrobble(
    client: &reqwest::Client,
    api_key: &str,
    secret: &str,
    session_key: &str,
    track: &ScrobbleTrack,
    timestamp: i64,
) -> Result<(), String> {
    let mut params = vec![
        ("method", "track.scrobble".to_string()),
        ("artist", track.artist.clone()),
        ("track", track.track.clone()),
        ("timestamp", timestamp.max(0).to_string()),
        ("api_key", api_key.to_string()),
        ("sk", session_key.to_string()),
    ];
    if let Some(album) = &track.album {
        params.push(("album", album.clone()));
    }
    if let Some(duration) = track.duration_secs {
        params.push(("duration", duration.to_string()));
    }
    post_signed(client, params, secret).await.map(|_| ())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scrobble_artist_sends_the_whole_credit_by_default() {
        let artist = || Some("A feat. B".to_owned());
        let first = || Some("A".to_owned());
        assert_eq!(scrobble_artist(artist(), first(), false).as_deref(), Some("A feat. B"));
    }

    #[test]
    fn scrobble_artist_sends_only_the_first_when_asked() {
        let artist = || Some("A feat. B".to_owned());
        assert_eq!(
            scrobble_artist(artist(), Some("A".to_owned()), true).as_deref(),
            Some("A")
        );
    }

    #[test]
    fn scrobble_artist_falls_back_when_the_first_artist_is_unknown() {
        // Servers without the OpenSubsonic artists extension send the credit as
        // one opaque string: there is nothing to take the first artist from, and
        // guessing at separators would split "Simon & Garfunkel".
        let artist = || Some("Simon & Garfunkel".to_owned());
        assert_eq!(
            scrobble_artist(artist(), None, true).as_deref(),
            Some("Simon & Garfunkel")
        );
        // A blank one must not win, or the play is dropped rather than filed.
        assert_eq!(
            scrobble_artist(artist(), Some("  ".to_owned()), true).as_deref(),
            Some("Simon & Garfunkel")
        );
        // Nothing at all stays nothing, whichever way the preference is set.
        assert_eq!(scrobble_artist(None, None, true), None);
        assert_eq!(scrobble_artist(None, None, false), None);
    }

    #[test]
    fn parses_recent_tracks_with_quirks() {
        // Now-playing entry (no date), string numbers, "#text" fields.
        let body: Value = serde_json::from_str(
            r##"{
              "recenttracks": {
                "track": [
                  {"name": "Live One", "artist": {"#text": "A"}, "album": {"#text": ""},
                   "@attr": {"nowplaying": "true"}},
                  {"name": "Song B", "artist": {"#text": "B"}, "album": {"#text": "Alb"},
                   "date": {"uts": "1700000100"}},
                  {"name": "Song A", "artist": {"#text": "A"}, "album": {"#text": "Alb"},
                   "date": {"uts": "1700000000"}}
                ],
                "@attr": {"page": "1", "totalPages": "3", "total": "512"}
              }
            }"##,
        )
        .unwrap();
        let page = parse_recent(&body).unwrap();
        assert_eq!(page.scrobbles.len(), 2, "now-playing entry skipped");
        assert_eq!(page.scrobbles[0].track, "Song B");
        assert_eq!(page.scrobbles[0].ts, 1_700_000_100);
        assert_eq!(page.scrobbles[1].artist, "A");
        assert_eq!(page.total_pages, 3);
        assert_eq!(page.total, 512);
    }

    #[test]
    fn single_track_object_and_errors() {
        // Last.fm returns an object (not array) for a single item.
        let body: Value = serde_json::from_str(
            r##"{"recenttracks": {"track":
                {"name": "Solo", "artist": {"#text": "X"}, "date": {"uts": "1700000000"}},
                "@attr": {"page": "1", "totalPages": "1", "total": "1"}}}"##,
        )
        .unwrap();
        let page = parse_recent(&body).unwrap();
        assert_eq!(page.scrobbles.len(), 1);
        assert_eq!(page.scrobbles[0].album, None);

        let error: Value =
            serde_json::from_str(r#"{"error": 6, "message": "User not found"}"#).unwrap();
        assert!(parse_recent(&error).unwrap_err().contains("User not found"));
    }

    #[test]
    fn credential_shape_accepts_only_32_hex_characters() {
        assert!(check_credential_shape("API key", "9071b02c5295280feb8ee2c95ff0d677").is_ok());
        // Truncated by a partial paste.
        let err = check_credential_shape("shared secret", "9071b02c5295").unwrap_err();
        assert!(err.contains("12 characters"), "{err}");
        // Right length, but not hex — e.g. a password pasted into the field.
        assert!(check_credential_shape("API key", "z".repeat(32).as_str()).is_err());
        // Whitespace counts, so a trailing newline is caught rather than signed.
        assert!(check_credential_shape("API key", "9071b02c5295280feb8ee2c95ff0d677 ").is_err());
    }

    #[test]
    fn signature_sorts_by_name_and_appends_the_secret() {
        // Deliberately out of order: the signature is over the *sorted*
        // parameters, concatenated name-then-value with no separators.
        let params = vec![
            ("track", "Song Name".to_string()),
            ("method", "track.scrobble".to_string()),
            ("api_key", "KEY".to_string()),
        ];
        // md5("api_keyKEYmethodtrack.scrobbletrackSong NameSECRET")
        assert_eq!(sign(&params, "SECRET"), "0e8cdbbc14a294327a6f15dbbfc3fd44");
    }

    #[test]
    fn signature_excludes_format() {
        let signed = vec![("method", "auth.getMobileSession".to_string())];
        let mut with_format = signed.clone();
        with_format.push(("format", "json".to_string()));
        assert_eq!(sign(&signed, "s"), sign(&with_format, "s"));
    }

    #[test]
    fn md5_hex_matches_a_known_digest() {
        assert_eq!(md5_hex("sesamec19b2d"), "26719a1196d2a940705a59634eb18eab");
    }

    #[test]
    fn submissions_need_an_artist_and_a_title() {
        assert!(ScrobbleTrack::from_parts(None, Some("T".into()), None, 0).is_none());
        assert!(ScrobbleTrack::from_parts(Some("A".into()), None, None, 0).is_none());
        // Whitespace-only metadata is as unusable as none at all.
        assert!(ScrobbleTrack::from_parts(Some("  ".into()), Some("T".into()), None, 0).is_none());
    }

    #[test]
    fn submissions_carry_album_and_duration_when_known() {
        let track =
            ScrobbleTrack::from_parts(Some("A".into()), Some("T".into()), Some("Al".into()), 245_000)
                .expect("artist and title are present");
        assert_eq!(track.album.as_deref(), Some("Al"));
        assert_eq!(track.duration_secs, Some(245));
        // An unknown duration is omitted rather than sent as zero.
        let unknown = ScrobbleTrack::from_parts(Some("A".into()), Some("T".into()), None, 0)
            .expect("artist and title are present");
        assert_eq!(unknown.duration_secs, None);
        assert_eq!(unknown.album, None);
    }
}
