//! CSV exports read-model (feature F3, exports slice).
//!
//! Backs the CSV export buttons (the dioxus-app `data::csv_export` server fn):
//! schema-versioned, timestamped CSV dumps of the transactions / budget /
//! subscriptions lists for the current cycle. The DTOs below are the service
//! truth for this slice: a [`CsvExportDto`] carrying the rendered CSV body plus
//! its `filename`, `version`, `generatedAt` timestamp and `rowCount` (the
//! dioxus-app maps it onto its own wire struct).
//!
//! Text cells are guarded against spreadsheet formula injection at every
//! position a spreadsheet import could start a cell (the start of the text and
//! right after `,` `;` tab CR LF, so `;`-separator locales are covered too; see
//! `csv_field`); money cells are exact CHF strings.
//!
//! Every service fn takes `&dyn DatabaseAdapter` (the PORT) + an `as_of`
//! `NaiveDate` (to resolve the month cycle to export; no page filters are
//! applied), returns `Result<CsvExportDto, PhoskError>`, rendering the RFC-4180
//! CSV body. The body holds only the header row and the data rows; the schema
//! version and timestamp travel in the DTO fields, not in the file. The JSON/PDF
//! dumps named in the todo ("Later") are intentionally out of scope here.

use serde::{Deserialize, Serialize};

use phosk_adapter_db::DatabaseAdapter;
use phosk_core::cycle::Period;
use phosk_core::error::PhoskError;
use phosk_core::money::Money;

/// The schema version carried in every export's `version` field, bumped
/// when the column layout changes so older dumps stay identifiable.
pub const EXPORT_SCHEMA_VERSION: &str = "1";

/// A rendered CSV export — the body plus its provenance metadata.
///
/// `csv` is the full RFC-4180 document (header row + data rows); `filename` the
/// suggested download name (e.g. `"phosk-transactions-2026-06-18.csv"`); `version`
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
/// Covers the whole month cycle (the page's shop / category / search filters are
/// not applied); one data row per receipt, money columns rendered as exact CHF
/// strings; header row first, version + timestamp in the DTO fields.
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
/// One data row per category cap (name, cap, spent, remaining); header row first,
/// version + timestamp in the DTO fields.
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
/// header row first, version + timestamp in the DTO fields.
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

/// Leading characters that make a spreadsheet (Excel, Calc, Sheets) evaluate a
/// cell as a formula or DDE command (the OWASP "CSV injection" list).
const FORMULA_TRIGGERS: [char; 6] = ['=', '+', '-', '@', '\t', '\r'];

/// Characters after which a spreadsheet import may start a new cell, whatever
/// the file's own delimiter is: `,` (the file's), `;` (Excel's list separator in
/// the de-CH / fr-CH / de-DE locales), tab, and CR / LF (a new row).
const CELL_BREAKS: [char; 5] = [',', ';', '\t', '\r', '\n'];

/// Render one TEXT cell: neutralise spreadsheet formula injection, then quote it
/// if it contains a comma, quote, or newline (RFC-4180), doubling any embedded
/// quotes. Plain fields pass through unchanged.
///
/// Text cells carry user- and OCR-supplied strings (shop names, categories,
/// subscription names), which are hostile input for the spreadsheet that opens
/// the export. Checking only the first character is not enough: Excel in a
/// `;`-separator locale splits `x;=cmd…` into two cells, and quoting does not
/// help there. So every position that could start a cell is guarded: the start
/// of the text and every position right after one of [`CELL_BREAKS`]. From such
/// a position, any `"` and whitespace (which an importer may drop) are skipped;
/// if the next character is one of [`FORMULA_TRIGGERS`], a `'` is inserted in
/// front of it, so the spreadsheet shows it as text instead of running it.
/// Money cells never go through here (a negative amount must stay a number).
fn csv_field(s: &str) -> String {
    let neutralised = neutralise_formulas(s);
    if neutralised.contains([',', '"', '\n', '\r']) {
        format!("\"{}\"", neutralised.replace('"', "\"\""))
    } else {
        neutralised
    }
}

/// The formula guard of [`csv_field`], applied before RFC-4180 quoting.
fn neutralise_formulas(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 1);
    let mut at_cell_start = true;
    for c in s.chars() {
        if at_cell_start {
            if FORMULA_TRIGGERS.contains(&c) {
                out.push('\'');
                at_cell_start = false;
            } else if !(c == '"' || (c.is_whitespace() && !CELL_BREAKS.contains(&c))) {
                at_cell_start = false;
            }
        }
        out.push(c);
        // Tab and CR are triggers AND breaks: the text after them is a new cell.
        if CELL_BREAKS.contains(&c) {
            at_cell_start = true;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::{FORMULA_TRIGGERS, csv_field};

    /// Split a rendered cell the way any spreadsheet import might (`,`, `;`,
    /// tab, CR, LF), strip what an importer may drop in front of a value
    /// (quotes, whitespace), and fail if any piece starts with a trigger.
    fn assert_no_live_formula(input: &str) {
        let out = csv_field(input);
        for piece in out.split([',', ';', '\t', '\r', '\n']) {
            let cell = piece.trim_start_matches(|c: char| c == '"' || c.is_whitespace());
            assert!(
                !cell.starts_with(FORMULA_TRIGGERS),
                "live formula piece {cell:?} in {out:?} (input {input:?})"
            );
        }
    }

    #[test]
    fn every_formula_trigger_is_prefixed() {
        assert_eq!(csv_field("=1+1"), "'=1+1");
        assert_eq!(csv_field("+41 79"), "'+41 79");
        assert_eq!(csv_field("-5"), "'-5");
        assert_eq!(csv_field("@SUM(A1)"), "'@SUM(A1)");
        // Tab and CR are triggers AND cell breaks, so the text after them is
        // checked again.
        assert_eq!(csv_field("\t=1"), "'\t'=1");
        // CR also forces RFC-4180 quoting; the prefixes land inside the quotes.
        assert_eq!(csv_field("\r=1"), "\"'\r'=1\"");
    }

    #[test]
    fn triggers_after_a_semicolon_are_prefixed() {
        // de-CH / fr-CH / de-DE Excel splits a double-clicked .csv on `;`.
        assert_eq!(csv_field("x;=cmd|' /C calc'!A0;"), "x;'=cmd|' /C calc'!A0;");
        assert_eq!(csv_field("x;=1+1"), "x;'=1+1");
        // Leading spaces or quotes in front of the trigger are skipped.
        assert_eq!(csv_field("a; -1"), "a; '-1");
        assert_eq!(csv_field("x;\"=1+1\";"), "\"x;\"\"'=1+1\"\";\"");
    }

    #[test]
    fn triggers_after_a_line_break_tab_or_comma_are_prefixed() {
        assert_eq!(csv_field("x\n=1+1;"), "\"x\n'=1+1;\"");
        assert_eq!(csv_field("x\r\n-1"), "\"x\r\n'-1\"");
        assert_eq!(csv_field("x\t=1"), "x\t'=1");
        assert_eq!(csv_field("x, @SUM(A1)"), "\"x, '@SUM(A1)\"");
    }

    #[test]
    fn no_piece_of_a_hostile_cell_is_a_live_formula() {
        for input in [
            "x;=cmd|' /C calc'!A0;",
            "x;\"=1+1\";",
            "x\n=1+1;",
            "a; -1",
            "x;=1+1",
            "x;\" \"\"=1",
            ";;=1",
            "\r\n\t=1",
            "x,\"=HYPERLINK(\"\"http://a\"\")\"",
            "=1;+2\n-3\r@4\t=5,=6",
            "x;\u{a0}=1",
        ] {
            assert_no_live_formula(input);
        }
    }

    #[test]
    fn non_triggers_after_a_break_pass_through() {
        assert_eq!(csv_field("a;b"), "a;b");
        assert_eq!(csv_field("x; y"), "x; y");
        assert_eq!(csv_field("Domain + email"), "Domain + email");
        assert_eq!(csv_field("a;b-c"), "a;b-c");
    }

    #[test]
    fn neutralised_cells_are_still_rfc4180_quoted() {
        assert_eq!(
            csv_field("=HYPERLINK(\"x\",\"y\")"),
            "\"'=HYPERLINK(\"\"x\"\",\"\"y\"\")\""
        );
    }

    #[test]
    fn plain_and_interior_characters_pass_through() {
        assert_eq!(csv_field("Migros"), "Migros");
        assert_eq!(csv_field("iCloud+ 2TB"), "iCloud+ 2TB");
        assert_eq!(csv_field("Coop-Pronto"), "Coop-Pronto");
        assert_eq!(csv_field("a@b"), "a@b");
        assert_eq!(csv_field(""), "");
        assert_eq!(csv_field("Coffee, snacks"), "\"Coffee, snacks\"");
    }
}
