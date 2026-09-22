//! `phosk_debts` — the debts + personal-IOU read-model (ADR-002 L5 feature crate, F3).
//!
//! A SERVICE over the [`DatabaseAdapter`] PORT (ADR-010 gateway). It backs the
//! Debts page: the payoff-trajectory hero, the KPI band with strategy targets,
//! the debt card/row grid with amortization-derived payoff meters, the
//! right-dock inspector (balance decay series + payment history), and the
//! personal-IOU net-position beam.
//!
//! Two feature modules, mirroring the dioxus wire structs in
//! `frontend/dioxus-app/src/data/debts.rs`:
//!
//! - [`debts`] — institutional debts: list, stats, trajectory, detail, payments.
//! - [`personal_ious`] — personal IOUs: list + net-position stats.
//! - [`iou_write`] — the personal-IOU write path: create · edit · delete ·
//!   record payment · settle.
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

pub mod debts;
pub mod iou_write;
pub mod personal_ious;

pub use debts::{
    DebtDetailDto, DebtDto, DebtPaymentDto, DebtStatsDto, DecaySeriesDto, TrajPointDto,
    TrajectoryDto,
};
pub use debts::{debt_detail, debt_payments, debt_stats, list_debts, trajectory};
pub use iou_write::{NewPersonalIou, PersonalIouEdit};
pub use iou_write::{
    create_personal_iou, delete_personal_iou, edit_personal_iou, record_iou_payment,
    settle_personal_iou,
};
pub use personal_ious::{IouStatsDto, PersonalIouDto};
pub use personal_ious::{iou_stats, list_personal_ious};
