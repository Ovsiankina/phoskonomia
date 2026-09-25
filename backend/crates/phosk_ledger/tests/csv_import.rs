#![allow(
    // Test-only: the workspace denies these in production (see the sibling
    // ledger integration tests for the same crate-wide exemption).
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::doc_markdown,
    clippy::missing_const_for_fn
)]
//! Tests for `phosk_ledger::import` — CSV bank-export import (T22).
//!
//! The requirement: a CSV bank export, read through an explicit column
//! mapping, becomes ledger records stamped `Imported`. Swiss formats (dates
//! `dd.mm.yyyy` / `dd.mm.yy` / ISO, apostrophe-grouped amounts, `,` or `;`
//! delimiters, quoted fields) parse straight to exact centimes. Re-importing
//! the same (or an overlapping) file adds nothing. The file is hostile input:
//! bad rows are reported by row number without aborting the good ones, and
//! oversized input is refused. All data below is synthetic.

use std::fmt::Write as _;

use chrono::NaiveDate;
use phosk_adapter_db::DatabaseAdapter;
use phosk_core::error::PhoskError;
use phosk_db_memory::MemoryDb;
use phosk_ledger::import::{
    AmountColumns, CsvMapping, ImportOptions, MAX_FILE_BYTES, MAX_ROWS, import_csv, parse_csv,
    parse_csv_as_of,
};
use phosk_model::Source;

fn d(y: i32, m: u32, day: u32) -> NaiveDate {
    NaiveDate::from_ymd_opt(y, m, day).expect("valid date")
}

/// `Date;Description;Amount` with a header, signed amounts, `;` delimiter.
fn signed_mapping() -> CsvMapping {
    CsvMapping {
        delimiter: b';',
        has_header: true,
        date: 0,
        description: 1,
        amount: AmountColumns::Signed { column: 2 },
        currency: None,
    }
}

fn options(mapping: CsvMapping) -> ImportOptions {
    ImportOptions {
        mapping,
        category: "Imported".to_owned(),
    }
}

const SIGNED: &str = "Date;Description;Amount\n\
    02.06.2026;Kiosk Sonnenblick;-12.50\n\
    03.06.2026;\"Bäckerei Musterli; Filiale 2\";-1'234.05\n\
    04.06.2026;Gehalt Beispiel AG;5'000.00\n";

// ── Parsing ────────────────────────────────────────────────────────────────

#[test]
fn parses_swiss_amounts_to_exact_centimes_and_quoted_delimiters() {
    let parsed = parse_csv(SIGNED.as_bytes(), &signed_mapping()).expect("parse ok");
    assert!(parsed.errors.is_empty(), "{:?}", parsed.errors);
    assert_eq!(parsed.rows.len(), 2, "the credit row is not a spend");
    assert_eq!(parsed.credits_skipped, 1);

    assert_eq!(parsed.rows[0].date, d(2026, 6, 2));
    assert_eq!(parsed.rows[0].description, "Kiosk Sonnenblick");
    assert_eq!(
        parsed.rows[0].amount.centimes(),
        1_250,
        "debit stored positive"
    );

    assert_eq!(parsed.rows[1].description, "Bäckerei Musterli; Filiale 2");
    assert_eq!(parsed.rows[1].amount.centimes(), 123_405);
    assert_eq!(parsed.rows[1].row, 3, "row numbers count the header");
}

#[test]
fn accepts_all_three_date_forms_and_typographic_apostrophe() {
    let csv = "01.02.2026,Laden Eins,-1\u{2019}000.10\n\
               1.2.26,Laden Zwei,-3.5\n\
               2026-02-03,Laden Drei,-0.05\n";
    let mapping = CsvMapping {
        delimiter: b',',
        has_header: false,
        ..signed_mapping()
    };
    let parsed = parse_csv(csv.as_bytes(), &mapping).expect("parse ok");
    assert!(parsed.errors.is_empty(), "{:?}", parsed.errors);
    let got: Vec<(NaiveDate, i64)> = parsed
        .rows
        .iter()
        .map(|r| (r.date, r.amount.centimes()))
        .collect();
    assert_eq!(
        got,
        vec![
            (d(2026, 2, 1), 100_010),
            (d(2026, 2, 1), 350),
            (d(2026, 2, 3), 5),
        ]
    );
}

#[test]
fn debit_and_credit_columns() {
    let csv = "Datum,Text,Belastung,Gutschrift\r\n\
               10.06.2026,Velo Werkstatt,89.90,\r\n\
               11.06.2026,Rückerstattung,,20.00\r\n\
               12.06.2026,Beide gefüllt,1.00,2.00\r\n";
    let mapping = CsvMapping {
        delimiter: b',',
        has_header: true,
        date: 0,
        description: 1,
        amount: AmountColumns::DebitCredit {
            debit: 2,
            credit: 3,
        },
        currency: None,
    };
    let parsed = parse_csv(csv.as_bytes(), &mapping).expect("parse ok");
    assert_eq!(parsed.rows.len(), 1);
    assert_eq!(parsed.rows[0].amount.centimes(), 8_990);
    assert_eq!(parsed.credits_skipped, 1);
    assert_eq!(parsed.errors.len(), 1);
    assert_eq!(parsed.errors[0].row, 4);
}

#[test]
fn currency_column_accepts_chf_only() {
    let csv = "05.06.2026;Markt Alpha;-4.00;CHF\n\
               05.06.2026;Markt Beta;-4.00;EUR\n\
               05.06.2026;Markt Gamma;-4.00;\n";
    let mapping = CsvMapping {
        has_header: false,
        currency: Some(3),
        ..signed_mapping()
    };
    let parsed = parse_csv(csv.as_bytes(), &mapping).expect("parse ok");
    assert_eq!(parsed.rows.len(), 2, "CHF and blank (= CHF) are accepted");
    assert_eq!(parsed.errors.len(), 1);
    assert_eq!(parsed.errors[0].row, 2);
}

#[test]
fn bad_rows_are_reported_without_aborting_good_ones() {
    let csv = "Date;Description;Amount\n\
               31.02.2026;Kein Datum;-1.00\n\
               01.06.2026;Zu viele Stellen;-1.005\n\
               01.06.2026;Float;-1e3\n\
               01.06.2026;;-1.00\n\
               01.06.2026;Null;0.00\n\
               01.06.2026;Kurz\n\
               01.06.2026;\"offen\"x;-1.00\n\
               02.06.2026;Gut;-2.00\n";
    let parsed = parse_csv(csv.as_bytes(), &signed_mapping()).expect("parse ok");
    let rows: Vec<usize> = parsed.errors.iter().map(|e| e.row).collect();
    assert_eq!(rows, vec![2, 3, 4, 5, 6, 7, 8]);
    assert!(parsed.errors.iter().all(|e| !e.reason.is_empty()));
    assert_eq!(parsed.rows.len(), 1);
    assert_eq!(parsed.rows[0].row, 9);
}

#[test]
fn quoted_field_may_span_lines_and_escape_quotes() {
    let csv = "01.06.2026;\"Laden \"\"Zum Anker\"\"\nZeile 2\";-7.00\n\
               02.06.2026;Danach;-1.00\n";
    let mapping = CsvMapping {
        has_header: false,
        ..signed_mapping()
    };
    let parsed = parse_csv(csv.as_bytes(), &mapping).expect("parse ok");
    assert!(parsed.errors.is_empty(), "{:?}", parsed.errors);
    assert_eq!(parsed.rows[0].description, "Laden \"Zum Anker\" Zeile 2");
    assert_eq!(
        parsed.rows[1].row, 2,
        "row = record number, not line number"
    );
}

#[test]
fn garbage_never_panics() {
    let inputs: [&[u8]; 4] = [b"", b"\"", b";;;;\n\"\"\"\n", b"\n\n\r\r\n"];
    for input in inputs {
        let _ = parse_csv(input, &signed_mapping());
    }
}

#[test]
fn non_utf8_file_is_refused() {
    assert!(matches!(
        parse_csv(b"\xff\xfe\x00garbage", &signed_mapping()),
        Err(PhoskError::Invalid(_))
    ));
}

#[test]
fn unterminated_quote_refuses_the_whole_file() {
    // A stray quote would otherwise swallow every later record into one
    // field; half-importing the file would silently drop those rows.
    let mut csv = String::from("Date;Description;Amount\n");
    for i in 1..=9 {
        writeln!(csv, "0{i}.06.2026;Laden {i};-1.00").unwrap();
    }
    csv.push_str("10.06.2026;\"Offen;-1.00\n");
    for _ in 0..490 {
        csv.push_str("11.06.2026;Danach;-1.00\n");
    }
    match parse_csv(csv.as_bytes(), &signed_mapping()) {
        Err(PhoskError::Invalid(msg)) => {
            assert!(msg.contains("row 11"), "names the opening row: {msg}");
        }
        other => panic!("expected a file-level error, got {other:?}"),
    }
}

#[test]
fn oversized_input_is_refused() {
    let big = vec![b'a'; MAX_FILE_BYTES + 1];
    assert!(matches!(
        parse_csv(&big, &signed_mapping()),
        Err(PhoskError::Invalid(_))
    ));

    let many = "01.06.2026;x;-1.00\n".repeat(MAX_ROWS + 1);
    assert!(matches!(
        parse_csv(many.as_bytes(), &signed_mapping()),
        Err(PhoskError::Invalid(_))
    ));
}

#[test]
fn over_long_field_is_a_row_error() {
    let csv = format!(
        "Date;Description;Amount\n01.06.2026;{};-1.00\n",
        "x".repeat(2_000)
    );
    let parsed = parse_csv(csv.as_bytes(), &signed_mapping()).expect("parse ok");
    assert!(parsed.rows.is_empty());
    assert_eq!(parsed.errors.len(), 1);
    assert_eq!(parsed.errors[0].row, 2);
}

#[test]
fn invalid_mapping_is_refused() {
    let mapping = CsvMapping {
        delimiter: b'"',
        ..signed_mapping()
    };
    assert!(matches!(
        parse_csv(SIGNED.as_bytes(), &mapping),
        Err(PhoskError::Invalid(_))
    ));
}

// ── Import into the ledger ─────────────────────────────────────────────────

#[tokio::test]
async fn import_creates_imported_records_visible_to_aggregates() {
    let db = MemoryDb::seeded().expect("seed");
    let before = db.all_receipts().await.expect("list").len();

    let report = import_csv(&db, SIGNED.as_bytes(), &options(signed_mapping()))
        .await
        .expect("import ok");
    assert_eq!(report.imported, 2);
    assert_eq!(report.duplicates, 0);
    assert_eq!(report.credits_skipped, 1);

    let all = db.all_receipts().await.expect("list");
    assert_eq!(all.len(), before + 2);
    let imported: Vec<_> = all
        .iter()
        .filter(|r| r.provenance.source == Source::Imported)
        .collect();
    assert_eq!(imported.len(), 2);
    for r in &imported {
        assert!(r.slug.starts_with("import:"));
        assert_eq!(r.source_kind, "IMPORT");
        assert_eq!(r.category, "Imported");
    }

    let spend = db
        .transactions_between(d(2026, 6, 3), d(2026, 6, 3))
        .await
        .expect("range");
    assert!(
        spend
            .iter()
            .any(|t| t.shop == "Bäckerei Musterli; Filiale 2" && t.amount.centimes() == 123_405)
    );
}

#[tokio::test]
async fn reimport_and_overlap_add_nothing_twice() {
    let db = MemoryDb::seeded().expect("seed");
    let opts = options(CsvMapping {
        has_header: false,
        ..signed_mapping()
    });
    // Two identical coffees on the same day are two real spends.
    let first = "01.06.2026;Café Test;-4.50\n01.06.2026;Café Test;-4.50\n02.06.2026;Kiosk;-2.00\n";
    let overlap = "02.06.2026;Kiosk;-2.00\n01.06.2026;Café Test;-4.50\n01.06.2026;Café Test;-4.50\n\
                   03.06.2026;Neu;-9.00\n";

    let r1 = import_csv(&db, first.as_bytes(), &opts)
        .await
        .expect("first");
    assert_eq!((r1.imported, r1.duplicates), (3, 0));
    let after_first = db.all_receipts().await.expect("list").len();

    let r2 = import_csv(&db, first.as_bytes(), &opts)
        .await
        .expect("again");
    assert_eq!((r2.imported, r2.duplicates), (0, 3));
    assert_eq!(db.all_receipts().await.expect("list").len(), after_first);

    let r3 = import_csv(&db, overlap.as_bytes(), &opts)
        .await
        .expect("overlap");
    assert_eq!((r3.imported, r3.duplicates), (1, 3));
    assert_eq!(
        db.all_receipts().await.expect("list").len(),
        after_first + 1
    );
}

#[tokio::test]
async fn import_reports_row_errors_and_keeps_valid_rows() {
    let db = MemoryDb::seeded().expect("seed");
    let csv = "Date;Description;Amount\nnope;X;-1.00\n01.06.2026;Gut;-1.00\n";
    let report = import_csv(&db, csv.as_bytes(), &options(signed_mapping()))
        .await
        .expect("import ok");
    assert_eq!(report.imported, 1);
    assert_eq!(report.errors.len(), 1);
    assert_eq!(report.errors[0].row, 2);
}

#[tokio::test]
async fn blank_category_is_refused() {
    let db = MemoryDb::seeded().expect("seed");
    let opts = ImportOptions {
        mapping: signed_mapping(),
        category: "  ".to_owned(),
    };
    assert!(matches!(
        import_csv(&db, SIGNED.as_bytes(), &opts).await,
        Err(PhoskError::Invalid(_))
    ));
}

// ── Header detection ───────────────────────────────────────────────────────

#[test]
fn header_is_the_first_non_blank_record() {
    let csv = "\n\r\nDate;Description;Amount\n02.06.2026;Kiosk;-1.00\n";
    let parsed = parse_csv(csv.as_bytes(), &signed_mapping()).expect("parse ok");
    assert!(parsed.errors.is_empty(), "{:?}", parsed.errors);
    assert_eq!(parsed.rows.len(), 1);
    assert_eq!(parsed.rows[0].row, 4, "blank lines still count as rows");
}

// ── Dates: two-digit pivot and plausibility window ─────────────────────────

fn no_header() -> CsvMapping {
    CsvMapping {
        has_header: false,
        ..signed_mapping()
    }
}

fn dates_as_of(today: NaiveDate, dates: &[&str]) -> (Vec<NaiveDate>, Vec<usize>) {
    let csv = dates.iter().fold(String::new(), |mut csv, d| {
        writeln!(csv, "{d};Laden;-1.00").unwrap();
        csv
    });
    let parsed = parse_csv_as_of(csv.as_bytes(), &no_header(), today).expect("parse ok");
    (
        parsed.rows.iter().map(|r| r.date).collect(),
        parsed.errors.iter().map(|e| e.row).collect(),
    )
}

#[test]
fn two_digit_years_pivot_on_next_year() {
    // As of 2026: yy <= 27 is 20yy, yy >= 28 is 19yy (so 28..=69 lands
    // before the window — see the next test).
    let (got, errors) = dates_as_of(d(2026, 9, 25), &["01.01.27", "31.12.99", "01.01.70"]);
    assert!(errors.is_empty(), "{errors:?}");
    assert_eq!(got, vec![d(2027, 1, 1), d(1999, 12, 31), d(1970, 1, 1)]);
}

#[test]
fn dates_outside_the_plausibility_window_are_row_errors() {
    let today = d(2026, 9, 25);
    // Window: 1970-01-01 ..= today + 1 year.
    let (got, errors) = dates_as_of(
        today,
        &[
            "1970-01-01",
            "31.12.1969",
            "2027-09-25",
            "2027-09-26",
            "0000-01-01",
            "9999-12-31",
            "01.01.28", // 1928, before the window
        ],
    );
    assert_eq!(got, vec![d(1970, 1, 1), d(2027, 9, 25)]);
    assert_eq!(errors, vec![2, 4, 5, 6, 7]);
}

// ── Description sanitising ─────────────────────────────────────────────────

#[test]
fn description_drops_control_and_bidi_characters() {
    let csv = "02.06.2026;\"Shop\u{0}\u{7}\u{202E}FHC\u{202C}\u{2066}x\u{2069}\tEnd\";-1.00\n\
               02.06.2026;\u{202E}\u{1b};-1.00\n";
    let parsed = parse_csv(csv.as_bytes(), &no_header()).expect("parse ok");
    assert_eq!(parsed.rows.len(), 1);
    assert_eq!(parsed.rows[0].description, "ShopFHCx End");
    assert_eq!(parsed.errors.len(), 1, "nothing left = empty description");
    assert_eq!(parsed.errors[0].row, 2);
}

// ── Dedupe normalisation ───────────────────────────────────────────────────

fn hash_of(csv: &str, mapping: &CsvMapping) -> String {
    let parsed = parse_csv(csv.as_bytes(), mapping).expect("parse ok");
    assert!(parsed.errors.is_empty(), "{:?}", parsed.errors);
    assert_eq!(parsed.rows.len(), 1);
    parsed.rows[0].content_hash.clone()
}

#[test]
fn equivalent_rows_share_a_content_hash() {
    let m = no_header();
    let base = hash_of("01.06.2026;Kiosk Am Platz;-1234.5\n", &m);
    for variant in [
        "01.06.2026;Kiosk Am Platz;-1'234.50\n",
        "01.06.2026;Kiosk Am Platz;-1\u{2019}234.50\n",
        "2026-06-01;Kiosk Am Platz;-1234.50\n",
        "1.6.26;Kiosk Am Platz;-1234.50\n",
        "01.06.2026;  KIOSK   am\tplatz ;-1234.50\n",
        "01.06.2026;\"Kiosk\nAm Platz\";-1234.50\n",
    ] {
        assert_eq!(hash_of(variant, &m), base, "{variant:?}");
    }
    let debit_credit = CsvMapping {
        amount: AmountColumns::DebitCredit {
            debit: 2,
            credit: 3,
        },
        ..m
    };
    assert_eq!(
        hash_of("01.06.2026;Kiosk Am Platz;1'234.50;\n", &debit_credit),
        base,
        "signed and debit/credit mappings agree"
    );
    assert_ne!(
        hash_of("01.06.2026;Kiosk Am Platz;-1234.55\n", &m),
        base,
        "a different amount is a different row"
    );
}
