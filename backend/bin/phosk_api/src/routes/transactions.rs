//! Transactions / line items.
use axum::Router;
use axum::routing::get;

use super::not_impl;

pub fn transactions() -> Router {
    Router::new()
        .route(
            "/transactions",
            get(|| async { not_impl("ledger: list transactions (filters/sort/search)") }).post(
                || async {
                    not_impl("ledger: create transaction (manual + photo-upload pipeline)")
                },
            ),
        )
        .route(
            "/transactions/{id}",
            get(|| async {
                not_impl("ledger: transaction detail (lines, OCR regions, confidence)")
            })
            .patch(|| async { not_impl("ledger: edit transaction") })
            .delete(|| async { not_impl("ledger: delete transaction") }),
        )
        .route(
            "/transactions/{id}/lines",
            ni!(get, "ledger: list line items for a receipt"),
        )
        .route(
            "/transactions/{id}/lines/{lineIndex}",
            ni!(
                patch,
                "ledger: review/edit a line item (category/signal/confirm)"
            ),
        )
        .route(
            "/transactions/{id}/reprocess",
            ni!(post, "ai: reprocess low-confidence lines (OCR+LLM re-read)"),
        )
}
