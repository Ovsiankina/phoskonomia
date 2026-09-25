//! `import` — CSV bank-export import (the `Imported` write path).
//!
//! [`parse_csv`] turns a bank export into validated spend rows through an
//! explicit [`CsvMapping`]; [`import_csv`] persists those rows as receipts
//! stamped [`Source::Imported`] through the same `insert_receipt` port method
//! manual entry uses (so they reach the dashboard projection immediately).
//!
//! **Hostile input.** The bytes are bounded before decoding ([`MAX_FILE_BYTES`],
//! [`MAX_ROWS`], [`MAX_FIELD_BYTES`]); a malformed row becomes a [`RowError`]
//! (row number + fixed reason, never an echo of the content) and the valid rows
//! still import. A quote left open at end of file is a file-level error: it
//! would otherwise swallow every later record, and half-importing a file
//! drops rows silently. Dates must fall in [`EARLIEST_DATE`] ..= one year
//! after today. Control and bidi-override characters are stripped from the
//! booking text before it becomes a shop name. Nothing here panics on garbage.
//!
//! **Money.** Amounts go through [`Money::parse_chf`] — integer arithmetic only,
//! apostrophe (`'` / `’`) thousands groups, at most two decimals. No float at
//! any step.
//!
//! **Dedupe.** Each row's slug is `import:<sha256>` of its normalised content
//! (date, centimes, currency, whitespace-collapsed lowercase description —
//! no Unicode normalisation, so NFC and NFD spellings differ) plus
//! its occurrence index among identical rows of the same file. The slug is the
//! port's idempotency key, so an already-stored slug is skipped (never
//! replaced — that would discard user edits). Re-importing a file, or one that
//! overlaps it, adds only the rows not yet stored; two identical spends on the
//! same day in one file stay two records.
//!
//! [`Source::Imported`]: phosk_model::Source

use std::collections::{HashMap, HashSet};

use chrono::{Datelike, NaiveDate};
use phosk_adapter_db::DatabaseAdapter;
use phosk_core::error::PhoskError;
use phosk_core::money::Money;
use phosk_id::ReceiptId;
use phosk_model::{Provenance, Receipt};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// Largest accepted file, in bytes (5 MiB — years of a personal account).
pub const MAX_FILE_BYTES: usize = 5 * 1024 * 1024;
/// Largest accepted number of records (header included).
pub const MAX_ROWS: usize = 20_000;
/// Longest accepted single field, in bytes.
pub const MAX_FIELD_BYTES: usize = 1_024;
/// Most fields a record may have.
const MAX_FIELDS: usize = 64;
/// Earliest accepted booking date (1970-01-01); the latest is one year after
/// today. Anything outside is a row error (a typo or hostile input).
pub const EARLIEST_DATE: NaiveDate = match NaiveDate::from_ymd_opt(1970, 1, 1) {
    Some(d) => d,
    None => NaiveDate::MIN,
};

/// Which column(s) carry the amount.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", tag = "kind")]
pub enum AmountColumns {
    /// One signed column: negative = money out (a spend), positive = money in.
    Signed {
        /// 0-based column index.
        column: usize,
    },
    /// Separate unsigned debit (money out) and credit (money in) columns;
    /// exactly one of them is filled per row.
    DebitCredit {
        /// 0-based debit column index.
        debit: usize,
        /// 0-based credit column index.
        credit: usize,
    },
}

/// The explicit column mapping of a bank export (0-based column indices).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CsvMapping {
    /// Field delimiter: `b','` or `b';'`.
    pub delimiter: u8,
    /// Whether the first non-blank record is a header (skipped; leading blank
    /// lines before it are allowed and still counted as rows). A preamble of
    /// non-blank lines is not supported.
    pub has_header: bool,
    /// Booking date: `dd.mm.yyyy`, `dd.mm.yy` or ISO `yyyy-mm-dd`. A
    /// two-digit year up to next year's (`yy <= today's yy + 1`) is 20yy,
    /// above it 19yy.
    pub date: usize,
    /// Booking text; becomes the shop name.
    pub description: usize,
    /// Amount column(s).
    pub amount: AmountColumns,
    /// Optional ISO currency column; blank means CHF, anything else but CHF
    /// is a row error (the ledger is single-currency).
    pub currency: Option<usize>,
}

/// Import settings: the mapping plus the category every imported row gets.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportOptions {
    /// How to read the file.
    pub mapping: CsvMapping,
    /// Category stamped on each imported receipt (required, trimmed).
    pub category: String,
}

/// One rejected record: its 1-based record number (header = row 1) and why.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RowError {
    /// 1-based record number in the file (a quoted multi-line field is still
    /// one record).
    pub row: usize,
    /// Fixed, content-free reason.
    pub reason: String,
}

/// One validated spend row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImportRow {
    /// 1-based record number in the file.
    pub row: usize,
    /// Booking date.
    pub date: NaiveDate,
    /// Whitespace-collapsed booking text.
    pub description: String,
    /// The spend, positive exact centimes.
    pub amount: Money,
    /// Lowercase hex SHA-256 dedupe key (see the module docs).
    pub content_hash: String,
}

/// What [`parse_csv`] found.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ParsedCsv {
    /// Valid spend rows, in file order.
    pub rows: Vec<ImportRow>,
    /// Valid rows that are money coming in — not spend, so not imported.
    pub credits_skipped: u32,
    /// Rejected rows, in file order.
    pub errors: Vec<RowError>,
}

/// What [`import_csv`] did.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportReport {
    /// New receipts written.
    pub imported: u32,
    /// Rows already in the ledger from an earlier import.
    pub duplicates: u32,
    /// Credit (money in) rows, not imported.
    pub credits_skipped: u32,
    /// Rejected rows.
    pub errors: Vec<RowError>,
}

/// Parse a CSV bank export into validated spend rows.
///
/// # Errors
/// [`PhoskError::Invalid`] — for the whole file, nothing imported — if the
/// mapping is unusable, the file exceeds [`MAX_FILE_BYTES`] / [`MAX_ROWS`], it
/// is not UTF-8, or a quoted field is still open at end of file. Every other
/// problem is a per-row [`RowError`].
pub fn parse_csv(bytes: &[u8], mapping: &CsvMapping) -> Result<ParsedCsv, PhoskError> {
    parse_csv_as_of(bytes, mapping, chrono::Utc::now().date_naive())
}

/// [`parse_csv`] with an explicit "today" (the two-digit-year pivot and the
/// upper end of the date window depend on it).
///
/// # Errors
/// As [`parse_csv`].
pub fn parse_csv_as_of(
    bytes: &[u8],
    mapping: &CsvMapping,
    today: NaiveDate,
) -> Result<ParsedCsv, PhoskError> {
    validate_mapping(mapping)?;
    if bytes.len() > MAX_FILE_BYTES {
        return Err(PhoskError::Invalid(format!(
            "CSV file exceeds {MAX_FILE_BYTES} bytes"
        )));
    }
    let text = std::str::from_utf8(bytes)
        .map_err(|_| PhoskError::Invalid("CSV file must be UTF-8 encoded".to_owned()))?;
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    let records = read_records(text, char::from(mapping.delimiter))?;

    let mut out = ParsedCsv::default();
    let mut seen: HashMap<String, u32> = HashMap::new();
    let dates = DateRules::as_of(today);
    let skip = usize::from(mapping.has_header);
    for record in records.into_iter().skip(skip) {
        let fields = match record.fields {
            Ok(fields) => fields,
            Err(reason) => {
                out.errors.push(row_error(record.row, reason));
                continue;
            }
        };
        match parse_row(&fields, mapping, &dates) {
            Ok(Parsed::Spend {
                date,
                description,
                amount,
            }) => {
                let key = normalised_key(date, amount, &description);
                let occurrence = seen.entry(key.clone()).or_insert(0);
                let content_hash = sha256_hex(&format!("{key}\u{1f}{occurrence}"));
                *occurrence += 1;
                out.rows.push(ImportRow {
                    row: record.row,
                    date,
                    description,
                    amount,
                    content_hash,
                });
            }
            Ok(Parsed::Credit) => out.credits_skipped += 1,
            Err(reason) => out.errors.push(row_error(record.row, reason)),
        }
    }
    Ok(out)
}

/// Parse `bytes` and write every new spend row as an [`Source::Imported`]
/// receipt (category `options.category`, `source_kind = "IMPORT"`, no lines).
///
/// Rows whose content hash is already stored are counted as duplicates and
/// left untouched.
///
/// # Errors
/// [`PhoskError::Invalid`] for a blank category or a file-level problem (see
/// [`parse_csv`]); any adapter error aborts the import. Rows written before
/// the failure stay, and re-running the import is safe (it skips them).
///
/// [`Source::Imported`]: phosk_model::Source
#[tracing::instrument(level = "debug", skip_all, fields(bytes = bytes.len()))]
pub async fn import_csv(
    db: &dyn DatabaseAdapter,
    bytes: &[u8],
    options: &ImportOptions,
) -> Result<ImportReport, PhoskError> {
    let category = options.category.trim();
    if category.is_empty() {
        return Err(PhoskError::Invalid("category is required".to_owned()));
    }
    let parsed = parse_csv(bytes, &options.mapping)?;
    // One read of the stored slugs instead of a lookup per row (a per-row
    // `receipt_by_slug` is a table scan on SurrealDB — O(n²) per import).
    let mut stored: HashSet<String> = db
        .all_receipts()
        .await?
        .into_iter()
        .map(|r| r.slug)
        .collect();

    let mut report = ImportReport {
        imported: 0,
        duplicates: 0,
        credits_skipped: parsed.credits_skipped,
        errors: parsed.errors,
    };
    for row in parsed.rows {
        let slug = format!("import:{}", row.content_hash);
        if stored.contains(&slug) {
            report.duplicates += 1;
            continue;
        }
        db.insert_receipt(
            Receipt {
                id: ReceiptId::new(),
                slug: slug.clone(),
                shop: row.description,
                date: row.date,
                category: category.to_owned(),
                amount: row.amount,
                fixed: false,
                provenance: Provenance::imported(),
                source_kind: "IMPORT".to_owned(),
                ocr_engine: String::new(),
                ocr_regions: 0,
            },
            Vec::new(),
        )
        .await?;
        stored.insert(slug);
        report.imported += 1;
    }
    tracing::debug!(
        imported = report.imported,
        duplicates = report.duplicates,
        errors = report.errors.len(),
        "CSV import done"
    );
    Ok(report)
}

// ── Row validation ─────────────────────────────────────────────────────────

enum Parsed {
    Spend {
        date: NaiveDate,
        description: String,
        amount: Money,
    },
    Credit,
}

fn validate_mapping(m: &CsvMapping) -> Result<(), PhoskError> {
    if !matches!(m.delimiter, b',' | b';') {
        return Err(PhoskError::Invalid(
            "delimiter must be ',' or ';'".to_owned(),
        ));
    }
    let amount_cols = match m.amount {
        AmountColumns::Signed { column } => vec![column],
        AmountColumns::DebitCredit { debit, credit } => vec![debit, credit],
    };
    let mut cols = vec![m.date, m.description];
    cols.extend(amount_cols);
    cols.extend(m.currency);
    if cols.iter().any(|&c| c >= MAX_FIELDS) {
        return Err(PhoskError::Invalid(format!(
            "column indices must be below {MAX_FIELDS}"
        )));
    }
    let mut unique = cols.clone();
    unique.sort_unstable();
    unique.dedup();
    if unique.len() != cols.len() {
        return Err(PhoskError::Invalid(
            "each mapped column must be distinct".to_owned(),
        ));
    }
    Ok(())
}

/// The date rules as of one "today": two-digit-year pivot and window.
struct DateRules {
    /// Two-digit years up to this are 20yy, above it 19yy.
    pivot: u32,
    /// Latest accepted date (today + 1 year).
    latest: NaiveDate,
}

impl DateRules {
    fn as_of(today: NaiveDate) -> Self {
        let latest = today
            .checked_add_months(chrono::Months::new(12))
            .unwrap_or(NaiveDate::MAX);
        let next_year = u32::try_from(today.year() + 1).unwrap_or(0);
        Self {
            pivot: next_year % 100,
            latest,
        }
    }
}

fn parse_row(fields: &[String], m: &CsvMapping, dates: &DateRules) -> Result<Parsed, &'static str> {
    if fields.iter().any(|f| f.len() > MAX_FIELD_BYTES) {
        return Err("a field is too long");
    }
    let get = |i: usize| fields.get(i).map(|s| s.trim()).ok_or("missing column");

    let date = parse_date(get(m.date)?, dates.pivot).ok_or("unrecognised date")?;
    if date < EARLIEST_DATE || date > dates.latest {
        return Err("date out of range");
    }
    let description = strip_unsafe(get(m.description)?)
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    if description.is_empty() {
        return Err("description is empty");
    }
    if let Some(c) = m.currency {
        let currency = get(c)?;
        if !(currency.is_empty() || currency.eq_ignore_ascii_case("CHF")) {
            return Err("unsupported currency (only CHF)");
        }
    }
    // Signed centimes, negative = money out. `parse_chf` bounds the magnitude
    // to `i64::MAX`, so negating either way cannot overflow.
    let signed = match m.amount {
        AmountColumns::Signed { column } => amount(get(column)?)?.centimes(),
        AmountColumns::DebitCredit { debit, credit } => match (get(debit)?, get(credit)?) {
            (d, "") if !d.is_empty() => -unsigned(d)?.centimes(),
            ("", c) if !c.is_empty() => unsigned(c)?.centimes(),
            _ => return Err("exactly one of debit or credit must be filled"),
        },
    };
    match signed {
        0 => Err("amount is zero"),
        c if c > 0 => Ok(Parsed::Credit),
        c => Ok(Parsed::Spend {
            date,
            description,
            amount: Money::from_centimes(-c),
        }),
    }
}

/// Drop control characters (whitespace ones become a space, collapsed later)
/// and the bidi embedding/override/isolate characters that can make a shop
/// name display in a misleading order — the same set the chat panel strips.
fn strip_unsafe(s: &str) -> String {
    s.chars()
        .filter(|c| !matches!(c, '\u{202A}'..='\u{202E}' | '\u{2066}'..='\u{2069}'))
        .filter_map(|c| match c {
            c if c.is_control() && c.is_whitespace() => Some(' '),
            c if c.is_control() => None,
            c => Some(c),
        })
        .collect()
}

/// A signed amount (`-1'234.50`).
fn amount(s: &str) -> Result<Money, &'static str> {
    Money::parse_chf(s).map_err(|_| "unrecognised amount")
}

/// A debit/credit column value: must carry no sign.
fn unsigned(s: &str) -> Result<Money, &'static str> {
    let m = amount(s)?;
    if m.centimes() < 0 || s.starts_with(['+', '-', '\u{2212}']) {
        return Err("debit/credit amounts must be unsigned");
    }
    Ok(m)
}

/// `dd.mm.yyyy`, `dd.mm.yy` (20yy up to `pivot`, else 19yy) or `yyyy-mm-dd`.
fn parse_date(s: &str, pivot: u32) -> Option<NaiveDate> {
    let num = |p: &str, lens: &[usize]| -> Option<u32> {
        if lens.contains(&p.len()) && p.bytes().all(|b| b.is_ascii_digit()) {
            p.parse().ok()
        } else {
            None
        }
    };
    let parts: Vec<&str>;
    let (y, m, d) = if s.contains('.') {
        parts = s.split('.').collect();
        let [d, m, y] = parts.as_slice() else {
            return None;
        };
        let year = match y.len() {
            2 => match num(y, &[2])? {
                yy if yy <= pivot => 2000 + yy,
                yy => 1900 + yy,
            },
            _ => num(y, &[4])?,
        };
        (year, num(m, &[1, 2])?, num(d, &[1, 2])?)
    } else {
        parts = s.split('-').collect();
        let [y, m, d] = parts.as_slice() else {
            return None;
        };
        (num(y, &[4])?, num(m, &[2])?, num(d, &[2])?)
    };
    NaiveDate::from_ymd_opt(i32::try_from(y).ok()?, m, d)
}

/// The content that identifies a row across files (dedupe key material).
fn normalised_key(date: NaiveDate, amount: Money, description: &str) -> String {
    format!(
        "v1\u{1f}{date}\u{1f}{}\u{1f}CHF\u{1f}{}",
        amount.centimes(),
        description.to_lowercase()
    )
}

fn row_error(row: usize, reason: &str) -> RowError {
    RowError {
        row,
        reason: reason.to_owned(),
    }
}

/// SHA-256 of the text, lowercase hex.
fn sha256_hex(text: &str) -> String {
    use std::fmt::Write as _;
    let digest = Sha256::digest(text.as_bytes());
    digest.iter().fold(String::with_capacity(64), |mut s, b| {
        let _ = write!(s, "{b:02x}");
        s
    })
}

// ── RFC 4180 reader ────────────────────────────────────────────────────────

/// One record: its 1-based number and its fields, or why it is malformed.
struct Record {
    row: usize,
    fields: Result<Vec<String>, &'static str>,
}

/// Split `text` into records (RFC 4180: `"` quoting, `""` escapes, quoted
/// fields may hold delimiters and newlines; `\n`, `\r\n` or `\r` end a
/// record). Blank lines are skipped but still counted.
fn read_records(text: &str, delim: char) -> Result<Vec<Record>, PhoskError> {
    let mut records = Vec::new();
    let mut chars = text.chars().peekable();
    let mut row = 0;
    while chars.peek().is_some() {
        row += 1;
        if row > MAX_ROWS {
            return Err(PhoskError::Invalid(format!(
                "CSV file exceeds {MAX_ROWS} rows"
            )));
        }
        let mut fields = vec![String::new()];
        let mut bad: Option<&'static str> = None;
        let mut quoted = false; // inside a quoted field
        let mut after_quote = false; // a quoted field just closed
        loop {
            let Some(c) = chars.next() else {
                if quoted {
                    return Err(PhoskError::Invalid(format!(
                        "unterminated quoted field starting at row {row}"
                    )));
                }
                break;
            };
            if quoted {
                if c == '"' {
                    if chars.peek() == Some(&'"') {
                        chars.next();
                        push(&mut fields, c);
                    } else {
                        quoted = false;
                        after_quote = true;
                    }
                } else {
                    push(&mut fields, c);
                }
                continue;
            }
            match c {
                '\n' => break,
                '\r' => {
                    if chars.peek() == Some(&'\n') {
                        chars.next();
                    }
                    break;
                }
                c if c == delim => {
                    after_quote = false;
                    if fields.len() >= MAX_FIELDS {
                        bad = Some("too many fields");
                    } else {
                        fields.push(String::new());
                    }
                }
                '"' if !after_quote && fields.last().is_some_and(String::is_empty) => {
                    quoted = true;
                }
                c => {
                    if after_quote {
                        bad = Some("text after a closing quote");
                    }
                    push(&mut fields, c);
                }
            }
        }
        let blank = fields.len() == 1 && fields[0].is_empty() && bad.is_none() && !after_quote;
        if !blank {
            records.push(Record {
                row,
                fields: bad.map_or(Ok(fields), Err),
            });
        }
    }
    Ok(records)
}

/// Append to the current field, capping it just past the limit (the row is
/// then rejected by [`parse_row`] without holding the whole overlong field).
fn push(fields: &mut [String], c: char) {
    if let Some(f) = fields.last_mut()
        && f.len() <= MAX_FIELD_BYTES
    {
        f.push(c);
    }
}
