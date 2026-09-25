//! Manual entry of a new transaction (T36): the wire types of the "NEW"
//! form on the Transactions page and the `#[server]` fn that saves it.
//!
//! The form sends raw text. The server parses it (date, CHF amounts to exact
//! centimes without a float, quantities) and hands the result to
//! `phosk_ledger::transactions::create_transaction`, which validates the entry,
//! derives every money value and stamps `UserEntered` provenance. The logic lives
//! in [`create_transaction_with`], which takes the database port, so tests drive
//! it against a fresh `MemoryDb` and never touch the process-global stack.

use dioxus::prelude::*;
use phosk_core::money::Money;
use serde::{Deserialize, Serialize};

/// One line item as typed into the form.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NewTxnLineForm {
    /// Item name.
    pub name: String,
    /// Quantity as typed (`"2"`, `"0.5"`, `"0,5"`).
    pub qty: String,
    /// Unit price in CHF as typed (`"4.20"`, `"CHF 1'234.50"`).
    pub unit_price: String,
    /// Line category; empty = the receipt's category.
    pub category: String,
}

/// The "NEW transaction" form as typed. Mirrors
/// `phosk_ledger::transactions::NewTransaction`: with `lines` empty the entry
/// is total-only and `total` is required; with lines the total is derived and
/// a non-blank `total` is only a cross-check.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NewTxnForm {
    /// Shop name.
    pub shop: String,
    /// Date as the date input sends it (`YYYY-MM-DD`).
    pub date: String,
    /// Primary category.
    pub category: String,
    /// `true` for a standing/fixed charge.
    pub fixed: bool,
    /// Receipt total in CHF as typed; blank = none.
    pub total: String,
    /// Line items; empty = a total-only entry.
    pub lines: Vec<NewTxnLineForm>,
}

/// What a saved entry became (mirrors `phosk_ledger`'s `CreatedTxnDto`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreatedTxnDto {
    /// The new receipt's stable id (slug), as the list returns it.
    pub id: String,
    /// The stored total, exact centimes.
    #[serde(with = "phosk_model::money_centimes")]
    pub amount: Money,
    /// Number of stored line items.
    pub item_count: u32,
}

/// Save a manually entered transaction.
///
/// REAL: `create_transaction_with` parses the typed text, then composes
/// `phosk_ledger::transactions::create_transaction` (validation, derived
/// totals, `UserEntered` provenance). Failures from the entry itself carry a
/// short message written for the page (see [`create_error_text`]); a failure
/// to open the store (`build_session`) passes its own text through for now.
#[server]
pub async fn create_transaction(form: NewTxnForm) -> Result<CreatedTxnDto, ServerFnError> {
    #[cfg(feature = "server-deps")]
    {
        let session = crate::data::build_session().await?;
        create_transaction_with(session.db(), form).await
    }
    #[cfg(not(feature = "server-deps"))]
    {
        let _ = form;
        Err(ServerFnError::new("server-only"))
    }
}

/// The text to show for a failed save: the server's own message, or a generic
/// line when the server could not be reached.
#[must_use]
pub fn create_error_text(err: &ServerFnError) -> String {
    match err {
        ServerFnError::ServerError { message, .. } if !message.is_empty() => message.clone(),
        _ => "Could not reach the server, transaction not saved.".to_owned(),
    }
}

/// The logic behind [`create_transaction`], driven through the database port.
///
/// Order: the typed text is parsed and checked first ([`entry::parse`], 400,
/// nothing written), then `create_transaction` validates again and writes.
///
/// # Errors
/// A `ServerFnError` whose message is page-safe: one of the field messages of
/// [`entry`], or a fixed line for a ledger/store refusal. Adapter errors can
/// carry engine detail (file paths, driver text), so this function never
/// forwards their text.
#[cfg(feature = "server-deps")]
pub(crate) async fn create_transaction_with(
    db: &dyn phosk_adapter_db::DatabaseAdapter,
    form: NewTxnForm,
) -> Result<CreatedTxnDto, ServerFnError> {
    let input = entry::parse(&form).map_err(|m| entry::reply(400, &m))?;
    let created = phosk_ledger::transactions::create_transaction(db, input)
        .await
        .map_err(entry::failed)?;
    Ok(CreatedTxnDto {
        id: created.id,
        amount: created.amount,
        item_count: created.item_count,
    })
}

/// Parsing and page-safe checks for [`create_transaction`].
///
/// Labels, quantities and unit prices go through the same checks as the line
/// editor (`transactions::line_fix`: trimmed, bounded, no control characters,
/// quantity in (0, 100000], unit price at most CHF 1'000'000), so a line this
/// form stores is one that editor would accept. On top: the shop is bounded
/// like an item name, a stated total is capped at [`MAX_TOTAL_CENTIMES`] and an
/// entry has at most [`MAX_LINES`] lines. With those caps the derived total is
/// at most 500 × 100000 × CHF 1'000'000 (5·10¹⁵ centimes), far inside `i64`, so
/// the ledger's `Overflow` cannot be reached from this form. The ledger stays
/// the authority and re-checks everything.
#[cfg(feature = "server-deps")]
mod entry {
    use chrono::NaiveDate;
    use dioxus::prelude::ServerFnError;
    use phosk_core::error::PhoskError;
    use phosk_core::money::Money;
    use phosk_ledger::transactions::{NewLineInput, NewTransaction};

    use super::{NewTxnForm, NewTxnLineForm};
    use crate::data::transactions::line_fix::{
        check_qty, check_unit_price, label, parse_qty, MAX_CATEGORY_CHARS, MAX_NAME_CHARS,
    };

    /// Most line items one entry may carry.
    pub(super) const MAX_LINES: usize = 500;
    /// Largest accepted stated total: CHF 1'000'000, in centimes.
    pub(super) const MAX_TOTAL_CENTIMES: i64 = 100_000_000;

    /// The typed form → the ledger's input, or the first problem found.
    pub(super) fn parse(form: &NewTxnForm) -> Result<NewTransaction, String> {
        let shop = label(&form.shop, "Shop", MAX_NAME_CHARS).map_err(hint)?;
        let date = NaiveDate::parse_from_str(form.date.trim(), "%Y-%m-%d")
            .map_err(|_| "Date: pick a date.".to_owned())?;
        let category = label(&form.category, "Category", MAX_CATEGORY_CHARS).map_err(hint)?;
        let amount = match form.total.trim() {
            "" => None,
            raw => Some(total(raw).map_err(|m| format!("Total: {m}"))?),
        };
        if form.lines.len() > MAX_LINES {
            return Err(format!("Too many line items (at most {MAX_LINES})."));
        }
        let lines = form
            .lines
            .iter()
            .enumerate()
            .map(|(i, l)| line(l).map_err(|m| format!("Line {}: {m}", i + 1)))
            .collect::<Result<Vec<_>, _>>()?;
        if lines.is_empty() && amount.is_none() {
            return Err("Enter a total or at least one line item.".to_owned());
        }
        Ok(NewTransaction {
            shop,
            date,
            category,
            fixed: form.fixed,
            amount,
            lines,
        })
    }

    fn line(l: &NewTxnLineForm) -> Result<NewLineInput, String> {
        let name = label(&l.name, "Item name", MAX_NAME_CHARS).map_err(hint)?;
        let qty = parse_qty(&l.qty).and_then(check_qty).map_err(hint)?;
        let unit_price = chf(&l.unit_price).map_err(|m| format!("Unit price: {m}"))?;
        check_unit_price(unit_price.centimes()).map_err(hint)?;
        let category = match l.category.trim() {
            "" => String::new(),
            raw => label(raw, "Category", MAX_CATEGORY_CHARS).map_err(hint)?,
        };
        Ok(NewLineInput {
            name,
            qty,
            unit_price,
            category,
            signal_id: String::new(),
        })
    }

    /// A stated total: CHF text, not negative, at most [`MAX_TOTAL_CENTIMES`].
    fn total(raw: &str) -> Result<Money, String> {
        let amount = chf(raw)?;
        if amount.centimes() > MAX_TOTAL_CENTIMES {
            return Err("too large (at most CHF 1'000'000).".to_owned());
        }
        Ok(amount)
    }

    /// Typed CHF text → exact centimes (`Money::parse_chf`, no float), not
    /// negative.
    fn chf(raw: &str) -> Result<Money, String> {
        let amount = Money::parse_chf(raw).map_err(hint)?;
        if amount < Money::ZERO {
            return Err("cannot be negative.".to_owned());
        }
        Ok(amount)
    }

    /// The text of a check's refusal. The parsers and `line_fix` checks return
    /// fixed hints that never repeat the input.
    fn hint(err: PhoskError) -> String {
        match err {
            PhoskError::Invalid(hint) => hint,
            _ => "not a valid number.".to_owned(),
        }
    }

    pub(super) fn reply(code: u16, message: &str) -> ServerFnError {
        ServerFnError::ServerError {
            message: message.to_owned(),
            code,
            details: None,
        }
    }

    /// A ledger or store failure, mapped by kind; its text is never forwarded.
    pub(super) fn failed(err: PhoskError) -> ServerFnError {
        match err {
            PhoskError::Invalid(_) => reply(
                400,
                "The transaction was refused. Check the amounts and try again.",
            ),
            PhoskError::Overflow(_) => reply(400, "The amounts are too large."),
            PhoskError::NotFound(_) | PhoskError::InvalidDate(_) => {
                reply(500, "The transaction could not be saved. Try again.")
            }
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        fn message(err: &ServerFnError) -> (u16, &str) {
            match err {
                ServerFnError::ServerError { code, message, .. } => (*code, message.as_str()),
                other => panic!("expected a server error, got {other:?}"),
            }
        }

        /// Every ledger/store failure maps to a fixed line; its detail is dropped.
        #[test]
        fn a_ledger_failure_never_forwards_its_text() {
            let secret = "C:/Users/me/db.surreal: driver said no";
            let cases = [
                (
                    PhoskError::Invalid(secret.into()),
                    400,
                    "The transaction was refused. Check the amounts and try again.",
                ),
                (
                    PhoskError::Overflow(secret.into()),
                    400,
                    "The amounts are too large.",
                ),
                (
                    PhoskError::NotFound(secret.into()),
                    500,
                    "The transaction could not be saved. Try again.",
                ),
            ];
            for (err, code, text) in cases {
                assert_eq!(message(&failed(err)), (code, text));
            }
        }
    }
}
