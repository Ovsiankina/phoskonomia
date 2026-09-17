//! Transactions read-model (F3).
//!
//! Backs the Transactions page (filterable dated list, accordion receipt lines,
//! full receipt screen with OCR reference) and the Dashboard "Recent" tape.
//! Mirrors React `GET /transactions` (with filter params), `/transactions/{id}`,
//! `/transactions/{id}/lines`.

use dioxus::prelude::*;
use phosk_core::money::Money;
use serde::{Deserialize, Serialize};

/// One receipt row in the list (`TransactionListDto::transactions` element).
///
/// `date` is the day label the page groups on; `item_count` / `low_conf_count`
/// drive the meta marks; `signal_ids` are the tracked item-signals this receipt
/// feeds; `fixed` marks a standing charge.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TransactionDto {
    /// Stable id.
    pub id: String,
    /// Day label, e.g. `"16 JUN"` (the list groups by this).
    pub date: String,
    /// Shop name.
    pub shop: String,
    /// Primary category.
    pub category: String,
    /// Receipt total.
    #[serde(with = "phosk_model::money_centimes")]
    pub amount: Money,
    /// Number of line items.
    pub item_count: u32,
    /// Count of low-confidence lines (drives the ⚠ mark).
    pub low_conf_count: u32,
    /// Tracked item-signal ids this receipt feeds (⌁ marks).
    pub signal_ids: Vec<String>,
    /// `true` for a fixed/standing charge.
    pub fixed: bool,
}

/// The list summary band (`TransactionListDto::summary`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TxnSummaryDto {
    /// Number of entries in the current filter.
    pub entry_count: u32,
    /// Total CHF across the filtered entries.
    #[serde(with = "phosk_model::money_centimes")]
    pub total_amount: Money,
    /// Human period label, e.g. `"JUN 2026"`.
    pub period_label: String,
}

/// `GET /transactions` — the filtered list + summary + filter option lists.
///
/// `transactions` is already filtered/sorted server-side per the params; the
/// page just groups by `date`. `available_shops`/`available_categories` populate
/// the filter dropdowns.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TransactionListDto {
    /// The filtered receipts.
    pub transactions: Vec<TransactionDto>,
    /// Roll-up for the page header.
    pub summary: TxnSummaryDto,
    /// Shop options for the filter dropdown.
    pub available_shops: Vec<String>,
    /// Category options for the filter dropdown.
    pub available_categories: Vec<String>,
}

/// One parsed receipt line (`TxnLinesDto::lines` element).
///
/// `line_total` is backend-derived (not recomputed client-side); `confidence`
/// 0–1 (lines < 0.7 render coral + a confirm/correct affordance); `signal_id`
/// links a tracked item-signal pill (else the line shows its `category`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TxnLineDto {
    /// Item name.
    pub name: String,
    /// Quantity.
    pub qty: f64,
    /// Unit price.
    #[serde(with = "phosk_model::money_centimes")]
    pub unit_price: Money,
    /// Line total (backend-derived).
    #[serde(with = "phosk_model::money_centimes")]
    pub line_total: Money,
    /// Line category.
    pub category: String,
    /// Tracked item-signal id, if any (empty = plain category tag).
    pub signal_id: String,
    /// OCR/LLM reading confidence, 0–1.
    pub confidence: f64,
}

/// `GET /transactions/{id}/lines` — the parsed lines + signal feeds + low-conf count.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TxnLinesDto {
    /// The parsed line items.
    pub lines: Vec<TxnLineDto>,
    /// Tracked item-signal ids this receipt feeds.
    pub sigs: Vec<String>,
    /// Count of lines below the 0.7 confidence threshold.
    pub low_conf: u32,
}

/// The receipt photo source descriptor (`TxnDetailDto::source`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TxnSourceDto {
    /// Source kind, e.g. `"PHOTO"`.
    #[serde(rename = "type")]
    pub kind: String,
    /// OCR engine label, e.g. `"PADDLEOCR"`.
    pub ocr_engine: String,
}

/// `GET /transactions/{id}` — receipt detail for the full-screen receipt view.
///
/// `avg_confidence` drives the reading-confidence bar; `ocr_regions` is the
/// count of detected boxes (the receipt screen also falls back to line count).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TxnDetailDto {
    /// Stable id (echoed).
    pub id: String,
    /// Average reading confidence across lines, 0–1.
    pub avg_confidence: f64,
    /// Photo/OCR source descriptor.
    pub source: TxnSourceDto,
    /// Detected OCR region count.
    pub ocr_regions: u32,
}

/// Transaction filters for `GET /transactions` (all optional, empty = "all").
///
/// `period` is `day|week|month|quarter|year` (empty = ALL); `shop`/`category`
/// narrow the set; `sort` is `date|amount|shop`; `q` is free-text search.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TxnFilter {
    /// Horizon: `""|day|week|month|quarter|year`.
    pub period: String,
    /// Shop filter (empty = all).
    pub shop: String,
    /// Category filter (empty = all).
    pub category: String,
    /// Sort key: `date|amount|shop`.
    pub sort: String,
    /// Free-text query.
    pub q: String,
}

/// The filtered transaction list (`GET /transactions`).
///
/// REAL: composes `phosk_ledger::transactions::list_transactions` — it resolves
/// the `period` window, applies the shop/category/q filters, sorts by `sort`, and
/// derives each row's item/low-conf/signal marks from its lines.
#[server]
pub async fn list_transactions(filter: TxnFilter) -> Result<TransactionListDto, ServerFnError> {
    #[cfg(feature = "server-deps")]
    {
        let session = crate::data::build_session().await?;
        let svc_filter = phosk_ledger::transactions::TxnFilter {
            period: filter.period,
            shop: filter.shop,
            category: filter.category,
            sort: filter.sort,
            q: filter.q,
        };
        let list = phosk_ledger::transactions::list_transactions(
            session.db(),
            crate::data::today(),
            svc_filter,
        )
        .await
        .map_err(|e| ServerFnError::new(e.to_string()))?;
        Ok(TransactionListDto {
            transactions: list.transactions.into_iter().map(map_txn).collect(),
            summary: TxnSummaryDto {
                entry_count: list.summary.entry_count,
                total_amount: list.summary.total_amount,
                period_label: list.summary.period_label,
            },
            available_shops: list.available_shops,
            available_categories: list.available_categories,
        })
    }
    #[cfg(not(feature = "server-deps"))]
    {
        let _ = filter;
        Err(ServerFnError::new("server-only"))
    }
}

/// One receipt's parsed lines (`GET /transactions/{id}/lines`).
///
/// REAL: composes `phosk_ledger::line_items::transaction_lines` for the receipt
/// slug.
#[server]
pub async fn get_transaction_lines(id: String) -> Result<TxnLinesDto, ServerFnError> {
    #[cfg(feature = "server-deps")]
    {
        let session = crate::data::build_session().await?;
        let l = phosk_ledger::line_items::transaction_lines(session.db(), &id)
            .await
            .map_err(|e| ServerFnError::new(e.to_string()))?;
        Ok(TxnLinesDto {
            lines: l.lines.into_iter().map(map_line).collect(),
            sigs: l.sigs,
            low_conf: l.low_conf,
        })
    }
    #[cfg(not(feature = "server-deps"))]
    {
        let _ = id;
        Err(ServerFnError::new("server-only"))
    }
}

/// One receipt's detail (`GET /transactions/{id}`).
///
/// REAL: composes `phosk_ledger::transactions::transaction_detail` for the slug.
#[server]
pub async fn get_transaction(id: String) -> Result<TxnDetailDto, ServerFnError> {
    #[cfg(feature = "server-deps")]
    {
        let session = crate::data::build_session().await?;
        let d = phosk_ledger::transactions::transaction_detail(session.db(), &id)
            .await
            .map_err(|e| ServerFnError::new(e.to_string()))?;
        Ok(TxnDetailDto {
            id: d.id,
            avg_confidence: d.avg_confidence,
            source: TxnSourceDto {
                kind: d.source.kind,
                ocr_engine: d.source.ocr_engine,
            },
            ocr_regions: d.ocr_regions,
        })
    }
    #[cfg(not(feature = "server-deps"))]
    {
        let _ = id;
        Err(ServerFnError::new("server-only"))
    }
}

// ── mappers (service DTO → wire DTO) ───────────────────────────────────────────

/// Map a `phosk_ledger` receipt row onto the wire [`TransactionDto`].
#[cfg(feature = "server-deps")]
fn map_txn(t: phosk_ledger::transactions::TransactionDto) -> TransactionDto {
    TransactionDto {
        id: t.id,
        date: t.date,
        shop: t.shop,
        category: t.category,
        amount: t.amount,
        item_count: t.item_count,
        low_conf_count: t.low_conf_count,
        signal_ids: t.signal_ids,
        fixed: t.fixed,
    }
}

/// Map a `phosk_ledger` parsed line onto the wire [`TxnLineDto`].
#[cfg(feature = "server-deps")]
fn map_line(l: phosk_ledger::line_items::TxnLineDto) -> TxnLineDto {
    TxnLineDto {
        name: l.name,
        qty: l.qty,
        unit_price: l.unit_price,
        line_total: l.line_total,
        category: l.category,
        signal_id: l.signal_id,
        confidence: l.confidence,
    }
}
