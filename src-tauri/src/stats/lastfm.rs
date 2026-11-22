//! Last.fm scrobble import via user.getRecentTracks. Parsed defensively
//! through serde_json::Value — the Last.fm API returns objects instead of
//! arrays for single items and other quirks.

use serde_json::Value;

use super::scrobble::ScrobbleTrack;
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

/// A scrobbling session: the key, and the account name as Last.fm spells it
/// (the sign-in accepts other spellings, the import does not).
pub struct Session {
    pub key: String,
    pub username: Option<String>,
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
) -> Result<Session, String> {
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
    let key = body["session"]["key"]
        .as_str()
        .map(str::to_owned)
        .ok_or_else(|| "Last.fm did not return a session key".to_string())?;
    let username = body["session"]["name"].as_str().map(str::to_owned);
    Ok(Session { key, username })
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
        assert!(check_credential_shape("API key", "0123456789abcdef0123456789abcdef").is_ok());
        // Truncated by a partial paste.
        let err = check_credential_shape("shared secret", "0123456789ab").unwrap_err();
        assert!(err.contains("12 characters"), "{err}");
        // Right length, but not hex — e.g. a password pasted into the field.
        assert!(check_credential_shape("API key", "z".repeat(32).as_str()).is_err());
        // Whitespace counts, so a trailing newline is caught rather than signed.
        assert!(check_credential_shape("API key", "0123456789abcdef0123456789abcdef ").is_err());
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
}
