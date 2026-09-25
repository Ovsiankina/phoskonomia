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
/// totals, `UserEntered` provenance). Failures carry a short message that is
/// safe to show as-is (see [`create_error_text`]).
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

/// The text to show for a failed save: the server's own message (always
/// user-safe for [`create_transaction`]), or a generic line when the server
/// could not be reached.
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
/// carry engine detail (file paths, driver text), so their text is never
/// forwarded.
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
/// The checks mirror what `create_transaction` refuses (blank shop, category
/// or item name; quantity not above zero; negative amount; neither total nor
/// lines), so the user gets a message naming the field. The ledger stays the
/// authority and re-checks everything.
#[cfg(feature = "server-deps")]
mod entry {
    use chrono::NaiveDate;
    use dioxus::prelude::ServerFnError;
    use phosk_core::error::PhoskError;
    use phosk_core::money::Money;
    use phosk_ledger::transactions::{NewLineInput, NewTransaction};

    use super::{NewTxnForm, NewTxnLineForm};

    /// The typed form → the ledger's input, or the first problem found.
    pub(super) fn parse(form: &NewTxnForm) -> Result<NewTransaction, String> {
        let shop = required(&form.shop, "Shop")?;
        let date = NaiveDate::parse_from_str(form.date.trim(), "%Y-%m-%d")
            .map_err(|_| "Date: pick a date.".to_owned())?;
        let category = required(&form.category, "Category")?;
        let amount = match form.total.trim() {
            "" => None,
            raw => Some(chf(raw).map_err(|m| format!("Total: {m}"))?),
        };
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
        let name = required(&l.name, "Item name")?;
        let qty = crate::data::transactions::line_fix::parse_qty(&l.qty).map_err(hint)?;
        if qty <= 0.0 {
            return Err("Quantity must be greater than zero.".to_owned());
        }
        let unit_price = chf(&l.unit_price).map_err(|m| format!("Unit price: {m}"))?;
        Ok(NewLineInput {
            name,
            qty,
            unit_price,
            category: l.category.trim().to_owned(),
            signal_id: String::new(),
        })
    }

    /// A trimmed, non-empty field.
    fn required(raw: &str, what: &str) -> Result<String, String> {
        match raw.trim() {
            "" => Err(format!("{what} is required.")),
            value => Ok(value.to_owned()),
        }
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

    /// The text of a parser refusal. Both parsers return fixed hints that never
    /// repeat the input.
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
}
