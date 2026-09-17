//! Provenance + the correction audit log (locked decision #4).
//!
//! Two distinct concerns, deliberately NOT per-field history:
//!
//! - [`Provenance`] is embedded in every machine-derivable entity: where a value
//!   came from ([`Source`]) and how confident the machine is ([`Provenance::confidence`]).
//!   Lines below `0.7` are "low-confidence" and the UI flags them in coral.
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

/// Where a value came from + how confident the machine is about it.
///
/// `confidence` is in `0.0..=1.0`. User-authored values are `1.0`; values below
/// `0.7` are "low-confidence" and surfaced for review (see [`Provenance::is_low_confidence`]).
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

    /// Whether this value is below the `0.7` review threshold (the coral flag).
    #[must_use]
    pub fn is_low_confidence(&self) -> bool {
        self.confidence < 0.7
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
