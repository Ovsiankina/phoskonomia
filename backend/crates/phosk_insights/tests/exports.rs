#![allow(
    // Test-only: the workspace denies these in production, but `clippy.toml`'s
    // allow-in-tests only covers `#[test]` bodies, not integration-test helpers
    // or module docs, so the exemption is made explicit crate-wide (mirrors the
    // dashboard integration test).
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::doc_markdown,
    clippy::missing_const_for_fn,
    clippy::float_cmp,
    clippy::suboptimal_flops,
    clippy::bool_assert_comparison,
    clippy::needless_collect,
    clippy::comparison_chain,
    clippy::redundant_closure_for_method_calls,
    clippy::case_sensitive_file_extension_comparisons,
    clippy::cast_possible_truncation
)]
//! RED integration tests for `phosk_insights::exports` — the CSV export
//! read-model (feature F3, exports slice).
//!
//! These tests pin the **stable contract** the `exports` skeleton commits to: a
//! [`CsvExportDto`] per list (transactions / budget / subscriptions) carrying a
//! schema `version`, a `generatedAt` timestamp (the `as_of` ISO date), a
//! `rowCount` of data rows (header excluded), a suggested `filename`, and an
//! RFC-4180 `csv` body whose header + data rows mirror the same list filters the
//! pages use — with money rendered as Swiss CHF strings **at the edge only**
//! (centimes everywhere internally; the CSV column is the single render site).
//!
//! Every value is pinned against the deterministic Swiss seed
//! (`MemoryDb::seeded()`, `as_of = 2026-06-18`, the June cycle):
//!   - transactions: the 9 June receipts `t1..t9`;
//!   - budget: the 8 category caps;
//!   - subscriptions: the 6 subs.
//!
//! All service bodies are `todo!()`, so these tests MUST compile and then FAIL at
//! runtime (the green phase makes them pass). No production logic lives here; the
//! tests use `expect("msg")` (never bare `unwrap()`), per the conventions.

use chrono::NaiveDate;

use phosk_adapter_db::DatabaseAdapter;
use phosk_db_memory::MemoryDb;
use phosk_insights::exports::{
    CsvExportDto, EXPORT_SCHEMA_VERSION, ExportKind, export_budget_csv, export_subscriptions_csv,
    export_transactions_csv,
};

// ── helpers ────────────────────────────────────────────────────────────────────

fn naive(y: i32, m: u32, d: u32) -> NaiveDate {
    NaiveDate::from_ymd_opt(y, m, d).expect("valid test date")
}

/// The spec's "today": 2026-06-18, day 18 of the June cycle.
fn today() -> NaiveDate {
    naive(2026, 6, 18)
}

fn seeded() -> MemoryDb {
    MemoryDb::seeded().expect("seed is valid")
}

/// Split a CSV body into logical lines (RFC-4180 CRLF or bare LF), dropping a
/// trailing empty line if the document ends with a newline.
fn lines(csv: &str) -> Vec<String> {
    csv.replace("\r\n", "\n")
        .split('\n')
        .map(str::to_owned)
        .filter(|l| !l.is_empty())
        .collect()
}

/// The header row (first logical line) of a CSV body.
fn header(csv: &str) -> String {
    lines(csv).first().cloned().unwrap_or_default()
}

/// The data rows (everything after the header).
fn data_rows(csv: &str) -> Vec<String> {
    let mut rows = lines(csv);
    if rows.is_empty() {
        return rows;
    }
    rows.remove(0);
    rows
}

// ── shared metadata invariants (all three exports) ──────────────────────────────

/// Every export stamps the current schema version (`EXPORT_SCHEMA_VERSION`), and
/// it is the const `"1"`.
#[tokio::test]
async fn every_export_stamps_the_schema_version() {
    let db = seeded();
    let as_of = today();
    assert_eq!(EXPORT_SCHEMA_VERSION, "1", "schema version pinned at 1");

    for dto in [
        export_transactions_csv(&db, as_of)
            .await
            .expect("transactions export ok"),
        export_budget_csv(&db, as_of)
            .await
            .expect("budget export ok"),
        export_subscriptions_csv(&db, as_of)
            .await
            .expect("subscriptions export ok"),
    ] {
        assert_eq!(dto.version, EXPORT_SCHEMA_VERSION, "version field stamped");
    }
}

/// `generatedAt` is the `as_of` date as an ISO `YYYY-MM-DD` string — the export
/// timestamp, not "now" (so the export is deterministic for a given `as_of`).
#[tokio::test]
async fn generated_at_is_the_as_of_iso_date() {
    let db = seeded();
    let as_of = today();

    let tx = export_transactions_csv(&db, as_of)
        .await
        .expect("transactions export ok");
    assert_eq!(
        tx.generated_at, "2026-06-18",
        "generatedAt is the as_of ISO date"
    );

    // A different as_of stamps a different timestamp.
    let other = export_transactions_csv(&db, naive(2026, 5, 9))
        .await
        .expect("transactions export ok");
    assert_eq!(other.generated_at, "2026-05-09", "tracks as_of");
}

/// `rowCount` counts DATA rows only — it excludes the header row and matches the
/// number of CSV data lines.
#[tokio::test]
async fn row_count_excludes_the_header_row() {
    let db = seeded();
    let as_of = today();

    let tx = export_transactions_csv(&db, as_of)
        .await
        .expect("transactions export ok");
    assert_eq!(
        tx.row_count as usize,
        data_rows(&tx.csv).len(),
        "rowCount == data lines (header excluded)"
    );
    // The header is present and is NOT counted.
    assert_eq!(
        lines(&tx.csv).len(),
        tx.row_count as usize + 1,
        "header + data"
    );
}

/// The CSV body always carries a header row before any data.
#[tokio::test]
async fn every_export_has_a_header_row() {
    let db = seeded();
    let as_of = today();

    for dto in [
        export_transactions_csv(&db, as_of)
            .await
            .expect("transactions export ok"),
        export_budget_csv(&db, as_of)
            .await
            .expect("budget export ok"),
        export_subscriptions_csv(&db, as_of)
            .await
            .expect("subscriptions export ok"),
    ] {
        let h = header(&dto.csv);
        assert!(!h.is_empty(), "header row present");
        // A header row carries column labels, not a bare amount — at least one comma.
        assert!(h.contains(','), "header is comma-separated: {h:?}");
    }
}

// ── transactions export ─────────────────────────────────────────────────────────

/// The transactions CSV covers exactly the June cycle's 9 receipts (`t1..t9`),
/// mirroring the transactions-list cycle filter — one data row per receipt.
#[tokio::test]
async fn transactions_export_has_one_row_per_june_receipt() {
    let db = seeded();
    let dto = export_transactions_csv(&db, today())
        .await
        .expect("transactions export ok");

    assert_eq!(dto.row_count, 9, "9 June receipts t1..t9");
    assert_eq!(data_rows(&dto.csv).len(), 9, "9 data rows");
}

/// The transactions filename names the list + the cycle it covers and ends in
/// `.csv`.
#[tokio::test]
async fn transactions_filename_names_list_and_cycle() {
    let db = seeded();
    let dto = export_transactions_csv(&db, today())
        .await
        .expect("transactions export ok");

    assert!(
        dto.filename.ends_with(".csv"),
        "filename ends in .csv: {:?}",
        dto.filename
    );
    assert!(
        dto.filename.contains("transaction"),
        "filename names the list: {:?}",
        dto.filename
    );
    assert!(
        dto.filename.contains("2026-06"),
        "filename names the June cycle: {:?}",
        dto.filename
    );
}

/// Each receipt's identifying cells appear in the CSV: its shop, its ISO date,
/// and its amount rendered as a Swiss CHF string (apostrophe thousands group,
/// always two decimals) — the single render site for money.
#[tokio::test]
async fn transactions_rows_carry_shop_date_and_swiss_amount() {
    let db = seeded();
    let dto = export_transactions_csv(&db, today())
        .await
        .expect("transactions export ok");
    let rows = data_rows(&dto.csv);

    // t8 — Landlord, 2026-06-01, CHF 1680.00 (apostrophe-grouped thousands).
    let rent = rows
        .iter()
        .find(|r| r.contains("Landlord"))
        .expect("rent row present");
    assert!(rent.contains("2026-06-01"), "rent date: {rent:?}");
    assert!(
        rent.contains("1'680.00"),
        "rent amount Swiss-formatted with apostrophe group: {rent:?}"
    );

    // t9 — Helsana, 2026-06-01, CHF 318.00 (no group, two decimals).
    let ins = rows
        .iter()
        .find(|r| r.contains("Helsana"))
        .expect("insurance row present");
    assert!(ins.contains("318.00"), "insurance amount: {ins:?}");

    // t4 — Galaxus, 2026-06-13, CHF 129.90 (sub-thousand, fractional francs).
    let shop = rows
        .iter()
        .find(|r| r.contains("Galaxus"))
        .expect("Galaxus row present");
    assert!(shop.contains("2026-06-13"), "Galaxus date: {shop:?}");
    assert!(shop.contains("129.90"), "Galaxus amount: {shop:?}");
}

/// The two `Migros` June receipts (`t1` CHF 58.75, `t5` CHF 12.80) are BOTH
/// present — duplicate shop names are not collapsed (it is a per-receipt list).
#[tokio::test]
async fn transactions_export_keeps_duplicate_shops_distinct() {
    let db = seeded();
    let dto = export_transactions_csv(&db, today())
        .await
        .expect("transactions export ok");
    let rows = data_rows(&dto.csv);

    let migros: Vec<&String> = rows.iter().filter(|r| r.contains("Migros")).collect();
    assert_eq!(migros.len(), 2, "two distinct Migros receipts (t1, t5)");
    assert!(
        migros.iter().any(|r| r.contains("58.75")),
        "t1 amount present"
    );
    assert!(
        migros.iter().any(|r| r.contains("12.80")),
        "t5 amount present (two decimals, not 12.8)"
    );
}

/// A cycle with no receipts (April 2026) yields a header-only CSV: zero data
/// rows, `rowCount == 0`, but the header + metadata are still emitted.
#[tokio::test]
async fn transactions_export_empty_cycle_is_header_only() {
    let db = seeded();
    let dto = export_transactions_csv(&db, naive(2026, 4, 15))
        .await
        .expect("transactions export ok");

    assert_eq!(dto.row_count, 0, "April has no seeded receipts");
    assert!(data_rows(&dto.csv).is_empty(), "no data rows");
    assert!(!header(&dto.csv).is_empty(), "header still present");
    assert_eq!(dto.version, EXPORT_SCHEMA_VERSION, "metadata still stamped");
    assert_eq!(dto.generated_at, "2026-04-15", "timestamp still stamped");
}

// ── budget export ───────────────────────────────────────────────────────────────

/// The budget CSV has one data row per category cap — the 8 seeded envelopes.
#[tokio::test]
async fn budget_export_has_one_row_per_category_cap() {
    let db = seeded();
    let dto = export_budget_csv(&db, today())
        .await
        .expect("budget export ok");

    assert_eq!(dto.row_count, 8, "8 seeded category caps");
    assert_eq!(data_rows(&dto.csv).len(), 8, "8 data rows");
}

/// Each capped category carries its name + its cap rendered as a Swiss CHF
/// string. Groceries cap CHF 800.00, Rent cap CHF 1'680.00 (apostrophe group).
#[tokio::test]
async fn budget_rows_carry_name_and_swiss_cap() {
    let db = seeded();
    let dto = export_budget_csv(&db, today())
        .await
        .expect("budget export ok");
    let rows = data_rows(&dto.csv);

    let groceries = rows
        .iter()
        .find(|r| r.contains("Groceries"))
        .expect("Groceries row present");
    assert!(
        groceries.contains("800.00"),
        "Groceries cap CHF 800.00: {groceries:?}"
    );

    let rent = rows
        .iter()
        .find(|r| r.contains("Rent"))
        .expect("Rent row present");
    assert!(
        rent.contains("1'680.00"),
        "Rent cap Swiss-formatted: {rent:?}"
    );

    // Every seeded category name appears once.
    for name in [
        "Groceries",
        "Going out",
        "Coffee & snacks",
        "Transport",
        "Rent",
        "Health insurance",
        "Shopping",
        "Subscriptions",
    ] {
        let count = rows.iter().filter(|r| r.contains(name)).count();
        assert!(count >= 1, "category {name:?} present in budget CSV");
    }
}

/// The budget filename names the budget list and ends in `.csv`.
#[tokio::test]
async fn budget_filename_names_list() {
    let db = seeded();
    let dto = export_budget_csv(&db, today())
        .await
        .expect("budget export ok");
    assert!(
        dto.filename.ends_with(".csv"),
        "ends in .csv: {:?}",
        dto.filename
    );
    assert!(
        dto.filename.contains("budget"),
        "names the budget list: {:?}",
        dto.filename
    );
}

// ── subscriptions export ────────────────────────────────────────────────────────

/// The subscriptions CSV has one data row per subscription — the 6 seeded subs.
#[tokio::test]
async fn subscriptions_export_has_one_row_per_subscription() {
    let db = seeded();
    let dto = export_subscriptions_csv(&db, today())
        .await
        .expect("subscriptions export ok");

    assert_eq!(dto.row_count, 6, "6 seeded subscriptions");
    assert_eq!(data_rows(&dto.csv).len(), 6, "6 data rows");
}

/// Each subscription carries its name, its amount (Swiss CHF), and its cadence.
/// Netflix CHF 19.90 monthly; Domain + email CHF 42.00 yearly.
#[tokio::test]
async fn subscriptions_rows_carry_name_amount_and_cadence() {
    let db = seeded();
    let dto = export_subscriptions_csv(&db, today())
        .await
        .expect("subscriptions export ok");
    let rows = data_rows(&dto.csv);

    let netflix = rows
        .iter()
        .find(|r| r.contains("Netflix"))
        .expect("Netflix row present");
    assert!(
        netflix.contains("19.90"),
        "Netflix amount CHF 19.90: {netflix:?}"
    );
    assert!(netflix.contains("monthly"), "Netflix cadence: {netflix:?}");

    let domain = rows
        .iter()
        .find(|r| r.contains("Domain + email"))
        .expect("Domain row present");
    assert!(
        domain.contains("42.00"),
        "Domain amount CHF 42.00: {domain:?}"
    );
    assert!(domain.contains("yearly"), "Domain cadence: {domain:?}");

    // All six names present.
    for name in [
        "Netflix",
        "Spotify Family",
        "iCloud+ 2TB",
        "Gym membership",
        "NYT Digital",
        "Domain + email",
    ] {
        assert!(
            rows.iter().any(|r| r.contains(name)),
            "subscription {name:?} present"
        );
    }
}

/// A subscription name containing a comma (`"iCloud+ 2TB"` is safe, but
/// `"Spotify Family"` is multi-word) is correctly RFC-4180 escaped: the CSV body
/// must remain parseable. We assert the row exists and the field count is stable.
#[tokio::test]
async fn subscriptions_csv_is_rfc4180_well_formed() {
    let db = seeded();
    let dto = export_subscriptions_csv(&db, today())
        .await
        .expect("subscriptions export ok");

    let cols = header(&dto.csv).split(',').count();
    assert!(
        cols >= 3,
        "header declares >=3 columns: {:?}",
        header(&dto.csv)
    );

    // Every data row, once RFC-4180 quoting is honored, has the same logical
    // field count as the header (no stray unescaped commas leak extra fields).
    for row in data_rows(&dto.csv) {
        let fields = split_rfc4180(&row);
        assert_eq!(
            fields.len(),
            cols,
            "row field count matches header ({cols} cols): {row:?}"
        );
    }
}

/// Minimal RFC-4180 field splitter: splits on commas outside double-quotes,
/// honoring `""` as an escaped quote within a quoted field. Test-only.
fn split_rfc4180(row: &str) -> Vec<String> {
    let mut fields = Vec::new();
    let mut cur = String::new();
    let mut in_quotes = false;
    let mut chars = row.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '"' if in_quotes && chars.peek() == Some(&'"') => {
                cur.push('"');
                chars.next();
            }
            '"' => in_quotes = !in_quotes,
            ',' if !in_quotes => {
                fields.push(std::mem::take(&mut cur));
            }
            other => cur.push(other),
        }
    }
    fields.push(cur);
    fields
}

// ── spreadsheet formula injection ───────────────────────────────────────────────

/// Text cells are user- or OCR-supplied (a shop name read off a photo, a
/// subscription typed by hand), so they are hostile input for whatever
/// spreadsheet opens the file. A text cell that starts with `=`, `+`, `-`, `@`
/// or a tab would be evaluated as a formula (CSV/formula injection), so the
/// export prefixes it with a `'` and the spreadsheet keeps it as plain text.
/// The same characters later in a cell are left alone, and so are the seeded
/// names.
#[tokio::test]
async fn text_cells_that_look_like_formulas_are_neutralised() {
    let db = seeded();
    let mut sub = db
        .subscription_by_slug("netflix")
        .await
        .expect("seeded netflix subscription");
    sub.name = "=HYPERLINK(\"https://example.invalid\",\"x\")".to_owned();
    sub.cadence = "+1".to_owned();
    sub.month = "-2+3".to_owned();
    sub.status = "@SUM(A1)".to_owned();
    sub.category = "\tcmd".to_owned();
    db.upsert_subscription(sub)
        .await
        .expect("upsert hostile subscription");
    // Excel in the de-CH / fr-CH / de-DE locales splits a .csv on `;`, and a
    // line break starts a new row, so a trigger after either is a cell start
    // too. The category is the last column, where such a payload hurts most.
    let mut semi = db
        .subscription_by_slug("spotify")
        .await
        .expect("seeded spotify subscription");
    semi.name = "x;=cmd|' /C calc'!A0;".to_owned();
    semi.category = "x\n=1+1;".to_owned();
    db.upsert_subscription(semi)
        .await
        .expect("upsert semicolon subscription");

    let dto = export_subscriptions_csv(&db, today())
        .await
        .expect("subscriptions export ok");
    let rows = data_rows(&dto.csv);
    let hostile = rows
        .iter()
        .find(|r| r.contains("HYPERLINK"))
        .expect("hostile row present");
    let fields = split_rfc4180(hostile);
    // name,amount,cadence,day,month,status,category
    assert_eq!(fields.len(), 7, "still seven fields: {hostile:?}");
    assert_eq!(
        fields[0], "'=HYPERLINK(\"https://example.invalid\",\"x\")",
        "leading = neutralised"
    );
    assert_eq!(fields[1], "19.90", "money cell untouched");
    assert_eq!(fields[2], "'+1", "leading + neutralised");
    assert_eq!(fields[4], "'-2+3", "leading - neutralised");
    assert_eq!(fields[5], "'@SUM(A1)", "leading @ neutralised");
    assert_eq!(fields[6], "'\tcmd", "leading tab neutralised");

    // The `;` / line-break payloads are prefixed where each new cell starts.
    assert!(
        dto.csv
            .contains("\nx;'=cmd|' /C calc'!A0;,15.95,monthly,28,,ok,\"x\n'=1+1;\""),
        "semicolon and newline payloads neutralised: {:?}",
        dto.csv
    );
    // Whichever of `,` `;` tab CR LF a spreadsheet splits on, and whatever
    // quotes or spaces it drops in front of a value, no cell of this export
    // (all amounts here are positive) starts with a formula trigger.
    for piece in dto.csv.split([',', ';', '\t', '\r', '\n']) {
        let cell = piece.trim_start_matches(|c: char| c == '"' || c.is_whitespace());
        assert!(
            !cell.starts_with(['=', '+', '-', '@']),
            "live formula cell {cell:?} in {:?}",
            dto.csv
        );
    }

    // Interior `+` is not a formula trigger: the seeded name passes through.
    let icloud = rows
        .iter()
        .find(|r| r.contains("iCloud"))
        .expect("iCloud row present");
    assert_eq!(
        split_rfc4180(icloud)[0],
        "iCloud+ 2TB",
        "interior + untouched"
    );
}

// ── ExportKind ──────────────────────────────────────────────────────────────────

/// `ExportKind` is a 3-variant enum, one per filterable list — a compile + value
/// pin so the green phase keeps the three kinds.
#[test]
fn export_kind_has_the_three_list_variants() {
    let kinds = [
        ExportKind::Transactions,
        ExportKind::Budget,
        ExportKind::Subscriptions,
    ];
    assert_eq!(kinds.len(), 3, "exactly three export kinds");
    // Copy + Eq are part of the contract.
    assert_eq!(ExportKind::Budget, ExportKind::Budget);
    assert_ne!(ExportKind::Transactions, ExportKind::Subscriptions);
}

// ── port-object safety ──────────────────────────────────────────────────────────

/// Every export service is callable behind the `&dyn DatabaseAdapter` PORT handle
/// (ADR-010) — the read-model never sees the concrete `MemoryDb` type.
#[tokio::test]
async fn exports_work_through_the_port_trait_object() {
    let db = seeded();
    let port: &dyn DatabaseAdapter = &db;
    let as_of = today();

    let tx: CsvExportDto = export_transactions_csv(port, as_of)
        .await
        .expect("transactions export ok");
    assert_eq!(tx.row_count, 9);

    let budget = export_budget_csv(port, as_of)
        .await
        .expect("budget export ok");
    assert_eq!(budget.row_count, 8);

    let subs = export_subscriptions_csv(port, as_of)
        .await
        .expect("subscriptions export ok");
    assert_eq!(subs.row_count, 6);
}
