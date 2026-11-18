//! Personal dashboard: the built frontend (web/dist, embedded in the binary) plus a JSON API.
//! Listens on 127.0.0.1 by default; any other address requires a password (HTTP Basic).

mod api;
mod assets;
mod middleware;

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use axum::Router;

use crate::App;
use crate::error::{Error, Result};
use crate::services::sync;

/// How often the running dashboard checks whether the data needs a refresh.
const REFRESH_EVERY: Duration = Duration::from_secs(1800);
const MIN_PASSWORD_LEN: usize = 12;

pub fn router(app: App, password: Option<String>) -> Router {
    let password: Option<Arc<str>> = password.map(Into::into);
    api::routes()
        .fallback(assets::static_file)
        .with_state(app)
        .layer(axum::middleware::from_fn(middleware::language))
        .layer(axum::middleware::from_fn_with_state(
            password,
            middleware::require_password,
        ))
        .layer(axum::middleware::from_fn(middleware::security_headers))
}

pub async fn serve(app: App, addr: SocketAddr) -> Result<()> {
    let password = app.config.dashboard_password().map(str::to_owned);
    if !addr.ip().is_loopback()
        && password
            .as_deref()
            .is_none_or(|p| p.len() < MIN_PASSWORD_LEN)
    {
        crate::invalid!(
            "per ascoltare su {} serve una password di almeno {MIN_PASSWORD_LEN} caratteri: imposta [dashboard] password in config.toml",
            addr.ip()
        );
    }
    // Keep data fresh while the dashboard runs (in addition to the scheduled syncs).
    let bg = app.clone();
    tokio::spawn(async move {
        loop {
            if let Ok(mut store) = bg.store()
                && let Some(msg) =
                    sync::sync_if_stale(&mut store, bg.config.enable_banking.as_ref()).await
            {
                eprintln!("({msg})");
            }
            tokio::time::sleep(REFRESH_EVERY).await;
        }
    });
    let listener = tokio::net::TcpListener::bind(addr).await?;
    eprintln!("Dashboard: http://{addr}");
    axum::serve(listener, router(app, password))
        .await
        .map_err(Error::from)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;
    use crate::storage::Database;
    use axum::body::Body;
    use axum::http::{HeaderMap, Request, StatusCode, header};
    use base64::Engine;
    use http_body_util::BodyExt;
    use tower::ServiceExt;

    fn app(password: Option<&str>) -> Router {
        let dir = std::env::temp_dir().join(format!("fine-web-test-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let db = Database::open(&dir.join("fine.db")).unwrap();
        router(App::new(db, Config::default()), password.map(String::from))
    }

    async fn get(app: Router, path: &str) -> (StatusCode, HeaderMap, String) {
        let resp = app
            .oneshot(Request::builder().uri(path).body(Body::empty()).unwrap())
            .await
            .unwrap();
        let (parts, body) = resp.into_parts();
        let bytes = body.collect().await.unwrap().to_bytes();
        (
            parts.status,
            parts.headers,
            String::from_utf8_lossy(&bytes).into_owned(),
        )
    }

    fn header(h: &HeaderMap, name: header::HeaderName) -> &str {
        h.get(name).and_then(|v| v.to_str().ok()).unwrap_or("")
    }

    #[test]
    fn mime_types() {
        use assets::mime_for;
        assert_eq!(mime_for("index.html"), "text/html; charset=utf-8");
        assert_eq!(
            mime_for("assets/app-1a2b.js"),
            "text/javascript; charset=utf-8"
        );
        assert_eq!(mime_for("assets/app-1a2b.css"), "text/css; charset=utf-8");
        assert_eq!(mime_for("favicon.svg"), "image/svg+xml");
        assert_eq!(mime_for("noext"), "application/octet-stream");
    }

    #[tokio::test]
    async fn serves_index_and_spa_fallback() {
        let (status, headers, body) = get(app(None), "/").await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(
            header(&headers, header::CONTENT_TYPE),
            "text/html; charset=utf-8"
        );
        assert!(body.contains("<html"), "index.html expected");
        assert_eq!(header(&headers, header::CACHE_CONTROL), "no-store");

        // Client-side routes get the app page too.
        let (status, headers, body2) = get(app(None), "/movimenti?tag=casa").await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(
            header(&headers, header::CONTENT_TYPE),
            "text/html; charset=utf-8"
        );
        assert_eq!(body, body2);
    }

    #[tokio::test]
    async fn missing_assets_and_api_paths_are_404() {
        let (status, _, _) = get(app(None), "/assets/missing-abc123.js").await;
        assert_eq!(status, StatusCode::NOT_FOUND);
        let (status, _, _) = get(app(None), "/qualcosa.png").await;
        assert_eq!(status, StatusCode::NOT_FOUND);
        let (status, headers, body) = get(app(None), "/api/inesistente").await;
        assert_eq!(status, StatusCode::NOT_FOUND);
        assert!(header(&headers, header::CONTENT_TYPE).starts_with("application/json"));
        assert!(body.contains("error"));
    }

    #[tokio::test]
    async fn security_headers_on_every_response() {
        for path in ["/", "/api/status", "/api/inesistente"] {
            let (_, headers, _) = get(app(None), path).await;
            let csp = header(&headers, header::CONTENT_SECURITY_POLICY);
            assert!(csp.contains("script-src 'self';"), "{path}: {csp}");
            assert!(!csp.contains("script-src 'self' 'unsafe-inline'"), "{path}");
            assert!(csp.contains("frame-ancestors 'none'"), "{path}");
            assert_eq!(header(&headers, header::X_FRAME_OPTIONS), "DENY", "{path}");
            assert_eq!(
                header(&headers, header::X_CONTENT_TYPE_OPTIONS),
                "nosniff",
                "{path}"
            );
            assert_eq!(
                header(&headers, header::REFERRER_POLICY),
                "no-referrer",
                "{path}"
            );
        }
    }

    #[tokio::test]
    async fn password_protects_static_files_too() {
        let (status, headers, _) = get(app(Some("segretissima-123")), "/").await;
        assert_eq!(status, StatusCode::UNAUTHORIZED);
        assert!(header(&headers, header::WWW_AUTHENTICATE).starts_with("Basic"));

        let token = base64::engine::general_purpose::STANDARD.encode("fine:segretissima-123");
        let resp = app(Some("segretissima-123"))
            .oneshot(
                Request::builder()
                    .uri("/")
                    .header(header::AUTHORIZATION, format!("Basic {token}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
    }
}
