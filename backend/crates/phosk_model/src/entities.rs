//! Domain entities (the persisted shapes behind the `DatabaseAdapter` port).
//!
//! These are the richer write-side concepts that sit ALONGSIDE the read-only
//! dashboard trio (`Transaction`/`Category`/`BudgetConfig` in the crate root —
//! those stay, the dashboard slice depends on them). Each entity here carries
//! BOTH a typed id (the relation key, from [`phosk_id`]) AND a human
//! `name`/`label` (what the UI and AI show), per ADR-008 / locked decision #3.
//!
//! Pure data only — no behaviour, no adapter deps (`phosk_model`'s charter).
//! Money fields serialise as exact i64 centimes (`crate::money_centimes` /
//! `crate::opt_money_centimes`); dates are [`chrono::NaiveDate`]; machine-derived
//! values embed a [`Provenance`].
//!
//! Every entity also carries a `slug: String` — the stable, human-readable seed
//! id (`"t1"`, `"coffee"`, `"vw"`) the wire DTOs key on. The typed id is
//! authoritative internally; the slug is the UI/seed convenience the
//! `*_by_slug` port methods resolve and the DTOs emit as their string `id`.

use chrono::NaiveDate;
use phosk_core::money::Money;
use phosk_id::{
    AlertId, BudgetChangeId, BudgetId, CategoryId, ChargeId, ChatId, DebtId, FeedItemId,
    LineItemId, MessageId, PaymentId, PersonalIouId, PreferenceId, ReceiptId, SignalId,
    SubscriptionId, SuggestionId,
};
use serde::{Deserialize, Serialize};

use crate::provenance::{Provenance, Source};

// ── Ledger context ───────────────────────────────────────────────────────────

/// A captured spend document: a dated, categorised total at a shop, with its
/// itemised [`LineItem`]s stored separately and keyed by [`Receipt::id`].
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Receipt {
    /// Identity (relation key).
    pub id: ReceiptId,
    /// Stable seed/UI id the wire DTOs key on (`"t1"`).
    pub slug: String,
    /// Shop display name — the human identity for UI/AI.
    pub shop: String,
    /// Calendar date of the spend.
    pub date: NaiveDate,
    /// Primary category name.
    pub category: String,
    /// Receipt total (= Σ line totals), exact centimes.
    #[serde(with = "crate::money_centimes")]
    pub amount: Money,
    /// Whether this is a standing/fixed charge.
    pub fixed: bool,
    /// Where the receipt's data came from + confidence.
    pub provenance: Provenance,
    /// `"PHOTO" | "MANUAL" | "IMPORT"`.
    pub source_kind: String,
    /// OCR engine used, e.g. `"PADDLEOCR"`; empty for non-photo sources.
    pub ocr_engine: String,
    /// Number of OCR regions detected (0 for non-photo sources).
    pub ocr_regions: u32,
}

/// One itemised row on a [`Receipt`]. Its `line_total` is backend-derived
/// (`round(qty * unit_price)`), never trusted from raw input.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LineItem {
    /// Identity (relation key).
    pub id: LineItemId,
    /// The receipt this line belongs to.
    pub receipt_id: ReceiptId,
    /// Item display name.
    pub name: String,
    /// Quantity (fractional for weighed goods).
    pub qty: f64,
    /// Per-unit price, exact centimes.
    #[serde(with = "crate::money_centimes")]
    pub unit_price: Money,
    /// Line total, backend-derived = `round(qty * unit_price)`, exact centimes.
    #[serde(with = "crate::money_centimes")]
    pub line_total: Money,
    /// Category name for this line.
    pub category: String,
    /// Linked tracked item-[`Signal`], if any.
    pub signal_id: Option<SignalId>,
    /// Origin + confidence — `confidence < 0.7` drives the coral review flag.
    pub provenance: Provenance,
}

/// A tracked item-signal (Coffee, Beer, …): a recurring purchasable the user is
/// watching. A candidate signal is simply one with `tracked == false`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Signal {
    /// Identity (relation key).
    pub id: SignalId,
    /// Stable seed/UI id (`"coffee"`).
    pub slug: String,
    /// Display label, e.g. `"Oat-milk flat white"`.
    pub label: String,
    /// Parent category name, e.g. `"Coffee & snacks"`.
    pub parent: String,
    /// Unit of measure, e.g. `"cups"`, `"kg"`.
    pub unit: String,
    /// The day tracking began.
    pub since: NaiveDate,
    /// `false` = a candidate (not yet tracked).
    pub tracked: bool,
    /// Candidate pitch text; empty once tracked.
    pub desc: String,
    /// The 12-point trend spark (unitless per-cycle points the SVG draws). A
    /// persisted design/seed attribute of the signal — the momentum baseline the
    /// `delta_pct` is read against.
    pub series: Vec<f64>,
    /// Signed momentum percent vs the trailing-N baseline (e.g. `+28`). Stored on
    /// the signal (a curated display value, not recomputable from `series` alone).
    pub delta_pct: i32,
    /// Count of receipts this signal appears on in the current cycle.
    pub txns: u32,
    /// Origin + confidence (candidates are typically [`Source::LlmInferred`]).
    pub provenance: Provenance,
}

/// One [`LineItem`] rolled into a [`Signal`] — the unit of a signal's history.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SignalOccurrence {
    /// The signal this occurrence contributes to.
    pub signal_id: SignalId,
    /// The line item that produced it.
    pub line_item_id: LineItemId,
    /// When it happened.
    pub date: NaiveDate,
    /// Shop where it happened.
    pub shop: String,
    /// Quantity bought.
    pub qty: f64,
    /// Amount spent, exact centimes.
    #[serde(with = "crate::money_centimes")]
    pub amount: Money,
}

// ── Planning context ─────────────────────────────────────────────────────────

/// The cycle-level budget config; supersedes `BudgetConfig` for writes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Budget {
    /// Identity (relation key).
    pub id: BudgetId,
    /// Overall cycle spend ceiling, exact centimes.
    #[serde(with = "crate::money_centimes")]
    pub monthly_budget: Money,
    /// Savings target for the cycle, exact centimes.
    #[serde(with = "crate::money_centimes")]
    pub savings_target: Money,
}

/// One entry of the global budget's change history: a single user edit of the
/// monthly budget or the savings target, oldest→newest in append order.
///
/// The global [`BudgetConfig`](crate::BudgetConfig) is a singleton without a
/// history of its own, so every accepted change appends one of these; the
/// entry carries the [`Provenance`] of the edit.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BudgetChange {
    /// Identity of this history entry.
    pub id: BudgetChangeId,
    /// `"monthly_budget" | "savings_target"`.
    pub field: String,
    /// The value before the edit, exact centimes.
    #[serde(with = "crate::money_centimes")]
    pub old_value: Money,
    /// The value after the edit, exact centimes.
    #[serde(with = "crate::money_centimes")]
    pub new_value: Money,
    /// The day the change was made.
    pub at: NaiveDate,
    /// Who made it ([`Source::UserModified`] for a user edit).
    pub provenance: Provenance,
}

/// A per-category spending envelope — richer than the dashboard's `Category`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CategoryCap {
    /// Identity (relation key).
    pub id: CategoryId,
    /// Stable seed/UI id.
    pub slug: String,
    /// Category name — the identity for UI/AI.
    pub name: String,
    /// Spending cap, or `None` for unlimited. Exact centimes.
    #[serde(with = "crate::opt_money_centimes")]
    pub cap: Option<Money>,
    /// Whether this is a fixed (non-discretionary) envelope.
    pub fixed: bool,
    /// Display glyph.
    pub glyph: String,
    /// AI guidance line for this envelope.
    pub note: String,
    /// Origin + confidence.
    pub provenance: Provenance,
}

/// One prior cycle's recorded spend (and cap) for a category — the input to the
/// trailing-N momentum/`histAvg` baseline.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BudgetHistory {
    /// Which category this history row is for.
    pub category_id: CategoryId,
    /// Start date of the cycle this row covers.
    pub cycle_start: NaiveDate,
    /// Spend in that cycle, exact centimes.
    #[serde(with = "crate::money_centimes")]
    pub spent: Money,
    /// The cap that applied in that cycle, if any. Exact centimes.
    #[serde(with = "crate::opt_money_centimes")]
    pub cap: Option<Money>,
}

/// A surfaced alert (rule-generated or LLM-inferred) with its action buttons.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Alert {
    /// Identity (relation key).
    pub id: AlertId,
    /// Stable seed/UI id (`"a1"`).
    pub slug: String,
    /// `"alert" | "warn" | "info" | "llm"`.
    pub tone: String,
    /// Short tag, e.g. `"GOING OUT"`, `"BUDGET"`.
    pub tag: String,
    /// Headline.
    pub head: String,
    /// Body text.
    pub body: String,
    /// `"over_budget" | "at_risk" | "savings" | "recurring_missing" | …`.
    pub kind: String,
    /// [`Source::RuleGenerated`] or [`Source::LlmInferred`].
    pub source: Source,
    /// `"active" | "dismissed" | "snoozed"`.
    pub status: String,
    /// Deep-link target (a category/sub id), if any.
    pub target: Option<String>,
    /// The action buttons offered on this alert.
    pub actions: Vec<AlertAction>,
    /// When the alert was created.
    pub created: NaiveDate,
    /// The snooze in force while `status` is `"snoozed"`; `None` for an alert
    /// that was never snoozed (or was snoozed before snoozes carried a term).
    #[serde(default)]
    pub snooze: Option<AlertSnooze>,
}

/// How long an [`Alert`] stays snoozed, and how bad its condition was when it
/// was snoozed — the two things that bring it back.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct AlertSnooze {
    /// The alert is hidden while the day is before this date and resurfaces
    /// on it.
    pub until: NaiveDate,
    /// The rules-engine level of the alert's target at snooze time:
    /// `0` no rule fires, `1` at risk, `2` over budget. A later, strictly
    /// higher level resurfaces the alert early.
    pub level: u8,
}

/// One action button on an [`Alert`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AlertAction {
    /// Button label, e.g. `"VIEW"`, `"RAISE CAP"`, `"DISMISS"`, `"SNOOZE"`, `"MARK PAID"`.
    pub label: String,
    /// Behaviour: `"navigate" | "dismiss" | "snooze" | "apply"`.
    pub kind: String,
}

// ── Recurring context ──────────────────────────────────────────────────────

/// A recurring charge (subscription / standing payment).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Subscription {
    /// Identity (relation key).
    pub id: SubscriptionId,
    /// Stable seed/UI id (`"netflix"`).
    pub slug: String,
    /// Display name.
    pub name: String,
    /// Per-charge amount, exact centimes.
    #[serde(with = "crate::money_centimes")]
    pub amount: Money,
    /// `"monthly" | "yearly"`.
    pub cadence: String,
    /// Day-of-month for monthly; `0` for yearly.
    pub day: u32,
    /// Month label for yearly (e.g. `"FEB"`); empty for monthly.
    pub month: String,
    /// `"ok" | "soon" | "due" | "watch" | "paused" | "cancelled"`.
    pub status: String,
    /// Category name.
    pub category: String,
    /// Display glyph.
    pub glyph: String,
    /// Date the subscription began.
    pub since: NaiveDate,
    /// Free-text note.
    pub note: String,
    /// [`Source::UserEntered`] or [`Source::LlmInferred`] (auto-detected).
    pub source: Source,
    /// Origin + confidence.
    pub provenance: Provenance,
}

/// A recorded billing event of a [`Subscription`].
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Charge {
    /// Identity (relation key).
    pub id: ChargeId,
    /// The subscription this charge belongs to.
    pub subscription_id: SubscriptionId,
    /// When it was billed.
    pub date: NaiveDate,
    /// Amount billed, exact centimes.
    #[serde(with = "crate::money_centimes")]
    pub amount: Money,
    /// Note, e.g. `"confirmed"`, `"auto-detected"`.
    pub note: String,
    /// Origin + confidence.
    pub provenance: Provenance,
}

// ── Debts context ──────────────────────────────────────────────────────────

/// An institutional debt (lease, loan, card, tax, …).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Debt {
    /// Identity (relation key).
    pub id: DebtId,
    /// Stable seed/UI id (`"vw"`).
    pub slug: String,
    /// Display name.
    pub name: String,
    /// Lender / creditor name.
    pub lender: String,
    /// `"LEASE" | "LOAN" | "CARD" | "TAX" | "BNPL" | "MEDICAL"`.
    pub kind: String,
    /// Current outstanding balance, exact centimes.
    #[serde(with = "crate::money_centimes")]
    pub balance: Money,
    /// Original amount borrowed, exact centimes.
    #[serde(with = "crate::money_centimes")]
    pub orig: Money,
    /// Scheduled monthly payment, exact centimes.
    #[serde(with = "crate::money_centimes")]
    pub monthly: Money,
    /// Annual percentage rate, `0.0..=1.0`.
    pub apr: f64,
    /// Payment day-of-month.
    pub day: u32,
    /// Term in months (`0` = revolving).
    pub term: u32,
    /// `"high" | "due" | "watch" | "ok"`.
    pub status: String,
    /// Display glyph.
    pub glyph: String,
    /// Date the debt was opened.
    pub since: NaiveDate,
    /// Free-text note.
    pub note: String,
    /// [`Source::UserEntered`] or [`Source::LlmInferred`].
    pub source: Source,
    /// Origin + confidence.
    pub provenance: Provenance,
}

/// One recorded payment against a [`Debt`].
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DebtPayment {
    /// Identity (relation key).
    pub id: PaymentId,
    /// The debt this payment reduced.
    pub debt_id: DebtId,
    /// When it was paid.
    pub date: NaiveDate,
    /// Amount paid, exact centimes.
    #[serde(with = "crate::money_centimes")]
    pub amount: Money,
    /// Outstanding balance after the payment, exact centimes.
    #[serde(with = "crate::money_centimes")]
    pub balance_after: Money,
    /// Origin + confidence.
    pub provenance: Provenance,
}

/// An informal person-to-person debt (an IOU).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PersonalIou {
    /// Identity (relation key).
    pub id: PersonalIouId,
    /// Stable seed/UI id (`"i1"`).
    pub slug: String,
    /// `"in"` (owed to you) | `"out"` (you owe).
    pub dir: String,
    /// The other person's name.
    pub person: String,
    /// The person's initials (UI avatar).
    pub initials: String,
    /// Outstanding amount, exact centimes.
    #[serde(with = "crate::money_centimes")]
    pub amount: Money,
    /// Original amount, exact centimes.
    #[serde(with = "crate::money_centimes")]
    pub of: Money,
    /// Reason / memo.
    pub reason: String,
    /// Date it originated.
    pub since: NaiveDate,
    /// Origin + confidence.
    pub provenance: Provenance,
}

// ── Settings context ───────────────────────────────────────────────────────

/// A user setting. Typed access (parsing `value`) happens in the service layer;
/// the stored shape is a stringified key/value with a surface tag.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Preference {
    /// Identity (relation key).
    pub id: PreferenceId,
    /// Setting key, e.g. `"momentum_baseline_cycles"`, `"currency"`.
    pub key: String,
    /// Stringified value; the service parses it to the typed form.
    pub value: String,
    /// Page/section this preference belongs to.
    pub surface: String,
    /// Whether the value is stored on-device only.
    pub stored_on_device: bool,
    /// [`Source::UserModified`] once changed, else [`Source::RuleGenerated`] (default).
    pub provenance: Provenance,
}

// ── AI context ─────────────────────────────────────────────────────────────

/// An AI conversation thread.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Chat {
    /// Identity (relation key).
    pub id: ChatId,
    /// When the chat began.
    pub started: NaiveDate,
}

/// One message in a [`Chat`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Message {
    /// Identity (relation key).
    pub id: MessageId,
    /// The chat this message belongs to.
    pub chat_id: ChatId,
    /// `"usr"` (user) | `"sys"` (assistant).
    pub who: String,
    /// Message text.
    pub text: String,
    /// When it was sent.
    pub at: NaiveDate,
}

/// A candidate AI suggestion (a cap cut, a recurring detection, a signal …).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AiSuggestion {
    /// Identity (relation key).
    pub id: SuggestionId,
    /// `"budget_cut" | "recurring" | "signal_candidate" | "cap"`.
    pub kind: String,
    /// Suggestion text shown to the user.
    pub text: String,
    /// Confidence in `0.0..=1.0`.
    pub confidence: f64,
    /// The entity the suggestion acts on, if any.
    pub target: Option<String>,
    /// Estimated savings if accepted, exact centimes (or `None`).
    #[serde(with = "crate::opt_money_centimes")]
    pub estimated_savings: Option<Money>,
    /// `"open" | "accepted" | "dismissed"`.
    pub status: String,
}

/// An entry in the AI activity feed.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FeedItem {
    /// Identity (relation key).
    pub id: FeedItemId,
    /// `"categorize" | "reprocess" | "suggest" | "detect"`.
    pub kind: String,
    /// Feed text.
    pub text: String,
    /// Confidence in `0.0..=1.0`, if applicable.
    pub conf: Option<f64>,
    /// Transient state, e.g. `"running"`, or `None`.
    pub state: Option<String>,
    /// When the activity occurred.
    pub at: NaiveDate,
    /// Button labels offered on this feed item.
    pub actions: Vec<String>,
    /// Whether this feed item is a candidate (awaiting confirmation).
    pub cand: bool,
}

#[cfg(test)]
mod tests {
    use super::*;
    use phosk_id::ReceiptId;

    fn naive(y: i32, m: u32, d: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, d).expect("valid test date")
    }

    fn prov() -> Provenance {
        Provenance {
            source: Source::Ocr,
            confidence: 0.91,
        }
    }

    #[test]
    fn receipt_round_trips_and_amount_is_exact_centimes() {
        let r = Receipt {
            id: ReceiptId::new(),
            slug: "t1".to_owned(),
            shop: "Migros".to_owned(),
            date: naive(2026, 6, 18),
            category: "GROCERIES".to_owned(),
            amount: Money::from_centimes(5875),
            fixed: false,
            provenance: prov(),
            source_kind: "PHOTO".to_owned(),
            ocr_engine: "PADDLEOCR".to_owned(),
            ocr_regions: 12,
        };
        let value: serde_json::Value = serde_json::to_value(&r).expect("to value");
        assert_eq!(value["amount"], serde_json::json!(5875));
        let json = serde_json::to_string(&r).expect("serialize");
        let back: Receipt = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(r, back);
    }

    #[test]
    fn line_item_optional_signal_and_money_round_trip() {
        let line = LineItem {
            id: phosk_id::LineItemId::new(),
            receipt_id: ReceiptId::new(),
            name: "Oat milk".to_owned(),
            qty: 2.0,
            unit_price: Money::from_centimes(295),
            line_total: Money::from_centimes(590),
            category: "GROCERIES".to_owned(),
            signal_id: None,
            provenance: Provenance {
                source: Source::Ocr,
                confidence: 0.6,
            },
        };
        assert!(line.provenance.is_low_confidence());
        let json = serde_json::to_string(&line).expect("serialize");
        let back: LineItem = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(line, back);
    }

    #[test]
    fn category_cap_unlimited_serialises_cap_as_null() {
        let c = CategoryCap {
            id: phosk_id::CategoryId::new(),
            slug: "misc".to_owned(),
            name: "Misc".to_owned(),
            cap: None,
            fixed: false,
            glyph: "*".to_owned(),
            note: String::new(),
            provenance: prov(),
        };
        let value: serde_json::Value = serde_json::to_value(&c).expect("to value");
        assert!(value["cap"].is_null());
    }

    #[test]
    fn debt_round_trips_with_all_money_and_rate_fields() {
        let d = Debt {
            id: phosk_id::DebtId::new(),
            slug: "vw".to_owned(),
            name: "VW Lease".to_owned(),
            lender: "VW Financial".to_owned(),
            kind: "LEASE".to_owned(),
            balance: Money::from_centimes(1_200_000),
            orig: Money::from_centimes(2_400_000),
            monthly: Money::from_centimes(45_000),
            apr: 0.039,
            day: 5,
            term: 48,
            status: "ok".to_owned(),
            glyph: "C".to_owned(),
            since: naive(2024, 1, 5),
            note: String::new(),
            source: Source::UserEntered,
            provenance: Provenance::user_entered(),
        };
        let json = serde_json::to_string(&d).expect("serialize");
        let back: Debt = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(d, back);
    }

    #[test]
    fn ai_suggestion_optional_savings_round_trips() {
        let s = AiSuggestion {
            id: phosk_id::SuggestionId::new(),
            kind: "budget_cut".to_owned(),
            text: "Cut going-out cap".to_owned(),
            confidence: 0.82,
            target: Some("going-out".to_owned()),
            estimated_savings: Some(Money::from_centimes(8_000)),
            status: "open".to_owned(),
        };
        let value: serde_json::Value = serde_json::to_value(&s).expect("to value");
        assert_eq!(value["estimated_savings"], serde_json::json!(8_000));
        let json = serde_json::to_string(&s).expect("serialize");
        let back: AiSuggestion = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(s, back);
    }
}
