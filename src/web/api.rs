//! The JSON API used by the dashboard.

use axum::extract::{Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::{Deserialize, Serialize};

use crate::App;
use crate::domain::budget::Budget;
use crate::domain::calendar::parse_month;
use crate::error::Error;
use crate::services::ai::{self, AiOutcome};
use crate::services::sync::{self, SyncReport};
use crate::services::{budgets, categories, connect, planning, reports, tags};
use crate::storage::{CategoryPatch, SearchParams};

/// Any failure becomes `400 {"error": "..."}` with the message for the user.
pub struct ApiError(Error);

impl From<Error> for ApiError {
    fn from(e: Error) -> Self {
        Self(e)
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({ "error": self.0.chain() })),
        )
            .into_response()
    }
}

type ApiResult<T> = Result<Json<T>, ApiError>;

const OK: &str = "ok";

pub fn routes() -> Router<App> {
    Router::new()
        .route("/api/status", get(status))
        .route("/api/summary", get(summary))
        .route("/api/trend", get(trend))
        .route("/api/transactions", get(transactions))
        .route(
            "/api/categories",
            get(list_categories).post(create_category),
        )
        .route("/api/categories/update", post(update_category))
        .route("/api/categories/delete", post(delete_category))
        .route("/api/rules", get(list_rules))
        .route("/api/rules/delete", post(delete_rule))
        .route("/api/categorize", post(categorize))
        .route("/api/budgets", get(budget_status).post(save_budget))
        .route("/api/budgets/preview", post(preview_budget))
        .route("/api/budgets/delete", post(delete_budget))
        .route("/api/recurring", get(recurring))
        .route("/api/sinking", get(sinking_funds))
        .route("/api/balance", get(balance))
        .route("/api/tags", get(list_tags))
        .route("/api/tag", post(tag))
        .route("/api/untag", post(untag))
        .route("/api/amortize", post(amortize))
        .route("/api/accounts", get(accounts))
        .route("/api/accounts/rename", post(rename_account))
        .route("/api/accounts/remove", post(remove_account))
        .route("/api/banks", get(banks))
        .route("/api/connect", post(start_connect))
        .route("/api/connect/finish", post(finish_connect))
        .route("/api/sync", post(sync_now))
}

#[derive(Deserialize)]
struct MonthQuery {
    month: Option<String>,
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
struct NameQuery {
    name: Option<String>,
}

#[derive(Deserialize)]
struct BanksQuery {
    country: Option<String>,
}

#[derive(Deserialize)]
struct NameBody {
    name: String,
}

#[derive(Deserialize)]
struct IdBody {
    id: i64,
}

#[derive(Deserialize)]
struct NewCategoryBody {
    name: String,
    kind: String,
    #[serde(default)]
    icon: String,
    #[serde(default)]
    color: String,
}

#[derive(Deserialize)]
struct UpdateCategoryBody {
    name: String,
    #[serde(flatten)]
    patch: CategoryPatch,
}

#[derive(Deserialize)]
struct DeleteCategoryBody {
    name: String,
    move_to: Option<String>,
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
struct BudgetBody {
    budget: Budget,
    /// Current name, when renaming
    replaces: Option<String>,
}

#[derive(Deserialize)]
struct AmortizeBody {
    transaction_id: i64,
    months: i64,
}

#[derive(Deserialize)]
struct AccountBody {
    uid: String,
    label: Option<String>,
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

#[derive(Serialize)]
struct SyncOutcome {
    sync: SyncReport,
    ai: Option<AiOutcome>,
    ai_error: Option<String>,
}

fn country(c: Option<String>) -> String {
    c.unwrap_or_else(|| "IT".into()).to_uppercase()
}

// ---- Reports

async fn status(State(app): State<App>) -> ApiResult<impl Serialize> {
    Ok(Json(
        app.blocking(|s| reports::status(s, App::today())).await?,
    ))
}

async fn summary(State(app): State<App>, Query(q): Query<MonthQuery>) -> ApiResult<impl Serialize> {
    let summary = app
        .blocking(move |s| {
            let day = q.month.as_deref().map(parse_month).transpose()?;
            reports::monthly_summary(s, day.unwrap_or_else(App::today))
        })
        .await?;
    Ok(Json(summary))
}

async fn trend(State(app): State<App>) -> ApiResult<impl Serialize> {
    Ok(Json(
        app.blocking(|s| reports::trend(s, App::today(), 12))
            .await?,
    ))
}

async fn recurring(State(app): State<App>) -> ApiResult<impl Serialize> {
    Ok(Json(
        app.blocking(|s| reports::recurring(s, App::today()))
            .await?,
    ))
}

async fn sinking_funds(State(app): State<App>) -> ApiResult<impl Serialize> {
    Ok(Json(
        app.blocking(|s| reports::sinking_funds(s, App::today()))
            .await?,
    ))
}

async fn balance(State(app): State<App>, Query(q): Query<DaysQuery>) -> ApiResult<impl Serialize> {
    let days = q.days.unwrap_or(365);
    Ok(Json(
        app.blocking(move |s| reports::balance_history(s, App::today(), days))
            .await?,
    ))
}

// ---- Transactions, categories and rules

async fn transactions(
    State(app): State<App>,
    Query(q): Query<SearchParams>,
) -> ApiResult<impl Serialize> {
    Ok(Json(app.blocking(move |s| s.db().search(&q)).await?))
}

async fn list_categories(State(app): State<App>) -> ApiResult<impl Serialize> {
    Ok(Json(app.blocking(|s| categories::list(s)).await?))
}

async fn create_category(State(app): State<App>, Json(b): Json<NewCategoryBody>) -> ApiResult<()> {
    app.blocking(move |s| categories::create(s, &b.name, &b.kind, b.icon, b.color))
        .await?;
    Ok(Json(()))
}

async fn update_category(
    State(app): State<App>,
    Json(b): Json<UpdateCategoryBody>,
) -> ApiResult<()> {
    app.blocking(move |s| categories::update(s, &b.name, &b.patch))
        .await?;
    Ok(Json(()))
}

async fn delete_category(
    State(app): State<App>,
    Json(b): Json<DeleteCategoryBody>,
) -> ApiResult<impl Serialize> {
    Ok(Json(
        app.blocking(move |s| categories::delete(s, &b.name, b.move_to.as_deref()))
            .await?,
    ))
}

async fn list_rules(State(app): State<App>) -> ApiResult<impl Serialize> {
    Ok(Json(app.blocking(|s| categories::list_rules(s)).await?))
}

async fn delete_rule(State(app): State<App>, Json(b): Json<IdBody>) -> ApiResult<()> {
    app.blocking(move |s| categories::delete_rule(s, b.id))
        .await?;
    Ok(Json(()))
}

async fn categorize(
    State(app): State<App>,
    Json(b): Json<CategorizeBody>,
) -> ApiResult<impl Serialize> {
    Ok(Json(
        app.blocking(move |s| {
            categories::categorize(
                s,
                &b.transaction_ids,
                &b.category,
                b.learn_rule,
                b.pattern.as_deref(),
            )
        })
        .await?,
    ))
}

async fn amortize(State(app): State<App>, Json(b): Json<AmortizeBody>) -> ApiResult<&'static str> {
    app.blocking(move |s| planning::amortize(s, b.transaction_id, b.months))
        .await?;
    Ok(Json(OK))
}

// ---- Tags

async fn list_tags(
    State(app): State<App>,
    Query(q): Query<PeriodQuery>,
) -> ApiResult<impl Serialize> {
    Ok(Json(
        app.blocking(move |s| tags::list(s, q.from.as_deref(), q.to.as_deref()))
            .await?,
    ))
}

async fn tag(State(app): State<App>, Json(b): Json<TagBody>) -> ApiResult<impl Serialize> {
    Ok(Json(
        app.blocking(move |s| {
            tags::tag(
                s,
                &b.transaction_ids,
                &b.tags,
                b.learn_rule,
                b.pattern.as_deref(),
            )
        })
        .await?,
    ))
}

async fn untag(State(app): State<App>, Json(b): Json<TagBody>) -> ApiResult<usize> {
    Ok(Json(
        app.blocking(move |s| tags::untag(s, &b.transaction_ids, &b.tags))
            .await?,
    ))
}

// ---- Budgets

async fn budget_status(
    State(app): State<App>,
    Query(q): Query<NameQuery>,
) -> ApiResult<serde_json::Value> {
    let value = app
        .blocking(move |s| {
            Ok(match q.name {
                Some(n) => serde_json::to_value(budgets::status_of(s, &n, App::today())?)?,
                None => serde_json::to_value(budgets::statuses(s, App::today())?)?,
            })
        })
        .await?;
    Ok(Json(value))
}

async fn save_budget(
    State(app): State<App>,
    Json(b): Json<BudgetBody>,
) -> ApiResult<impl Serialize> {
    Ok(Json(
        app.blocking(move |s| budgets::upsert(s, b.budget, b.replaces.as_deref()))
            .await?,
    ))
}

async fn preview_budget(
    State(app): State<App>,
    Json(b): Json<BudgetBody>,
) -> ApiResult<impl Serialize> {
    Ok(Json(
        app.blocking(move |s| budgets::preview(s, b.budget, App::today()))
            .await?,
    ))
}

async fn delete_budget(State(app): State<App>, Json(b): Json<NameBody>) -> ApiResult<()> {
    app.blocking(move |s| budgets::delete(s, &b.name)).await?;
    Ok(Json(()))
}

// ---- Accounts and bank connections

async fn accounts(State(app): State<App>) -> ApiResult<impl Serialize> {
    Ok(Json(app.blocking(|s| reports::accounts(s)).await?))
}

async fn rename_account(
    State(app): State<App>,
    Json(b): Json<AccountBody>,
) -> ApiResult<&'static str> {
    app.blocking(move |s| connect::rename(s, &b.uid, b.label.as_deref()))
        .await?;
    Ok(Json(OK))
}

async fn remove_account(
    State(app): State<App>,
    Json(b): Json<AccountBody>,
) -> ApiResult<impl Serialize> {
    let mut store = app.store()?;
    Ok(Json(
        connect::remove(&mut store, app.config.enable_banking.as_ref(), &b.uid).await?,
    ))
}

async fn banks(State(app): State<App>, Query(q): Query<BanksQuery>) -> ApiResult<Vec<String>> {
    let banks = app
        .bank()?
        .aspsps(&country(q.country))
        .await
        .map_err(Error::from)?;
    Ok(Json(banks.into_iter().map(|b| b.name).collect()))
}

async fn start_connect(
    State(app): State<App>,
    Json(b): Json<ConnectBody>,
) -> ApiResult<impl Serialize> {
    let eb = app.config.enable_banking()?;
    let mut store = app.store()?;
    let pending = connect::start(
        &mut store,
        eb,
        &b.bank,
        &country(b.country),
        b.days.unwrap_or(180),
    )
    .await?;
    Ok(Json(pending))
}

async fn finish_connect(
    State(app): State<App>,
    Json(b): Json<FinishBody>,
) -> ApiResult<impl Serialize> {
    let eb = app.config.enable_banking()?;
    let mut store = app.store()?;
    Ok(Json(connect::finish(&mut store, eb, &b.url).await?))
}

async fn sync_now(State(app): State<App>) -> ApiResult<SyncOutcome> {
    let client = app.bank()?;
    let mut store = app.store()?;
    let report = sync::sync_all(&mut store, &client).await?;
    let (ai, ai_error) = match ai::Backend::detect() {
        Some(backend) => match ai::categorize_pending(&mut store, &backend).await {
            Ok(outcome) => (outcome, None),
            Err(e) => (None, Some(e.chain())),
        },
        None => (None, None),
    };
    Ok(Json(SyncOutcome {
        sync: report,
        ai,
        ai_error,
    }))
}
