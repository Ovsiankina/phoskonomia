//! `phosk_adapter_ocr` — the `OcrAdapter` **port** (ADR-005/010 layering).
//!
//! This is the L2 contract seam between the photo-import pipeline and whatever
//! turns a raw receipt image into text. A receipt photo arrives as opaque bytes
//! (hostile input from the Pi queue); an OCR adapter reads it and returns the
//! [`OcrResult`] — the full transcribed text plus the per-region boxes the
//! downstream LLM extraction uses to ground line items and totals.
//!
//! Per ADR-010 this crate depends only on the foundation
//! ([`phosk_core::error::PhoskError`]) plus serde / async-trait / tracing. It
//! holds **no** concrete adapter: `PaddleOCR` (microservice) and the vision-LLM
//! alternative are L3 impls wired in only by a composition root, and their
//! vendor types (HTTP JSON, model tensors) die at that adapter — they never
//! reach this port or any feature crate.
//!
//! The trait is **async** because OCR is I/O (a subprocess or a network call to
//! a sandboxed microservice) and **object-safe** so a swappable
//! `Arc<dyn OcrAdapter + Send + Sync>` works. That is why it uses
//! [`mod@async_trait`]: it desugars `async fn` in the trait to a boxed future,
//! keeping the trait `dyn`-compatible. [`extract`](OcrAdapter::extract) returns
//! `Result<_, PhoskError>` (the one taxonomy) — an adapter maps its own
//! failures into a `PhoskError` and never panics.
//!
//! [`FakeOcr`] is the deterministic test/dev impl: it ignores the bytes and
//! returns a canned Swiss-receipt transcription so pipeline and feature tests
//! have a stable OCR stage with no `PaddleOCR` / vision model present.

use async_trait::async_trait;
use phosk_core::error::PhoskError;
use serde::{Deserialize, Serialize};

/// One spatially-located run of recognised text within a receipt image.
///
/// `bbox` is the axis-aligned bounding box in **image pixel coordinates** as
/// `(x, y, width, height)`, origin top-left. `confidence` is the adapter's own
/// `0.0..=1.0` certainty for this region; the pipeline flags low-confidence runs
/// (mirroring the LLM `< 0.7` rule) for human review rather than trusting them.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OcrRegion {
    /// The recognised text of this region (already UTF-8, never the raw bytes).
    pub text: String,
    /// Recognition certainty for this region, in `0.0..=1.0`.
    pub confidence: f64,
    /// Bounding box in image pixels: `(x, y, width, height)`, top-left origin.
    pub bbox: (u32, u32, u32, u32),
}

/// The full result of running OCR over one receipt image.
///
/// `full_text` is the whole transcription as the adapter laid it out (typically
/// top-to-bottom, newline-separated). `regions` carries the same text split into
/// positioned [`OcrRegion`]s so downstream extraction can reason about layout
/// (columns, totals at the bottom, etc.). The two are consistent views of the
/// same recognition, not independent data.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OcrResult {
    /// The entire transcribed text of the receipt.
    pub full_text: String,
    /// The per-region breakdown with positions and confidences.
    pub regions: Vec<OcrRegion>,
}

/// The OCR **port** (ADR-005/010): one async, object-safe trait that turns a raw
/// receipt image into an [`OcrResult`]. Swapping OCR technology (`PaddleOCR` ↔ a
/// vision LLM) is a new `impl` of this trait, never a change to pipeline or
/// feature code.
///
/// Object-safe by construction (the single async method goes through
/// [`async_trait`]; no generics, no `Self`-typed returns) so it can live behind
/// `Arc<dyn OcrAdapter + Send + Sync>`.
#[async_trait]
pub trait OcrAdapter: Send + Sync {
    /// Transcribe `image` (raw, untrusted encoded bytes — PNG/JPEG/etc.) into an
    /// [`OcrResult`].
    ///
    /// # Errors
    ///
    /// Returns a [`PhoskError`] if the bytes cannot be decoded or the OCR engine
    /// fails. Adapters validate the input themselves (the bytes are hostile) and
    /// surface every failure through `PhoskError` — they never panic.
    async fn extract(&self, image: &[u8]) -> Result<OcrResult, PhoskError>;
}

/// Deterministic, dependency-free [`OcrAdapter`] for tests and offline dev.
///
/// It **ignores** the image bytes and always returns the same canned
/// Swiss-receipt transcription, giving pipeline and feature tests a stable OCR
/// stage when no `PaddleOCR` service or vision model is available. It rejects an
/// **empty** byte slice with [`PhoskError::Invalid`] so callers still exercise
/// the error path (the photo pipeline must reject zero-length blobs from the
/// queue before any real OCR work).
#[derive(Debug, Default, Clone, Copy)]
pub struct FakeOcr;

impl FakeOcr {
    /// Construct the fake adapter.
    #[must_use]
    pub const fn new() -> Self {
        Self
    }
}

#[async_trait]
impl OcrAdapter for FakeOcr {
    async fn extract(&self, image: &[u8]) -> Result<OcrResult, PhoskError> {
        if image.is_empty() {
            return Err(PhoskError::Invalid("ocr: empty image bytes".to_owned()));
        }
        tracing::debug!(bytes = image.len(), "FakeOcr: returning canned receipt");

        // A small, realistic Migros-style Swiss receipt. Stable forever so tests
        // can assert exact text / region geometry.
        let regions = vec![
            OcrRegion {
                text: "MIGROS GENEVE".to_owned(),
                confidence: 0.99,
                bbox: (40, 30, 320, 44),
            },
            OcrRegion {
                text: "Bananes Bio".to_owned(),
                confidence: 0.97,
                bbox: (40, 120, 220, 32),
            },
            OcrRegion {
                text: "2.45".to_owned(),
                confidence: 0.98,
                bbox: (400, 120, 80, 32),
            },
            OcrRegion {
                text: "Pain complet".to_owned(),
                confidence: 0.95,
                bbox: (40, 160, 240, 32),
            },
            OcrRegion {
                text: "3.20".to_owned(),
                confidence: 0.96,
                bbox: (400, 160, 80, 32),
            },
            OcrRegion {
                text: "TOTAL CHF".to_owned(),
                confidence: 0.99,
                bbox: (40, 240, 200, 36),
            },
            OcrRegion {
                text: "5.65".to_owned(),
                confidence: 0.99,
                bbox: (400, 240, 90, 36),
            },
        ];

        let full_text =
            "MIGROS GENEVE\nBananes Bio 2.45\nPain complet 3.20\nTOTAL CHF 5.65\n".to_owned();

        Ok(OcrResult { full_text, regions })
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::float_cmp)]
mod tests {
    use super::*;

    fn fake() -> FakeOcr {
        FakeOcr::new()
    }

    #[tokio::test]
    async fn extract_returns_canned_receipt() {
        let out = fake()
            .extract(b"\x89PNG fake bytes")
            .await
            .expect("fake ocr never fails on non-empty input");

        assert!(out.full_text.starts_with("MIGROS GENEVE"));
        assert!(out.full_text.contains("TOTAL CHF 5.65"));
        assert_eq!(out.regions.len(), 7);
    }

    #[tokio::test]
    async fn extract_is_deterministic() {
        let a = fake().extract(b"one").await.expect("ok");
        let b = fake().extract(b"two - different bytes").await.expect("ok");
        // Canned: identical regardless of input bytes.
        assert_eq!(a, b);
    }

    #[tokio::test]
    async fn empty_image_is_rejected() {
        let err = fake()
            .extract(&[])
            .await
            .expect_err("empty bytes must be rejected");
        assert!(matches!(err, PhoskError::Invalid(_)));
        assert_eq!(err.http_status(), 400);
    }

    #[tokio::test]
    async fn regions_carry_positions_and_confidence() {
        let out = fake().extract(b"x").await.expect("ok");
        let total = out
            .regions
            .iter()
            .find(|r| r.text == "5.65")
            .expect("total region present");
        assert_eq!(total.bbox, (400, 240, 90, 36));
        assert!(
            !phosk_model::is_low_confidence(total.confidence),
            "total should be high-confidence"
        );
    }

    #[test]
    fn result_round_trips_through_json() {
        let original = OcrResult {
            full_text: "A\nB".to_owned(),
            regions: vec![OcrRegion {
                text: "A".to_owned(),
                confidence: 0.81,
                bbox: (1, 2, 3, 4),
            }],
        };
        let json = serde_json::to_string(&original).expect("serialize");
        let back: OcrResult = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(original, back);
    }

    #[test]
    fn is_object_safe_behind_arc_dyn() {
        // Compile-time proof the port stays object-safe / swappable.
        let _adapter: std::sync::Arc<dyn OcrAdapter> = std::sync::Arc::new(FakeOcr::new());
    }
}
