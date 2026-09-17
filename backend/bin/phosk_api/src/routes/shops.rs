//! Shops.
use axum::Router;

pub fn shops() -> Router {
    Router::new().route(
        "/shops",
        ni!(get, "ledger: list shops (name/txn_count/total)"),
    )
}
