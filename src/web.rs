//! Personal dashboard: a single embedded page plus a small JSON API.
//! Listens on 127.0.0.1 by default; any other address requires a password (HTTP Basic).

use anyhow::{Result, bail};
use axum::{
    Json, Router,
    extract::{Query, Request, State},
    http::{HeaderMap, StatusCode, header},
    middleware::{self, Next},
    response::{Html, IntoResponse, Response},
    routing::{get, post},
};
use base64::Engine;
use serde::{Deserialize, Serialize};
use std::net::SocketAddr;
use std::sync::Arc;

use crate::config::Paths;
use crate::{analysis, assistant, categorize, db, ops};

const INDEX: &str = include_str!("web/index.html");

struct AppState {
    paths: Paths,
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
struct AmortizeBody {
    transaction_id: i64,
    months: i64,
}

pub fn router(paths: Paths, password: Option<String>) -> Router {
    let state: Shared = Arc::new(AppState { paths, password });
    Router::new()
        .route("/", get(|| async { Html(INDEX) }))
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
            "/api/amortize",
            post(|State(s): State<Shared>, Json(b): Json<AmortizeBody>| async move {
                with_db(&s, move |c| ops::amortize(c, b.transaction_id, b.months).map(|_| "ok")).await
            }),
        )
        .layer(middleware::from_fn_with_state(state.clone(), auth))
        .with_state(state)
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
    h.insert(header::CACHE_CONTROL, "no-store".parse().expect("static"));
    h.insert(
        header::CONTENT_SECURITY_POLICY,
        "default-src 'self'; script-src 'self' 'unsafe-inline'; style-src 'self' 'unsafe-inline'; img-src 'self' data:"
            .parse()
            .expect("static"),
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
    let password = if addr.ip().is_loopback() {
        None
    } else {
        password
    };
    // Keep data fresh while the dashboard runs (in addition to the scheduled syncs).
    let db_path = paths.db.clone();
    tokio::spawn(async move {
        loop {
            if let Ok(mut conn) = db::open(&db_path)
                && let Some(msg) =
                    crate::sync::sync_if_stale(&mut conn, config.enable_banking.as_ref()).await
            {
                eprintln!("({msg})");
            }
            tokio::time::sleep(std::time::Duration::from_secs(1800)).await;
        }
    });
    let listener = tokio::net::TcpListener::bind(addr).await?;
    eprintln!("Dashboard: http://{addr}");
    axum::serve(listener, router(paths, password)).await?;
    Ok(())
}
