//! CSV exports read-model (feature F3, exports slice).
//!
//! Backs `/exports/*.csv`: schema-versioned, timestamped CSV dumps of the
//! transactions / budget / subscriptions lists, mirroring the same list filters
//! the pages use. There is NO dioxus DTO file for exports yet (the
//! build-contract derives the shape from
//! `backend/documentation/backend-features-todo.md §5 Exports`), so the DTOs
//! below are the wire truth for this slice: a [`CsvExportDto`] carrying the
//! rendered CSV body plus its `filename`, `version`, `generatedAt` timestamp and
//! `rowCount`.
//!
//! Every service fn takes `&dyn DatabaseAdapter` (the PORT) + an `as_of`
//! `NaiveDate` (to resolve the cycle the filters bound), returns
//! `Result<CsvExportDto, PhoskError>`, rendering the RFC-4180 CSV body with its
//! provenance metadata. The JSON/PDF dumps named in the todo ("Later") are
//! intentionally out of scope here.

use serde::{Deserialize, Serialize};

use phosk_adapter_db::DatabaseAdapter;
use phosk_core::cycle::Period;
use phosk_core::error::PhoskError;
use phosk_core::money::Money;

/// The schema version stamped into every export (and its `version` field), bumped
/// when the column layout changes so older dumps stay identifiable.
pub const EXPORT_SCHEMA_VERSION: &str = "1";

/// A rendered CSV export — the body plus its provenance metadata.
///
/// `csv` is the full RFC-4180 document (header row + data rows); `filename` the
/// suggested download name (e.g. `"phosk-transactions-2026-06.csv"`); `version`
/// the [`EXPORT_SCHEMA_VERSION`]; `generated_at` the export timestamp (ISO date);
/// `row_count` the number of data rows (excluding the header).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CsvExportDto {
    /// Suggested download filename.
    pub filename: String,
    /// Schema version stamp ([`EXPORT_SCHEMA_VERSION`]).
    pub version: String,
    /// Export timestamp (ISO `YYYY-MM-DD`).
    pub generated_at: String,
    /// Number of data rows (excluding the header row).
    pub row_count: u32,
    /// The full CSV document (header + rows), RFC-4180.
    pub csv: String,
}

/// Which list a CSV export renders. Mirrors the three filterable list pages
/// (`backend-features-todo.md §5 Exports`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ExportKind {
    /// The transactions list.
    Transactions,
    /// The budget / category envelopes.
    Budget,
    /// The subscriptions list.
    Subscriptions,
}

/// Export the transactions list for the cycle containing `as_of` as CSV.
///
/// Mirrors the transactions-list filters; one data row per receipt, money columns
/// rendered as exact CHF strings; header + version + timestamp stamped.
///
/// # Errors
/// Propagates any [`PhoskError`] from cycle resolution or adapter reads.
#[tracing::instrument(level = "debug", skip_all, fields(as_of = %as_of))]
pub async fn export_transactions_csv(
    db: &dyn DatabaseAdapter,
    as_of: chrono::NaiveDate,
) -> Result<CsvExportDto, PhoskError> {
    let window = Period::Month.resolve(as_of)?;
    let mut receipts = db.receipts_between(window.start, window.end).await?;
    // Stable order: newest first, ties by slug — mirrors the transactions list.
    receipts.sort_by(|a, b| b.date.cmp(&a.date).then_with(|| a.slug.cmp(&b.slug)));

    let header = "date,shop,category,amount,fixed,source";
    let rows: Vec<String> = receipts
        .iter()
        .map(|r| {
            format!(
                "{},{},{},{},{},{}",
                r.date,
                csv_field(&r.shop),
                csv_field(&r.category),
                chf_plain(r.amount),
                r.fixed,
                csv_field(&r.source_kind),
            )
        })
        .collect();

    Ok(build_export("transactions", as_of, header, &rows))
}

/// Export the budget / category envelopes for the cycle containing `as_of` as CSV.
///
/// One data row per category cap (name, cap, spent, remaining); header + version +
/// timestamp stamped.
///
/// # Errors
/// Propagates any [`PhoskError`] from cycle resolution or adapter reads.
#[tracing::instrument(level = "debug", skip_all, fields(as_of = %as_of))]
pub async fn export_budget_csv(
    db: &dyn DatabaseAdapter,
    as_of: chrono::NaiveDate,
) -> Result<CsvExportDto, PhoskError> {
    let window = Period::Month.resolve(as_of)?;
    let receipts = db.receipts_between(window.start, window.end).await?;
    let caps = db.category_caps().await?;

    let header = "category,cap,spent,remaining,fixed";
    let mut rows = Vec::with_capacity(caps.len());
    for cap in &caps {
        // Spend-to-date this cycle for the category.
        let spent = Money::sum(
            receipts
                .iter()
                .filter(|r| r.category == cap.name)
                .map(|r| r.amount),
        )?;
        // remaining = cap − spent (signed); blank when the channel is unlimited.
        let (cap_cell, remaining_cell) = match cap.cap {
            Some(c) => (chf_plain(c), chf_plain(c.checked_sub(spent)?)),
            None => (String::new(), String::new()),
        };
        rows.push(format!(
            "{},{},{},{},{}",
            csv_field(&cap.name),
            cap_cell,
            chf_plain(spent),
            remaining_cell,
            cap.fixed,
        ));
    }

    Ok(build_export("budget", as_of, header, &rows))
}

/// Export the subscriptions list for the cycle containing `as_of` as CSV.
///
/// One data row per subscription (name, amount, cadence, next charge, status);
/// header + version + timestamp stamped.
///
/// # Errors
/// Propagates any [`PhoskError`] from cycle resolution or adapter reads.
#[tracing::instrument(level = "debug", skip_all, fields(as_of = %as_of))]
pub async fn export_subscriptions_csv(
    db: &dyn DatabaseAdapter,
    as_of: chrono::NaiveDate,
) -> Result<CsvExportDto, PhoskError> {
    let mut subs = db.subscriptions().await?;
    // Stable order: by slug, mirroring the subscriptions list's default.
    subs.sort_by(|a, b| a.slug.cmp(&b.slug));

    let header = "name,amount,cadence,day,month,status,category";
    let rows: Vec<String> = subs
        .iter()
        .map(|s| {
            format!(
                "{},{},{},{},{},{},{}",
                csv_field(&s.name),
                chf_plain(s.amount),
                csv_field(&s.cadence),
                s.day,
                csv_field(&s.month),
                csv_field(&s.status),
                csv_field(&s.category),
            )
        })
        .collect();

    Ok(build_export("subscriptions", as_of, header, &rows))
}

// ── internal helpers ──────────────────────────────────────────────────────────

/// Assemble a [`CsvExportDto`] from a header line and the rendered data rows.
fn build_export(
    list: &str,
    as_of: chrono::NaiveDate,
    header: &str,
    rows: &[String],
) -> CsvExportDto {
    let mut csv = String::with_capacity(header.len() + rows.iter().map(String::len).sum::<usize>());
    csv.push_str(header);
    for row in rows {
        csv.push('\n');
        csv.push_str(row);
    }
    CsvExportDto {
        filename: format!("phosk-{list}-{as_of}.csv"),
        version: EXPORT_SCHEMA_VERSION.to_owned(),
        generated_at: as_of.to_string(),
        row_count: u32::try_from(rows.len()).unwrap_or(u32::MAX),
        csv,
    }
}

/// Render a [`Money`] as a Swiss CHF amount for a CSV cell — apostrophe thousands
/// grouping, always two decimals — but WITHOUT the `CHF ` prefix that
/// [`Money`]'s `Display` carries (e.g. `1'680.00`, `-12.40`). Reusing the single
/// canonical formatter keeps the Swiss grouping identical everywhere.
fn chf_plain(m: Money) -> String {
    m.to_string()
        .strip_prefix("CHF ")
        .map_or_else(|| m.to_string(), str::to_owned)
}

/// Quote a CSV field if it contains a comma, quote, or newline (RFC-4180),
/// doubling any embedded quotes. Plain fields pass through unchanged.
fn csv_field(s: &str) -> String {
    if s.contains([',', '"', '\n', '\r']) {
        format!("\"{}\"", s.replace('"', "\"\""))
    } else {
        s.to_owned()
    }
}
