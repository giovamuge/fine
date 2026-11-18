//! Request middleware: password, language, security headers.

use std::sync::Arc;

use axum::extract::{Request, State};
use axum::http::{HeaderMap, HeaderValue, StatusCode, header};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use base64::Engine;

use crate::i18n::Lang;

const CSP: &str = "default-src 'self'; script-src 'self'; style-src 'self' 'unsafe-inline'; \
img-src 'self' data:; font-src 'self'; connect-src 'self'; object-src 'none'; base-uri 'self'; \
form-action 'self'; frame-ancestors 'none'";

/// Constant-time comparison to avoid leaking the password through timing.
fn same(a: &[u8], b: &[u8]) -> bool {
    a.len() == b.len() && a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

/// HTTP Basic with any user name and the dashboard password.
fn authorized(headers: &HeaderMap, password: &str) -> bool {
    let Some(encoded) = headers
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Basic "))
    else {
        return false;
    };
    let Ok(decoded) = base64::engine::general_purpose::STANDARD.decode(encoded.trim()) else {
        return false;
    };
    let given = decoded.splitn(2, |b| *b == b':').nth(1).unwrap_or_default();
    same(given, password.as_bytes())
}

/// A configured password is always enforced, on localhost too: a tunnel (cloudflared, ngrok,
/// tailscale serve...) connects from 127.0.0.1, so "loopback" does not mean "only me".
pub(super) async fn require_password(
    State(password): State<Option<Arc<str>>>,
    req: Request,
    next: Next,
) -> Response {
    if let Some(pw) = &password
        && !authorized(req.headers(), pw)
    {
        return (
            StatusCode::UNAUTHORIZED,
            [(header::WWW_AUTHENTICATE, "Basic realm=\"fine\"")],
            "Autenticazione richiesta",
        )
            .into_response();
    }
    next.run(req).await
}

/// The dashboard's language for the text the server writes (alerts, budget descriptions).
pub(super) async fn language(req: Request, next: Next) -> Response {
    let lang = req
        .headers()
        .get("x-fine-lang")
        .and_then(|v| v.to_str().ok())
        .map(Lang::parse)
        .unwrap_or_default();
    lang.scope(next.run(req)).await
}

pub(super) async fn security_headers(req: Request, next: Next) -> Response {
    let mut resp = next.run(req).await;
    let h = resp.headers_mut();
    h.insert(
        header::X_CONTENT_TYPE_OPTIONS,
        HeaderValue::from_static("nosniff"),
    );
    h.insert(header::X_FRAME_OPTIONS, HeaderValue::from_static("DENY"));
    h.insert(
        header::REFERRER_POLICY,
        HeaderValue::from_static("no-referrer"),
    );
    // Hashed assets set their own cache policy; everything else (API, index) is never stored.
    h.entry(header::CACHE_CONTROL)
        .or_insert(HeaderValue::from_static("no-store"));
    h.insert(
        header::CONTENT_SECURITY_POLICY,
        HeaderValue::from_static(CSP),
    );
    resp
}
