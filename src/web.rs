//! Personal dashboard: the built frontend (web/dist, embedded in the binary) plus a small JSON API.
//! Listens on 127.0.0.1 by default; any other address requires a password (HTTP Basic).

use anyhow::{Result, bail};
use axum::{
    Json, Router,
    extract::{Query, Request, State},
    http::{HeaderMap, HeaderValue, StatusCode, Uri, header},
    middleware::{self, Next},
    response::{IntoResponse, Response},
    routing::{get, post},
};
use base64::Engine;
use serde::{Deserialize, Serialize};
use std::net::SocketAddr;
use std::sync::Arc;

use crate::config::{Config, Paths};
use crate::{ai, analysis, assistant, categorize, connect, db, enablebanking, ops, sync, tags};

/// The frontend built by build.rs (React + Vite). Hashed files live under assets/.
#[derive(rust_embed::RustEmbed)]
#[folder = "web/dist/"]
struct Assets;

const CSP: &str = "default-src 'self'; script-src 'self'; style-src 'self' 'unsafe-inline'; \
img-src 'self' data:; font-src 'self'; connect-src 'self'; object-src 'none'; base-uri 'self'; \
form-action 'self'; frame-ancestors 'none'";

struct AppState {
    paths: Paths,
    config: Config,
    password: Option<String>,
}

type Shared = Arc<AppState>;

struct ApiError(anyhow::Error);

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({ "error": format!("{:#}", self.0) })),
        )
            .into_response()
    }
}

async fn with_db<T, F>(state: &Shared, f: F) -> Result<Json<T>, ApiError>
where
    T: Serialize + Send + 'static,
    F: FnOnce(&mut rusqlite::Connection) -> Result<T> + Send + 'static,
{
    let path = state.paths.db.clone();
    tokio::task::spawn_blocking(move || {
        let mut conn = db::open(&path)?;
        f(&mut conn)
    })
    .await
    .map_err(|e| ApiError(e.into()))?
    .map(Json)
    .map_err(ApiError)
}

/// For handlers that call the bank (async): the connection lives inside the future.
async fn with_db_async<T, F, Fut>(state: &Shared, f: F) -> Result<Json<T>, ApiError>
where
    T: Serialize,
    F: FnOnce(rusqlite::Connection, Config) -> Fut,
    Fut: std::future::Future<Output = Result<T>>,
{
    let conn = db::open(&state.paths.db).map_err(ApiError)?;
    f(conn, state.config.clone())
        .await
        .map(Json)
        .map_err(ApiError)
}

#[derive(Deserialize)]
struct BanksQuery {
    country: Option<String>,
}

#[derive(Deserialize)]
struct ConnectBody {
    bank: String,
    country: Option<String>,
    days: Option<u64>,
}

#[derive(Deserialize)]
struct FinishBody {
    url: String,
}

#[derive(Deserialize)]
struct AccountBody {
    uid: String,
    label: Option<String>,
}

#[derive(Serialize)]
struct SyncOutcome {
    sync: sync::SyncReport,
    ai: Option<ai::AiOutcome>,
    ai_error: Option<String>,
}

fn today() -> chrono::NaiveDate {
    chrono::Local::now().date_naive()
}

#[derive(Deserialize)]
struct MonthQuery {
    month: Option<String>,
}

#[derive(Deserialize)]
struct CategorizeBody {
    transaction_ids: Vec<i64>,
    category: String,
    #[serde(default)]
    learn_rule: bool,
    pattern: Option<String>,
}

#[derive(Deserialize)]
struct TagBody {
    transaction_ids: Vec<i64>,
    tags: Vec<String>,
    #[serde(default)]
    learn_rule: bool,
    pattern: Option<String>,
}

#[derive(Deserialize)]
struct PeriodQuery {
    from: Option<String>,
    to: Option<String>,
}

#[derive(Deserialize)]
struct DaysQuery {
    days: Option<u64>,
}

#[derive(Deserialize)]
struct AmortizeBody {
    transaction_id: i64,
    months: i64,
}

pub fn router(paths: Paths, config: Config, password: Option<String>) -> Router {
    let state: Shared = Arc::new(AppState {
        paths,
        config,
        password,
    });
    Router::new()
        .route("/api/status", get(|State(s): State<Shared>| async move { with_db(&s, |c| assistant::status(c, today())).await }))
        .route(
            "/api/summary",
            get(|State(s): State<Shared>, Query(q): Query<MonthQuery>| async move {
                with_db(&s, move |c| {
                    let day = match q.month {
                        Some(m) => chrono::NaiveDate::parse_from_str(&format!("{m}-01"), "%Y-%m-%d")?,
                        None => today(),
                    };
                    analysis::monthly_summary(c, day)
                })
                .await
            }),
        )
        .route("/api/trend", get(|State(s): State<Shared>| async move { with_db(&s, |c| analysis::trend(c, today(), 12)).await }))
        .route(
            "/api/transactions",
            get(|State(s): State<Shared>, Query(q): Query<ops::SearchParams>| async move { with_db(&s, move |c| ops::search(c, &q)).await }),
        )
        .route("/api/categories", get(|State(s): State<Shared>| async move { with_db(&s, |c| ops::list_categories(c)).await }))
        .route("/api/recurring", get(|State(s): State<Shared>| async move { with_db(&s, |c| analysis::recurring(c, today())).await }))
        .route(
            "/api/sinking",
            get(|State(s): State<Shared>| async move {
                with_db(&s, |c| Ok(analysis::sinking_funds(&analysis::load_planned(c)?, today()))).await
            }),
        )
        .route(
            "/api/categorize",
            post(|State(s): State<Shared>, Json(b): Json<CategorizeBody>| async move {
                with_db(&s, move |c| {
                    categorize::categorize(c, &b.transaction_ids, &b.category, b.learn_rule, b.pattern.as_deref())
                })
                .await
            }),
        )
        .route(
            "/api/balance",
            get(|State(s): State<Shared>, Query(q): Query<DaysQuery>| async move {
                with_db(&s, move |c| analysis::balance_history(c, today(), q.days.unwrap_or(365).clamp(7, 3650))).await
            }),
        )
        .route(
            "/api/tags",
            get(|State(s): State<Shared>, Query(q): Query<PeriodQuery>| async move {
                with_db(&s, move |c| tags::list_tags(c, q.from.as_deref(), q.to.as_deref())).await
            }),
        )
        .route(
            "/api/tag",
            post(|State(s): State<Shared>, Json(b): Json<TagBody>| async move {
                with_db(&s, move |c| tags::tag(c, &b.transaction_ids, &b.tags, b.learn_rule, b.pattern.as_deref())).await
            }),
        )
        .route(
            "/api/untag",
            post(|State(s): State<Shared>, Json(b): Json<TagBody>| async move {
                with_db(&s, move |c| tags::untag(c, &b.transaction_ids, &b.tags)).await
            }),
        )
        .route("/api/accounts", get(|State(s): State<Shared>| async move { with_db(&s, |c| ops::list_accounts(c)).await }))
        .route(
            "/api/accounts/rename",
            post(|State(s): State<Shared>, Json(b): Json<AccountBody>| async move {
                with_db(&s, move |c| connect::rename(c, &b.uid, b.label.as_deref()).map(|_| "ok")).await
            }),
        )
        .route(
            "/api/accounts/remove",
            post(|State(s): State<Shared>, Json(b): Json<AccountBody>| async move {
                with_db_async(&s, |mut c, cfg| async move { connect::remove(&mut c, cfg.enable_banking.as_ref(), &b.uid).await }).await
            }),
        )
        .route(
            "/api/banks",
            get(|State(s): State<Shared>, Query(q): Query<BanksQuery>| async move {
                with_db_async(&s, |_, cfg| async move {
                    let country = q.country.unwrap_or_else(|| "IT".into()).to_uppercase();
                    let banks = enablebanking::Client::new(cfg.enable_banking()?)?.aspsps(&country).await?;
                    Ok(banks.into_iter().map(|b| b.name).collect::<Vec<_>>())
                })
                .await
            }),
        )
        .route(
            "/api/connect",
            post(|State(s): State<Shared>, Json(b): Json<ConnectBody>| async move {
                with_db_async(&s, |mut c, cfg| async move {
                    let country = b.country.unwrap_or_else(|| "IT".into()).to_uppercase();
                    connect::start(&mut c, cfg.enable_banking()?, &b.bank, &country, b.days.unwrap_or(180)).await
                })
                .await
            }),
        )
        .route(
            "/api/connect/finish",
            post(|State(s): State<Shared>, Json(b): Json<FinishBody>| async move {
                with_db_async(&s, |mut c, cfg| async move { connect::finish(&mut c, cfg.enable_banking()?, &b.url).await }).await
            }),
        )
        .route(
            "/api/sync",
            post(|State(s): State<Shared>| async move {
                with_db_async(&s, |mut c, cfg| async move {
                    let client = enablebanking::Client::new(cfg.enable_banking()?)?;
                    let report = sync::sync_all(&mut c, &client).await?;
                    let (ai, ai_error) = match ai::detect() {
                        Some(b) => match ai::categorize_pending(&mut c, &b).await {
                            Ok(o) => (o, None),
                            Err(e) => (None, Some(format!("{e:#}"))),
                        },
                        None => (None, None),
                    };
                    Ok(SyncOutcome { sync: report, ai, ai_error })
                })
                .await
            }),
        )
        .route(
            "/api/amortize",
            post(|State(s): State<Shared>, Json(b): Json<AmortizeBody>| async move {
                with_db(&s, move |c| ops::amortize(c, b.transaction_id, b.months).map(|_| "ok")).await
            }),
        )
        .fallback(static_file)
        .layer(middleware::from_fn_with_state(state.clone(), auth))
        .with_state(state)
}

/// Content type by extension (only what Vite emits, plus a safe default).
fn mime_for(path: &str) -> &'static str {
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
async fn static_file(uri: Uri) -> Response {
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

/// Constant-time comparison to avoid leaking the password through timing.
fn same(a: &[u8], b: &[u8]) -> bool {
    a.len() == b.len() && a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

fn authorized(headers: &HeaderMap, password: &str) -> bool {
    let Some(value) = headers
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
    else {
        return false;
    };
    let Some(encoded) = value.strip_prefix("Basic ") else {
        return false;
    };
    let Ok(decoded) = base64::engine::general_purpose::STANDARD.decode(encoded.trim()) else {
        return false;
    };
    let given = decoded.splitn(2, |b| *b == b':').nth(1).unwrap_or_default();
    same(given, password.as_bytes())
}

async fn auth(State(state): State<Shared>, req: Request, next: Next) -> Response {
    let headers = req.headers().clone();
    if let Some(pw) = &state.password
        && !authorized(&headers, pw)
    {
        return (
            StatusCode::UNAUTHORIZED,
            [(header::WWW_AUTHENTICATE, "Basic realm=\"fine\"")],
            "Autenticazione richiesta",
        )
            .into_response();
    }
    let mut resp = next.run(req).await;
    let h = resp.headers_mut();
    h.insert(
        header::X_CONTENT_TYPE_OPTIONS,
        "nosniff".parse().expect("static"),
    );
    h.insert(header::X_FRAME_OPTIONS, "DENY".parse().expect("static"));
    h.insert(
        header::REFERRER_POLICY,
        "no-referrer".parse().expect("static"),
    );
    // Hashed assets set their own cache policy; everything else (API, index) is never stored.
    h.entry(header::CACHE_CONTROL)
        .or_insert_with(|| "no-store".parse().expect("static"));
    h.insert(
        header::CONTENT_SECURITY_POLICY,
        CSP.parse().expect("static"),
    );
    resp
}

pub async fn serve(
    paths: Paths,
    config: crate::config::Config,
    addr: SocketAddr,
    password: Option<String>,
) -> Result<()> {
    if !addr.ip().is_loopback() && password.as_deref().is_none_or(|p| p.len() < 12) {
        bail!(
            "per ascoltare su {} serve una password di almeno 12 caratteri: imposta [dashboard] password in config.toml",
            addr.ip()
        );
    }
    // A configured password is always enforced, on localhost too: a tunnel (cloudflared, ngrok,
    // tailscale serve...) connects from 127.0.0.1, so "loopback" does not mean "only me".
    // Keep data fresh while the dashboard runs (in addition to the scheduled syncs).
    let db_path = paths.db.clone();
    let bg_config = config.clone();
    tokio::spawn(async move {
        loop {
            if let Ok(mut conn) = db::open(&db_path)
                && let Some(msg) =
                    crate::sync::sync_if_stale(&mut conn, bg_config.enable_banking.as_ref()).await
            {
                eprintln!("({msg})");
            }
            tokio::time::sleep(std::time::Duration::from_secs(1800)).await;
        }
    });
    let listener = tokio::net::TcpListener::bind(addr).await?;
    eprintln!("Dashboard: http://{addr}");
    axum::serve(listener, router(paths, config, password)).await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use http_body_util::BodyExt;
    use tower::ServiceExt;

    fn app(password: Option<&str>) -> Router {
        let dir = std::env::temp_dir().join(format!("fine-web-test-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        router(
            Paths::in_dir(dir),
            Config::default(),
            password.map(String::from),
        )
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
