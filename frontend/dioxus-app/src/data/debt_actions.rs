//! Debt + personal-IOU write path (T39).
//!
//! The Debts page's forms send what the user TYPED — CHF amounts, an APR in
//! percent, a day and a term — as plain text. Everything is parsed and checked
//! here, on the server: money becomes exact `i64` centimes via
//! `Money::parse_chf` (no float), the APR percentage becomes the `0..=1` rate
//! `phosk_debts` stores. The writes are `phosk_debts::debt_write` and
//! `phosk_debts::iou_write`.
//!
//! Every failure reaches the page as one of the fixed texts in [`msg`]: a
//! `PhoskError` (ids, centime figures, store details) never does. Each
//! `#[server]` fn only builds the session and delegates to its `*_with` inner
//! fn, which is what the tests drive against a fresh `MemoryDb`.
//!
//! Which actions a row offers (a payment on a paid-off debt, settling a
//! settled IOU) is decided server-side too, and travels on the read DTOs as
//! `actions` (see [`crate::data::debts`]).

use dioxus::prelude::*;
use serde::{Deserialize, Serialize};

/// The debt types the create/edit form offers — `phosk_debts`'s accepted
/// kinds (the WASM client cannot depend on that crate; a test asserts the two
/// lists are equal, so they cannot drift apart in either direction).
pub const DEBT_KINDS: [&str; 6] = ["LEASE", "LOAN", "CARD", "TAX", "BNPL", "MEDICAL"];

/// The debt create/edit form, exactly as typed. Parsed server-side.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DebtForm {
    /// Debt name (the id is derived from it once, at creation).
    pub name: String,
    /// Lender.
    pub lender: String,
    /// One of [`DEBT_KINDS`].
    pub kind: String,
    /// Outstanding balance, CHF text. Read on create only: afterwards the
    /// balance moves through payments, so a stale edit form cannot undo one.
    pub balance: String,
    /// Original amount, CHF text.
    pub orig: String,
    /// Scheduled monthly payment, CHF text. Create only, like `apr`, `day`
    /// and `term`: afterwards the plan and the rate change through T17's
    /// `phosk_debts::debt_plan` rules, which this form does not run.
    pub monthly: String,
    /// APR in percent, e.g. `"4.9"`. Create only.
    pub apr: String,
    /// Payment day of month, `1..=31`. Create only.
    pub day: String,
    /// Term in months (`0` = revolving). Create only.
    pub term: String,
    /// Free note.
    pub note: String,
}

/// The personal-IOU create/edit form, exactly as typed. Parsed server-side.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IouForm {
    /// `"in"` (owed to you) or `"out"` (you owe).
    pub dir: String,
    /// The other person.
    pub person: String,
    /// The ORIGINAL amount, CHF text. Editing it keeps what was repaid.
    pub amount: String,
    /// What it was for.
    pub reason: String,
}

/// Shown when an action fails before the server could answer.
const UNREACHABLE: &str = "Could not reach the server, nothing was saved.";

/// The text to show for a failed action: the server's own (fixed, page-safe)
/// message, or a generic line when the request never got an answer.
pub fn action_error_text(err: &ServerFnError) -> String {
    match err {
        ServerFnError::ServerError { message, .. } if !message.is_empty() => message.clone(),
        _ => UNREACHABLE.to_string(),
    }
}

/// Declares a `#[server]` fn that builds the session and delegates to the
/// named `*_with` inner fn with the same arguments.
macro_rules! delegate {
    ($(#[$doc:meta])* $name:ident => $inner:ident ( $($arg:ident : $ty:ty),* ) -> $ret:ty) => {
        $(#[$doc])*
        #[server]
        pub async fn $name($($arg: $ty),*) -> Result<$ret, ServerFnError> {
            #[cfg(feature = "server-deps")]
            {
                // Session detail (paths, store errors) never reaches the page.
                let session = crate::data::build_session()
                    .await
                    .map_err(|_| ServerFnError::new(msg::RETRY))?;
                $inner(session.db(), $($arg),*).await
            }
            #[cfg(not(feature = "server-deps"))]
            {
                let _ = ($($arg,)*);
                Err(ServerFnError::new("server-only"))
            }
        }
    };
}

delegate!(
    /// Create a debt; returns its id. REAL: `phosk_debts::debt_write::create_debt`.
    create_debt => create_debt_with(form: DebtForm) -> String
);
delegate!(
    /// Edit a debt. REAL: `phosk_debts::debt_write::edit_debt`.
    edit_debt => edit_debt_with(id: String, form: DebtForm) -> ()
);
delegate!(
    /// Delete a debt and its payments. REAL: `phosk_debts::debt_write::delete_debt`.
    delete_debt => delete_debt_with(id: String) -> ()
);
delegate!(
    /// Record the scheduled instalment. REAL: `phosk_debts::debt_write::record_payment`.
    pay_debt => pay_debt_with(id: String, amount: String) -> ()
);
delegate!(
    /// Record an extra (principal-only) payment. REAL: `phosk_debts::debt_write::extra_payment`.
    pay_debt_extra => pay_debt_extra_with(id: String, amount: String) -> ()
);
delegate!(
    /// Create a personal IOU; returns its id. REAL: `phosk_debts::iou_write::create_personal_iou`.
    create_iou => create_iou_with(form: IouForm) -> String
);
delegate!(
    /// Edit a personal IOU. REAL: `phosk_debts::iou_write::edit_personal_iou`.
    edit_iou => edit_iou_with(id: String, form: IouForm) -> ()
);
delegate!(
    /// Delete a personal IOU. REAL: `phosk_debts::iou_write::delete_personal_iou`.
    delete_iou => delete_iou_with(id: String) -> ()
);
delegate!(
    /// Record a partial repayment. REAL: `phosk_debts::iou_write::record_iou_payment`.
    pay_iou => pay_iou_with(id: String, amount: String) -> ()
);
delegate!(
    /// Settle what is left. REAL: `phosk_debts::iou_write::settle_personal_iou`.
    settle_iou => settle_iou_with(id: String) -> ()
);

// ── inner fns (server only) ───────────────────────────────────────────────────

#[cfg(feature = "server-deps")]
pub(crate) use inner::*;

#[cfg(feature = "server-deps")]
mod inner {
    use dioxus::prelude::ServerFnError;
    use phosk_adapter_db::DatabaseAdapter;
    use phosk_core::error::PhoskError;
    use phosk_core::money::Money;
    use phosk_debts::debt_write::{self, DebtEdit, NewDebt, NewDebtPayment};
    use phosk_debts::iou_write::{self, NewPersonalIou, PersonalIouEdit};

    use super::{msg, DebtForm, IouForm, DEBT_KINDS};

    /// Largest amount any field accepts: CHF 100'000'000.00.
    const MAX_AMOUNT: Money = Money::from_centimes(10_000_000_000);
    /// Longest id accepted. A slug is at most as long as the name it comes
    /// from ([`MAX_NAME`]) plus an IOU's `-<n>` suffix, so this leaves room.
    const MAX_ID_LEN: usize = 80;
    /// Longest name, lender or person, in characters.
    const MAX_NAME: usize = 60;
    /// Longest note or reason, in characters.
    const MAX_TEXT: usize = 280;

    /// A fixed failure text.
    fn fail(text: &'static str) -> ServerFnError {
        ServerFnError::new(text)
    }

    /// Map a service failure onto a fixed text: `gone` for a missing record,
    /// `invalid` for a refused input. The `PhoskError`'s own detail is dropped.
    fn store_error(gone: &'static str, invalid: &'static str, e: &PhoskError) -> ServerFnError {
        match e {
            PhoskError::NotFound(_) => fail(gone),
            PhoskError::Invalid(_) => fail(invalid),
            PhoskError::InvalidDate(_) | PhoskError::Overflow(_) => fail(msg::RETRY),
        }
    }

    /// A well-formed slug (`[a-z0-9-]`, bounded), else `gone`: a malformed id
    /// cannot name a record.
    fn slug<'a>(gone: &'static str, id: &'a str) -> Result<&'a str, ServerFnError> {
        let ok = !id.is_empty()
            && id.len() <= MAX_ID_LEN
            && id
                .bytes()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-');
        if ok {
            Ok(id)
        } else {
            Err(fail(gone))
        }
    }

    /// CHF text → exact centimes in `0..=MAX_AMOUNT`, else `bad`.
    fn amount(text: &str, bad: &'static str) -> Result<Money, ServerFnError> {
        match Money::parse_chf(text) {
            Ok(m) if m >= Money::ZERO && m <= MAX_AMOUNT => Ok(m),
            _ => Err(fail(bad)),
        }
    }

    /// APR percent text (up to two decimals, `0..=100`) → the `0..=1` rate
    /// `phosk_debts` stores. Parsed as integer basis points first, so `4.9`
    /// becomes exactly the rate nearest 0.049.
    fn apr(text: &str) -> Result<f64, ServerFnError> {
        let t = text.trim().trim_end_matches('%').trim_end();
        let (whole, frac) = t.split_once('.').unwrap_or((t, ""));
        let digits = |s: &str| !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit());
        if !digits(whole)
            || (!frac.is_empty() && !digits(frac))
            || frac.len() > 2
            || whole.len() > 3
        {
            return Err(fail(msg::APR));
        }
        let whole: u32 = whole.parse().map_err(|_| fail(msg::APR))?;
        let frac: u32 = format!("{frac:0<2}").parse().map_err(|_| fail(msg::APR))?;
        let bp = whole * 100 + frac;
        if bp > 10_000 {
            return Err(fail(msg::APR));
        }
        Ok(f64::from(bp) / 10_000.0)
    }

    /// `text` unless it is longer than `max` characters, else `bad`.
    fn bounded(text: String, max: usize, bad: &'static str) -> Result<String, ServerFnError> {
        if text.chars().count() <= max {
            Ok(text)
        } else {
            Err(fail(bad))
        }
    }

    /// A small whole number in `range`, else `bad`.
    fn number(
        text: &str,
        range: std::ops::RangeInclusive<u32>,
        bad: &'static str,
    ) -> Result<u32, ServerFnError> {
        match text.trim().parse::<u32>() {
            Ok(n) if range.contains(&n) => Ok(n),
            _ => Err(fail(bad)),
        }
    }

    /// The fields both create and edit write, parsed and bounded.
    struct DebtFields {
        name: String,
        lender: String,
        kind: String,
        orig: Money,
        note: String,
    }

    fn debt_fields(f: DebtForm) -> Result<DebtFields, ServerFnError> {
        let kind = f.kind.trim().to_ascii_uppercase();
        if !DEBT_KINDS.contains(&kind.as_str()) {
            return Err(fail(msg::KIND));
        }
        Ok(DebtFields {
            name: bounded(f.name, MAX_NAME, msg::NAME)?,
            lender: bounded(f.lender, MAX_NAME, msg::LENDER)?,
            orig: amount(&f.orig, msg::ORIG)?,
            note: bounded(f.note, MAX_TEXT, msg::NOTE)?,
            kind,
        })
    }

    /// The card glyph for a new debt of `kind` (the seed's set).
    fn glyph(kind: &str) -> &'static str {
        match kind {
            "LEASE" => "⊟",
            "CARD" => "▭",
            "LOAN" => "▤",
            "TAX" => "§",
            _ => "◇",
        }
    }

    pub(crate) async fn create_debt_with(
        db: &dyn DatabaseAdapter,
        form: DebtForm,
    ) -> Result<String, ServerFnError> {
        let balance = amount(&form.balance, msg::BALANCE)?;
        let monthly = amount(&form.monthly, msg::MONTHLY)?;
        let apr = apr(&form.apr)?;
        let day = number(&form.day, 1..=31, msg::DAY)?;
        let term = number(&form.term, 0..=1200, msg::TERM)?;
        let p = debt_fields(form)?;
        let input = NewDebt {
            name: p.name,
            lender: p.lender,
            glyph: glyph(&p.kind).to_owned(),
            kind: p.kind,
            balance,
            orig: p.orig,
            monthly,
            apr,
            day,
            term,
            since: crate::data::today(),
            note: p.note,
        };
        debt_write::create_debt(db, input)
            .await
            .map_err(|e| store_error(msg::DEBT_GONE, msg::DEBT_INVALID, &e))
    }

    pub(crate) async fn edit_debt_with(
        db: &dyn DatabaseAdapter,
        id: String,
        form: DebtForm,
    ) -> Result<(), ServerFnError> {
        let id = slug(msg::DEBT_GONE, &id)?;
        let p = debt_fields(form)?;
        // Balance, plan (monthly / day / term), APR, status, glyph and since
        // stay as they are (see `DebtForm`): nothing here re-sends them.
        let edit = DebtEdit {
            name: Some(p.name),
            lender: Some(p.lender),
            kind: Some(p.kind),
            orig: Some(p.orig),
            note: Some(p.note),
            ..DebtEdit::default()
        };
        debt_write::edit_debt(db, id, edit)
            .await
            .map_err(|e| store_error(msg::DEBT_GONE, msg::DEBT_INVALID, &e))
    }

    pub(crate) async fn delete_debt_with(
        db: &dyn DatabaseAdapter,
        id: String,
    ) -> Result<(), ServerFnError> {
        let id = slug(msg::DEBT_GONE, &id)?;
        debt_write::delete_debt(db, id)
            .await
            .map_err(|e| store_error(msg::DEBT_GONE, msg::RETRY, &e))
    }

    pub(crate) async fn pay_debt_with(
        db: &dyn DatabaseAdapter,
        id: String,
        amount_text: String,
    ) -> Result<(), ServerFnError> {
        let id = slug(msg::DEBT_GONE, &id)?;
        let payment = NewDebtPayment {
            date: crate::data::today(),
            amount: amount(&amount_text, msg::PAYMENT)?,
        };
        debt_write::record_payment(db, id, payment)
            .await
            .map(drop)
            .map_err(|e| store_error(msg::DEBT_GONE, msg::PAYMENT_RANGE, &e))
    }

    pub(crate) async fn pay_debt_extra_with(
        db: &dyn DatabaseAdapter,
        id: String,
        amount_text: String,
    ) -> Result<(), ServerFnError> {
        let id = slug(msg::DEBT_GONE, &id)?;
        let payment = NewDebtPayment {
            date: crate::data::today(),
            amount: amount(&amount_text, msg::PAYMENT)?,
        };
        debt_write::extra_payment(db, id, payment)
            .await
            .map(drop)
            .map_err(|e| store_error(msg::DEBT_GONE, msg::PAYMENT_RANGE, &e))
    }

    /// `"in"` / `"out"`, else the fixed direction text.
    fn dir(text: &str) -> Result<String, ServerFnError> {
        let d = text.trim().to_ascii_lowercase();
        if d == "in" || d == "out" {
            Ok(d)
        } else {
            Err(fail(msg::DIR))
        }
    }

    pub(crate) async fn create_iou_with(
        db: &dyn DatabaseAdapter,
        form: IouForm,
    ) -> Result<String, ServerFnError> {
        let input = NewPersonalIou {
            dir: dir(&form.dir)?,
            amount: amount(&form.amount, msg::IOU_AMOUNT)?,
            person: bounded(form.person, MAX_NAME, msg::PERSON)?,
            // Blank: the service derives the initials from the name.
            initials: String::new(),
            reason: bounded(form.reason, MAX_TEXT, msg::REASON)?,
            since: crate::data::today(),
        };
        iou_write::create_personal_iou(db, input)
            .await
            .map_err(|e| store_error(msg::IOU_GONE, msg::IOU_INVALID, &e))
    }

    pub(crate) async fn edit_iou_with(
        db: &dyn DatabaseAdapter,
        id: String,
        form: IouForm,
    ) -> Result<(), ServerFnError> {
        let id = slug(msg::IOU_GONE, &id)?;
        let edit = PersonalIouEdit {
            dir: Some(dir(&form.dir)?),
            of: Some(amount(&form.amount, msg::IOU_AMOUNT)?),
            person: Some(bounded(form.person, MAX_NAME, msg::PERSON)?),
            reason: Some(bounded(form.reason, MAX_TEXT, msg::REASON)?),
            ..PersonalIouEdit::default()
        };
        iou_write::edit_personal_iou(db, id, edit)
            .await
            .map_err(|e| store_error(msg::IOU_GONE, msg::IOU_INVALID, &e))
    }

    pub(crate) async fn delete_iou_with(
        db: &dyn DatabaseAdapter,
        id: String,
    ) -> Result<(), ServerFnError> {
        let id = slug(msg::IOU_GONE, &id)?;
        iou_write::delete_personal_iou(db, id)
            .await
            .map_err(|e| store_error(msg::IOU_GONE, msg::RETRY, &e))
    }

    pub(crate) async fn pay_iou_with(
        db: &dyn DatabaseAdapter,
        id: String,
        amount_text: String,
    ) -> Result<(), ServerFnError> {
        let id = slug(msg::IOU_GONE, &id)?;
        let payment = amount(&amount_text, msg::PAYMENT)?;
        iou_write::record_iou_payment(db, id, payment)
            .await
            .map(drop)
            .map_err(|e| store_error(msg::IOU_GONE, msg::PAYMENT_RANGE, &e))
    }

    pub(crate) async fn settle_iou_with(
        db: &dyn DatabaseAdapter,
        id: String,
    ) -> Result<(), ServerFnError> {
        let id = slug(msg::IOU_GONE, &id)?;
        iou_write::settle_personal_iou(db, id)
            .await
            .map(drop)
            .map_err(|e| store_error(msg::IOU_GONE, msg::RETRY, &e))
    }
}

/// The fixed, user-facing failure texts. None repeats the input or a figure.
#[cfg(feature = "server-deps")]
mod msg {
    pub(super) const BALANCE: &str = "Balance: enter an amount in CHF, like 1250.50.";
    pub(super) const ORIG: &str = "Original amount: enter an amount in CHF, like 1250.50.";
    pub(super) const MONTHLY: &str = "Monthly payment: enter an amount in CHF, like 1250.50.";
    pub(super) const APR: &str = "APR: enter a percentage from zero to one hundred, like 4.9.";
    pub(super) const DAY: &str = "Payment day: enter a day of the month.";
    pub(super) const TERM: &str = "Term: enter a whole number of months, zero if revolving.";
    pub(super) const NAME: &str = "Name: use at most sixty characters.";
    pub(super) const LENDER: &str = "Lender: use at most sixty characters.";
    pub(super) const NOTE: &str = "Note: use at most two hundred eighty characters.";
    pub(super) const PERSON: &str = "Person: use at most sixty characters.";
    pub(super) const REASON: &str = "Reason: use at most two hundred eighty characters.";
    pub(super) const KIND: &str = "Type: pick one of the listed debt types.";
    pub(super) const DEBT_INVALID: &str = "Could not save the debt: the name must be new and not \
        blank, the lender filled in, the original amount above zero and the balance no more than \
        the original.";
    pub(super) const DIR: &str = "Direction: pick owed to you or you owe.";
    pub(super) const IOU_AMOUNT: &str = "Amount: enter an amount in CHF, like 1250.50.";
    pub(super) const IOU_INVALID: &str = "Could not save the IOU: the name must not be blank and \
        the amount must be above zero and no less than what was already repaid.";
    pub(super) const PAYMENT: &str = "Payment: enter an amount in CHF, like 1250.50.";
    pub(super) const PAYMENT_RANGE: &str =
        "The payment must be above zero and no more than what is still owed.";
    pub(super) const DEBT_GONE: &str = "This debt no longer exists.";
    pub(super) const IOU_GONE: &str = "This IOU no longer exists.";
    pub(super) const RETRY: &str = "Could not save, please try again.";
}
