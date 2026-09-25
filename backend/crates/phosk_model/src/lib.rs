//! `phosk_model` — domain data, the stability surface (ADR-002 L4). Pure data.
//!
//! These are the finance concepts the read-only dashboard slice operates on:
//! a [`Transaction`] (a dated, categorised spend at a shop), a [`Category`]
//! (a spending bucket with an optional budget cap), and the global
//! [`BudgetConfig`] (the cycle ceiling + savings target). They carry **no
//! behaviour** and **no adapter dependencies** — just `phosk_core` for the
//! CHF-typed [`Money`], `chrono` for dates, and `serde` for round-tripping.
//!
//! **IDs / provenance are deliberately out of scope here.** Per ADR-008 the
//! human terms (shop *name*, category *name*) are the identity, and the
//! read-only dashboard never needs receipt/item provenance, so no UUIDs and no
//! provenance fields appear — they would land in a later slice if a write path
//! earns them (ADR-005, ADR-011).
//!
//! **Money serialisation is exact, not CHF-float.** `Money` is serialised as
//! its lossless i64 *centimes* (see [`money_centimes`]), not as a CHF `f64`.
//! The float conversion (`Money::as_chf_f64`) is the HTTP edge's job (ADR-010);
//! the domain — including this serialisation, used at the in-memory-seed /
//! persistence boundary — stays in exact centimes with no float.

use chrono::NaiveDate;
use phosk_core::money::Money;
use serde::{Deserialize, Serialize};

pub mod entities;
pub mod provenance;

pub use entities::{
    AiSuggestion, Alert, AlertAction, AlertSnooze, Budget, BudgetChange, BudgetHistory,
    CategoryCap, Charge, Chat, Debt, DebtPayment, FeedItem, LineItem, Message, PersonalIou,
    Preference, Receipt, Signal, SignalOccurrence, Subscription,
};
pub use provenance::{
    CorrectionEvent, LOW_CONFIDENCE_THRESHOLD, Provenance, Source, is_low_confidence,
};

/// A single dated spend: an `amount` of [`Money`] at a `shop`, classified into a
/// spending `category`. Identity is the human `shop` / `category` *names*
/// (ADR-008), not an opaque id — none is carried.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Transaction {
    /// The calendar date the spend occurred (no time-of-day in this slice).
    pub date: NaiveDate,
    /// The shop's display name, e.g. `"Migros"`. This is its identity (ADR-008).
    pub shop: String,
    /// The spending category's name, e.g. `"GROCERIES"` (ADR-008 identity).
    pub category: String,
    /// The exact amount spent. Serialised as i64 centimes, never CHF-float.
    #[serde(with = "money_centimes")]
    pub amount: Money,
}

/// A spending bucket with an optional budget `cap`. A `None` cap means the
/// category is **unlimited** (no ceiling to compare spend against).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Category {
    /// The category's display name, its identity (ADR-008), e.g. `"HOUSING"`.
    pub name: String,
    /// The budget ceiling for the cycle, or `None` for an unlimited category.
    #[serde(with = "opt_money_centimes")]
    pub cap: Option<Money>,
}

/// The global budget configuration for a cycle: the overall spend ceiling and
/// the savings target the dashboard measures progress against.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BudgetConfig {
    /// The overall spend ceiling for one cycle (e.g. monthly), in exact [`Money`].
    #[serde(with = "money_centimes")]
    pub monthly_budget: Money,
    /// The amount the household aims to save in the cycle, in exact [`Money`].
    #[serde(with = "money_centimes")]
    pub savings_target: Money,
}

/// `serde` adapter that round-trips a [`Money`] through its exact i64 centimes.
///
/// Used via `#[serde(with = "money_centimes")]`. This keeps the domain's wire
/// form lossless (no CHF-`f64` ever enters or leaves a `Money` here); the
/// CHF-number form is only ever produced at the HTTP edge (ADR-010).
pub mod money_centimes {
    use phosk_core::money::Money;
    use serde::{Deserialize, Deserializer, Serializer};

    /// Serialise a [`Money`] as its raw i64 centime count.
    ///
    /// # Errors
    /// Propagates any error the underlying `Serializer` raises.
    pub fn serialize<S>(money: &Money, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_i64(money.centimes())
    }

    /// Deserialise a [`Money`] from a raw i64 centime count.
    ///
    /// # Errors
    /// Propagates any error the underlying `Deserializer` raises (e.g. a value
    /// that is not a valid i64).
    pub fn deserialize<'de, D>(deserializer: D) -> Result<Money, D::Error>
    where
        D: Deserializer<'de>,
    {
        let centimes = i64::deserialize(deserializer)?;
        Ok(Money::from_centimes(centimes))
    }
}

/// `serde` adapter for `Option<Money>` (the [`Category::cap`] field), round-trip
/// through `Option<i64>` centimes. `None` ⇄ JSON `null` (an unlimited category).
pub mod opt_money_centimes {
    use phosk_core::money::Money;
    use serde::{Deserialize, Deserializer, Serializer};

    /// Serialise an `Option<Money>` as `Option<i64>` centimes (`None` → `null`).
    ///
    /// # Errors
    /// Propagates any error the underlying `Serializer` raises.
    pub fn serialize<S>(money: &Option<Money>, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        match money {
            Some(m) => serializer.serialize_some(&m.centimes()),
            None => serializer.serialize_none(),
        }
    }

    /// Deserialise an `Option<Money>` from `Option<i64>` centimes (`null` → `None`).
    ///
    /// # Errors
    /// Propagates any error the underlying `Deserializer` raises.
    pub fn deserialize<'de, D>(deserializer: D) -> Result<Option<Money>, D::Error>
    where
        D: Deserializer<'de>,
    {
        let centimes = Option::<i64>::deserialize(deserializer)?;
        Ok(centimes.map(Money::from_centimes))
    }
}

/// `serde` adapter for `Vec<Money>` — round-trips through `Vec<i64>` centimes.
///
/// Used via `#[serde(with = "money_vec_centimes")]` on chart-series fields
/// (e.g. a per-cycle spend spark). Exact centimes throughout — no CHF-float ever
/// enters the wire form (ADR-010, locked decision #2).
pub mod money_vec_centimes {
    use phosk_core::money::Money;
    use serde::{Deserialize, Deserializer, Serialize, Serializer};

    /// Serialise a `Vec<Money>` as a `Vec<i64>` of raw centime counts.
    ///
    /// # Errors
    /// Propagates any error the underlying `Serializer` raises.
    pub fn serialize<S>(money: &[Money], serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let centimes: Vec<i64> = money.iter().map(|m| m.centimes()).collect();
        centimes.serialize(serializer)
    }

    /// Deserialise a `Vec<Money>` from a `Vec<i64>` of raw centime counts.
    ///
    /// # Errors
    /// Propagates any error the underlying `Deserializer` raises.
    pub fn deserialize<'de, D>(deserializer: D) -> Result<Vec<Money>, D::Error>
    where
        D: Deserializer<'de>,
    {
        let centimes = Vec::<i64>::deserialize(deserializer)?;
        Ok(centimes.into_iter().map(Money::from_centimes).collect())
    }
}

/// `serde` adapter for `Option<Vec<Money>>` — round-trips through
/// `Option<Vec<i64>>` centimes (`None` ⇄ JSON `null`).
///
/// Used via `#[serde(with = "opt_money_vec_centimes")]` on optional comparison
/// series (e.g. a prior cycle's cumulative spend that may be absent).
pub mod opt_money_vec_centimes {
    use phosk_core::money::Money;
    use serde::{Deserialize, Deserializer, Serializer};

    /// Serialise an `Option<Vec<Money>>` as `Option<Vec<i64>>` (`None` → `null`).
    ///
    /// # Errors
    /// Propagates any error the underlying `Serializer` raises.
    pub fn serialize<S>(money: &Option<Vec<Money>>, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        match money {
            Some(v) => {
                let centimes: Vec<i64> = v.iter().map(|m| m.centimes()).collect();
                serializer.serialize_some(&centimes)
            }
            None => serializer.serialize_none(),
        }
    }

    /// Deserialise an `Option<Vec<Money>>` from `Option<Vec<i64>>` (`null` → `None`).
    ///
    /// # Errors
    /// Propagates any error the underlying `Deserializer` raises.
    pub fn deserialize<'de, D>(deserializer: D) -> Result<Option<Vec<Money>>, D::Error>
    where
        D: Deserializer<'de>,
    {
        let centimes = Option::<Vec<i64>>::deserialize(deserializer)?;
        Ok(centimes.map(|v| v.into_iter().map(Money::from_centimes).collect()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn naive(y: i32, m: u32, d: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, d).expect("valid test date")
    }

    // ── Transaction ────────────────────────────────────────────────────────
    #[test]
    fn transaction_construction_holds_its_fields() {
        let tx = Transaction {
            date: naive(2026, 6, 1),
            shop: "Migros".to_owned(),
            category: "GROCERIES".to_owned(),
            amount: Money::from_chf(58, 75).expect("valid amount"),
        };
        assert_eq!(tx.date, naive(2026, 6, 1));
        assert_eq!(tx.shop, "Migros");
        assert_eq!(tx.category, "GROCERIES");
        assert_eq!(tx.amount.centimes(), 5875);
    }

    #[test]
    fn transaction_roundtrips_through_json() {
        let tx = Transaction {
            date: naive(2026, 6, 1),
            shop: "Landlord".to_owned(),
            category: "HOUSING".to_owned(),
            amount: Money::from_chf(1680, 0).expect("valid amount"),
        };
        let json = serde_json::to_string(&tx).expect("serialize");
        let back: Transaction = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(tx, back);
    }

    #[test]
    fn transaction_amount_serialises_as_exact_centimes_not_chf_float() {
        let tx = Transaction {
            date: naive(2026, 6, 18),
            shop: "Coop Pronto".to_owned(),
            category: "GROCERIES".to_owned(),
            amount: Money::from_chf(12, 40).expect("valid amount"),
        };
        let value: serde_json::Value = serde_json::to_value(&tx).expect("to value");
        // The wire form is the lossless i64 centime count (1240), never 12.4 CHF.
        assert_eq!(value["amount"], serde_json::json!(1240));
        assert!(value["amount"].is_i64());
    }

    #[test]
    fn transaction_negative_amount_roundtrips() {
        let tx = Transaction {
            date: naive(2026, 6, 2),
            shop: "Coop".to_owned(),
            category: "GROCERIES".to_owned(),
            amount: Money::from_centimes(-1500), // a refund / correction
        };
        let json = serde_json::to_string(&tx).expect("serialize");
        let back: Transaction = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(tx, back);
        assert_eq!(back.amount.centimes(), -1500);
    }

    // ── Category ─────────────────────────────────────────────────────────────
    #[test]
    fn category_with_cap_construction() {
        let c = Category {
            name: "GROCERIES".to_owned(),
            cap: Some(Money::from_chf(800, 0).expect("valid")),
        };
        assert_eq!(c.name, "GROCERIES");
        assert_eq!(c.cap.expect("some").centimes(), 80_000);
    }

    #[test]
    fn category_unlimited_has_no_cap() {
        let c = Category {
            name: "MISC".to_owned(),
            cap: None,
        };
        assert!(c.cap.is_none());
    }

    #[test]
    fn category_with_cap_roundtrips_through_json() {
        let c = Category {
            name: "HOUSING".to_owned(),
            cap: Some(Money::from_chf(1680, 0).expect("valid")),
        };
        let json = serde_json::to_string(&c).expect("serialize");
        let back: Category = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(c, back);
    }

    #[test]
    fn category_unlimited_roundtrips_and_serialises_cap_as_null() {
        let c = Category {
            name: "MISC".to_owned(),
            cap: None,
        };
        let value: serde_json::Value = serde_json::to_value(&c).expect("to value");
        assert!(value["cap"].is_null());

        let json = serde_json::to_string(&c).expect("serialize");
        let back: Category = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(c, back);
        assert!(back.cap.is_none());
    }

    #[test]
    fn category_cap_serialises_as_exact_centimes() {
        let c = Category {
            name: "DINING & CAFÉS".to_owned(),
            cap: Some(Money::from_chf(350, 0).expect("valid")),
        };
        let value: serde_json::Value = serde_json::to_value(&c).expect("to value");
        assert_eq!(value["cap"], serde_json::json!(35_000));
    }

    // ── BudgetConfig ─────────────────────────────────────────────────────────
    #[test]
    fn budget_config_construction() {
        let cfg = BudgetConfig {
            monthly_budget: Money::from_chf(4200, 0).expect("valid"),
            savings_target: Money::from_chf(900, 0).expect("valid"),
        };
        assert_eq!(cfg.monthly_budget.centimes(), 420_000);
        assert_eq!(cfg.savings_target.centimes(), 90_000);
    }

    #[test]
    fn budget_config_roundtrips_through_json() {
        let cfg = BudgetConfig {
            monthly_budget: Money::from_chf(4200, 0).expect("valid"),
            savings_target: Money::from_chf(900, 0).expect("valid"),
        };
        let json = serde_json::to_string(&cfg).expect("serialize");
        let back: BudgetConfig = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(cfg, back);
    }

    #[test]
    fn budget_config_serialises_both_amounts_as_exact_centimes() {
        let cfg = BudgetConfig {
            monthly_budget: Money::from_chf(4200, 0).expect("valid"),
            savings_target: Money::from_chf(900, 0).expect("valid"),
        };
        let value: serde_json::Value = serde_json::to_value(&cfg).expect("to value");
        assert_eq!(value["monthly_budget"], serde_json::json!(420_000));
        assert_eq!(value["savings_target"], serde_json::json!(90_000));
    }

    // ── Cross-type clone / equality ──────────────────────────────────────────
    #[test]
    fn types_are_clone_and_eq() {
        let tx = Transaction {
            date: naive(2026, 6, 5),
            shop: "Netflix".to_owned(),
            category: "SUBSCRIPTIONS".to_owned(),
            amount: Money::from_chf(14, 95).expect("valid"),
        };
        assert_eq!(tx.clone(), tx);
    }
}
