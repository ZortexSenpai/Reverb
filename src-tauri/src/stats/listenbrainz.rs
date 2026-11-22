//! ListenBrainz: importing listens for the stats page and submitting Plays.
//!
//! Every call authenticates with a user token (`Authorization: Token …`) from
//! the account's settings page — there is no password exchange or request
//! signing as with Last.fm. The API root is configurable, because servers
//! other than listenbrainz.org speak the same protocol: a self-hosted
//! ListenBrainz, or scrobble servers such as Maloja and Koito that accept
//! ListenBrainz submissions under a path of their own. Those often accept
//! submissions without serving listen history back, which is why importing is
//! probed separately from validating the token.

use std::time::Duration;

use reqwest::Url;
use serde_json::{json, Value};

use super::scrobble::ScrobbleTrack;
use super::store::StatScrobble;

pub const DEFAULT_API_ROOT: &str = "https://api.listenbrainz.org";
/// The most listens one `GET …/listens` returns (`MAX_ITEMS_PER_GET`).
pub const PAGE_SIZE: usize = 1000;
/// Self-hosted servers can sit behind slow links, but a request that has not
/// answered in this long is not going to.
const REQUEST_TIMEOUT: Duration = Duration::from_secs(30);

/// Turn whatever the user pasted into an API root: blank means
/// listenbrainz.org, a missing scheme means https (as for the server address
/// at sign-in), and the version or endpoint suffix some guides quote along with
/// the address (`…/1`, `…/1/submit-listens`) is dropped, since every call
/// appends its own. The result has no trailing slash.
pub fn normalize_api_root(raw: &str) -> Result<String, String> {
    let raw = raw.trim();
    if raw.is_empty() {
        return Ok(DEFAULT_API_ROOT.to_owned());
    }
    let with_scheme = if raw.contains("://") { raw.to_owned() } else { format!("https://{raw}") };
    let url = Url::parse(&with_scheme)
        .map_err(|_| format!("“{raw}” is not a valid address."))?;
    if !matches!(url.scheme(), "http" | "https") || url.host_str().is_none() {
        return Err(format!("“{raw}” is not an http(s) address."));
    }
    let mut root = url.as_str().split(['?', '#']).next().unwrap_or_default().to_owned();
    loop {
        let trimmed = root.trim_end_matches('/');
        let stripped = ["/1/submit-listens", "/1/validate-token", "/1"]
            .iter()
            .find_map(|suffix| trimmed.strip_suffix(suffix));
        match stripped {
            Some(shorter) => root = shorter.to_owned(),
            None => {
                root = trimmed.to_owned();
                break;
            }
        }
    }
    Ok(root)
}

/// `{api_root}/1/{segments…}`, with each segment percent-encoded — a user name
/// can hold characters that mean something in a path.
fn endpoint(api_root: &str, segments: &[&str]) -> Result<Url, String> {
    let mut url = Url::parse(api_root).map_err(|e| format!("Invalid ListenBrainz address: {e}"))?;
    url.path_segments_mut()
        .map_err(|_| "Invalid ListenBrainz address".to_owned())?
        .pop_if_empty()
        .push("1")
        .extend(segments);
    Ok(url)
}

fn authorized(request: reqwest::RequestBuilder, token: &str) -> reqwest::RequestBuilder {
    request.header("Authorization", format!("Token {token}")).timeout(REQUEST_TIMEOUT)
}

/// The parsed body of a successful call, or the server's own explanation of a
/// failed one (`{"code": 401, "error": "…"}`), falling back to the HTTP status.
async fn read_body(response: Result<reqwest::Response, reqwest::Error>) -> Result<Value, String> {
    let response = response.map_err(|e| format!("ListenBrainz request failed: {e}"))?;
    let status = response.status();
    let body: Value = response.json().await.unwrap_or(Value::Null);
    if status.is_success() {
        return Ok(body);
    }
    let detail = body["error"].as_str().or_else(|| body["message"].as_str());
    Err(match detail {
        Some(detail) => format!("ListenBrainz: {detail}"),
        None => format!("ListenBrainz answered {status}"),
    })
}

/// Check a token and learn whose it is. ListenBrainz answers an unknown token
/// with 200 and `"valid": false` rather than an error status.
pub async fn validate_token(
    client: &reqwest::Client,
    api_root: &str,
    token: &str,
) -> Result<String, String> {
    let url = endpoint(api_root, &["validate-token"])?;
    let body = read_body(authorized(client.get(url), token).send().await).await?;
    if body["valid"].as_bool() != Some(true) {
        return Err("ListenBrainz did not accept this user token. Copy it again from your \
                    ListenBrainz settings page."
            .to_owned());
    }
    // Servers that only mimic the API may not name the user; imports need the
    // name, submissions do not.
    Ok(body["user_name"].as_str().unwrap_or_default().to_owned())
}

/// The submission body: `playing_now` without a timestamp, `single` with the
/// unix second the play started.
fn listen_payload(track: &ScrobbleTrack, listened_at: Option<i64>) -> Value {
    let mut additional_info = json!({
        "media_player": "Reverb",
        "submission_client": "Reverb",
        "submission_client_version": env!("CARGO_PKG_VERSION"),
    });
    if let Some(secs) = track.duration_secs {
        additional_info["duration_ms"] = json!(secs * 1000);
    }
    let mut metadata = json!({
        "artist_name": track.artist,
        "track_name": track.track,
        "additional_info": additional_info,
    });
    if let Some(album) = &track.album {
        metadata["release_name"] = json!(album);
    }
    let mut listen = json!({ "track_metadata": metadata });
    if let Some(ts) = listened_at {
        listen["listened_at"] = json!(ts.max(0));
    }
    json!({
        "listen_type": if listened_at.is_some() { "single" } else { "playing_now" },
        "payload": [listen],
    })
}

/// Submit a finished play (`listened_at` = when it started) or, with `None`,
/// announce it as now playing.
pub async fn submit(
    client: &reqwest::Client,
    api_root: &str,
    token: &str,
    track: &ScrobbleTrack,
    listened_at: Option<i64>,
) -> Result<(), String> {
    let url = endpoint(api_root, &["submit-listens"])?;
    let request = authorized(client.post(url), token).json(&listen_payload(track, listened_at));
    read_body(request.send().await).await.map(|_| ())
}

#[derive(Debug)]
pub struct ListensPage {
    /// The usable listens, in whatever order the server sent them.
    pub scrobbles: Vec<StatScrobble>,
    /// How many listens the server returned, usable or not. A page shorter
    /// than requested is the last one.
    pub returned: usize,
}

/// The oldest `PAGE_SIZE` listens *after* `min_ts` (exclusive). With `min_ts`
/// set ListenBrainz walks forward in time, so an import can page from its
/// cursor towards now and stop anywhere without leaving a gap behind.
pub async fn fetch_listens(
    client: &reqwest::Client,
    api_root: &str,
    token: &str,
    user: &str,
    min_ts: i64,
) -> Result<ListensPage, String> {
    let mut url = endpoint(api_root, &["user", user, "listens"])?;
    url.query_pairs_mut()
        .append_pair("min_ts", &min_ts.max(0).to_string())
        .append_pair("count", &PAGE_SIZE.to_string());
    parse_listens(&read_body(authorized(client.get(url), token).send().await).await?)
}

/// The account's total number of listens, for the import's progress display.
pub async fn fetch_listen_count(
    client: &reqwest::Client,
    api_root: &str,
    token: &str,
    user: &str,
) -> Result<u64, String> {
    let url = endpoint(api_root, &["user", user, "listen-count"])?;
    let body = read_body(authorized(client.get(url), token).send().await).await?;
    body["payload"]["count"].as_u64().ok_or_else(|| "ListenBrainz sent no listen count".into())
}

fn parse_listens(body: &Value) -> Result<ListensPage, String> {
    let Some(listens) = body["payload"]["listens"].as_array() else {
        return Err("ListenBrainz sent no listens".into());
    };
    let text = |value: &Value| {
        value.as_str().map(str::trim).filter(|s| !s.is_empty()).map(str::to_owned)
    };
    let scrobbles = listens
        .iter()
        .filter_map(|listen| {
            let ts = listen["listened_at"]
                .as_i64()
                .or_else(|| listen["listened_at"].as_str()?.parse().ok())?;
            let metadata = &listen["track_metadata"];
            Some(StatScrobble {
                ts,
                artist: text(&metadata["artist_name"])?,
                track: text(&metadata["track_name"])?,
                album: text(&metadata["release_name"]),
            })
        })
        .collect();
    Ok(ListensPage { scrobbles, returned: listens.len() })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blank_api_root_means_listenbrainz_org() {
        assert_eq!(normalize_api_root("  ").unwrap(), DEFAULT_API_ROOT);
    }

    #[test]
    fn api_root_drops_trailing_slashes_and_endpoint_suffixes() {
        let cases = [
            ("https://api.listenbrainz.org/", "https://api.listenbrainz.org"),
            ("https://api.listenbrainz.org/1/", "https://api.listenbrainz.org"),
            (
                "https://maloja.example/apis/listenbrainz/1/submit-listens",
                "https://maloja.example/apis/listenbrainz",
            ),
            ("http://192.168.1.5:42010/apis/listenbrainz", "http://192.168.1.5:42010/apis/listenbrainz"),
            ("koito.example/apis/listenbrainz/", "https://koito.example/apis/listenbrainz"),
        ];
        for (raw, root) in cases {
            assert_eq!(normalize_api_root(raw).unwrap(), root, "{raw}");
        }
    }

    #[test]
    fn api_root_rejects_what_is_not_a_web_address() {
        assert!(normalize_api_root("ftp://example.com").is_err());
        assert!(normalize_api_root("https://").is_err());
    }

    #[test]
    fn endpoints_append_to_a_root_with_a_path_and_encode_segments() {
        let url = endpoint("https://maloja.example/apis/listenbrainz", &["submit-listens"]).unwrap();
        assert_eq!(url.as_str(), "https://maloja.example/apis/listenbrainz/1/submit-listens");
        let url = endpoint(DEFAULT_API_ROOT, &["user", "a b/c", "listens"]).unwrap();
        assert_eq!(url.as_str(), "https://api.listenbrainz.org/1/user/a%20b%2Fc/listens");
    }

    fn track() -> ScrobbleTrack {
        ScrobbleTrack {
            artist: "Artist".into(),
            track: "Song".into(),
            album: Some("Album".into()),
            duration_secs: Some(245),
        }
    }

    #[test]
    fn a_finished_play_is_a_single_listen_with_its_start_time() {
        let body = listen_payload(&track(), Some(1_700_000_000));
        assert_eq!(body["listen_type"], "single");
        let listen = &body["payload"][0];
        assert_eq!(listen["listened_at"], 1_700_000_000);
        assert_eq!(listen["track_metadata"]["artist_name"], "Artist");
        assert_eq!(listen["track_metadata"]["track_name"], "Song");
        assert_eq!(listen["track_metadata"]["release_name"], "Album");
        assert_eq!(listen["track_metadata"]["additional_info"]["duration_ms"], 245_000);
    }

    #[test]
    fn now_playing_carries_no_timestamp_and_omits_unknowns() {
        let bare = ScrobbleTrack { album: None, duration_secs: None, ..track() };
        let body = listen_payload(&bare, None);
        assert_eq!(body["listen_type"], "playing_now");
        let listen = &body["payload"][0];
        assert!(listen.get("listened_at").is_none());
        assert!(listen["track_metadata"].get("release_name").is_none());
        assert!(listen["track_metadata"]["additional_info"].get("duration_ms").is_none());
    }

    #[test]
    fn parses_listens_and_skips_unusable_ones() {
        let body: Value = serde_json::from_str(
            r#"{"payload": {"count": 3, "listens": [
                {"listened_at": 1700000100,
                 "track_metadata": {"artist_name": "B", "track_name": "Two", "release_name": "Alb"}},
                {"listened_at": 1700000050,
                 "track_metadata": {"artist_name": "", "track_name": "No artist"}},
                {"listened_at": 1700000000,
                 "track_metadata": {"artist_name": "A", "track_name": "One"}}
            ]}}"#,
        )
        .unwrap();
        let page = parse_listens(&body).unwrap();
        assert_eq!(page.returned, 3, "the short-page check counts what was sent");
        assert_eq!(page.scrobbles.len(), 2);
        assert_eq!(page.scrobbles[0].album.as_deref(), Some("Alb"));
        assert_eq!(page.scrobbles[1].ts, 1_700_000_000);
        assert_eq!(page.scrobbles[1].album, None);
    }

    #[test]
    fn a_body_without_listens_is_an_error() {
        assert!(parse_listens(&json!({"code": 404, "error": "Cannot find user"})).is_err());
    }
}
