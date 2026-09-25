//! Debt plan path — adjust plan, refinance (T17). RED stub.

use chrono::NaiveDate;
use serde::{Deserialize, Serialize};

use phosk_adapter_db::DatabaseAdapter;
use phosk_core::error::PhoskError;
use phosk_core::money::Money;

/// A plan adjustment.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlanAdjust {
    /// New monthly.
    #[serde(default, with = "phosk_model::opt_money_centimes")]
    pub monthly: Option<Money>,
    /// New day.
    pub day: Option<u32>,
    /// New remaining term.
    pub remaining_term: Option<u32>,
}

/// A refinance.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Refinance {
    /// New apr.
    pub apr: f64,
    /// New lender.
    pub lender: Option<String>,
    /// New monthly.
    #[serde(default, with = "phosk_model::opt_money_centimes")]
    pub monthly: Option<Money>,
    /// New day.
    pub day: Option<u32>,
    /// New remaining term.
    pub remaining_term: Option<u32>,
}

/// Stub.
///
/// # Errors
/// Always.
pub async fn adjust_plan(
    _db: &dyn DatabaseAdapter,
    _slug: &str,
    _on: NaiveDate,
    _adjust: PlanAdjust,
) -> Result<(), PhoskError> {
    Err(PhoskError::Invalid("not implemented".to_owned()))
}

/// Stub.
///
/// # Errors
/// Always.
pub async fn refinance(
    _db: &dyn DatabaseAdapter,
    _slug: &str,
    _on: NaiveDate,
    _refi: Refinance,
) -> Result<(), PhoskError> {
    Err(PhoskError::Invalid("not implemented".to_owned()))
}
