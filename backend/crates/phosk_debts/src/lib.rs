//! `phosk_debts` — the debts + personal-IOU read-model (ADR-002 L5 feature crate, F3).
//!
//! A SERVICE over the [`DatabaseAdapter`] PORT (ADR-010 gateway). It backs the
//! Debts page: the payoff-trajectory hero, the KPI band with strategy targets,
//! the debt card/row grid with amortization-derived payoff meters, the
//! right-dock inspector (balance decay series + payment history), and the
//! personal-IOU net-position beam.
//!
//! Four feature modules; the read DTOs mirror the dioxus wire structs in
//! `frontend/dioxus-app/src/data/debts.rs`:
//!
//! - [`debts`] — institutional debts: list, stats, trajectory, detail, payments.
//! - [`debt_write`] — the debt write path: create / edit / delete, record a
//!   scheduled instalment, record an extra (principal-only) payment.
//! - [`debt_plan`] — plan changes: adjust the instalment / day / term,
//!   refinance the outstanding balance.
//! - [`personal_ious`] — personal IOUs: list + net-position stats.
//!
//! **Layering (ADR-010).** Every service fn takes `&dyn DatabaseAdapter` (the
//! PORT) plus the read context (`as_of: NaiveDate` for label/trajectory math, or
//! a `slug` for a single entity) and returns `Result<Dto, PhoskError>`. It
//! depends only on the PORT trait crate and the domain/foundation crates — never
//! on a concrete adapter (`phosk_db_memory` is a *dev*-dependency, tests only).
//!
//! **Money is exact centimes (centimes-everywhere).** Every money field
//! serializes as its lossless i64 centime count via
//! [`phosk_model::money_centimes`]. CHF `f64` never appears on the wire. There is
//! no `unwrap`/`expect`/`panic!` in this code: every fallible step maps to a
//! [`PhoskError`].
//!
//! The DTO shapes and signatures are fixed (the wire contract); the service
//! bodies and amortization helpers compute the derivations from persisted data.
//!
//! [`DatabaseAdapter`]: phosk_adapter_db::DatabaseAdapter
//! [`PhoskError`]: phosk_core::error::PhoskError

pub mod debt_plan;
pub mod debt_write;
pub mod debts;
pub mod personal_ious;

pub use debt_plan::{PlanAdjust, Refinance, adjust_plan, refinance};
pub use debt_write::{DebtEdit, NewDebt, NewDebtPayment};
pub use debt_write::{create_debt, delete_debt, edit_debt, extra_payment, record_payment};

pub use debts::{
    DebtDetailDto, DebtDto, DebtPaymentDto, DebtStatsDto, DecaySeriesDto, TrajPointDto,
    TrajectoryDto,
};
pub use debts::{debt_detail, debt_payments, debt_stats, list_debts, trajectory};
pub use personal_ious::{IouStatsDto, PersonalIouDto};
pub use personal_ious::{iou_stats, list_personal_ious};
