//! Provenance + the correction audit log (locked decision #4).
//!
//! Two distinct concerns, deliberately NOT per-field history:
//!
//! - [`Provenance`] is embedded in every machine-derivable entity: where a value
//!   came from ([`Source`]) and how confident the machine is ([`Provenance::confidence`]).
//!   Lines below `0.7`, or with a NaN/out-of-range confidence, are "low-confidence"
//!   and the UI flags them in coral (see [`is_low_confidence`]).
//! - [`CorrectionEvent`] is a SEPARATE append-only audit log — one row per
//!   user edit (which entity, which field, old → new, when). It is not stored on
//!   the entity and is not a full field-by-field history.

use chrono::NaiveDate;
use phosk_id::CorrectionId;
use serde::{Deserialize, Serialize};

/// Where a value came from. Drives confidence defaults and the audit trail.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Source {
    /// Read off a receipt by the OCR engine.
    Ocr,
    /// Inferred by the local LLM (category guess, recurring detection, …).
    LlmInferred,
    /// Typed in by the user — authoritative, confidence `1.0`.
    UserEntered,
    /// Edited by the user after the fact — authoritative, confidence `1.0`.
    UserModified,
    /// Brought in by a bulk import (bank export, prior tool).
    Imported,
    /// Produced by a deterministic rule (e.g. the alert engine).
    RuleGenerated,
}

/// The confidence floor below which a machine-derived value is "low-confidence"
/// and surfaced for review (the coral flag).
///
/// This is the single source of truth for that floor: every layer that needs
/// it (backend AI tools, the WASM UI) reads this constant rather than keeping
/// its own copy. Compare against it with [`is_low_confidence`], not a bare
/// `<` — a bare comparison lets a NaN/out-of-range confidence read as
/// "confident" (see [`is_low_confidence`]).
pub const LOW_CONFIDENCE_THRESHOLD: f64 = 0.7;

/// Whether a machine confidence value counts as low-confidence.
///
/// `confidence` is expected in `0.0..=1.0`; model output is hostile input, so
/// anything outside that range — NaN, ±infinity, negative, or above `1.0` — is
/// treated as low-confidence too, never as confident. A bare
/// `confidence < LOW_CONFIDENCE_THRESHOLD` comparison does not catch this:
/// every comparison with NaN is `false`, so a NaN confidence would otherwise
/// compare as "confident".
#[must_use]
pub fn is_low_confidence(confidence: f64) -> bool {
    !(0.0..=1.0).contains(&confidence) || confidence < LOW_CONFIDENCE_THRESHOLD
}

/// Where a value came from + how confident the machine is about it.
///
/// `confidence` is in `0.0..=1.0`. User-authored values are `1.0`; values below
/// [`LOW_CONFIDENCE_THRESHOLD`] are "low-confidence" and surfaced for review
/// (see [`Provenance::is_low_confidence`]).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Provenance {
    /// The origin of the value.
    pub source: Source,
    /// Machine confidence in `0.0..=1.0`. `UserEntered`/`UserModified` ⇒ `1.0`.
    pub confidence: f64,
}

impl Provenance {
    /// A user-authored value: [`Source::UserEntered`] at full confidence.
    #[must_use]
    pub const fn user_entered() -> Self {
        Self {
            source: Source::UserEntered,
            confidence: 1.0,
        }
    }

    /// A user-edited value: [`Source::UserModified`] at full confidence.
    #[must_use]
    pub const fn user_modified() -> Self {
        Self {
            source: Source::UserModified,
            confidence: 1.0,
        }
    }

    /// A value from a bulk import (bank export): [`Source::Imported`] at full
    /// confidence — the bank's booking is the record of the spend.
    #[must_use]
    pub const fn imported() -> Self {
        Self {
            source: Source::Imported,
            confidence: 1.0,
        }
    }

    /// Whether this value is below the [`LOW_CONFIDENCE_THRESHOLD`] review
    /// threshold (the coral flag). See [`is_low_confidence`] for how an
    /// out-of-range confidence (NaN included) is handled.
    #[must_use]
    pub fn is_low_confidence(&self) -> bool {
        is_low_confidence(self.confidence)
    }
}

/// One entry in the correction audit log (locked decision #4): a single user
/// edit of a single field of a single entity.
///
/// `entity_id` is the **stringified** typed id of the corrected entity (the log
/// spans every entity kind, so a uniform string is used rather than an enum of
/// id types).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CorrectionEvent {
    /// Identity of this audit entry.
    pub id: CorrectionId,
    /// Stringified typed id of the corrected entity (receipt / line / …).
    pub entity_id: String,
    /// The field that changed, e.g. `"category"`, `"amount"`, `"shop"`.
    pub field: String,
    /// The value before the edit (stringified for the uniform log).
    pub old_value: String,
    /// The value after the edit (stringified for the uniform log).
    pub new_value: String,
    /// The day the correction was recorded.
    pub at: NaiveDate,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn user_entered_is_full_confidence() {
        let p = Provenance::user_entered();
        assert_eq!(p.source, Source::UserEntered);
        assert!((p.confidence - 1.0).abs() < f64::EPSILON);
        assert!(!p.is_low_confidence());
    }

    #[test]
    fn imported_is_full_confidence_import_source() {
        let p = Provenance::imported();
        assert_eq!(p.source, Source::Imported);
        assert!(!p.is_low_confidence());
    }

    #[test]
    fn low_confidence_threshold_is_strict_below_point_seven() {
        assert!(
            Provenance {
                source: Source::Ocr,
                confidence: 0.69
            }
            .is_low_confidence()
        );
        assert!(
            !Provenance {
                source: Source::Ocr,
                confidence: 0.70
            }
            .is_low_confidence()
        );
    }

    // ── is_low_confidence: hostile-input confidence values ──────────────────

    #[test]
    fn nan_confidence_is_low_confidence() {
        assert!(
            is_low_confidence(f64::NAN),
            "NaN must never compare as confident"
        );
    }

    #[test]
    fn positive_infinity_confidence_is_low_confidence() {
        assert!(is_low_confidence(f64::INFINITY));
    }

    #[test]
    fn negative_infinity_confidence_is_low_confidence() {
        assert!(is_low_confidence(f64::NEG_INFINITY));
    }

    #[test]
    fn negative_confidence_is_low_confidence() {
        assert!(is_low_confidence(-0.1));
    }

    #[test]
    fn confidence_above_one_is_low_confidence() {
        assert!(is_low_confidence(1.5));
    }

    #[test]
    fn confidence_exactly_at_threshold_is_not_low_confidence() {
        assert!(!is_low_confidence(LOW_CONFIDENCE_THRESHOLD));
    }

    #[test]
    fn a_normal_confident_value_is_not_low_confidence() {
        assert!(!is_low_confidence(0.9));
    }

    #[test]
    fn nan_provenance_is_low_confidence_not_confident() {
        // The regression this whole predicate exists for: a bare `confidence
        // < LOW_CONFIDENCE_THRESHOLD` on `Provenance` would let a NaN reading
        // (hostile model output) through as "confident".
        assert!(
            Provenance {
                source: Source::LlmInferred,
                confidence: f64::NAN,
            }
            .is_low_confidence()
        );
    }

    #[test]
    fn provenance_round_trips_through_json() {
        let p = Provenance {
            source: Source::LlmInferred,
            confidence: 0.42,
        };
        let json = serde_json::to_string(&p).expect("serialize");
        let back: Provenance = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(p, back);
    }

    #[test]
    fn correction_event_round_trips_through_json() {
        let ev = CorrectionEvent {
            id: CorrectionId::new(),
            entity_id: "t1".to_owned(),
            field: "category".to_owned(),
            old_value: "GROCERIES".to_owned(),
            new_value: "DINING".to_owned(),
            at: NaiveDate::from_ymd_opt(2026, 6, 18).expect("valid date"),
        };
        let json = serde_json::to_string(&ev).expect("serialize");
        let back: CorrectionEvent = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(ev, back);
    }
}
