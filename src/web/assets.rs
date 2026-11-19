//! The frontend built by build.rs (React + Vite), embedded in the binary.

use axum::Json;
use axum::http::{HeaderValue, StatusCode, Uri, header};
use axum::response::{IntoResponse, Response};

#[derive(rust_embed::RustEmbed)]
#[folder = "web/dist/"]
struct Assets;

/// Content type by extension (only what Vite emits, plus a safe default).
pub(super) fn mime_for(path: &str) -> &'static str {
    match path.rsplit_once('.').map(|(_, ext)| ext) {
        Some("html") => "text/html; charset=utf-8",
        Some("js" | "mjs") => "text/javascript; charset=utf-8",
        Some("css") => "text/css; charset=utf-8",
        Some("json" | "map" | "webmanifest") => "application/json; charset=utf-8",
        Some("svg") => "image/svg+xml",
        Some("png") => "image/png",
        Some("ico") => "image/x-icon",
        Some("woff2") => "font/woff2",
        Some("woff") => "font/woff",
        Some("txt") => "text/plain; charset=utf-8",
        _ => "application/octet-stream",
    }
}

/// Serves the embedded frontend. Unknown non-API paths get index.html (the app routes
/// client-side); unknown /api paths and missing assets are 404.
pub(super) async fn static_file(uri: Uri) -> Response {
    let path = uri.path().trim_start_matches('/');
    if path.starts_with("api/") || path == "api" {
        return (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({ "error": "endpoint inesistente" })),
        )
            .into_response();
    }
    let (name, file) = match Assets::get(path) {
        Some(f) if !path.is_empty() && !path.ends_with('/') => (path, f),
        _ => {
            // Files that are clearly assets must not fall back to the app page.
            if path.starts_with("assets/")
                || path.rsplit('/').next().is_some_and(|n| n.contains('.'))
            {
                return (StatusCode::NOT_FOUND, "non trovato").into_response();
            }
            match Assets::get("index.html") {
                Some(f) => ("index.html", f),
                None => return (StatusCode::NOT_FOUND, "dashboard non compilata").into_response(),
            }
        }
    };
    // Vite names bundled files with a content hash: they can be cached forever.
    let cache = if name.starts_with("assets/") {
        "public, max-age=31536000, immutable"
    } else {
        "no-store"
    };
    (
        [
            (
                header::CONTENT_TYPE,
                HeaderValue::from_static(mime_for(name)),
            ),
            (header::CACHE_CONTROL, HeaderValue::from_static(cache)),
        ],
        file.data.into_owned(),
    )
        .into_response()
}
