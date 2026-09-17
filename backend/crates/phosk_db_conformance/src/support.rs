//! Shared helpers: panic-free assertions and synthetic fixtures.
//!
//! Assertions return a [`Failure`] instead of panicking, so the library stays
//! within the workspace `unwrap`/`expect`/`panic` denials; the generated test
//! returns it and the harness prints the message. Fixture floats use exactly
//! representable values (`0.5`, `2.0`, …) so a write→read round trip can be
//! compared with `==` on the whole struct.

use std::fmt::Debug;

use chrono::NaiveDate;
use phosk_core::error::PhoskError;
use phosk_core::money::Money;
use phosk_id::{LineItemId, ReceiptId};
use phosk_model::{LineItem, Provenance, Receipt, Source};

/// Why a check failed: the first failing assertion, or an unexpected port error.
#[derive(Debug)]
pub struct Failure(pub String);

impl From<PhoskError> for Failure {
    fn from(e: PhoskError) -> Self {
        Self(format!("unexpected port error: {e}"))
    }
}

impl From<&str> for Failure {
    fn from(msg: &str) -> Self {
        Self(msg.to_owned())
    }
}

/// The result every check returns.
pub type Outcome = Result<(), Failure>;

/// A calendar date, or a failure naming it.
pub fn date(y: i32, m: u32, d: u32) -> Result<NaiveDate, Failure> {
    NaiveDate::from_ymd_opt(y, m, d).ok_or_else(|| Failure(format!("bad date {y}-{m}-{d}")))
}

/// Fail with `what` unless `cond` holds.
pub fn ensure(cond: bool, what: &str) -> Outcome {
    if cond { Ok(()) } else { Err(what.into()) }
}

/// Fail unless `got == want`, showing both values.
pub fn ensure_eq<T: PartialEq + Debug>(got: &T, want: &T, what: &str) -> Outcome {
    if got == want {
        return Ok(());
    }
    Err(Failure(format!("{what}: expected {want:?}, got {got:?}")))
}

/// Fail unless `res` is a [`PhoskError::NotFound`].
pub fn ensure_not_found<T: Debug>(res: Result<T, PhoskError>, what: &str) -> Outcome {
    match res {
        Err(PhoskError::NotFound(_)) => Ok(()),
        other => Err(Failure(format!("{what}: expected NotFound, got {other:?}"))),
    }
}

/// Fail unless `res` is a [`PhoskError::Invalid`].
pub fn ensure_invalid<T: Debug>(res: Result<T, PhoskError>, what: &str) -> Outcome {
    match res {
        Err(PhoskError::Invalid(_)) => Ok(()),
        other => Err(Failure(format!("{what}: expected Invalid, got {other:?}"))),
    }
}

/// The first element of a seeded list, used as a template for new records.
pub fn first<T>(items: Vec<T>) -> Result<T, Failure> {
    items
        .into_iter()
        .next()
        .ok_or_else(|| "the seeded store has no such record".into())
}

/// Sort by a string key so two reads with unspecified order can be compared.
pub fn sorted<T, F: Fn(&T) -> String>(mut items: Vec<T>, key: F) -> Vec<T> {
    items.sort_by_key(|x| key(x));
    items
}

/// A synthetic photo receipt (not part of the seed).
pub fn receipt(id: ReceiptId, slug: &str, day: NaiveDate, cents: i64) -> Receipt {
    Receipt {
        id,
        slug: slug.to_owned(),
        shop: "Conformance Market".to_owned(),
        date: day,
        category: "Groceries".to_owned(),
        amount: Money::from_centimes(cents),
        fixed: false,
        provenance: Provenance {
            source: Source::Ocr,
            confidence: 0.75,
        },
        source_kind: "PHOTO".to_owned(),
        ocr_engine: "PADDLEOCR".to_owned(),
        ocr_regions: 3,
    }
}

/// A synthetic line item bound to `receipt_id`.
pub fn line(receipt_id: ReceiptId, name: &str, cents: i64) -> LineItem {
    LineItem {
        id: LineItemId::new(),
        receipt_id,
        name: name.to_owned(),
        qty: 2.0,
        unit_price: Money::from_centimes(cents / 2),
        line_total: Money::from_centimes(cents),
        category: "Groceries".to_owned(),
        signal_id: None,
        provenance: Provenance {
            source: Source::Ocr,
            confidence: 0.5,
        },
    }
}
