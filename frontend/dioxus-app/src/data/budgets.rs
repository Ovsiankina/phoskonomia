//! Budgets + categories read-model (F3).
//!
//! Shared by the Budgets page (envelopes, allocation console, KPI band, channel
//! inspector) and the Dashboard (the `c-channels` strip + category-budgets
//! matrix). Mirrors React `GET /categories`, `/budget/totals`,
//! `/budget/allocation`, `/categories/{name}`, `/categories/{name}/transactions`.

use dioxus::prelude::*;
use phosk_core::money::Money;
use serde::{Deserialize, Serialize};

/// One budget envelope (`GET /categories` element; also the dashboard channels).
///
/// `budget` is the cap; `spent`/`proj`/`remaining` the cycle figures (proj =
/// projected end-of-cycle); `used_pct` the integer percent of cap used; `fixed`
/// marks an untunable standing charge; `items` the entry count; `spark` the
/// sparkline points; `hist` the per-cycle history bars; `next` the fixed-charge
/// due label; `note` the AI guidance line.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CategoryDto {
    /// Category name (its identity), e.g. `"Groceries"`.
    pub name: String,
    /// Cap / budget for the cycle.
    #[serde(with = "phosk_model::money_centimes")]
    pub budget: Money,
    /// Spent so far this cycle.
    #[serde(with = "phosk_model::money_centimes")]
    pub spent: Money,
    /// Projected end-of-cycle spend.
    #[serde(with = "phosk_model::money_centimes")]
    pub proj: Money,
    /// `budget − spent` (negative if over).
    #[serde(with = "phosk_model::money_centimes")]
    pub remaining: Money,
    /// Integer percent of cap used (0–999).
    pub used_pct: i32,
    /// `true` for a fixed/standing charge (untunable).
    pub fixed: bool,
    /// Entry count this cycle.
    pub items: u32,
    /// Sparkline points (unitless daily spend).
    pub spark: Vec<f64>,
    /// Per-cycle history bars (CHF as raw chart numbers — presentation series).
    pub hist: Vec<f64>,
    /// Due label for a fixed charge (empty otherwise).
    pub next: String,
    /// One-line AI guidance for this channel.
    pub note: String,
}

/// `GET /budget/totals` — the Budgets KPI band figures.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BudgetTotalsDto {
    /// Monthly budget ceiling.
    #[serde(with = "phosk_model::money_centimes")]
    pub budget: Money,
    /// Sum of all caps.
    #[serde(with = "phosk_model::money_centimes")]
    pub allocated: Money,
    /// Spent so far.
    #[serde(with = "phosk_model::money_centimes")]
    pub spent: Money,
    /// Projected end-of-cycle spend.
    #[serde(with = "phosk_model::money_centimes")]
    pub projected: Money,
    /// Budget left.
    #[serde(with = "phosk_model::money_centimes")]
    pub remaining: Money,
    /// CHF allocated beyond budget (0 if none).
    #[serde(with = "phosk_model::money_centimes")]
    pub over_allocated: Money,
    /// CHF budget not yet allocated to a cap.
    #[serde(with = "phosk_model::money_centimes")]
    pub unallocated: Money,
    /// Number of envelopes.
    pub envelope_count: u32,
}

/// One segment of the allocation bar (`AllocationDto::segments` element).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AllocSegmentDto {
    /// Segment / category name.
    pub name: String,
    /// Its cap (drives the segment width).
    #[serde(with = "phosk_model::money_centimes")]
    pub cap: Money,
    /// Share of the bar (0–1). `None` (absent in the wire payload) → the page
    /// falls back to cap/domain; a present `Some(0.0)` renders 0% width — faithful
    /// to the JSX `seg.share != null` presence test (NOT a value `!= 0` test).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub share: Option<f64>,
    /// `true` for fixed charges (rendered hatched).
    pub fixed: bool,
}

/// The GEMMA4 allocation advice line.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AllocAdviceDto {
    /// Model badge.
    pub model: String,
    /// Advice sentence.
    pub text: String,
}

/// `GET /budget/allocation` — the channel-mix bar segments + AI advice.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AllocationDto {
    /// Ordered cap segments.
    pub segments: Vec<AllocSegmentDto>,
    /// AI advice on the mix.
    pub ai_advice: AllocAdviceDto,
}

/// `GET /categories/{name}` — the channel inspector detail.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CategoryDetailDto {
    /// Projected end-of-cycle spend.
    #[serde(with = "phosk_model::money_centimes")]
    pub projected_spend: Money,
    /// N-cycle average spend.
    #[serde(with = "phosk_model::money_centimes")]
    pub hist_avg: Money,
    /// CHF over cap (0 if under).
    #[serde(with = "phosk_model::money_centimes")]
    pub over_cap_amount: Money,
    /// AI guidance paragraph.
    pub guidance: String,
}

/// A category's recent transaction (`GET /categories/{name}/transactions` row).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CategoryTxnDto {
    /// Stable id.
    pub id: String,
    /// Date label, e.g. `"16 JUN"`.
    pub date: String,
    /// Shop.
    pub shop: String,
    /// Amount.
    #[serde(with = "phosk_model::money_centimes")]
    pub amount: Money,
}

/// The budget envelopes (`GET /categories`).
///
/// REAL: composes `phosk_planning::budgets::categories` for the seeded cycle.
#[server]
pub async fn get_categories() -> Result<Vec<CategoryDto>, ServerFnError> {
    #[cfg(feature = "server-deps")]
    {
        let session = crate::data::build_session().await?;
        let cats = phosk_planning::budgets::categories(session.db(), crate::data::today())
            .await
            .map_err(|e| ServerFnError::new(e.to_string()))?;
        Ok(cats.into_iter().map(map_category).collect())
    }
    #[cfg(not(feature = "server-deps"))]
    {
        Err(ServerFnError::new("server-only"))
    }
}

/// The Budgets KPI band (`GET /budget/totals`).
///
/// REAL: composes `phosk_planning::budgets::budget_totals`.
#[server]
pub async fn get_budget_totals() -> Result<BudgetTotalsDto, ServerFnError> {
    #[cfg(feature = "server-deps")]
    {
        let session = crate::data::build_session().await?;
        let t = phosk_planning::budgets::budget_totals(session.db(), crate::data::today())
            .await
            .map_err(|e| ServerFnError::new(e.to_string()))?;
        Ok(BudgetTotalsDto {
            budget: t.budget,
            allocated: t.allocated,
            spent: t.spent,
            projected: t.projected,
            remaining: t.remaining,
            over_allocated: t.over_allocated,
            unallocated: t.unallocated,
            envelope_count: t.envelope_count,
        })
    }
    #[cfg(not(feature = "server-deps"))]
    {
        Err(ServerFnError::new("server-only"))
    }
}

/// The allocation console (`GET /budget/allocation`).
///
/// REAL: composes `phosk_planning::budgets::allocation`.
#[server]
pub async fn get_allocation() -> Result<AllocationDto, ServerFnError> {
    #[cfg(feature = "server-deps")]
    {
        let session = crate::data::build_session().await?;
        let a = phosk_planning::budgets::allocation(session.db(), crate::data::today())
            .await
            .map_err(|e| ServerFnError::new(e.to_string()))?;
        Ok(AllocationDto {
            segments: a
                .segments
                .into_iter()
                .map(|s| AllocSegmentDto {
                    name: s.name,
                    cap: s.cap,
                    share: s.share,
                    fixed: s.fixed,
                })
                .collect(),
            ai_advice: AllocAdviceDto {
                model: a.ai_advice.model,
                text: a.ai_advice.text,
            },
        })
    }
    #[cfg(not(feature = "server-deps"))]
    {
        Err(ServerFnError::new("server-only"))
    }
}

/// One channel's inspector detail (`GET /categories/{name}`).
///
/// REAL: composes `phosk_planning::budgets::category_detail` for the named channel.
#[server]
pub async fn get_category_detail(name: String) -> Result<CategoryDetailDto, ServerFnError> {
    #[cfg(feature = "server-deps")]
    {
        let session = crate::data::build_session().await?;
        let d = phosk_planning::budgets::category_detail(session.db(), crate::data::today(), &name)
            .await
            .map_err(|e| ServerFnError::new(e.to_string()))?;
        Ok(CategoryDetailDto {
            projected_spend: d.projected_spend,
            hist_avg: d.hist_avg,
            over_cap_amount: d.over_cap_amount,
            guidance: d.guidance,
        })
    }
    #[cfg(not(feature = "server-deps"))]
    {
        let _ = name;
        Err(ServerFnError::new("server-only"))
    }
}

/// A category's recent transactions (`GET /categories/{name}/transactions`).
///
/// REAL: composes `phosk_planning::budgets::category_transactions`.
#[server]
pub async fn get_category_transactions(name: String) -> Result<Vec<CategoryTxnDto>, ServerFnError> {
    #[cfg(feature = "server-deps")]
    {
        let session = crate::data::build_session().await?;
        let rows = phosk_planning::budgets::category_transactions(
            session.db(),
            crate::data::today(),
            &name,
        )
        .await
        .map_err(|e| ServerFnError::new(e.to_string()))?;
        Ok(rows
            .into_iter()
            .map(|r| CategoryTxnDto {
                id: r.id,
                date: r.date,
                shop: r.shop,
                amount: r.amount,
            })
            .collect())
    }
    #[cfg(not(feature = "server-deps"))]
    {
        let _ = name;
        Err(ServerFnError::new("server-only"))
    }
}

// ── mappers (service DTO → wire DTO) ───────────────────────────────────────────

/// Map a `phosk_planning` envelope onto the wire [`CategoryDto`].
#[cfg(feature = "server-deps")]
fn map_category(c: phosk_planning::budgets::CategoryDto) -> CategoryDto {
    CategoryDto {
        name: c.name,
        budget: c.budget,
        spent: c.spent,
        proj: c.proj,
        remaining: c.remaining,
        used_pct: c.used_pct,
        fixed: c.fixed,
        items: c.items,
        spark: c.spark,
        hist: c.hist,
        next: c.next,
        note: c.note,
    }
}

// ════════════════════════════════════════════════════════════════════════════
// Inline cap edit (write path)
//
// `set_category_cap` persists a cap the user typed as CHF text. Parsing and
// validation run here, on the server, and are authoritative: the page sends
// the raw text and shows the message that comes back. The logic lives in
// `set_category_cap_with`, which takes the database port, so tests drive it
// against a fresh `MemoryDb` and never touch the process-global stack.
// ════════════════════════════════════════════════════════════════════════════

/// Persist one category's cap from user-typed CHF text.
///
/// REAL: `set_category_cap_with` validates the amount, then composes
/// `phosk_planning::budgets::set_cap` (the adapter stamps `UserModified`).
#[server]
pub async fn set_category_cap(name: String, amount: String) -> Result<(), ServerFnError> {
    #[cfg(feature = "server-deps")]
    {
        let session = crate::data::build_session().await?;
        set_category_cap_with(session.db(), &name, &amount).await
    }
    #[cfg(not(feature = "server-deps"))]
    {
        let _ = (name, amount);
        Err(ServerFnError::new("server-only"))
    }
}

/// Shown when a save fails before the server could answer (transport, decoding).
const CAP_UNREACHABLE: &str = "could not reach the server, cap not saved";

/// The text to show for a failed cap save: the server's own message for a
/// rejection (already page-safe, see `cap_store_error`), a generic line for
/// anything that failed before the server answered.
pub fn cap_error_text(err: &ServerFnError) -> String {
    match err {
        ServerFnError::ServerError { message, .. } if !message.is_empty() => message.clone(),
        _ => CAP_UNREACHABLE.to_string(),
    }
}

/// The editable text for a cap: plain francs, plus centimes only when they are
/// non-zero (`420`, `420.50`). No grouping, so the field reads cleanly and
/// `Money::parse_chf` reads it back exactly.
pub fn cap_input_text(cap: Money) -> String {
    let c = cap.centimes();
    let sign = if c < 0 { "-" } else { "" };
    let abs = c.unsigned_abs();
    let (whole, cents) = (abs / 100, abs % 100);
    if cents == 0 {
        format!("{sign}{whole}")
    } else {
        format!("{sign}{whole}.{cents:02}")
    }
}

/// Largest cap the edit accepts: CHF 10'000'000.00. Far above any household
/// envelope, and far enough below `i64::MAX` that summing every cap (the
/// `budget_totals` allocation) cannot overflow.
#[cfg(feature = "server-deps")]
const MAX_CAP: Money = Money::from_centimes(1_000_000_000);

/// Validate user-typed CHF text and store it as `name`'s cap.
///
/// Order: parse (no store access for garbage), range, then the envelope must
/// exist and must not be a fixed charge. The write goes through
/// `phosk_planning::budgets::set_cap`; the adapter stamps `UserModified`.
///
/// # Errors
/// A `ServerFnError` whose message is page-safe: a parser hint, one of
/// `cap_msg`, never an adapter's own text.
#[cfg(feature = "server-deps")]
pub(crate) async fn set_category_cap_with(
    db: &dyn phosk_adapter_db::DatabaseAdapter,
    name: &str,
    amount: &str,
) -> Result<(), ServerFnError> {
    let cap = parse_cap(amount)?;
    let envelope = db
        .category_cap_by_name(name)
        .await
        .map_err(cap_store_error)?;
    if envelope.fixed {
        return Err(ServerFnError::new(cap_msg::FIXED));
    }
    phosk_planning::budgets::set_cap(db, name, Some(cap))
        .await
        .map_err(cap_store_error)
}

/// Parse the typed amount and check it is a usable cap (`0..=MAX_CAP`).
#[cfg(feature = "server-deps")]
fn parse_cap(amount: &str) -> Result<Money, ServerFnError> {
    use phosk_core::error::PhoskError;
    let cap = Money::parse_chf(amount).map_err(|e| match e {
        // `parse_chf` only returns fixed hints that never echo the input.
        PhoskError::Invalid(hint) => ServerFnError::new(hint),
        other => cap_store_error(other),
    })?;
    if cap < Money::ZERO {
        return Err(ServerFnError::new(cap_msg::NEGATIVE));
    }
    if cap > MAX_CAP {
        return Err(ServerFnError::new(cap_msg::TOO_LARGE));
    }
    Ok(cap)
}

/// User-facing texts of the cap edit: fixed and free of digits, so they never
/// repeat an amount and read the same in any font.
#[cfg(feature = "server-deps")]
mod cap_msg {
    pub(super) const NEGATIVE: &str = "a cap cannot be negative";
    pub(super) const TOO_LARGE: &str = "a cap cannot exceed ten million CHF";
    pub(super) const FIXED: &str = "fixed charges are not tunable here";
    pub(super) const UNKNOWN: &str = "this budget category no longer exists";
    pub(super) const SAVE_FAILED: &str = "could not save the cap, try again";
}

/// Map a store failure to a page-safe error. Adapter errors can carry engine
/// detail (file paths, driver text), so only `NotFound` keeps a specific
/// message and everything else becomes a generic line.
#[cfg(feature = "server-deps")]
fn cap_store_error(err: phosk_core::error::PhoskError) -> ServerFnError {
    use phosk_core::error::PhoskError;
    match err {
        PhoskError::NotFound(_) => ServerFnError::new(cap_msg::UNKNOWN),
        PhoskError::Invalid(_) | PhoskError::InvalidDate(_) | PhoskError::Overflow(_) => {
            ServerFnError::new(cap_msg::SAVE_FAILED)
        }
    }
}

#[cfg(all(test, feature = "server-deps"))]
mod set_category_cap_tests {
    use std::future::Future;
    use std::pin::pin;
    use std::sync::Arc;
    use std::task::{Context, Poll, Wake, Waker};

    use dioxus::prelude::ServerFnError;
    use phosk_adapter_db::DatabaseAdapter;
    use phosk_core::error::PhoskError;
    use phosk_core::money::Money;
    use phosk_db_memory::MemoryDb;
    use phosk_model::{Provenance, Source};

    use super::{
        cap_error_text, cap_input_text, cap_msg, cap_store_error, set_category_cap_with,
        CAP_UNREACHABLE,
    };

    /// Minimal std-only executor. The memory adapter never waits on I/O, so a
    /// thread-park waker is enough and the tests need no async runtime.
    fn block_on<F: Future>(fut: F) -> F::Output {
        struct Unpark(std::thread::Thread);
        impl Wake for Unpark {
            fn wake(self: Arc<Self>) {
                self.0.unpark();
            }
        }
        let waker = Waker::from(Arc::new(Unpark(std::thread::current())));
        let mut cx = Context::from_waker(&waker);
        let mut fut = pin!(fut);
        loop {
            match fut.as_mut().poll(&mut cx) {
                Poll::Ready(out) => return out,
                Poll::Pending => std::thread::park(),
            }
        }
    }

    /// A fresh seeded store per test: never the process-global stack.
    fn fresh_db() -> MemoryDb {
        MemoryDb::seeded().expect("seeded memory db")
    }

    fn save(db: &MemoryDb, name: &str, amount: &str) -> Result<(), ServerFnError> {
        block_on(set_category_cap_with(db, name, amount))
    }

    fn cap_of(db: &MemoryDb, name: &str) -> Option<i64> {
        block_on(db.category_cap_by_name(name))
            .expect("seeded category")
            .cap
            .map(Money::centimes)
    }

    fn provenance_of(db: &MemoryDb, name: &str) -> Provenance {
        block_on(db.category_cap_by_name(name))
            .expect("seeded category")
            .provenance
    }

    /// The message of a rejected save, as the page would show it.
    fn rejection(result: Result<(), ServerFnError>) -> String {
        cap_error_text(&result.expect_err("the save should be rejected"))
    }

    #[test]
    fn saves_typed_chf_as_exact_centimes() {
        let db = fresh_db();
        save(&db, "Going out", "350.50").expect("valid cap");
        assert_eq!(cap_of(&db, "Going out"), Some(35_050));
    }

    #[test]
    fn accepts_the_grouped_form_the_page_displays() {
        let db = fresh_db();
        save(&db, "Groceries", " CHF 1\u{2019}234.5 ").expect("valid cap");
        assert_eq!(cap_of(&db, "Groceries"), Some(123_450));
    }

    #[test]
    fn zero_is_a_valid_cap() {
        let db = fresh_db();
        save(&db, "Shopping", "0").expect("zero cap");
        assert_eq!(cap_of(&db, "Shopping"), Some(0));
    }

    #[test]
    fn stamps_user_modified_provenance() {
        let db = fresh_db();
        assert_eq!(provenance_of(&db, "Transport").source, Source::UserEntered);
        save(&db, "Transport", "200").expect("valid cap");
        assert_eq!(provenance_of(&db, "Transport"), Provenance::user_modified());
    }

    #[test]
    fn saved_cap_is_what_the_budgets_read_returns() {
        let db = fresh_db();
        save(&db, "Shopping", "321.40").expect("valid cap");
        let cats = block_on(phosk_planning::budgets::categories(
            &db,
            crate::data::today(),
        ))
        .expect("categories read");
        let shopping = cats
            .iter()
            .find(|c| c.name == "Shopping")
            .expect("Shopping listed");
        assert_eq!(shopping.budget, Money::from_centimes(32_140));
    }

    #[test]
    fn rejects_negative_caps_and_leaves_the_store_alone() {
        for amount in ["-10", "\u{2212}0.05", "CHF -1'000"] {
            let db = fresh_db();
            assert_eq!(
                rejection(save(&db, "Going out", amount)),
                cap_msg::NEGATIVE,
                "{amount:?}"
            );
            assert_eq!(cap_of(&db, "Going out"), Some(40_000), "{amount:?}");
            assert_eq!(
                provenance_of(&db, "Going out"),
                Provenance::user_entered(),
                "{amount:?}"
            );
        }
    }

    #[test]
    fn rejects_garbage_with_the_parser_hint() {
        let db = fresh_db();
        for amount in ["", "  ", "abc", "12,50", "12.345", "1e3", "NaN", "350 CHF"] {
            let shown = rejection(save(&db, "Going out", amount));
            let hint = Money::parse_chf(amount).expect_err("garbage");
            assert_eq!(PhoskError::Invalid(shown), hint, "{amount:?}");
        }
        assert_eq!(cap_of(&db, "Going out"), Some(40_000));
        assert_eq!(provenance_of(&db, "Going out"), Provenance::user_entered());
    }

    #[test]
    fn rejects_overflow_and_absurd_caps_with_a_clear_message() {
        let db = fresh_db();
        let shown = rejection(save(&db, "Going out", "99999999999999999999"));
        assert!(shown.contains("too large"), "{shown}");
        assert_eq!(
            rejection(save(&db, "Going out", "10000000.01")),
            cap_msg::TOO_LARGE
        );
        assert_eq!(cap_of(&db, "Going out"), Some(40_000));

        save(&db, "Going out", "10'000'000").expect("the ceiling itself is allowed");
        assert_eq!(cap_of(&db, "Going out"), Some(1_000_000_000));
    }

    #[test]
    fn caps_at_the_ceiling_keep_the_budget_totals_computable() {
        let db = fresh_db();
        for name in [
            "Groceries",
            "Going out",
            "Coffee & snacks",
            "Transport",
            "Shopping",
            "Subscriptions",
        ] {
            save(&db, name, "10000000").expect("ceiling cap");
        }
        let totals = block_on(phosk_planning::budgets::budget_totals(
            &db,
            crate::data::today(),
        ))
        .expect("totals still add up");
        assert!(totals.allocated > Money::from_centimes(6_000_000_000));
    }

    #[test]
    fn unknown_category_is_a_clear_not_found() {
        let db = fresh_db();
        assert_eq!(rejection(save(&db, "Nonexistent", "100")), cap_msg::UNKNOWN);
    }

    #[test]
    fn fixed_charges_are_not_tunable() {
        let db = fresh_db();
        assert_eq!(rejection(save(&db, "Rent", "100")), cap_msg::FIXED);
        assert_eq!(cap_of(&db, "Rent"), Some(168_000));
        assert_eq!(provenance_of(&db, "Rent"), Provenance::user_entered());
    }

    #[test]
    fn store_failures_never_reach_the_page_verbatim() {
        let internal = [
            PhoskError::Invalid("engine detail: store file unwritable".to_owned()),
            PhoskError::InvalidDate("engine detail".to_owned()),
            PhoskError::Overflow("engine detail".to_owned()),
        ];
        for err in internal {
            assert_eq!(cap_error_text(&cap_store_error(err)), cap_msg::SAVE_FAILED);
        }
        assert_eq!(
            cap_error_text(&cap_store_error(PhoskError::NotFound(
                "category X".to_owned()
            ))),
            cap_msg::UNKNOWN
        );
    }

    #[test]
    fn page_shows_the_server_message_or_a_generic_line() {
        assert_eq!(
            cap_error_text(&ServerFnError::new(cap_msg::NEGATIVE)),
            cap_msg::NEGATIVE
        );
        assert_eq!(
            cap_error_text(&ServerFnError::Deserialization("bad body".to_owned())),
            CAP_UNREACHABLE
        );
        assert_eq!(cap_error_text(&ServerFnError::new("")), CAP_UNREACHABLE);
    }

    #[test]
    fn input_text_is_plain_and_parses_back_exactly() {
        assert_eq!(cap_input_text(Money::from_centimes(42_000)), "420");
        assert_eq!(cap_input_text(Money::from_centimes(35_050)), "350.50");
        assert_eq!(cap_input_text(Money::from_centimes(5)), "0.05");
        for c in [0, 5, 50, 42_000, 35_050, 123_456_789, -530] {
            let text = cap_input_text(Money::from_centimes(c));
            assert_eq!(
                Money::parse_chf(&text).map(Money::centimes),
                Ok(c),
                "{text:?}"
            );
        }
    }
}

// ── end of inline cap edit block ─────────────────────────────────────────────
