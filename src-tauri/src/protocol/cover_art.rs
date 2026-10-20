//! `reverb-art://` custom protocol: serves cover art to `<img>` tags, proxying
//! Navidrome's getCoverArt behind a disk cache. Keeps auth tokens out of the
//! DOM and avoids re-downloading art on every render.
//!
//! URL shape: `reverb-art://localhost/{coverArtId}?size=300`
//! (on Windows the webview requests `http://reverb-art.localhost/...` — same
//! path shape, handled identically).

use std::fs;
use std::path::PathBuf;

use tauri::http::{header, Request, Response, StatusCode};
use tauri::{Manager, Runtime, UriSchemeContext, UriSchemeResponder};

use crate::state::AppState;

pub fn handle<R: Runtime>(
    ctx: UriSchemeContext<'_, R>,
    request: Request<Vec<u8>>,
    responder: UriSchemeResponder,
) {
    let app = ctx.app_handle().clone();

    let uri = request.uri().clone();
    let cover_id = uri.path().trim_start_matches('/').to_owned();
    let size: Option<u32> = uri
        .query()
        .and_then(|q| {
            q.split('&')
                .find_map(|kv| kv.strip_prefix("size="))
                .map(|v| v.parse().ok())
        })
        .flatten();

    if cover_id.is_empty() {
        responder.respond(error_response(StatusCode::BAD_REQUEST));
        return;
    }

    tauri::async_runtime::spawn(async move {
        let cache_dir = match app.path().app_cache_dir() {
            Ok(dir) => dir.join("covers"),
            Err(_) => {
                responder.respond(error_response(StatusCode::INTERNAL_SERVER_ERROR));
                return;
            }
        };
        let cache_path = cache_file(&cache_dir, &cover_id, size);

        if let Ok(bytes) = fs::read(&cache_path) {
            responder.respond(image_response(bytes));
            return;
        }

        let state = app.state::<AppState>();
        match state.subsonic.get_cover_art(&cover_id, size).await {
            Ok((bytes, _content_type)) => {
                let _ = fs::create_dir_all(&cache_dir);
                let tmp = cache_path.with_extension("tmp");
                if fs::write(&tmp, &bytes).is_ok() {
                    let _ = fs::rename(&tmp, &cache_path);
                }
                responder.respond(image_response(bytes));
            }
            Err(e) => {
                log::debug!("cover art {cover_id} failed: {e}");
                responder.respond(error_response(StatusCode::NOT_FOUND));
            }
        }
    });
}

fn cache_file(dir: &std::path::Path, id: &str, size: Option<u32>) -> PathBuf {
    // Cover IDs can contain characters that are hostile to filesystems.
    let safe: String =
        id.chars().map(|c| if c.is_ascii_alphanumeric() || c == '-' || c == '_' { c } else { '_' }).collect();
    dir.join(format!("{safe}_{}", size.map_or("full".to_owned(), |s| s.to_string())))
}

fn image_response(bytes: Vec<u8>) -> Response<Vec<u8>> {
    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, sniff_content_type(&bytes))
        .header(header::CACHE_CONTROL, "public, max-age=604800")
        .header(header::ACCESS_CONTROL_ALLOW_ORIGIN, "*")
        .body(bytes)
        .expect("static response construction cannot fail")
}

fn error_response(status: StatusCode) -> Response<Vec<u8>> {
    Response::builder()
        .status(status)
        .header(header::ACCESS_CONTROL_ALLOW_ORIGIN, "*")
        .body(Vec::new())
        .expect("static response construction cannot fail")
}

/// The cache stores raw bytes without metadata, so sniff the type back out.
fn sniff_content_type(bytes: &[u8]) -> &'static str {
    match bytes {
        [0xFF, 0xD8, 0xFF, ..] => "image/jpeg",
        [0x89, b'P', b'N', b'G', ..] => "image/png",
        [b'G', b'I', b'F', b'8', ..] => "image/gif",
        [b'R', b'I', b'F', b'F', _, _, _, _, b'W', b'E', b'B', b'P', ..] => "image/webp",
        _ => "application/octet-stream",
    }
}
