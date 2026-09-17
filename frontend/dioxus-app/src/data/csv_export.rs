//! CSV exports (T35): the transactions, budget and subscriptions lists as a
//! downloadable CSV file.
//!
//! The server renders the file through `phosk_insights::exports` for the cycle
//! containing [`crate::data::today`] and returns the text plus a suggested
//! filename ([`CsvFileDto`]). Nothing is written to disk on the server; the
//! client turns the response into a download:
//!
//! * **web**: an `a[download]` whose `href` is [`csv_data_uri`] (the CSV,
//!   percent-encoded into a `data:` URI);
//! * **desktop**: a native save dialog, then the client writes the chosen file
//!   (see `components::csv_export` for why a `data:` link cannot work there).

use dioxus::prelude::*;
use serde::{Deserialize, Serialize};

/// Which list a CSV export renders (one per export button).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CsvExportKind {
    /// The receipts of the current cycle.
    Transactions,
    /// The category envelopes (cap, spent, remaining).
    Budget,
    /// The standing charges.
    Subscriptions,
}

/// A rendered CSV file, ready to hand to the user.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CsvFileDto {
    /// Suggested file name, a plain `*.csv` basename (checked server-side).
    pub filename: String,
    /// The whole CSV document (header row + data rows).
    pub csv: String,
    /// Number of data rows (header excluded).
    pub row_count: u32,
}

/// Prefix of every CSV `data:` URI built by [`csv_data_uri`].
const CSV_DATA_URI_PREFIX: &str = "data:text/csv;charset=utf-8,";

/// Build the `data:` URI a web `a[download]` link points at.
///
/// Every byte except the RFC 3986 unreserved set (`A-Z a-z 0-9 - . _ ~`) is
/// percent-encoded, so commas, quotes, `#`, `%`, newlines and non-ASCII UTF-8
/// all survive the round trip through the URL parser.
#[must_use]
pub fn csv_data_uri(csv: &str) -> String {
    const HEX: &[u8; 16] = b"0123456789ABCDEF";
    let mut out = String::with_capacity(CSV_DATA_URI_PREFIX.len() + csv.len() * 3);
    out.push_str(CSV_DATA_URI_PREFIX);
    for b in csv.bytes() {
        if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'.' | b'_' | b'~') {
            out.push(char::from(b));
        } else {
            out.push('%');
            out.push(char::from(HEX[usize::from(b >> 4)]));
            out.push(char::from(HEX[usize::from(b & 0x0F)]));
        }
    }
    out
}

// ══ T35: CSV export server fn ════════════════════════════════════════════════
//
// `export_csv` only builds the session and delegates to `export_csv_with`,
// which holds the logic and is what the tests drive (against a fresh seeded
// `MemoryDb`, never the process-global stack).

/// Render one list as a CSV file for the current cycle.
///
/// REAL: composes `phosk_insights::exports::export_{transactions,budget,subscriptions}_csv`.
/// Read-only: nothing is persisted and nothing is written to disk.
#[server]
pub async fn export_csv(kind: CsvExportKind) -> Result<CsvFileDto, ServerFnError> {
    #[cfg(feature = "server-deps")]
    {
        // The stack-assembly error text can carry adapter / path details.
        let session = crate::data::build_session()
            .await
            .map_err(|_| ServerFnError::new("CSV export unavailable"))?;
        export_csv_with(session.db(), kind, crate::data::today())
            .await
            .map_err(|e| export_error(&e))
    }
    #[cfg(not(feature = "server-deps"))]
    {
        let _ = kind;
        Err(ServerFnError::new("server-only"))
    }
}

/// The logic behind [`export_csv`]: render `kind` for the cycle containing
/// `as_of` through the database port, then check the suggested filename.
///
/// # Errors
/// Any [`phosk_core::error::PhoskError`] from the export service, or
/// `Invalid` if the service suggested an unsafe filename.
#[cfg(feature = "server-deps")]
pub(crate) async fn export_csv_with(
    db: &dyn phosk_adapter_db::DatabaseAdapter,
    kind: CsvExportKind,
    as_of: chrono::NaiveDate,
) -> Result<CsvFileDto, phosk_core::error::PhoskError> {
    use phosk_insights::exports;

    let file = match kind {
        CsvExportKind::Transactions => exports::export_transactions_csv(db, as_of).await?,
        CsvExportKind::Budget => exports::export_budget_csv(db, as_of).await?,
        CsvExportKind::Subscriptions => exports::export_subscriptions_csv(db, as_of).await?,
    };
    Ok(CsvFileDto {
        filename: checked_filename(file.filename)?,
        csv: file.csv,
        row_count: file.row_count,
    })
}

/// Accept only a plain `*.csv` basename (ASCII letters, digits, `-`, `_`, `.`;
/// no leading dot, no path separator). The name becomes the `download`
/// attribute on web and the suggested name in the desktop save dialog.
#[cfg(feature = "server-deps")]
fn checked_filename(name: String) -> Result<String, phosk_core::error::PhoskError> {
    let plain = name.len() > ".csv".len()
        && name.ends_with(".csv")
        && !name.starts_with('.')
        && name
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.'));
    if plain {
        Ok(name)
    } else {
        Err(phosk_core::error::PhoskError::Invalid(
            "export filename is not a plain csv file name".to_owned(),
        ))
    }
}

/// Map a service error to what the client sees: the stable error code only,
/// never the inner message (which may carry adapter or path details).
#[cfg(feature = "server-deps")]
fn export_error(e: &phosk_core::error::PhoskError) -> ServerFnError {
    ServerFnError::new(format!("CSV export failed ({})", e.code()))
}

#[cfg(all(test, feature = "server-deps"))]
mod csv_export_tests {
    use super::{
        checked_filename, csv_data_uri, export_csv_with, export_error, CsvExportKind,
        CSV_DATA_URI_PREFIX,
    };
    use phosk_adapter_db::DatabaseAdapter;
    use phosk_core::error::PhoskError;
    use phosk_db_memory::MemoryDb;

    fn seeded() -> MemoryDb {
        MemoryDb::seeded().expect("seed is valid")
    }

    fn lines(csv: &str) -> Vec<&str> {
        csv.lines().filter(|l| !l.is_empty()).collect()
    }

    /// Test-only inverse of `csv_data_uri`'s percent-encoding.
    fn decode(uri: &str) -> String {
        let body = uri
            .strip_prefix(CSV_DATA_URI_PREFIX)
            .expect("data URI prefix");
        let bytes = body.as_bytes();
        let mut out = Vec::with_capacity(bytes.len());
        let mut i = 0;
        while i < bytes.len() {
            if bytes[i] == b'%' {
                let hex = std::str::from_utf8(&bytes[i + 1..i + 3]).expect("ascii hex");
                out.push(u8::from_str_radix(hex, 16).expect("valid hex"));
                i += 3;
            } else {
                out.push(bytes[i]);
                i += 1;
            }
        }
        String::from_utf8(out).expect("utf-8")
    }

    #[tokio::test]
    async fn each_kind_exports_its_own_list_for_the_cycle() {
        let db = seeded();
        let as_of = crate::data::today();

        let tx = export_csv_with(&db, CsvExportKind::Transactions, as_of)
            .await
            .expect("transactions export");
        assert_eq!(tx.filename, "phosk-transactions-2026-06-18.csv");
        assert_eq!(tx.row_count, 9, "nine June receipts");
        assert!(
            tx.csv.starts_with("date,shop,category,amount"),
            "{}",
            tx.csv
        );

        let budget = export_csv_with(&db, CsvExportKind::Budget, as_of)
            .await
            .expect("budget export");
        assert_eq!(budget.filename, "phosk-budget-2026-06-18.csv");
        assert_eq!(budget.row_count, 8, "eight envelopes");
        assert!(
            budget.csv.starts_with("category,cap,spent"),
            "{}",
            budget.csv
        );

        let subs = export_csv_with(&db, CsvExportKind::Subscriptions, as_of)
            .await
            .expect("subscriptions export");
        assert_eq!(subs.filename, "phosk-subscriptions-2026-06-18.csv");
        assert_eq!(subs.row_count, 6, "six subscriptions");
        assert!(subs.csv.starts_with("name,amount,cadence"), "{}", subs.csv);
    }

    #[tokio::test]
    async fn row_count_matches_the_data_lines() {
        let db = seeded();
        for kind in [
            CsvExportKind::Transactions,
            CsvExportKind::Budget,
            CsvExportKind::Subscriptions,
        ] {
            let file = export_csv_with(&db, kind, crate::data::today())
                .await
                .expect("export");
            assert_eq!(
                lines(&file.csv).len(),
                file.row_count as usize + 1,
                "{kind:?}: header + rows"
            );
        }
    }

    #[tokio::test]
    async fn an_empty_cycle_is_a_header_only_file() {
        let db = seeded();
        let april = chrono::NaiveDate::from_ymd_opt(2026, 4, 15).expect("valid date");
        let file = export_csv_with(&db, CsvExportKind::Transactions, april)
            .await
            .expect("export");
        assert_eq!(file.row_count, 0);
        assert_eq!(
            lines(&file.csv),
            vec!["date,shop,category,amount,fixed,source"]
        );
        assert_eq!(file.filename, "phosk-transactions-2026-04-15.csv");
    }

    #[tokio::test]
    async fn export_reads_the_given_port_and_neutralises_formulas() {
        // A fresh store, edited locally: the export must see this edit (so it
        // reads the port it is handed) and must not ship a live formula.
        let db = seeded();
        let mut sub = db
            .subscription_by_slug("netflix")
            .await
            .expect("seeded netflix");
        sub.name = "=cmd|' /C calc'!A0".to_owned();
        db.upsert_subscription(sub).await.expect("upsert");

        let file = export_csv_with(&db, CsvExportKind::Subscriptions, crate::data::today())
            .await
            .expect("export");
        let row = lines(&file.csv)
            .into_iter()
            .find(|l| l.contains("calc"))
            .expect("edited row present");
        assert!(row.starts_with("'=cmd|"), "formula neutralised: {row:?}");
    }

    #[test]
    fn filenames_must_be_plain_csv_basenames() {
        for ok in ["phosk-budget-2026-06-18.csv", "a_b.csv"] {
            assert_eq!(checked_filename(ok.to_owned()).ok().as_deref(), Some(ok));
        }
        for bad in [
            "",
            ".csv",
            "../phosk.csv",
            "dir/phosk.csv",
            "dir\\phosk.csv",
            "phosk.csv\n",
            "phosk budget.csv",
            "phosk.txt",
            "phosk\u{e9}.csv",
        ] {
            assert!(
                matches!(
                    checked_filename(bad.to_owned()),
                    Err(PhoskError::Invalid(_))
                ),
                "{bad:?} must be rejected"
            );
        }
    }

    #[test]
    fn errors_reach_the_client_without_internal_detail() {
        let err = export_error(&PhoskError::NotFound(
            "surreal table receipt at /srv/secret/phosk.db".to_owned(),
        ));
        let shown = err.to_string();
        assert!(shown.contains("not_found"), "stable code kept: {shown}");
        assert!(!shown.contains("surreal"), "adapter detail leaked: {shown}");
        assert!(!shown.contains("/srv"), "path leaked: {shown}");
    }

    #[test]
    fn data_uri_percent_encodes_all_but_unreserved_bytes() {
        let uri = csv_data_uri("a,b\n\"Z\u{fc}rich\" #1 50%~_.-");
        assert_eq!(
            uri,
            "data:text/csv;charset=utf-8,a%2Cb%0A%22Z%C3%BCrich%22%20%231%2050%25~_.-"
        );
    }

    #[tokio::test]
    async fn data_uri_round_trips_a_real_export() {
        let db = seeded();
        let file = export_csv_with(&db, CsvExportKind::Budget, crate::data::today())
            .await
            .expect("export");
        let uri = csv_data_uri(&file.csv);
        assert!(
            uri[CSV_DATA_URI_PREFIX.len()..]
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"-._~%".contains(&b)),
            "only unreserved bytes and escapes in the body"
        );
        assert_eq!(decode(&uri), file.csv);
    }
}
