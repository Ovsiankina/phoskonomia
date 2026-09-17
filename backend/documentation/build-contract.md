# Phoskonomia Backend — Authoritative Build Contract

**Status:** AUTHORITATIVE. Every downstream swarm agent reads THIS file. It is derived from the
real wire spec (`frontend/dioxus-app/src/data/*.rs` — the ~30 `#[server]` view DTOs), the mock
data shapes of the since-deleted React prototype, the feature checklist
(`backend/documentation/backend-features-todo.md`), and the existing built crates. Where this
document and the AI-written `architectural-design-and-philosophy.md` disagree, THIS wins.

## Locked decisions (2026-06-22) — non-negotiable

1. **Transport: NO REST.** The frontend is Dioxus `#[server]` fns calling backend service crates
   in-process. We build & test the **service layer** (`phosk_*` crates), NOT the axum `phosk_api`
   bin. Ignore `phosk_api`.
2. **Money: CENTIMES EVERYWHERE.** `phosk_core::money::Money` is `i64` centimes. Every DTO money
   field serializes via `phosk_model::money_centimes` / `opt_money_centimes` (exact `i64`).
   `Vec<Money>` via the local `money_vec_centimes` / `opt_money_vec_centimes` (see
   `dioxus-app/src/data/dashboard.rs`). CHF `f64` (`Money::as_chf_f64`) is **render-only**, never a
   wire form. This contract includes the **`phosk_insights` centimes refactor** (§9).
3. **Identity:** every entity holds BOTH a typed id (newtype over `uuid::Uuid`, in a new `phosk_id`
   crate) AND a human `name: String`. Relations use the id; UI + AI use the name.
4. **Provenance:** a lightweight `Provenance { source: Source, confidence: f64 }` embedded in each
   entity that can be machine-derived, PLUS a separate `CorrectionEvent` audit-log type
   (entity id, field, old→new, when). NOT per-field history.
5. **Port:** ONE fat async `DatabaseAdapter` trait (`phosk_adapter_db`). Grow it with the methods
   features need; `phosk_db_memory` implements every method (`todo!()` / minimal in-mem is fine for
   the skeleton). Keep it object-safe (`async_trait`, `Arc<dyn ..>`).
6. **Momentum / deltaPct baseline:** trailing N-cycle average, DEFAULT `N = 3`, with `N` exposed as a
   user setting (`momentum_baseline_cycles`). ONE helper computes it (§ derived formulas).

## Conventions (clippy-enforced, real)

- NO `unwrap()`/`expect()`/`panic!` in non-test code (workspace-denied). Map every `Option`/`Result`
  to `PhoskError`. Tests MAY use `expect("msg")` (never bare `unwrap()`).
- One error taxonomy: `phosk_core::error::PhoskError` (closed enum: `InvalidDate`, `Invalid`,
  `NotFound`, `Overflow`). Add variants ONLY if genuinely needed (a new variant ⇒ also update
  `http_status()` and `code()`).
- `tracing = { version = "0.1", features = ["release_max_level_off"] }`; instrument services;
  libraries never install a subscriber.
- Each crate: `[lints]\nworkspace = true`. `unsafe_code` forbidden workspace-wide.

## Canonical pattern (copy the dashboard slice exactly)

- **Service fns** are free fns taking `db: &dyn DatabaseAdapter` + an `as_of: chrono::NaiveDate`
  (resolve the cycle internally via `Period::Month`) OR a resolved `CycleWindow`, returning
  `Result<Dto, PhoskError>`. See `phosk_planning::totals`, `phosk_ledger::{daily_spend,top_shops}`,
  `phosk_insights::{dashboard_totals,spend_series,top_shops}`.
- **DTOs** are `serde` `Serialize` structs with `#[serde(rename_all="camelCase")]`. Money fields use
  `#[serde(with="phosk_model::money_centimes")]` (exact i64 centimes). They mirror the
  `dioxus-app/src/data/*.rs` view structs field-for-field — those are the wire truth.
- **Tests** are integration tests in `<crate>/tests/<feature>.rs`, `#[tokio::test]`, building a
  `phosk_db_memory::MemoryDb` seed, calling the service, asserting EXACT centime values + derived
  fields by the agreed formula. (Unit `#[cfg(test)]` modules in `src/lib.rs` are also fine, matching
  the existing crates.)
- **Cycle / "today":** the seeded demo clock is **2026-06-18** (day 18 of the June cycle, 30 days,
  12 days left). `Period::Month.resolve(2026-06-18)` ⇒ `[2026-06-01, 2026-06-30]`. Use this for every
  derived-field assertion.

---

# 1. `phosk_id` — typed id newtypes (NEW crate, L0)

A new foundation crate. Each newtype wraps `uuid::Uuid`, is `Copy + Clone + PartialEq + Eq + Hash +
Debug`, serializes transparently (`#[serde(transparent)]`) as the UUID string, and exposes
`new()` (random v4), `from_uuid(Uuid)`, `as_uuid() -> Uuid`. Define a macro `id_newtype!` to avoid
boilerplate. The newtypes:

```rust
ReceiptId, LineItemId, CategoryId, BudgetId, SubscriptionId, ChargeId,
DebtId, PaymentId, PersonalIouId, SignalId, AlertId, ChatId, MessageId,
SuggestionId, FeedItemId, CorrectionId, PreferenceId
```

`Cargo.toml` deps: `uuid = { version = "1", features = ["v4", "serde"] }`, `serde`.
**Layering:** L0, depended on by `phosk_model` and adapters. Depends on nothing internal.
**Note:** the existing wire DTOs carry `id: String` (e.g. `"t1"`, `"coffee"`). Service DTOs keep
`id: String` on the wire (the frontend keys on strings); internally entities use the typed id, and
the service formats `typed_id.as_uuid().to_string()` (or a slug for seed determinism — see §8). The
human `name`/`label` is what the UI shows.

---

# 2. Shared value types added to `phosk_core` and `phosk_model`

## 2.1 `phosk_core` additions

- **No new `Money` work** — it is complete. Add (if a feature needs it) `Money::checked_mul_div` is
  NOT required; do scaled math inline as `phosk_planning`/`phosk_insights` already do
  (`centimes().checked_mul(..)` then `/ divisor`), surfacing `PhoskError::Overflow`.
- **`PhoskError`:** add variants ONLY if needed. Likely none required; `NotFound(String)` already
  covers "no such debt/subscription/signal id". If a write conflict needs distinguishing, add
  `Conflict(String)` (status 409, code `"conflict"`) — but only when a test demands it.

## 2.2 `phosk_model` additions (pure data, no behaviour, no adapter deps)

Add `phosk_id` as a dependency. Add the entities below (§3). All derive
`Debug, Clone, PartialEq, Serialize, Deserialize`; money fields use `money_centimes` /
`opt_money_centimes`. Dates are `chrono::NaiveDate`.

### Provenance & Source (embedded in machine-derivable entities)

```rust
/// Where a value came from + how confident the machine is.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum Source {
    Ocr,
    LlmInferred,
    UserEntered,
    UserModified,
    Imported,
    RuleGenerated,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Provenance {
    pub source: Source,
    /// 0.0..=1.0. UserEntered/UserModified ⇒ 1.0. Lines < 0.7 are "low-confidence".
    pub confidence: f64,
}

impl Provenance {
    pub fn user_entered() -> Self { Self { source: Source::UserEntered, confidence: 1.0 } }
    pub fn is_low_confidence(&self) -> bool { self.confidence < 0.7 }
}
```

### CorrectionEvent (separate audit log, NOT per-field history)

```rust
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CorrectionEvent {
    pub id: CorrectionId,
    /// Stringified typed id of the corrected entity (receipt/line/etc).
    pub entity_id: String,
    /// e.g. "category", "amount", "shop".
    pub field: String,
    pub old_value: String,
    pub new_value: String,
    pub at: chrono::NaiveDate,
}
```

The existing `Transaction`, `Category`, `BudgetConfig` structs in `phosk_model` STAY (the dashboard
slice depends on them). New richer entities are ADDED alongside; do not break the existing three or
their tests.

---

# 3. Domain ENTITIES (add to `phosk_model`)

Each struct below is the persisted shape behind the port. The wire DTOs (§5–8) are projections of
these; field names there are the source of truth for the JSON contract.

## Ledger context

```rust
pub struct Receipt {
    pub id: ReceiptId,
    pub shop: String,               // human identity for UI/AI
    pub date: NaiveDate,
    pub category: String,           // primary category name
    #[serde(with = "money_centimes")]
    pub amount: Money,              // receipt total (= Σ line totals)
    pub fixed: bool,                // standing/fixed charge
    pub provenance: Provenance,
    pub source_kind: String,        // "PHOTO" | "MANUAL" | "IMPORT"
    pub ocr_engine: String,         // "PADDLEOCR" | "" 
    pub ocr_regions: u32,
}

pub struct LineItem {
    pub id: LineItemId,
    pub receipt_id: ReceiptId,
    pub name: String,
    pub qty: f64,
    #[serde(with = "money_centimes")]
    pub unit_price: Money,
    #[serde(with = "money_centimes")]
    pub line_total: Money,          // backend-derived = round(qty * unit_price)
    pub category: String,
    pub signal_id: Option<SignalId>,// linked tracked item-signal
    pub provenance: Provenance,     // confidence drives the < 0.7 coral flag
}

pub struct Signal {                 // tracked item-signal (Coffee, Beer, ...)
    pub id: SignalId,
    pub label: String,              // "Oat-milk flat white"
    pub parent: String,             // parent category name "Coffee & snacks"
    pub unit: String,               // "cups", "kg", ...
    pub since: NaiveDate,           // tracking began
    pub tracked: bool,              // false = candidate
    pub desc: String,               // candidate pitch (empty when tracked)
    pub provenance: Provenance,     // LlmInferred for candidates
}

pub struct SignalOccurrence {       // one line rolled into a signal
    pub signal_id: SignalId,
    pub line_item_id: LineItemId,
    pub date: NaiveDate,
    pub shop: String,
    pub qty: f64,
    #[serde(with = "money_centimes")]
    pub amount: Money,
}
```

`SignalCandidate` is NOT a separate type — it is a `Signal` with `tracked == false` (+ `desc`,
`provenance.source == LlmInferred`), matching `signals.rs` (`candidate: bool`).

## Planning context

```rust
pub struct Budget {                 // the cycle-level config; supersedes BudgetConfig for writes
    pub id: BudgetId,
    #[serde(with = "money_centimes")]
    pub monthly_budget: Money,
    #[serde(with = "money_centimes")]
    pub savings_target: Money,
}

pub struct CategoryCap {            // per-category envelope (richer than phosk_model::Category)
    pub id: CategoryId,
    pub name: String,               // identity for UI/AI
    #[serde(with = "opt_money_centimes")]
    pub cap: Option<Money>,         // None = unlimited
    pub fixed: bool,
    pub glyph: String,
    pub note: String,               // AI guidance line
    pub provenance: Provenance,
}

pub struct BudgetHistory {          // one prior cycle's spend for a category
    pub category_id: CategoryId,
    pub cycle_start: NaiveDate,
    #[serde(with = "money_centimes")]
    pub spent: Money,
    #[serde(with = "opt_money_centimes")]
    pub cap: Option<Money>,
}

pub struct Alert {
    pub id: AlertId,
    pub tone: String,               // "alert" | "warn" | "info" | "llm"
    pub tag: String,                // "GOING OUT", "BUDGET", ...
    pub head: String,
    pub body: String,
    pub kind: String,               // "over_budget" | "at_risk" | "savings" | "recurring_missing" | ...
    pub source: Source,             // RuleGenerated | LlmInferred
    pub status: String,             // "active" | "dismissed" | "snoozed"
    pub target: Option<String>,     // deep-link (category/sub id)
    pub actions: Vec<AlertAction>,
    pub created: NaiveDate,
}

pub struct AlertAction {
    pub label: String,              // "VIEW" | "RAISE CAP" | "DISMISS" | "SNOOZE" | "MARK PAID"
    pub kind: String,               // "navigate" | "dismiss" | "snooze" | "apply"
}
```

## Recurring context

```rust
pub struct Subscription {
    pub id: SubscriptionId,
    pub name: String,
    #[serde(with = "money_centimes")]
    pub amount: Money,              // per-charge amount
    pub cadence: String,            // "monthly" | "yearly"
    pub day: u32,                   // day-of-month (monthly); 0 for yearly
    pub month: String,              // month label for yearly (e.g. "FEB"); "" for monthly
    pub status: String,            // "ok" | "soon" | "due" | "watch" | "paused"
    pub category: String,
    pub glyph: String,
    pub since: NaiveDate,
    pub note: String,
    pub source: Source,             // UserEntered | LlmInferred (auto-detected)
    pub provenance: Provenance,
}

pub struct Charge {                 // a recorded billing event of a subscription
    pub id: ChargeId,
    pub subscription_id: SubscriptionId,
    pub date: NaiveDate,
    #[serde(with = "money_centimes")]
    pub amount: Money,
    pub note: String,               // "confirmed" | "auto-detected"
    pub provenance: Provenance,
}
```

## Debts context

```rust
pub struct Debt {
    pub id: DebtId,
    pub name: String,
    pub lender: String,
    pub kind: String,               // "LEASE" | "LOAN" | "CARD" | "TAX" | "BNPL" | "MEDICAL"
    #[serde(with = "money_centimes")]
    pub balance: Money,             // current outstanding
    #[serde(with = "money_centimes")]
    pub orig: Money,                // original borrowed
    #[serde(with = "money_centimes")]
    pub monthly: Money,             // scheduled monthly payment
    pub apr: f64,                   // 0..1
    pub day: u32,                   // payment day-of-month
    pub term: u32,                  // months (0 = revolving)
    pub status: String,             // "high" | "due" | "watch" | "ok"
    pub glyph: String,
    pub since: NaiveDate,
    pub note: String,
    pub source: Source,             // UserEntered | LlmInferred
    pub provenance: Provenance,
}

pub struct DebtPayment {
    pub id: PaymentId,
    pub debt_id: DebtId,
    pub date: NaiveDate,
    #[serde(with = "money_centimes")]
    pub amount: Money,
    #[serde(with = "money_centimes")]
    pub balance_after: Money,
    pub provenance: Provenance,
}

pub struct PersonalIou {
    pub id: PersonalIouId,
    pub dir: String,                // "in" (owed to you) | "out" (you owe)
    pub person: String,
    pub initials: String,
    #[serde(with = "money_centimes")]
    pub amount: Money,              // outstanding
    #[serde(with = "money_centimes")]
    pub of: Money,                  // original
    pub reason: String,
    pub since: NaiveDate,
    pub provenance: Provenance,
}
```

## Settings context

```rust
pub struct Preference {
    pub id: PreferenceId,
    pub key: String,                // e.g. "momentum_baseline_cycles", "currency"
    pub value: String,              // stringified; typed access in the service
    pub surface: String,            // page/section the pref belongs to
    pub stored_on_device: bool,
    pub provenance: Provenance,     // UserModified once changed, else RuleGenerated (default)
}
```

`momentum_baseline_cycles` is a `Preference` with default value `"3"` (see §6 momentum helper).

## AI context

```rust
pub struct Chat {
    pub id: ChatId,
    pub started: NaiveDate,
}

pub struct Message {
    pub id: MessageId,
    pub chat_id: ChatId,
    pub who: String,                // "usr" | "sys"
    pub text: String,
    pub at: NaiveDate,
}

pub struct AiSuggestion {           // candidate cap / recurring / signal suggestion
    pub id: SuggestionId,
    pub kind: String,               // "budget_cut" | "recurring" | "signal_candidate" | "cap"
    pub text: String,
    pub confidence: f64,
    pub target: Option<String>,     // entity the suggestion acts on
    #[serde(with = "opt_money_centimes")]
    pub estimated_savings: Option<Money>,
    pub status: String,             // "open" | "accepted" | "dismissed"
}

pub struct FeedItem {               // AI activity feed entry
    pub id: FeedItemId,
    pub kind: String,               // "categorize" | "reprocess" | "suggest" | "detect"
    pub text: String,
    pub conf: Option<f64>,
    pub state: Option<String>,      // "running" | None
    pub at: NaiveDate,
    pub actions: Vec<String>,       // button labels
    pub cand: bool,
}
```

---

# 4. The grown `DatabaseAdapter` PORT (`phosk_adapter_db`)

Add ALL methods below to the ONE fat trait (keep the existing three). Every method is `async`,
returns `Result<_, PhoskError>`, object-safe. `phosk_db_memory` implements every one (minimal in-mem
or `todo!()` for write paths that no test exercises yet — but reads used by tested services MUST be
real against the seed). Group by context for readability; it remains ONE trait.

```rust
// ── Ledger ──────────────────────────────────────────────────────────────────
async fn receipts_between(&self, from: NaiveDate, to: NaiveDate) -> Result<Vec<Receipt>, PhoskError>;
async fn receipt(&self, id: ReceiptId) -> Result<Receipt, PhoskError>;
async fn receipt_by_slug(&self, slug: &str) -> Result<Receipt, PhoskError>; // seed ids "t1".. (NotFound if absent)
async fn line_items(&self, receipt: ReceiptId) -> Result<Vec<LineItem>, PhoskError>;
async fn all_receipts(&self) -> Result<Vec<Receipt>, PhoskError>;          // for unfiltered list / shop directory
async fn insert_receipt(&self, r: Receipt, lines: Vec<LineItem>) -> Result<ReceiptId, PhoskError>;
async fn update_line_item(&self, line: LineItem) -> Result<(), PhoskError>;
async fn record_correction(&self, ev: CorrectionEvent) -> Result<(), PhoskError>;

// ── Signals ─────────────────────────────────────────────────────────────────
async fn signals(&self) -> Result<Vec<Signal>, PhoskError>;               // all (tracked + candidates)
async fn signal(&self, id: SignalId) -> Result<Signal, PhoskError>;
async fn signal_by_slug(&self, slug: &str) -> Result<Signal, PhoskError>; // "coffee" etc.
async fn signal_occurrences(&self, id: SignalId) -> Result<Vec<SignalOccurrence>, PhoskError>;
async fn set_signal_tracked(&self, id: SignalId, tracked: bool) -> Result<(), PhoskError>;

// ── Planning ────────────────────────────────────────────────────────────────
async fn category_caps(&self) -> Result<Vec<CategoryCap>, PhoskError>;
async fn category_cap_by_name(&self, name: &str) -> Result<CategoryCap, PhoskError>;
async fn set_category_cap(&self, name: &str, cap: Option<Money>) -> Result<(), PhoskError>;
async fn budget_history(&self, category: &str) -> Result<Vec<BudgetHistory>, PhoskError>;
async fn alerts(&self) -> Result<Vec<Alert>, PhoskError>;
async fn alert(&self, id: AlertId) -> Result<Alert, PhoskError>;
async fn update_alert_status(&self, id: AlertId, status: &str) -> Result<(), PhoskError>;

// ── Recurring ───────────────────────────────────────────────────────────────
async fn subscriptions(&self) -> Result<Vec<Subscription>, PhoskError>;
async fn subscription(&self, id: SubscriptionId) -> Result<Subscription, PhoskError>;
async fn subscription_by_slug(&self, slug: &str) -> Result<Subscription, PhoskError>;
async fn subscription_charges(&self, id: SubscriptionId) -> Result<Vec<Charge>, PhoskError>;
async fn upsert_subscription(&self, s: Subscription) -> Result<SubscriptionId, PhoskError>;
async fn record_charge(&self, c: Charge) -> Result<(), PhoskError>;

// ── Debts ───────────────────────────────────────────────────────────────────
async fn debts(&self) -> Result<Vec<Debt>, PhoskError>;
async fn debt(&self, id: DebtId) -> Result<Debt, PhoskError>;
async fn debt_by_slug(&self, slug: &str) -> Result<Debt, PhoskError>;
async fn debt_payments(&self, id: DebtId) -> Result<Vec<DebtPayment>, PhoskError>;
async fn upsert_debt(&self, d: Debt) -> Result<DebtId, PhoskError>;
async fn record_debt_payment(&self, p: DebtPayment) -> Result<(), PhoskError>;
async fn personal_ious(&self) -> Result<Vec<PersonalIou>, PhoskError>;
async fn upsert_personal_iou(&self, i: PersonalIou) -> Result<PersonalIouId, PhoskError>;

// ── Analytics support ─────────────────────────────────────────────────────────
// 12-cycle history: the service resolves windows and calls receipts_between per cycle,
// so NO new method is strictly required. Provide this only if a test needs a fast path:
async fn spend_history(&self, cycles: u32, as_of: NaiveDate) -> Result<Vec<BudgetHistory>, PhoskError>;

// ── Settings ────────────────────────────────────────────────────────────────
async fn preferences(&self) -> Result<Vec<Preference>, PhoskError>;
async fn preference(&self, key: &str) -> Result<Preference, PhoskError>;   // NotFound if unset
async fn set_preference(&self, key: &str, value: &str) -> Result<(), PhoskError>;

// ── AI ──────────────────────────────────────────────────────────────────────
async fn feed_items(&self) -> Result<Vec<FeedItem>, PhoskError>;
async fn chat_messages(&self, chat: ChatId) -> Result<Vec<Message>, PhoskError>;
async fn latest_chat(&self) -> Result<Option<Chat>, PhoskError>;
async fn append_message(&self, m: Message) -> Result<(), PhoskError>;
async fn clear_chat(&self, chat: ChatId) -> Result<(), PhoskError>;
async fn ai_suggestions(&self) -> Result<Vec<AiSuggestion>, PhoskError>;
async fn update_suggestion_status(&self, id: SuggestionId, status: &str) -> Result<(), PhoskError>;
```

**Slug methods exist** because the wire DTOs key on stable string ids (`"t1"`, `"coffee"`, `"vw"`).
The seed assigns each entity a deterministic slug; `*_by_slug` resolves it. Internally the typed id
is authoritative; the slug is a seed/UI convenience stored on the entity (add `pub slug: String` to
each entity if the simplest path — OR keep a slug→id map in `MemoryDb`). **Recommended: store
`slug` on each entity**, so the DTO emits `id = entity.slug` and the frontend contract is unchanged.

---

# 5. Per-feature SERVICE fn signatures + DTOs + DERIVED formulas

All DTOs already exist verbatim in `frontend/dioxus-app/src/data/*.rs` — the service crates produce
structurally identical DTOs (same camelCase keys, money as `money_centimes`). The `*.rs` files are
SEEDED placeholders today; the job is to make the SERVICE compute the same shapes from the port.
Below, only NON-OBVIOUS derived fields get a formula; field lists reference the spec file.

### Notation
`B` budget, `S` spent-to-date, `cap` category cap, `proj` projected, `N` cycle length (days),
`d` day index (1-based), `r` days left, `as_of` 2026-06-18, June `N=30, d=18, r=12`.

## 5.1 `phosk_ledger` (transactions · lines · signals · shops)

Spec: `transactions.rs`, `signals.rs`. DTOs: `TransactionDto`, `TxnSummaryDto`,
`TransactionListDto`, `TxnLineDto`, `TxnLinesDto`, `TxnDetailDto`, `TxnSourceDto`, `TxnFilter`,
`SignalDto`, `MoversDto`, `SignalOccurrenceDto`, `SignalDetailDto`.

```rust
pub async fn list_transactions(db: &dyn DatabaseAdapter, as_of: NaiveDate, filter: TxnFilter)
    -> Result<TransactionListDto, PhoskError>;
pub async fn transaction_lines(db: &dyn DatabaseAdapter, slug: &str) -> Result<TxnLinesDto, PhoskError>;
pub async fn transaction_detail(db: &dyn DatabaseAdapter, slug: &str) -> Result<TxnDetailDto, PhoskError>;
pub async fn list_signals(db: &dyn DatabaseAdapter, as_of: NaiveDate) -> Result<Vec<SignalDto>, PhoskError>;
pub async fn signal_candidates(db: &dyn DatabaseAdapter, as_of: NaiveDate) -> Result<Vec<SignalDto>, PhoskError>;
pub async fn movers(db: &dyn DatabaseAdapter, as_of: NaiveDate) -> Result<MoversDto, PhoskError>;
pub async fn signal_detail(db: &dyn DatabaseAdapter, as_of: NaiveDate, slug: &str) -> Result<SignalDetailDto, PhoskError>;
```

Derived:
- `TransactionDto.itemCount` = `line_items(receipt).len()`.
- `TransactionDto.lowConfCount` / `TxnLinesDto.lowConf` = count of lines with `confidence < 0.7`.
- `TransactionDto.signalIds` / `TxnLinesDto.sigs` = distinct `signal_id` slugs across the receipt's lines.
- `TxnLineDto.lineTotal` = backend-derived = `round(qty * unit_price_centimes)` as i64 (NOT trusted from input).
- `TxnSummaryDto.entryCount` / `totalAmount` = count and checked `Money::sum` over the FILTERED rows.
- `TxnDetailDto.avgConfidence` = mean of line confidences (0 if no lines).
- Filtering (`TxnFilter`): `shop`/`category` exact-match (empty = all); `q` lowercase substring on
  shop|category; `sort` ∈ {`date`(newest first, default), `amount`(desc), `shop`(asc)}; `period` resolves
  a `CycleWindow` via `Period` and bounds `receipts_between`.
- `SignalDto.series` = 12-cycle spend spark (unitless points). `deltaPct` = momentum vs trailing-N avg
  (§6). `cycleSpend` / `cycleQty` = `Σ` over occurrences in current cycle. `txns` = distinct receipts
  this cycle. `candidate` = `!tracked`.
- `MoversDto.riser` = max `deltaPct`; `faller` = min `deltaPct`; `all` = momentum-ranked.
- `SignalDetailDto.allTimeSpend` / `allTimeTxns` = `Σ` over ALL occurrences. `recent` = newest-first
  occurrences. `guidance` = AI line (seed/canned for now).

Writes (skeleton, `todo!()`-acceptable but signatures fixed):
```rust
pub async fn correct_line(db: &dyn DatabaseAdapter, line: LineItemId, field: &str, new_value: &str) -> Result<(), PhoskError>;
pub async fn track_signal(db: &dyn DatabaseAdapter, slug: &str) -> Result<(), PhoskError>;
pub async fn dismiss_signal(db: &dyn DatabaseAdapter, slug: &str) -> Result<(), PhoskError>;
```

## 5.2 `phosk_planning` (budgets · allocation · alerts) — EXTEND existing crate

Spec: `budgets.rs`, `dashboard.rs` (alerts). DTOs: `CategoryDto`, `BudgetTotalsDto`,
`AllocSegmentDto`, `AllocAdviceDto`, `AllocationDto`, `CategoryDetailDto`, `CategoryTxnDto`,
`AlertDto`, plus existing `CycleTotals`.

```rust
pub async fn categories(db: &dyn DatabaseAdapter, as_of: NaiveDate) -> Result<Vec<CategoryDto>, PhoskError>;
pub async fn budget_totals(db: &dyn DatabaseAdapter, as_of: NaiveDate) -> Result<BudgetTotalsDto, PhoskError>;
pub async fn allocation(db: &dyn DatabaseAdapter, as_of: NaiveDate) -> Result<AllocationDto, PhoskError>;
pub async fn category_detail(db: &dyn DatabaseAdapter, as_of: NaiveDate, name: &str) -> Result<CategoryDetailDto, PhoskError>;
pub async fn category_transactions(db: &dyn DatabaseAdapter, as_of: NaiveDate, name: &str) -> Result<Vec<CategoryTxnDto>, PhoskError>;
pub async fn alerts(db: &dyn DatabaseAdapter, as_of: NaiveDate) -> Result<Vec<AlertDto>, PhoskError>;
pub async fn set_cap(db: &dyn DatabaseAdapter, name: &str, cap: Option<Money>) -> Result<(), PhoskError>;
pub async fn act_on_alert(db: &dyn DatabaseAdapter, alert: &str, action: &str) -> Result<(), PhoskError>;
```

Derived (per category):
- `spent` = `Σ` current-cycle receipts in this category (spend-to-date `[start, as_of]`).
- `proj` (`projectedSpend`) = `spent * N / d` (run-rate; matches `phosk_planning::totals`).
- `remaining` = `cap − spent` (signed).
- `usedPct` = `round(100 * spent / cap)` (0 if cap is 0/unlimited).
- `overCapAmount` (detail) = `max(0, proj − cap)`.
- `histAvg` (detail) = trailing-N-cycle average spend (§6).
- `items` = entry count this cycle. `spark`/`hist` = chart series from daily/per-cycle spend.

Budget totals:
- `allocated` = `Σ` caps. `spent` = `Σ` all current-cycle spend-to-date. `projected` = `Σ` per-cat proj.
- `remaining` = `budget − spent`. `overAllocated` = `max(0, allocated − budget)`.
- `unallocated` = `max(0, budget − allocated)`. `envelopeCount` = count of caps.

Allocation segments: `cap` (width), `share = cap / Σcaps`, `fixed`. `aiAdvice` = canned line.

Alerts engine rules (generate from data; tone/kind per todo §2):
- over_budget: category `spent > cap` ⇒ tone `alert`.
- at_risk: `usedPct > 80` OR `proj > cap` ⇒ tone `warn`.
- savings: `saved` on track vs target ⇒ tone `info`.
- recurring_missing (cross-context w/ recurring): expected charge not seen this cycle.
`actions` carry labels (`VIEW`/`RAISE CAP`/`DISMISS`/`SNOOZE`/`MARK PAID`).

## 5.3 `phosk_recurring` (NEW crate — subscriptions · recurring detection)

Spec: `subscriptions.rs`, `dashboard.rs` (`RecurringDto`/`RecurringListDto`). DTOs: all of
`subscriptions.rs` + the dashboard `RecurringDto`/`RecurringListDto`.

```rust
pub async fn list_subscriptions(db: &dyn DatabaseAdapter, as_of: NaiveDate, filter: SubFilter) -> Result<Vec<SubscriptionDto>, PhoskError>;
pub async fn subscription_stats(db: &dyn DatabaseAdapter, as_of: NaiveDate) -> Result<SubStatsDto, PhoskError>;
pub async fn billing_sweep(db: &dyn DatabaseAdapter, as_of: NaiveDate) -> Result<BillingSweepDto, PhoskError>;
pub async fn subscription_detail(db: &dyn DatabaseAdapter, as_of: NaiveDate, slug: &str) -> Result<SubscriptionDetailDto, PhoskError>;
pub async fn recurring_summary(db: &dyn DatabaseAdapter, as_of: NaiveDate) -> Result<RecurringListDto, PhoskError>; // dashboard
```

Derived (THE crux — exact):
- `monthlyEquiv` = monthly: `amount`; yearly: `amount / 12` (integer-centime).
- `annual` = monthly: `amount * 12`; yearly: `amount`.
- `daysUntil` (monthly) = whole days from `as_of` to the next occurrence of day-of-month `day`
  (compute next date ≥ as_of with that day; cross month/year boundary). ≤ 0 ⇒ due. Yearly: days to
  next `(month, day)`.
- `nextLabel` = `"DD MON"` for the next charge date (formatting at the edge, but the DTO carries the
  label string here).
- `priceRose` = last charge amount > prior charge amount (from `subscription_charges` hist).
- `statusLabel` = map of `status`: `due`→"NOT SEEN", `soon`→"DUE SOON", `watch`→"REVIEW",
  `paused`→"PAUSED", else "ACTIVE".
- Stats: `count`, `monthly` = `Σ monthlyEquiv`, `annual` = `monthly * 12`, `autoCount` = count
  `source == LlmInferred`. `next30` = monthly charges with `daysUntil ∈ 0..=30`, soonest first, with
  `total = Σ amount`. `flagged` = subs in `watch`/`due`.
- Billing sweep: impulses = monthly subs, `status = "paid"` if `day < d` (current day index) else the
  sub's status. `footer.paidThisCycle` = `Σ` paid impulse amounts; `stillDue` = `Σ` non-paid.
- AI detection (`detect`, skeleton): scan receipts for repeated same-shop same-amount monthly charges
  ⇒ `Subscription { source: LlmInferred, status: ... }`; confirm flips to `UserEntered`.

`Cargo.toml` deps: `phosk_core`, `phosk_model`, `phosk_adapter_db`, `phosk_id`, `serde`, `chrono`,
`tracing`; dev: `phosk_db_memory`, `serde_json`, `tokio`.

## 5.4 `phosk_debts` (NEW crate — institutional debts · personal IOUs)

Spec: `debts.rs`. DTOs: all of `debts.rs`.

```rust
pub async fn list_debts(db: &dyn DatabaseAdapter, as_of: NaiveDate) -> Result<Vec<DebtDto>, PhoskError>;
pub async fn debt_stats(db: &dyn DatabaseAdapter, as_of: NaiveDate) -> Result<DebtStatsDto, PhoskError>;
pub async fn trajectory(db: &dyn DatabaseAdapter, as_of: NaiveDate, strategy: &str) -> Result<TrajectoryDto, PhoskError>;
pub async fn debt_detail(db: &dyn DatabaseAdapter, slug: &str) -> Result<DebtDetailDto, PhoskError>;
pub async fn debt_payments(db: &dyn DatabaseAdapter, slug: &str) -> Result<Vec<DebtPaymentDto>, PhoskError>;
pub async fn list_personal_ious(db: &dyn DatabaseAdapter) -> Result<Vec<PersonalIouDto>, PhoskError>;
pub async fn iou_stats(db: &dyn DatabaseAdapter) -> Result<IouStatsDto, PhoskError>;
```

Amortization engine (THE crux — exact, integer-centime, no panics):
- `monthlyRate` = `apr / 12`. `annualInterest` = `round(balance_centimes * apr)`.
- `monthsToPayoff`: amortize `balance` at `monthlyRate` paying `monthly`; count months until balance ≤ 0.
  **Cap at 600.** If `monthly <= monthly_interest` (`balance * monthlyRate`), the balance never
  amortizes ⇒ `monthsToPayoff = 600` (revolving/unknown; UI renders `—`).
- `interestRemaining`: `Σ` interest over the amortization life. When `payment <= monthly interest`
  (revolving) ⇒ treat as effectively infinite — use a large sentinel (the spec uses
  `annualInterest * 5` for revolving; KEEP that exact rule for seed determinism) so tests pin a value.
  Non-revolving: `annualInterest * monthsToPayoff / 12`.
- `paidOffPct` = `(orig − balance) / orig` (0..1; 0 if orig is 0).
- `statusLabel`: `high`→"HIGH INTEREST", `due`→"DUE SOON", `watch`→"REVIEW", else "ON TRACK".
- `groupLabel`: LEASE|LOAN→"LEASES & LOANS", CARD→"REVOLVING CREDIT", else "OBLIGATIONS".
- `hist` (balance spark): amortizing (non-revolving) = declining `balance + (5-i)*monthly`; revolving
  (CARD or status==high) = rising into today `balance - (5-i)*(monthly/3)`. (Match `debts.rs` exactly.)

Stats:
- `totalOwed`/`totalOrig`/`totalMonthly`/`totalInterestYr` = `Σ` respective fields.
- `weightedApr` = `Σ(balance*apr) / Σ balance` (0 if no balance).
- `paidOffTotalPct` = `(totalOrig − totalOwed) / totalOrig`.
- `avalancheTarget` = debt id with max `apr`. `snowballTarget` = debt id with min `balance`.
- `horizon` / `debtFreeLabel` = months to combined payoff + the projected month label.

Trajectory: history points (negative `m`) + projection to payoff; `xTicks` axis offsets;
`debtFreeLabel`. Detail decay series: `hist` (oldest→today) + `forward` (today→payoff), `todayIndex`.

IOU stats: `owedToYou` = `Σ` dir=="in"; `youOwe` = `Σ` dir=="out"; `net` = difference;
`countIn`/`countOut`. `repaidPct` = `(of − amount) / of`.

`Cargo.toml` deps: same set as `phosk_recurring`.

## 5.5 `phosk_insights` (EXTEND — dashboard + analytics) — see §9 for the centimes refactor

Spec: `dashboard.rs`, `analytics.rs`. Existing: `dashboard_totals`, `spend_series`, `top_shops`.
ADD analytics services + DTOs (`SpendPointDto`, `SpendHistoryDto`, `CyclePointDto`, `SpendStatsDto`,
`MomentumDto`, `WeekdayDto`, `RhythmStatsDto`, `RhythmDto`, `SuggestedCapDto`, `AnalyticsInsightDto`):

```rust
pub async fn spend_history(db: &dyn DatabaseAdapter, as_of: NaiveDate, cycles: u32) -> Result<SpendHistoryDto, PhoskError>;
pub async fn spend_stats(db: &dyn DatabaseAdapter, as_of: NaiveDate, cycles: u32) -> Result<SpendStatsDto, PhoskError>;
pub async fn category_momentum(db: &dyn DatabaseAdapter, as_of: NaiveDate) -> Result<Vec<MomentumDto>, PhoskError>;
pub async fn weekday_rhythm(db: &dyn DatabaseAdapter, as_of: NaiveDate) -> Result<RhythmDto, PhoskError>;
pub async fn analytics_insight(db: &dyn DatabaseAdapter, as_of: NaiveDate) -> Result<AnalyticsInsightDto, PhoskError>;
```

Derived:
- `SpendPointDto.over` = `spend > budget`. `projected` = the current in-progress cycle.
- `SpendStatsDto.avg` = mean spend over trailing 6 cycles. `curVsAvgPct` = `pct_delta(cur, avg)`,
  `curVsPrevPct` = `pct_delta(cur, prev)` where `pct_delta(a,b) = round(100*(a−b)/b)` (0 if b=0).
  `peak`/`low` = max/min spend cycles.
- `MomentumDto.deltaPct` = momentum vs **trailing-N (default 3) cycle average** (§6).
  `priorAvg` = that trailing-N average (Money).
- `RhythmStatsDto.weekendShare` = `round(100 * (Fri+Sat+Sun) / total)` of weekday buckets.
  `max`/`total`/`avg`/`peak` from the 7 buckets.

## 5.6 `phosk_settings` (NEW crate)

Spec: `backend-features-todo.md §6` (no dioxus DTO file yet — derive shapes from the todo + the
`/config` page). Minimal DTOs:

```rust
pub struct PreferenceDto { pub key: String, pub value: String, pub surface: String, pub storedOnDevice: bool }
pub struct SettingsSummaryDto { pub total_preferences: u32, pub changed_count: u32, pub engine: String, pub model: String }

pub async fn preferences(db: &dyn DatabaseAdapter) -> Result<Vec<PreferenceDto>, PhoskError>;
pub async fn settings_summary(db: &dyn DatabaseAdapter) -> Result<SettingsSummaryDto, PhoskError>;
pub async fn set_preference(db: &dyn DatabaseAdapter, key: &str, value: &str) -> Result<(), PhoskError>;
pub async fn reset_preference(db: &dyn DatabaseAdapter, key: &str) -> Result<(), PhoskError>;
/// THE momentum-baseline accessor every analytics/signal service reads:
pub async fn momentum_baseline_cycles(db: &dyn DatabaseAdapter) -> Result<u32, PhoskError>; // default 3
```

`momentum_baseline_cycles` reads the `Preference` key, parses to `u32`, defaults to `3` on
`NotFound` or parse failure. `changed_count` = prefs whose `provenance.source == UserModified`.

`Cargo.toml` deps: `phosk_core`, `phosk_model`, `phosk_adapter_db`, `phosk_id`, `serde`, `tracing`;
dev: `phosk_db_memory`, `serde_json`, `tokio`.

## 5.7 `phosk_ai` (NEW crate — read slice only for now)

Spec: `ai.rs`. DTOs: `AiFeedItemDto`, `AiChatMsgDto`, `AiStatusDto`, `AiPanelDto`. The real
Ollama/GEMMA wiring is deferred; this crate composes the panel from the port (feed items, chat,
status) and exposes the suggestion/feed write skeletons.

```rust
pub async fn ai_panel(db: &dyn DatabaseAdapter) -> Result<AiPanelDto, PhoskError>;
pub async fn dismiss_feed_item(db: &dyn DatabaseAdapter, id: &str) -> Result<(), PhoskError>;
pub async fn send_message(db: &dyn DatabaseAdapter, text: &str) -> Result<AiChatMsgDto, PhoskError>; // canned reply for now
pub async fn clear_chat(db: &dyn DatabaseAdapter) -> Result<(), PhoskError>;
pub async fn dashboard_insight(db: &dyn DatabaseAdapter, as_of: NaiveDate) -> Result<InsightDto, PhoskError>;
```

`AiStatusDto` is seeded (`online`, model "GEMMA4", engine "OLLAMA", location "LOCAL"). `InsightDto`
carries `estimatedSavings` as `money_centimes`.

`Cargo.toml` deps: `phosk_core`, `phosk_model`, `phosk_adapter_db`, `phosk_id`, `serde`, `chrono`,
`tracing`; dev: `phosk_db_memory`, `serde_json`, `tokio`.

---

# 6. The momentum / trailing-N baseline helper (§6 locked)

ONE helper, in `phosk_insights` (re-exported / depended on by `phosk_ledger` signals and
`phosk_planning` histAvg). Signature:

```rust
/// Trailing-N-cycle average of a per-cycle Money series (oldest→newest), EXCLUDING
/// the current in-progress cycle. N defaults to 3 (read from settings:
/// momentum_baseline_cycles). Returns Money::ZERO when there are no prior cycles.
pub fn trailing_avg(prior_cycles: &[Money], n: u32) -> Money;

/// deltaPct = round(100 * (current - trailingAvg) / trailingAvg); 0 when avg == 0.
pub fn delta_pct(current: Money, trailing_avg: Money) -> i32;
```

`trailing_avg` takes the last `n` elements of `prior_cycles` (or all if fewer), sums (checked), and
integer-divides by the count. `n` comes from `phosk_settings::momentum_baseline_cycles(db)`; pass it
in so the helper stays pure. Every `deltaPct`/`priorAvg`/`histAvg` derived field uses these two fns —
do not reimplement the formula per feature.

---

# 7. NEW crates to create (summary)

| Crate | Layer | Purpose | Internal deps |
|---|---|---|---|
| `phosk_id` | L0 | typed id newtypes | (none) |
| `phosk_recurring` | L5 | subscriptions + recurring detection | core, model, adapter_db, id |
| `phosk_debts` | L5 | debts + personal IOUs (amortization, strategy) | core, model, adapter_db, id |
| `phosk_settings` | L5 | preferences, momentum baseline accessor | core, model, adapter_db, id |
| `phosk_ai` | L5 | AI panel read slice + suggestion/feed skeletons | core, model, adapter_db, id |

EXTENDED crates: `phosk_model` (+`phosk_id`, entities, Provenance, CorrectionEvent),
`phosk_adapter_db` (+all port methods), `phosk_db_memory` (+impls & seed), `phosk_ledger`
(+transactions/signals services), `phosk_planning` (+budgets/categories/alerts), `phosk_insights`
(+analytics, +momentum helper, +centimes refactor §9). Add each new crate to the workspace by
existing under `backend/crates/*` (already globbed in root `Cargo.toml`). Add new workspace deps
(`uuid`) under `[workspace.dependencies]`.

---

# 8. Seed additions needed in `phosk_db_memory`

The seed must make EVERY tested derived field deterministic. Keep the existing 67 `Transaction`
line-items (May+June 2026) — `phosk_planning`/`phosk_ledger`/`phosk_insights` dashboard tests pin
those exact totals (June spend-to-date 316_180, full June 322_245, May 378_770). Layer NEW entities
ON TOP, with stable slugs matching the wire DTOs so `*_by_slug` resolves:

- **Receipts + LineItems:** model the `transactions.rs` seed receipts `t1..t9` (slugs `t1`..`t9`),
  with the exact itemised lines from `seed_lines` (t1/t2/t3/t5 itemised, including the low-confidence
  lines `< 0.7` and signal-linked lines). These drive `itemCount`, `lowConfCount`, `signalIds`,
  `avgConfidence`, line `lineTotal` derivations. Anchor amounts in centimes exactly as `transactions.rs`.
- **Signals:** `coffee`, `pain`, `beer`, `gruyere` (tracked) + `energy-drink` (candidate), with the
  12-point `series`, `since`, `unit`, and occurrences that reproduce `cycleSpend`/`cycleQty`/`txns`
  and `deltaPct` (coffee +28, beer −22, etc.). The `series` arrays from `signals.rs::seed_signals`
  ARE the trailing-cycle spend points the momentum helper consumes — seed them so `delta_pct(now,
  trailing_avg(series[..len-1], 3))` yields the pinned percents.
- **CategoryCaps + BudgetHistory:** the 8 channels from `budgets.rs::seed_categories` (Groceries,
  Going out, Coffee & snacks, Transport, Rent, Health insurance, Shopping, Subscriptions) with caps
  in centimes, `fixed` flags, and `hist` per-cycle arrays for `histAvg`/momentum. Note these differ
  from the existing `seed_categories` (uppercase GROCERIES etc. for the dashboard slice) — KEEP both:
  the dashboard slice uses the existing `Category` seed; the budgets page uses the new `CategoryCap`
  seed. Document this dual seed clearly.
- **Subscriptions + Charges:** the 6 subs from `subscriptions.rs::seed_subscriptions` (netflix,
  spotify, icloud, gym, nyt, domain) with `day`, `cadence`, `status`, `source`, `since`, and 3 prior
  charges each (for `priceRose` and `recent`). Slugs = the ids. These pin `daysUntil` against
  `as_of` 2026-06-18 (netflix day 22 ⇒ 4 days, etc.).
- **Debts + Payments + IOUs:** the 4 debts (vw, card, loan, tax) from `debts.rs::seed_debts` with
  exact balance/orig/monthly/apr, the 4 IOUs (i1..i4), and payment history. These pin the
  amortization outputs (`monthsToPayoff` cap, card ⇒ 999/revolving sentinel, tax apr 0).
- **Alerts:** the 3 dashboard alerts (a1/a2/a3) — OR generate them from the cap/spend data via the
  rules engine; seed the underlying overspend condition (Going out at 78% of cap) so the generator
  produces them deterministically.
- **Preferences:** seed `momentum_baseline_cycles = "3"` plus a handful of keys so
  `settings_summary` counts are pinned. Mark one or two as `UserModified` for `changedCount`.
- **AI feed + chat + suggestions:** the feed items, chat transcript, and status from `ai.rs`.

Build each new seed builder as a fallible `fn seed_x() -> Result<Vec<X>, PhoskError>` mapping every
`Money::from_chf`/`NaiveDate::from_ymd_opt`/parse to `PhoskError` (NO unwrap). Store new collections
as `Vec<_>` fields on `MemoryDb`; `MemoryDb::seeded()` populates them.

---

# 9. The `phosk_insights` centimes refactor (locked decision #2)

`phosk_insights` currently serializes money as CHF `f64` (`money_chf`, `money_vec_chf`,
`opt_money_vec_chf`) — this was the old HTTP-edge form. **Refactor every money field OFF CHF-float
ONTO exact centimes**, so `phosk_insights` DTOs match the Dioxus wire form:

- Replace `#[serde(serialize_with = "money_chf")]` ⇒ `#[serde(with = "phosk_model::money_centimes")]`
  on: `TotalsDto.{budget,spent,remaining,allocated,savingsTarget,saved,savingsProjected,
  lastCycleSpent,perDayToStayOnBudget}`, `ShopShareDto.total`, `TopShopsDto.maxTotal`.
- Replace `money_vec_chf` ⇒ a `money_vec_centimes` serde module (copy from `dashboard.rs`) on
  `SpendSeriesDto.{daily,cumulative,pace}`; `opt_money_vec_chf` ⇒ `opt_money_vec_centimes` on
  `SpendSeriesDto.lastCycleCumulative`.
- All NEW analytics money fields (`SpendPointDto.{spend,budget}`, `CyclePointDto.{spend,budget}`,
  `SpendStatsDto.{avg,totalSaved}`, `MomentumDto.{now,budget,priorAvg}`, `SuggestedCapDto.{amount,
  projectedSavings}`) use `money_centimes` from the start.
- Add `serde::Deserialize` to the insights DTOs (they currently only `Serialize`) so they round-trip
  like the dioxus DTOs and tests can deserialize.
- **Update the existing insights tests:** the JSON assertions currently expect CHF floats
  (`json!(4200.0)`, `json!(3161.80)`). After the refactor they must expect exact centimes
  (`json!(420_000)`, `json!(316_180)`, etc.). The `savingsRate`/`spentPct`/`vsLastCyclePct` unitless
  fields are UNCHANGED. Derived VALUES (the centime amounts) do not change — only their JSON encoding.
- The `dioxus-app/src/data/dashboard.rs` re-shaping shim (which today converts insights' Money fields
  one-by-one) becomes a trivial pass-through, but that is frontend code — do NOT edit it.

---

# 10. Build / test discipline for swarm agents

- Verify a crate compiles with `cargo build -p <crate>` — NEVER a full workspace build (it pulls the
  Dioxus frontend). Test with `cargo test -p <crate>`.
- When growing the port, `phosk_db_memory` MUST stay compiling (implement every new trait method,
  even if `todo!()` for untested write paths). Tested read paths must be real against the seed.
- Each derived-field test asserts EXACT centime integers against the seed at `as_of = 2026-06-18`.
- No `unwrap`/`expect`/`panic!` outside tests; map to `PhoskError`. `[lints] workspace = true` in
  every new crate's `Cargo.toml`.
