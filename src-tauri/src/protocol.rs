//! The `archive://` URI scheme.
//!
//! Images are streamed to the webview through a custom scheme rather than
//! handed over IPC as base64: a 12 MP photo is ~5 MB, base64 inflates it by a
//! third, and the whole string has to be held in memory on both sides. Here
//! the webview asks for a URL, this handler decrypts that one file, and the
//! bytes go straight into an `<img>`.
//!
//! Routes:
//!   archive://localhost/thumb/<fileId>  grid thumbnail, JPEG, cached
//!   archive://localhost/full/<fileId>   full size, transcoded only if needed
//!
//! Nothing decrypted is written anywhere except the thumbnail cache, and the
//! log lines below carry file ids and byte counts, never image data.

use std::sync::{Arc, Condvar, Mutex, OnceLock};

use tauri::http::{Request, Response, StatusCode};
use tauri::{Manager, UriSchemeContext, UriSchemeResponder, Wry};

use crate::ios::{thumbs, Backup};
use crate::AppState;

pub const SCHEME: &str = "archive";

/// Decoding a HEIC costs hundreds of milliseconds and is CPU-bound, so the
/// number running at once is capped. Without this, scrolling quickly through a
/// large camera roll would start a decode for every cell the grid touched and
/// leave an old machine thrashing.
///
/// Cache hits never take a permit — they are a file read.
struct Limit {
    available: Mutex<usize>,
    freed: Condvar,
}

impl Limit {
    fn acquire(&self) -> Permit<'_> {
        let mut available = self.available.lock().unwrap();
        while *available == 0 {
            available = self.freed.wait(available).unwrap();
        }
        *available -= 1;
        Permit { limit: self }
    }
}

struct Permit<'a> {
    limit: &'a Limit,
}

impl Drop for Permit<'_> {
    fn drop(&mut self) {
        *self.limit.available.lock().unwrap() += 1;
        self.limit.freed.notify_one();
    }
}

fn decode_limit() -> &'static Limit {
    static LIMIT: OnceLock<Limit> = OnceLock::new();
    LIMIT.get_or_init(|| {
        // Leave a core for the UI on the small machines this targets.
        let cores = std::thread::available_parallelism()
            .map(|n| n.get())
            .unwrap_or(2);
        Limit {
            available: Mutex::new(cores.saturating_sub(1).clamp(1, 4)),
            freed: Condvar::new(),
        }
    })
}

/// Entry point registered with the Tauri builder.
///
/// The work happens on a worker thread: decoding a HEIC takes hundreds of
/// milliseconds and must not block the UI thread or other image requests.
pub fn handle(
    ctx: UriSchemeContext<'_, Wry>,
    request: Request<Vec<u8>>,
    responder: UriSchemeResponder,
) {
    let app = ctx.app_handle().clone();
    std::thread::spawn(move || {
        responder.respond(serve(&app, &request));
    });
}

fn serve(app: &tauri::AppHandle, request: &Request<Vec<u8>>) -> Response<Vec<u8>> {
    let path = request.uri().path().trim_start_matches('/').to_string();
    let Some((kind, file_id)) = path.split_once('/') else {
        return error(StatusCode::BAD_REQUEST, "malformed image request");
    };

    // A file id is a SHA-1 hex digest. Rejecting anything else keeps path
    // separators and traversal sequences out of the cache filename below.
    if file_id.len() != 40 || !file_id.chars().all(|c| c.is_ascii_hexdigit()) {
        return error(StatusCode::BAD_REQUEST, "malformed item id");
    }

    let state = app.state::<AppState>();
    let (backup, cache_root) = {
        let guard = state.backup.lock().unwrap();
        match guard.as_ref() {
            // Clone the Arc and release the lock immediately: decoding is slow
            // and several images are usually in flight at once.
            Some(b) => (Arc::clone(b), state.cache_root(app, b)),
            None => return error(StatusCode::NOT_FOUND, "no backup is open"),
        }
    };

    match kind {
        "thumb" => serve_thumb(&backup, &cache_root, file_id, state.cache_limit()),
        "full" => serve_full(&backup, file_id),
        _ => error(StatusCode::NOT_FOUND, "unknown image request"),
    }
}

fn serve_thumb(
    backup: &Backup,
    cache_root: &std::path::Path,
    file_id: &str,
    limit: u64,
) -> Response<Vec<u8>> {
    if let Some(jpeg) = thumbs::cached(cache_root, file_id, thumbs::THUMB_EDGE) {
        return image_response(jpeg, "image/jpeg");
    }

    // Past this point there is real decoding to do, so wait for a slot. The
    // permit is released when it falls out of scope at the end of the call.
    let _permit = decode_limit().acquire();

    // Another request may have produced it while this one waited.
    if let Some(jpeg) = thumbs::cached(cache_root, file_id, thumbs::THUMB_EDGE) {
        return image_response(jpeg, "image/jpeg");
    }

    let Some(photo) = backup.photo_by_id(file_id) else {
        return error(StatusCode::NOT_FOUND, "that photo is not in this backup");
    };

    // A derivative iOS already rendered costs a file read instead of an HEVC
    // decode; when one exists it is worth roughly two orders of magnitude.
    let from_derivative = photo
        .derivative_id
        .as_deref()
        .and_then(|id| backup.read_by_id(id).ok())
        .and_then(|bytes| thumbs::make_thumbnail(&bytes, "jpg").ok());

    let jpeg = match from_derivative {
        Some(jpeg) => jpeg,
        None => {
            let bytes = match backup.read_by_id(file_id) {
                Ok(b) => b,
                Err(e) => return error(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()),
            };
            match thumbs::make_thumbnail(&bytes, &photo.ext) {
                Ok(jpeg) => jpeg,
                Err(e) => return error(StatusCode::UNSUPPORTED_MEDIA_TYPE, &e.to_string()),
            }
        }
    };

    thumbs::store(cache_root, file_id, thumbs::THUMB_EDGE, &jpeg);
    thumbs::evict_to_fit(cache_root, limit);
    image_response(jpeg, "image/jpeg")
}

fn serve_full(backup: &Backup, file_id: &str) -> Response<Vec<u8>> {
    let Some(photo) = backup.photo_by_id(file_id) else {
        return error(StatusCode::NOT_FOUND, "that photo is not in this backup");
    };
    let _permit = decode_limit().acquire();
    let bytes = match backup.read_by_id(file_id) {
        Ok(b) => b,
        Err(e) => return error(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()),
    };

    // JPEG and PNG the webview renders itself; only HEIC needs converting.
    if !photo.needs_transcode() {
        return image_response(bytes, photo.mime());
    }
    match thumbs::transcode_full(&bytes, &photo.ext) {
        Ok(jpeg) => image_response(jpeg, "image/jpeg"),
        Err(e) => error(StatusCode::UNSUPPORTED_MEDIA_TYPE, &e.to_string()),
    }
}

fn image_response(body: Vec<u8>, mime: &str) -> Response<Vec<u8>> {
    Response::builder()
        .status(StatusCode::OK)
        .header("Content-Type", mime)
        // Same bytes for the life of the window; the id is content-addressed.
        .header("Cache-Control", "private, max-age=86400")
        .header("Access-Control-Allow-Origin", "*")
        .body(body)
        .unwrap_or_else(|_| Response::new(Vec::new()))
}

fn error(status: StatusCode, message: &str) -> Response<Vec<u8>> {
    Response::builder()
        .status(status)
        .header("Content-Type", "text/plain; charset=utf-8")
        .header("Access-Control-Allow-Origin", "*")
        .body(message.as_bytes().to_vec())
        .unwrap_or_else(|_| Response::new(Vec::new()))
}
