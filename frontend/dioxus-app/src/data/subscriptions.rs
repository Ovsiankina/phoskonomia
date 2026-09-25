//! Subscriptions read-model (F3).
//!
//! Backs the Subscriptions page: the billing-sweep impulse train, the KPI band,
//! the tunable card/row grid and the right-dock inspector. Mirrors React
//! `GET /subscriptions` (sort/group/amounts params), `/subscriptions/stats`,
//! `/subscriptions/billing-sweep`, `/subscriptions/{id}`.

use dioxus::prelude::*;
use phosk_core::money::Money;
use serde::{Deserialize, Serialize};

/// One standing charge (`GET /subscriptions` element).
///
/// `cadence` is `"monthly"|"yearly"`; `status` `"ok"|"soon"|"due"|"watch"|
/// "paused"` (with a human `status_label`); `monthly_equiv`/`annual` are the
/// derived run-rates; `days_until`/`next_label` the cycle countdown; `source`
/// `"user"|"llm"` (auto-detected); `hist` the price-history bars; `glyph` the
/// card badge.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SubscriptionDto {
    /// Stable id.
    pub id: String,
    /// Service name, e.g. `"Netflix"`.
    pub name: String,
    /// Per-charge amount.
    #[serde(with = "phosk_model::money_centimes")]
    pub amount: Money,
    /// `"monthly"` or `"yearly"`.
    pub cadence: String,
    /// Status key.
    pub status: String,
    /// Human status label, e.g. `"DUE SOON"`.
    pub status_label: String,
    /// Monthly-equivalent run-rate.
    #[serde(with = "phosk_model::money_centimes")]
    pub monthly_equiv: Money,
    /// Annualized total.
    #[serde(with = "phosk_model::money_centimes")]
    pub annual: Money,
    /// Whole days until the next charge (monthly only; ≤0 = due).
    pub days_until: i32,
    /// Next-charge label, e.g. `"22 JUN"`.
    pub next_label: String,
    /// Day-of-month for monthly charges (0 for yearly).
    pub day: u32,
    /// Month label for yearly charges (empty for monthly).
    pub month: String,
    /// `"user"` or `"llm"` (auto-detected).
    pub source: String,
    /// Category, e.g. `"Entertainment"`.
    pub category: String,
    /// Card badge glyph, e.g. `"▶"`.
    pub glyph: String,
    /// Tracking-since label.
    pub since: String,
    /// `true` if the last charge rose vs the prior one.
    pub price_rose: bool,
    /// Price-history bars (raw chart numbers).
    pub hist: Vec<f64>,
    /// One-line note / AI guidance.
    pub note: String,
}

/// `GET /subscriptions/stats` — the KPI band roll-ups.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SubStatsDto {
    /// Active standing-charge count.
    pub count: u32,
    /// Monthly run-rate.
    #[serde(with = "phosk_model::money_centimes")]
    pub monthly: Money,
    /// Annualized total.
    #[serde(with = "phosk_model::money_centimes")]
    pub annual: Money,
    /// Count auto-detected by the AI.
    pub auto_count: u32,
    /// Next-30-days roll-up.
    pub next30: Next30Dto,
    /// Needs-attention roll-up.
    pub flagged: FlaggedDto,
}

/// The "next 30 days" KPI roll-up (`SubStatsDto::next30`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Next30Dto {
    /// Number of charges due in the next 30 days.
    pub count: u32,
    /// Their combined total.
    #[serde(with = "phosk_model::money_centimes")]
    pub total: Money,
    /// The charges, soonest first.
    pub items: Vec<Next30ItemDto>,
}

/// One upcoming charge in the next-30 roll-up.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Next30ItemDto {
    /// Service name.
    pub name: String,
    /// Days until it charges.
    pub days_until: i32,
}

/// The "needs attention" KPI roll-up (`SubStatsDto::flagged`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FlaggedDto {
    /// Number flagged by the AI for review.
    pub count: u32,
    /// Supporting note.
    pub note: String,
}

/// One impulse on the billing sweep (`BillingSweepDto::impulses` element).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImpulseDto {
    /// Subscription id (selects the inspector).
    pub id: String,
    /// Service name.
    pub name: String,
    /// Charge amount.
    #[serde(with = "phosk_model::money_centimes")]
    pub amount: Money,
    /// Day-of-cycle the charge lands on.
    pub day: u32,
    /// Status: `"paid"|"soon"|"due"|"watch"|"ok"`.
    pub status: String,
}

/// The sweep's cycle window (`BillingSweepDto::cycle`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SweepCycleDto {
    /// Current day-of-cycle (the TODAY marker).
    pub day: u32,
    /// Days in the cycle.
    pub days: u32,
    /// Short "today" label.
    pub as_of: String,
}

/// The next charge shown in the sweep footer.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SweepNextDto {
    /// Service name.
    pub name: String,
    /// Next-charge label.
    pub next_label: String,
    /// Amount.
    #[serde(with = "phosk_model::money_centimes")]
    pub amount: Money,
}

/// The sweep footer roll-up (`BillingSweepDto::footer`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SweepFooterDto {
    /// CHF paid so far this cycle.
    #[serde(with = "phosk_model::money_centimes")]
    pub paid_this_cycle: Money,
    /// CHF still due this cycle.
    #[serde(with = "phosk_model::money_centimes")]
    pub still_due: Money,
    /// The next upcoming charge.
    pub next: SweepNextDto,
    /// A short footer note.
    pub note: String,
}

/// `GET /subscriptions/billing-sweep` — the periodic impulse train.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BillingSweepDto {
    /// Cycle window for the sweep axis.
    pub cycle: SweepCycleDto,
    /// The charge impulses.
    pub impulses: Vec<ImpulseDto>,
    /// Footer roll-up.
    pub footer: SweepFooterDto,
}

/// One recorded charge in the inspector (`SubscriptionDetailDto::recent` row).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SubChargeDto {
    /// Stable id.
    pub id: String,
    /// Date label.
    pub date: String,
    /// Note, e.g. `"confirmed"`.
    pub note: String,
    /// Amount.
    #[serde(with = "phosk_model::money_centimes")]
    pub amount: Money,
}

/// The inspector guidance line (`SubscriptionDetailDto::guidance`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SubGuidanceDto {
    /// Guidance text.
    pub text: String,
    /// Severity, e.g. `"coral"` or empty.
    pub severity: String,
}

/// `GET /subscriptions/{id}` — the inspector payload (the list record + extras).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SubscriptionDetailDto {
    /// The headline subscription record.
    #[serde(flatten)]
    pub subscription: SubscriptionDto,
    /// Recent recorded charges.
    pub recent: Vec<SubChargeDto>,
    /// AI guidance.
    pub guidance: SubGuidanceDto,
    /// `true` for an AI candidate (CONFIRM/DISMISS instead of cancel).
    pub candidate: bool,
}

/// Subscription list options for `GET /subscriptions`.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SubFilter {
    /// Sort key: `due|amount|name`.
    pub sort: String,
    /// `"cadence"` to group, else empty.
    pub group: String,
    /// `monthly|annual` amount display mode (display only here).
    pub amounts: String,
}

/// The standing charges (`GET /subscriptions`).
///
/// REAL: composes `phosk_recurring::subscriptions::list_subscriptions` (honours
/// the `sort`/`group` params) for the seeded cycle.
#[server]
pub async fn list_subscriptions(filter: SubFilter) -> Result<Vec<SubscriptionDto>, ServerFnError> {
    #[cfg(feature = "server-deps")]
    {
        let session = crate::data::build_session().await?;
        let svc_filter = phosk_recurring::subscriptions::SubFilter {
            sort: filter.sort,
            group: filter.group,
            amounts: filter.amounts,
        };
        let subs = phosk_recurring::subscriptions::list_subscriptions(
            session.db(),
            crate::data::today(),
            svc_filter,
        )
        .await
        .map_err(crate::data::server_err)?;
        Ok(subs.into_iter().map(map_sub).collect())
    }
    #[cfg(not(feature = "server-deps"))]
    {
        let _ = filter;
        Err(ServerFnError::new("server-only"))
    }
}

/// The KPI band roll-ups (`GET /subscriptions/stats`).
///
/// REAL: composes `phosk_recurring::subscriptions::subscription_stats`.
#[server]
pub async fn get_subscription_stats() -> Result<SubStatsDto, ServerFnError> {
    #[cfg(feature = "server-deps")]
    {
        let session = crate::data::build_session().await?;
        let s =
            phosk_recurring::subscriptions::subscription_stats(session.db(), crate::data::today())
                .await
                .map_err(crate::data::server_err)?;
        Ok(map_stats(s))
    }
    #[cfg(not(feature = "server-deps"))]
    {
        Err(ServerFnError::new("server-only"))
    }
}

/// The billing-sweep impulse train (`GET /subscriptions/billing-sweep`).
///
/// REAL: composes `phosk_recurring::subscriptions::billing_sweep`.
#[server]
pub async fn get_billing_sweep() -> Result<BillingSweepDto, ServerFnError> {
    #[cfg(feature = "server-deps")]
    {
        let session = crate::data::build_session().await?;
        let b = phosk_recurring::subscriptions::billing_sweep(session.db(), crate::data::today())
            .await
            .map_err(crate::data::server_err)?;
        Ok(BillingSweepDto {
            cycle: SweepCycleDto {
                day: b.cycle.day,
                days: b.cycle.days,
                as_of: b.cycle.as_of,
            },
            impulses: b
                .impulses
                .into_iter()
                .map(|i| ImpulseDto {
                    id: i.id,
                    name: i.name,
                    amount: i.amount,
                    day: i.day,
                    status: i.status,
                })
                .collect(),
            footer: SweepFooterDto {
                paid_this_cycle: b.footer.paid_this_cycle,
                still_due: b.footer.still_due,
                next: SweepNextDto {
                    name: b.footer.next.name,
                    next_label: b.footer.next.next_label,
                    amount: b.footer.next.amount,
                },
                note: b.footer.note,
            },
        })
    }
    #[cfg(not(feature = "server-deps"))]
    {
        Err(ServerFnError::new("server-only"))
    }
}

/// One subscription's inspector payload (`GET /subscriptions/{id}`).
///
/// REAL: composes `phosk_recurring::subscriptions::subscription_detail` for the slug.
#[server]
pub async fn get_subscription(id: String) -> Result<SubscriptionDetailDto, ServerFnError> {
    #[cfg(feature = "server-deps")]
    {
        let session = crate::data::build_session().await?;
        let d = phosk_recurring::subscriptions::subscription_detail(
            session.db(),
            crate::data::today(),
            &id,
        )
        .await
        .map_err(crate::data::server_err)?;
        Ok(SubscriptionDetailDto {
            subscription: map_sub(d.subscription),
            recent: d
                .recent
                .into_iter()
                .map(|c| SubChargeDto {
                    id: c.id,
                    date: c.date,
                    note: c.note,
                    amount: c.amount,
                })
                .collect(),
            guidance: SubGuidanceDto {
                text: d.guidance.text,
                severity: d.guidance.severity,
            },
            candidate: d.candidate,
        })
    }
    #[cfg(not(feature = "server-deps"))]
    {
        let _ = id;
        Err(ServerFnError::new("server-only"))
    }
}

// ── mappers (service DTO → wire DTO) ───────────────────────────────────────────

/// Map a `phosk_recurring` subscription onto the wire [`SubscriptionDto`].
#[cfg(feature = "server-deps")]
fn map_sub(s: phosk_recurring::subscriptions::SubscriptionDto) -> SubscriptionDto {
    SubscriptionDto {
        id: s.id,
        name: s.name,
        amount: s.amount,
        cadence: s.cadence,
        status: s.status,
        status_label: s.status_label,
        monthly_equiv: s.monthly_equiv,
        annual: s.annual,
        days_until: s.days_until,
        next_label: s.next_label,
        day: s.day,
        month: s.month,
        source: s.source,
        category: s.category,
        glyph: s.glyph,
        since: s.since,
        price_rose: s.price_rose,
        hist: s.hist,
        note: s.note,
    }
}

/// Map the `phosk_recurring` stats roll-up onto the wire [`SubStatsDto`].
#[cfg(feature = "server-deps")]
fn map_stats(s: phosk_recurring::subscriptions::SubStatsDto) -> SubStatsDto {
    SubStatsDto {
        count: s.count,
        monthly: s.monthly,
        annual: s.annual,
        auto_count: s.auto_count,
        next30: Next30Dto {
            count: s.next30.count,
            total: s.next30.total,
            items: s
                .next30
                .items
                .into_iter()
                .map(|i| Next30ItemDto {
                    name: i.name,
                    days_until: i.days_until,
                })
                .collect(),
        },
        flagged: FlaggedDto {
            count: s.flagged.count,
            note: s.flagged.note,
        },
    }
}

// ════════════════════════════════════════════════════════════════════════════
//  RECURRING CANDIDATES (T32) — review AI-detected standing charges.
//
//  A candidate is a machine PROPOSAL (`Source::LlmInferred`). It only becomes a
//  user-tracked subscription when a human clicks CONFIRM; nothing in this block
//  (or the page) confirms on its own. The read is `recurring_detect::detect`;
//  the writes are `recurring_detect::{confirm_candidate, dismiss_candidate}`.
//  Each `#[server]` fn only builds the session and delegates to its `*_with`
//  inner fn, which holds the logic and is what the tests drive.
// ════════════════════════════════════════════════════════════════════════════

/// One OPEN AI-detected recurring candidate: not yet confirmed, not dismissed.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RecurringCandidateDto {
    /// Stable id (the proposed subscription slug) — the confirm/dismiss target.
    pub id: String,
    /// Inferred service name.
    pub name: String,
    /// The repeated per-charge amount.
    #[serde(with = "phosk_model::money_centimes")]
    pub amount: Money,
    /// Inferred cadence, `"monthly"` or `"yearly"`.
    pub cadence: String,
    /// Inferred day-of-month of the charge (0 when not monthly).
    pub day: u32,
    /// Number of matching charges observed (the evidence count).
    pub occurrences: u32,
    /// Detection confidence in `0.0..=1.0` (not money).
    pub confidence: f64,
    /// `true` below the `0.7` review threshold — flagged, never dropped.
    pub low_confidence: bool,
    /// Short rationale line from the detector.
    pub rationale: String,
}

/// The open recurring candidates, most-confident first.
///
/// REAL: composes `phosk_recurring::recurring_detect::detect` (read-only).
#[server]
pub async fn list_recurring_candidates() -> Result<Vec<RecurringCandidateDto>, ServerFnError> {
    #[cfg(feature = "server-deps")]
    {
        let session = crate::data::build_session().await?;
        list_recurring_candidates_with(session.db(), crate::data::today()).await
    }
    #[cfg(not(feature = "server-deps"))]
    {
        Err(ServerFnError::new("server-only"))
    }
}

/// Confirm an open candidate — the human approval that turns an AI proposal
/// into a tracked, user-entered subscription.
///
/// REAL: composes `phosk_recurring::recurring_detect::confirm_candidate`.
#[server]
pub async fn confirm_recurring_candidate(id: String) -> Result<(), ServerFnError> {
    #[cfg(feature = "server-deps")]
    {
        let session = crate::data::build_session().await?;
        confirm_recurring_candidate_with(session.db(), &id).await
    }
    #[cfg(not(feature = "server-deps"))]
    {
        let _ = id;
        Err(ServerFnError::new("server-only"))
    }
}

/// Dismiss an open candidate — it stops being proposed and is NOT promoted.
///
/// REAL: composes `phosk_recurring::recurring_detect::dismiss_candidate`.
#[server]
pub async fn dismiss_recurring_candidate(id: String) -> Result<(), ServerFnError> {
    #[cfg(feature = "server-deps")]
    {
        let session = crate::data::build_session().await?;
        dismiss_recurring_candidate_with(session.db(), &id).await
    }
    #[cfg(not(feature = "server-deps"))]
    {
        let _ = id;
        Err(ServerFnError::new("server-only"))
    }
}

/// Longest candidate id accepted (slugs are short; anything longer is junk).
#[cfg(feature = "server-deps")]
const MAX_CANDIDATE_ID_LEN: usize = 64;

/// Authoritative server-side id check: a non-empty ASCII slug
/// (`[A-Za-z0-9_-]`, at most [`MAX_CANDIDATE_ID_LEN`] bytes). The raw input is
/// never copied into the error.
#[cfg(feature = "server-deps")]
fn validate_candidate_id(id: &str) -> Result<&str, phosk_core::error::PhoskError> {
    let well_formed = !id.is_empty()
        && id.len() <= MAX_CANDIDATE_ID_LEN
        && id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_');
    if well_formed {
        Ok(id)
    } else {
        Err(phosk_core::error::PhoskError::Invalid(
            "malformed recurring candidate id".to_owned(),
        ))
    }
}

/// Map a confirm/dismiss failure onto a fixed, user-facing message. The
/// `PhoskError` text (ids, store details) never reaches the client.
#[cfg(feature = "server-deps")]
fn candidate_action_error(e: &phosk_core::error::PhoskError) -> ServerFnError {
    use phosk_core::error::PhoskError;
    let msg = match e {
        PhoskError::Invalid(_) | PhoskError::InvalidDate(_) => "That is not a valid candidate id.",
        PhoskError::NotFound(_) => {
            "This candidate is no longer open: it was already confirmed or dismissed."
        }
        PhoskError::Overflow(_) => "Could not update the candidate. Please try again.",
    };
    ServerFnError::new(msg)
}

/// List the open candidates against `db` as of `as_of`. Read-only.
#[cfg(feature = "server-deps")]
pub(crate) async fn list_recurring_candidates_with(
    db: &dyn phosk_adapter_db::DatabaseAdapter,
    as_of: chrono::NaiveDate,
) -> Result<Vec<RecurringCandidateDto>, ServerFnError> {
    let detection = phosk_recurring::recurring_detect::detect(db, as_of)
        .await
        .map_err(|_| ServerFnError::new("Could not load the recurring candidates."))?;
    Ok(detection
        .candidates
        .into_iter()
        .map(|c| RecurringCandidateDto {
            // The model's own review threshold, not a copy of it.
            low_confidence: phosk_model::is_low_confidence(c.confidence),
            id: c.id,
            name: c.name,
            amount: c.amount,
            cadence: c.cadence,
            day: c.day,
            occurrences: c.occurrences,
            confidence: c.confidence,
            rationale: c.rationale,
        })
        .collect())
}

/// Validate `id`, then confirm that open candidate against `db`. The backend
/// stamps `Provenance::user_entered()` on the promoted subscription.
#[cfg(feature = "server-deps")]
pub(crate) async fn confirm_recurring_candidate_with(
    db: &dyn phosk_adapter_db::DatabaseAdapter,
    id: &str,
) -> Result<(), ServerFnError> {
    let slug = validate_candidate_id(id).map_err(|e| candidate_action_error(&e))?;
    phosk_recurring::recurring_detect::confirm_candidate(db, slug)
        .await
        .map_err(|e| candidate_action_error(&e))
}

/// Validate `id`, then dismiss that open candidate against `db`.
#[cfg(feature = "server-deps")]
pub(crate) async fn dismiss_recurring_candidate_with(
    db: &dyn phosk_adapter_db::DatabaseAdapter,
    id: &str,
) -> Result<(), ServerFnError> {
    let slug = validate_candidate_id(id).map_err(|e| candidate_action_error(&e))?;
    phosk_recurring::recurring_detect::dismiss_candidate(db, slug)
        .await
        .map_err(|e| candidate_action_error(&e))
}

#[cfg(all(test, feature = "server-deps"))]
mod recurring_candidate_tests {
    use super::*;
    use phosk_adapter_db::DatabaseAdapter;
    use phosk_db_memory::MemoryDb;
    use phosk_model::{Provenance, Source};

    /// A FRESH seeded store per test — never the process-global stack.
    fn fresh() -> MemoryDb {
        MemoryDb::seeded().expect("seed is valid")
    }

    /// The user-facing message a `ServerFnError` carries to the client.
    fn message(e: &ServerFnError) -> String {
        match e {
            ServerFnError::ServerError { message, .. } => message.clone(),
            other => panic!("expected a ServerError, got {other:?}"),
        }
    }

    async fn open_ids(db: &MemoryDb) -> Vec<String> {
        list_recurring_candidates_with(db, crate::data::today())
            .await
            .expect("list candidates")
            .into_iter()
            .map(|c| c.id)
            .collect()
    }

    async fn subscription_count(db: &MemoryDb) -> usize {
        db.subscriptions().await.expect("subscriptions").len()
    }

    #[tokio::test]
    async fn lists_the_seeded_open_candidates_most_confident_first() {
        let db = fresh();
        let list = list_recurring_candidates_with(&db, crate::data::today())
            .await
            .expect("list candidates");
        let ids: Vec<&str> = list.iter().map(|c| c.id.as_str()).collect();
        // Both seeded LLM subs sit at 0.72; ties break by slug.
        assert_eq!(ids, ["icloud", "nyt"]);

        let icloud = &list[0];
        assert_eq!(icloud.name, "iCloud+ 2TB");
        assert_eq!(icloud.amount.centimes(), 999, "money crosses as centimes");
        assert_eq!(icloud.cadence, "monthly");
        assert_eq!(icloud.day, 15);
        assert_eq!(icloud.occurrences, 3);
        assert!((0.0..1.0).contains(&icloud.confidence));
        assert!(!icloud.low_confidence, "0.72 is above the 0.7 threshold");
        assert!(!icloud.rationale.is_empty());
    }

    #[tokio::test]
    async fn low_confidence_candidates_are_flagged_not_dropped() {
        let db = fresh();
        let mut nyt = db.subscription_by_slug("nyt").await.expect("nyt");
        nyt.provenance.confidence = 0.55;
        db.upsert_subscription(nyt)
            .await
            .expect("lower nyt confidence");

        let list = list_recurring_candidates_with(&db, crate::data::today())
            .await
            .expect("list candidates");
        let ids: Vec<&str> = list.iter().map(|c| c.id.as_str()).collect();
        assert_eq!(ids, ["icloud", "nyt"], "still listed, most-confident first");
        assert!(!list[0].low_confidence);
        assert!(list[1].low_confidence, "0.55 is below the 0.7 threshold");
    }

    #[tokio::test]
    async fn listing_candidates_never_confirms_them() {
        let db = fresh();
        let _ = open_ids(&db).await;
        let _ = open_ids(&db).await;
        for slug in ["icloud", "nyt"] {
            let sub = db.subscription_by_slug(slug).await.expect("sub");
            assert_eq!(
                sub.source,
                Source::LlmInferred,
                "{slug} is still a proposal"
            );
            assert_eq!(sub.provenance.source, Source::LlmInferred);
        }
        assert_eq!(open_ids(&db).await, ["icloud", "nyt"]);
    }

    #[tokio::test]
    async fn confirm_promotes_the_candidate_and_stamps_user_provenance() {
        let db = fresh();
        let before = db.subscription_by_slug("icloud").await.expect("icloud");
        let count = subscription_count(&db).await;

        confirm_recurring_candidate_with(&db, "icloud")
            .await
            .expect("confirm icloud");

        let after = db.subscription_by_slug("icloud").await.expect("icloud");
        assert_eq!(after.source, Source::UserEntered);
        assert_eq!(after.provenance, Provenance::user_entered());
        // Only the approval changed: same record, same money, same schedule.
        assert_eq!(after.id, before.id);
        assert_eq!(after.amount, before.amount);
        assert_eq!(after.status, before.status);
        assert_eq!(subscription_count(&db).await, count, "no duplicate row");
        assert_eq!(open_ids(&db).await, ["nyt"]);
    }

    #[tokio::test]
    async fn confirming_twice_is_safe() {
        let db = fresh();
        let count = subscription_count(&db).await;
        confirm_recurring_candidate_with(&db, "icloud")
            .await
            .expect("first confirm");

        let err = confirm_recurring_candidate_with(&db, "icloud")
            .await
            .expect_err("a second confirm finds no open candidate");
        assert!(
            message(&err).contains("no longer open"),
            "got: {}",
            message(&err)
        );

        let sub = db.subscription_by_slug("icloud").await.expect("icloud");
        assert_eq!(sub.source, Source::UserEntered, "still confirmed");
        assert_eq!(subscription_count(&db).await, count, "no duplicate row");
    }

    #[tokio::test]
    async fn dismiss_drops_the_candidate_without_promoting_it() {
        let db = fresh();
        let count = subscription_count(&db).await;

        dismiss_recurring_candidate_with(&db, "nyt")
            .await
            .expect("dismiss nyt");

        let sub = db.subscription_by_slug("nyt").await.expect("nyt");
        assert_eq!(sub.source, Source::LlmInferred, "dismiss never promotes");
        assert_eq!(sub.provenance.source, Source::LlmInferred);
        assert_eq!(sub.status, "paused");
        assert_eq!(subscription_count(&db).await, count);
        assert_eq!(open_ids(&db).await, ["icloud"]);
    }

    #[tokio::test]
    async fn dismissing_twice_is_safe_and_a_dismissed_candidate_cannot_be_confirmed() {
        let db = fresh();
        dismiss_recurring_candidate_with(&db, "nyt")
            .await
            .expect("first dismiss");

        let again = dismiss_recurring_candidate_with(&db, "nyt")
            .await
            .expect_err("a second dismiss finds no open candidate");
        assert!(message(&again).contains("no longer open"));

        let confirm = confirm_recurring_candidate_with(&db, "nyt")
            .await
            .expect_err("a dismissed proposal cannot be confirmed");
        assert!(message(&confirm).contains("no longer open"));

        let sub = db.subscription_by_slug("nyt").await.expect("nyt");
        assert_eq!(sub.source, Source::LlmInferred);
        assert_eq!(sub.status, "paused");
    }

    #[tokio::test]
    async fn a_user_entered_subscription_is_not_a_candidate() {
        let db = fresh();
        let before = db.subscription_by_slug("netflix").await.expect("netflix");

        let confirm = confirm_recurring_candidate_with(&db, "netflix")
            .await
            .expect_err("netflix is not a proposal");
        assert!(message(&confirm).contains("no longer open"));
        let dismiss = dismiss_recurring_candidate_with(&db, "netflix")
            .await
            .expect_err("netflix is not a proposal");
        assert!(message(&dismiss).contains("no longer open"));

        let after = db.subscription_by_slug("netflix").await.expect("netflix");
        assert_eq!(after, before, "a non-candidate is left untouched");
    }

    #[tokio::test]
    async fn malformed_ids_are_rejected_server_side() {
        let db = fresh();
        let too_long = "a".repeat(65);
        let bad = [
            "",
            " icloud",
            "icloud\n",
            "../icloud",
            "icloud;DELETE",
            "<script>",
            too_long.as_str(),
        ];
        for id in bad {
            for err in [
                confirm_recurring_candidate_with(&db, id)
                    .await
                    .expect_err("confirm rejects a malformed id"),
                dismiss_recurring_candidate_with(&db, id)
                    .await
                    .expect_err("dismiss rejects a malformed id"),
            ] {
                let msg = message(&err);
                assert!(msg.contains("not a valid candidate id"), "got: {msg}");
                if !id.trim().is_empty() {
                    assert!(!msg.contains(id.trim()), "input is not echoed: {msg}");
                }
            }
        }
        assert_eq!(open_ids(&db).await, ["icloud", "nyt"], "nothing changed");
    }

    #[tokio::test]
    async fn error_messages_do_not_leak_internals() {
        let db = fresh();
        let err = confirm_recurring_candidate_with(&db, "no-such-candidate")
            .await
            .expect_err("unknown id");
        let msg = message(&err);
        assert!(!msg.contains("no-such-candidate"), "no echoed id: {msg}");
        assert!(!msg.contains("not found:"), "no raw PhoskError text: {msg}");
        assert!(
            !msg.contains("recurring candidate "),
            "no raw detail: {msg}"
        );
    }
}
