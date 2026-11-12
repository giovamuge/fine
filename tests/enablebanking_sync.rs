//! End-to-end sync against a mocked Enable Banking API.
//! The key in tests/fixtures is a throwaway key generated only for these tests.

use fine::{db, enablebanking::Client, sync};
use jsonwebtoken::EncodingKey;
use serde_json::json;
use wiremock::matchers::{header_regex, method, path, query_param, query_param_is_missing};
use wiremock::{Mock, MockServer, ResponseTemplate};

fn client(base: &str) -> Client {
    let pem = std::fs::read("tests/fixtures/test_key.pem").unwrap();
    Client::with_key(base, "app-123", EncodingKey::from_rsa_pem(&pem).unwrap())
}

fn tx(reference: &str, amount: &str, ind: &str, name: &str) -> serde_json::Value {
    json!({
        "entry_reference": reference,
        "transaction_amount": {"amount": amount, "currency": "EUR"},
        "credit_debit_indicator": ind,
        "status": "BOOK",
        "booking_date": "2026-11-02",
        "creditor": {"name": name},
        "debtor": {"name": name}
    })
}

#[tokio::test]
async fn syncs_pages_dedups_and_categorizes() {
    let server = MockServer::start().await;
    let bearer = header_regex("authorization", r"^Bearer [\w-]+\.[\w-]+\.[\w-]+$");

    Mock::given(method("GET"))
        .and(path("/accounts/acc-1/transactions"))
        .and(query_param_is_missing("continuation_key"))
        .and(bearer)
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "transactions": [tx("r1", "45.20", "DBIT", "ESSELUNGA MILANO"), tx("r2", "2000.00", "CRDT", "ACME SPA")],
            "continuation_key": "k1"
        })))
        .mount(&server)
        .await;
    // Second page repeats the same continuation key: the client must stop instead of looping.
    Mock::given(method("GET"))
        .and(path("/accounts/acc-1/transactions"))
        .and(query_param("continuation_key", "k1"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "transactions": [tx("r3", "12.99", "DBIT", "NETFLIX.COM"), json!({"status": "PDNG", "transaction_amount": {"amount": "5", "currency": "EUR"}, "booking_date": "2026-11-03"})],
            "continuation_key": "k1"
        })))
        .expect(2) // once per sync: no infinite loop on the repeated key
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/accounts/acc-1/balances"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "balances": [{"balance_amount": {"amount": "1530.55", "currency": "EUR"}, "balance_type": "CLBD"}]
        })))
        .mount(&server)
        .await;

    let mut conn = db::open_in_memory().unwrap();
    conn.execute(
        "INSERT INTO accounts (uid, currency, name) VALUES ('acc-1', 'EUR', 'Conto')",
        [],
    )
    .unwrap();
    let c = client(&server.uri());

    let report = sync::sync_all(&mut conn, &c).await.unwrap();
    assert_eq!(report.accounts[0].error, None);
    assert_eq!(
        report.accounts[0].inserted, 3,
        "pending transaction must be skipped"
    );
    assert_eq!(
        report.auto_categorized, 2,
        "esselunga and netflix match seed rules"
    );

    let balance: i64 = conn
        .query_row("SELECT balance_cents FROM accounts", [], |r| r.get(0))
        .unwrap();
    assert_eq!(balance, 153055);
    let netflix_cents: i64 = conn
        .query_row(
            "SELECT amount_cents FROM transactions WHERE entry_reference = 'r3'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(netflix_cents, -1299);

    // Re-sync: overlap window returns the same data, nothing duplicated.
    let again = sync::sync_all(&mut conn, &c).await.unwrap();
    assert_eq!(again.accounts[0].inserted, 0);
}

#[tokio::test]
async fn respects_daily_sync_limit() {
    let server = MockServer::start().await;
    let mut conn = db::open_in_memory().unwrap();
    conn.execute(
        "INSERT INTO accounts (uid, currency) VALUES ('acc-1', 'EUR')",
        [],
    )
    .unwrap();
    let now = chrono::Utc::now().to_rfc3339();
    for _ in 0..sync::MAX_SYNCS_PER_DAY {
        conn.execute(
            "INSERT INTO sync_log (account_uid, at, ok) VALUES ('acc-1', ?1, 1)",
            [&now],
        )
        .unwrap();
    }
    let report = sync::sync_all(&mut conn, &client(&server.uri()))
        .await
        .unwrap();
    assert!(report.accounts[0].skipped.is_some());
    assert!(
        server.received_requests().await.unwrap().is_empty(),
        "no bank call when over the limit"
    );
}

#[tokio::test]
async fn network_failures_do_not_use_the_daily_limit() {
    let server = MockServer::start().await;
    let mut conn = db::open_in_memory().unwrap();
    conn.execute(
        "INSERT INTO accounts (uid, currency) VALUES ('acc-1', 'EUR')",
        [],
    )
    .unwrap();
    let now = chrono::Utc::now().to_rfc3339();
    // Three answers from the bank (they count) and one DNS failure at wake-up (it does not).
    for msg in [
        "Enable Banking 429 Too Many Requests: {}",
        "Enable Banking 422 Unprocessable Entity: {}",
        "Enable Banking 429 Too Many Requests: {}",
        "error sending request for url (...): dns error",
    ] {
        conn.execute(
            "INSERT INTO sync_log (account_uid, at, ok, message) VALUES ('acc-1', ?1, 0, ?2)",
            [&now, msg],
        )
        .unwrap();
    }
    let report = sync::sync_all(&mut conn, &client(&server.uri()))
        .await
        .unwrap();
    assert!(
        report.accounts[0].skipped.is_none(),
        "one real attempt is still allowed"
    );
    assert!(!server.received_requests().await.unwrap().is_empty());
}

#[tokio::test]
async fn reports_expired_consent_clearly() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/accounts/acc-1/transactions"))
        .respond_with(
            ResponseTemplate::new(401).set_body_string(r#"{"message":"Session expired"}"#),
        )
        .mount(&server)
        .await;
    let mut conn = db::open_in_memory().unwrap();
    conn.execute(
        "INSERT INTO accounts (uid, currency) VALUES ('acc-1', 'EUR')",
        [],
    )
    .unwrap();
    let report = sync::sync_all(&mut conn, &client(&server.uri()))
        .await
        .unwrap();
    let err = report.accounts[0].error.as_deref().unwrap();
    assert!(err.contains("fine connect"), "{err}");
}

#[test]
fn staleness_drives_automatic_sync() {
    let conn = db::open_in_memory().unwrap();
    assert!(
        !sync::is_stale(&conn).unwrap(),
        "no accounts: nothing to sync"
    );
    conn.execute(
        "INSERT INTO accounts (uid, currency) VALUES ('acc-1', 'EUR')",
        [],
    )
    .unwrap();
    assert!(sync::is_stale(&conn).unwrap(), "never synced");
    let old = (chrono::Utc::now() - chrono::Duration::hours(sync::STALE_HOURS + 1)).to_rfc3339();
    conn.execute(
        "INSERT INTO sync_log (account_uid, at, ok) VALUES ('acc-1', ?1, 1)",
        [&old],
    )
    .unwrap();
    assert!(sync::is_stale(&conn).unwrap(), "last sync older than 6h");
    let now = chrono::Utc::now().to_rfc3339();
    conn.execute(
        "INSERT INTO sync_log (account_uid, at, ok) VALUES ('acc-1', ?1, 0)",
        [&now],
    )
    .unwrap();
    assert!(
        sync::is_stale(&conn).unwrap(),
        "a failed sync does not count as fresh"
    );
    conn.execute(
        "INSERT INTO sync_log (account_uid, at, ok) VALUES ('acc-1', ?1, 1)",
        [&now],
    )
    .unwrap();
    assert!(!sync::is_stale(&conn).unwrap());
}

#[tokio::test]
async fn falls_back_to_shorter_history_when_bank_rejects_period() {
    let server = MockServer::start().await;
    let today = chrono::Utc::now().date_naive();
    let d30 = (today - chrono::Days::new(30))
        .format("%Y-%m-%d")
        .to_string();
    // A bank that only allows the last 30 days without the user present.
    Mock::given(method("GET"))
        .and(path("/accounts/acc-1/transactions"))
        .and(query_param("date_from", d30.as_str()))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "transactions": [tx("r1", "10.00", "DBIT", "ESSELUNGA")]
        })))
        .with_priority(1)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/accounts/acc-1/transactions"))
        .respond_with(ResponseTemplate::new(422).set_body_string(
            r#"{"code":422,"message":"Wrong transactions period requested","error":"WRONG_TRANSACTIONS_PERIOD"}"#,
        ))
        .with_priority(2)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/accounts/acc-1/balances"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"balances": []})))
        .mount(&server)
        .await;
    let mut conn = db::open_in_memory().unwrap();
    conn.execute(
        "INSERT INTO accounts (uid, currency) VALUES ('acc-1', 'EUR')",
        [],
    )
    .unwrap();
    let report = sync::sync_all(&mut conn, &client(&server.uri()))
        .await
        .unwrap();
    assert_eq!(report.accounts[0].error, None);
    assert_eq!(report.accounts[0].inserted, 1);
    let logged: i64 = conn
        .query_row("SELECT COUNT(*) FROM sync_log", [], |r| r.get(0))
        .unwrap();
    assert_eq!(logged, 1, "retries inside one sync count as one sync");
    let calls = server
        .received_requests()
        .await
        .unwrap()
        .iter()
        .filter(|r| r.url.path().ends_with("/transactions"))
        .count();
    assert_eq!(
        calls, 2,
        "initial 90 days, then one 30-day fallback: no quota wasted"
    );
}
