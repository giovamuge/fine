//! Exchange rates against a mocked Frankfurter API.

use fine::rates::Client;
use fine::services::rates;
use fine::storage::Store;
use serde_json::json;
use wiremock::matchers::{method, path, query_param};
use wiremock::{Mock, MockServer, ResponseTemplate};

fn foreign(store: &Store, id: i64, date: &str, cents: i64, currency: &str) {
    store
        .conn()
        .execute_batch("INSERT OR IGNORE INTO accounts (uid, currency) VALUES ('a', 'EUR')")
        .unwrap();
    store
        .conn()
        .execute(
            "INSERT INTO transactions (id, account_uid, booking_date, amount_cents, currency, merchant, dedup_key)
             VALUES (?1, 'a', ?2, ?3, ?4, 'x', ?1)",
            rusqlite::params![id, date, cents, currency],
        )
        .unwrap();
}

fn base_cents(store: &Store) -> Vec<Option<i64>> {
    let found = store.db().search(&Default::default()).unwrap();
    let mut by_id: Vec<_> = found
        .into_iter()
        .map(|t| (t.id, t.amount_eur.map(|m| m.cents())))
        .collect();
    by_id.sort();
    by_id.into_iter().map(|(_, c)| c).collect()
}

#[tokio::test]
async fn fetches_missing_rates_once_and_remembers_unknown_currencies() {
    let server = MockServer::start().await;
    // From a week before the first date (2026-10-04, a Sunday) to the last one.
    Mock::given(method("GET"))
        .and(path("/2026-09-27..2026-10-05"))
        .and(query_param("base", "EUR"))
        .and(query_param("symbols", "GBP,USD,ZZZ"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "amount": 1.0, "base": "EUR", "start_date": "2026-09-26", "end_date": "2026-10-05",
            "rates": {
                "2026-10-02": {"GBP": 0.85, "USD": 1.25},
                "2026-10-05": {"GBP": 0.80, "USD": 1.20}
            }
        })))
        .expect(1)
        .mount(&server)
        .await;
    let client = Client::with_base_url(&server.uri());
    let mut store = Store::open_in_memory().unwrap();
    foreign(&store, 1, "2026-10-04", -8500, "GBP"); // Sunday: Friday's rate
    foreign(&store, 2, "2026-10-05", -1200, "USD");
    foreign(&store, 3, "2026-10-05", -1000, "ZZZ"); // not published by the ECB
    foreign(&store, 4, "2026-10-05", -1000, "XXX"); // "no currency": never asked

    assert_eq!(rates::update(&mut store, &client).await.unwrap(), 4);
    assert_eq!(
        base_cents(&store),
        vec![Some(-10000), Some(-1000), None, None]
    );
    // Nothing missing that the service can give: no second request.
    assert_eq!(rates::update(&mut store, &client).await.unwrap(), 0);
}

#[tokio::test]
async fn nothing_published_yet_is_not_an_error() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(404).set_body_json(json!({"message": "not found"})))
        .mount(&server)
        .await;
    let mut store = Store::open_in_memory().unwrap();
    foreign(&store, 1, "2026-10-05", -1200, "USD");
    let client = Client::with_base_url(&server.uri());
    assert_eq!(rates::update(&mut store, &client).await.unwrap(), 0);
    // USD was not marked as unknown: it is asked again next time.
    assert_eq!(store.db().missing_rates().unwrap().len(), 1);
}
