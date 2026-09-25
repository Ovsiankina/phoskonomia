//! EDIT and DELETE of a recorded transaction from the receipt detail (T37):
//! the wire types and the `#[server]` fns.
//!
//! The form sends raw text, like the "NEW transaction" form (T36), and goes
//! through the same parsers and caps (`new_transaction::entry`,
//! `transactions::line_fix`). The logic lives in [`edit_transaction_with`] and
//! [`delete_transaction_with`], which take the database port, so tests drive
//! them against a fresh `MemoryDb` and never touch the process-global stack.
//! Every refusal carries a fixed, page-safe message; ledger and adapter text is
//! never forwarded.

use dioxus::prelude::*;
use phosk_core::money::Money;
use serde::{Deserialize, Serialize};

/// The EDIT form as typed, pre-filled from the detail. Mirrors
/// `phosk_ledger::transactions::TxnEdit`: a value equal to the stored one is
/// not a change, so an untouched form is a no-op.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EditTxnForm {
    /// The receipt's slug, as the list returns it.
    pub id: String,
    /// Shop name.
    pub shop: String,
    /// New date as the date input sends it (`YYYY-MM-DD`); blank = keep.
    pub date: String,
    /// Primary category.
    pub category: String,
    /// `true` for a standing/fixed charge.
    pub fixed: bool,
    /// New total in CHF as typed; blank = keep. Refused on an itemised
    /// receipt, whose total is derived from its lines.
    pub total: String,
}

/// What a saved edit produced (mirrors `phosk_ledger`'s `EditedTxnDto`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EditedTxnDto {
    /// The receipt's slug — unchanged by an edit.
    pub id: String,
    /// The stored total after the edit, exact centimes.
    #[serde(with = "phosk_model::money_centimes")]
    pub amount: Money,
    /// The stored date's display label after the edit (`"12 JUN"`), exactly as
    /// `phosk_ledger`'s `date_label` formats it — including when the edit left
    /// the date untouched. The page uses this instead of re-deriving a label
    /// from the pre-edit row, which would go stale the moment the date moves.
    pub date: String,
    /// The field names that changed (empty for a no-op edit).
    pub changed: Vec<String>,
}

/// Save an edit of a recorded transaction.
///
/// REAL: [`edit_transaction_with`] composes
/// `phosk_ledger::transactions::edit_transaction` (`UserModified`
/// provenance, audit log, projection replaced).
#[server]
pub async fn edit_transaction(form: EditTxnForm) -> Result<EditedTxnDto, ServerFnError> {
    #[cfg(feature = "server-deps")]
    {
        let session = crate::data::build_session()
            .await
            .map_err(|_| unavailable())?;
        edit_transaction_with(session.db(), form).await
    }
    #[cfg(not(feature = "server-deps"))]
    {
        let _ = form;
        Err(ServerFnError::new("server-only"))
    }
}

/// Delete a recorded transaction with its lines.
///
/// REAL: [`delete_transaction_with`] composes
/// `phosk_ledger::transactions::delete_transaction`.
#[server]
pub async fn delete_transaction(id: String) -> Result<(), ServerFnError> {
    #[cfg(feature = "server-deps")]
    {
        let session = crate::data::build_session()
            .await
            .map_err(|_| unavailable())?;
        delete_transaction_with(session.db(), id).await
    }
    #[cfg(not(feature = "server-deps"))]
    {
        let _ = id;
        Err(ServerFnError::new("server-only"))
    }
}

/// A receipt no longer found by its slug (deleted, or never existed).
const GONE: &str = "This transaction no longer exists.";
/// An itemised receipt's total was targeted by an amount edit.
const ITEMISED: &str =
    "This transaction is itemised: its total comes from its lines. Correct a line instead.";

/// The text to show for a failed EDIT or DELETE: one of this module's own
/// fixed lines, or `unreachable` for anything else. A `ServerError` can also
/// carry text this module never wrote — dioxus-fullstack's own synthesized
/// messages (an unhandled extractor/middleware failure, an `"HTTP {status}:
/// {body}"` from a raw response) or the `"server-only"` sentinel returned when
/// the `server-deps` feature is off — none of which is page-safe, so only a
/// known message is ever shown verbatim.
#[must_use]
pub fn txn_action_error_text(err: &ServerFnError, unreachable: &str) -> String {
    match err {
        ServerFnError::ServerError { message, .. } if is_known_message(message) => message.clone(),
        _ => unreachable.to_owned(),
    }
}

/// Whether `message` is one of the fixed lines [`edit_transaction_with`] /
/// [`delete_transaction_with`] can produce: [`GONE`], [`ITEMISED`], one of
/// `patch::failed`'s or `unavailable`'s constant lines, or a field hint from
/// `label` / `entry::total` / `parse_date` — those never embed anything the
/// caller typed, only a fixed shape prefixed by the field name.
fn is_known_message(message: &str) -> bool {
    const FIXED: &[&str] = &[
        GONE,
        ITEMISED,
        "The edit was refused. Check the values and try again.",
        "The transaction could not be saved. Try again.",
        "The transaction store is unavailable. Try again.",
        "The transaction could not be deleted. Try again.",
        "Date: pick a date.",
    ];
    FIXED.contains(&message)
        || message.starts_with("Shop ")
        || message.starts_with("Category ")
        || message.starts_with("Total: ")
}

/// The logic behind [`edit_transaction`], driven through the database port.
///
/// Order: the typed text is parsed and checked (400, nothing written), the
/// receipt is resolved (404), a total on an itemised receipt is refused (400),
/// then the ledger validates again and writes.
///
/// # Errors
/// A `ServerFnError` with a fixed, page-safe message.
#[cfg(feature = "server-deps")]
pub(crate) async fn edit_transaction_with(
    db: &dyn phosk_adapter_db::DatabaseAdapter,
    form: EditTxnForm,
) -> Result<EditedTxnDto, ServerFnError> {
    let edit = patch::parse(&form).map_err(|m| reply(400, &m))?;
    let receipt = db.receipt_by_slug(&form.id).await.map_err(patch::failed)?;
    if edit.amount.is_some()
        && !db
            .line_items(receipt.id)
            .await
            .map_err(patch::failed)?
            .is_empty()
    {
        return Err(reply(400, ITEMISED));
    }
    let edited = phosk_ledger::transactions::edit_transaction(db, &form.id, edit)
        .await
        .map_err(patch::failed)?;
    Ok(EditedTxnDto {
        id: edited.id,
        amount: edited.amount,
        date: edited.date,
        changed: edited.changed,
    })
}

/// The logic behind [`delete_transaction`], driven through the database port.
///
/// # Errors
/// 404 for an unknown (or already deleted) id; 500 with a fixed line for a
/// store failure.
#[cfg(feature = "server-deps")]
pub(crate) async fn delete_transaction_with(
    db: &dyn phosk_adapter_db::DatabaseAdapter,
    id: String,
) -> Result<(), ServerFnError> {
    phosk_ledger::transactions::delete_transaction(db, &id)
        .await
        .map_err(|err| match err {
            phosk_core::error::PhoskError::NotFound(_) => reply(404, GONE),
            _ => reply(500, "The transaction could not be deleted. Try again."),
        })
}

#[cfg(feature = "server-deps")]
use crate::data::new_transaction::entry::reply;

/// The store could not be opened. Its error can name paths or driver detail,
/// so it is replaced by a fixed line: every `ServerError` these fns return is
/// page-safe.
#[cfg(feature = "server-deps")]
fn unavailable() -> ServerFnError {
    reply(500, "The transaction store is unavailable. Try again.")
}

/// Parsing and error mapping for [`edit_transaction`]. The field checks are
/// T36's (`label` with the same caps, `entry::total`), so an edit can store
/// only what the NEW form could.
#[cfg(feature = "server-deps")]
mod patch {
    use dioxus::prelude::ServerFnError;
    use phosk_core::error::PhoskError;
    use phosk_ledger::transactions::TxnEdit;

    use super::{reply, EditTxnForm, GONE};
    use crate::data::new_transaction::entry::{hint, total};
    use crate::data::transactions::line_fix::{
        label, parse_date, MAX_CATEGORY_CHARS, MAX_NAME_CHARS,
    };

    /// The typed form → the ledger's patch, or the first problem found.
    pub(super) fn parse(form: &EditTxnForm) -> Result<TxnEdit, String> {
        let shop = label(&form.shop, "Shop", MAX_NAME_CHARS).map_err(hint)?;
        let category = label(&form.category, "Category", MAX_CATEGORY_CHARS).map_err(hint)?;
        let date = match form.date.trim() {
            "" => None,
            raw => Some(parse_date(raw)?),
        };
        let amount = match form.total.trim() {
            "" => None,
            raw => Some(total(raw).map_err(|m| format!("Total: {m}"))?),
        };
        Ok(TxnEdit {
            shop: Some(shop),
            date,
            category: Some(category),
            fixed: Some(form.fixed),
            amount,
        })
    }

    /// A ledger or store failure, mapped by kind; its text is never forwarded.
    pub(super) fn failed(err: PhoskError) -> ServerFnError {
        match err {
            PhoskError::NotFound(_) => reply(404, GONE),
            PhoskError::Invalid(_) => {
                reply(400, "The edit was refused. Check the values and try again.")
            }
            PhoskError::Overflow(_) | PhoskError::InvalidDate(_) => {
                reply(500, "The transaction could not be saved. Try again.")
            }
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        /// Every ledger/store failure maps to a fixed line; its detail is dropped.
        #[test]
        fn a_ledger_failure_never_forwards_its_text() {
            let secret = "C:/Users/me/db.surreal: driver said no";
            for err in [
                PhoskError::Invalid(secret.into()),
                PhoskError::NotFound(secret.into()),
                PhoskError::Overflow(secret.into()),
                PhoskError::InvalidDate(secret.into()),
            ] {
                match failed(err) {
                    ServerFnError::ServerError { message, .. } => {
                        assert!(!message.contains("driver"), "{message}");
                    }
                    other => panic!("expected a server error, got {other:?}"),
                }
            }
        }
    }
}
