//! `phosk_ocr_paddle` — the **concrete L3 `PaddleOCR` adapter** (ADR-010 layering).
//!
//! An [`OcrAdapter`] impl that talks to a standalone **`PaddleOCR`** microservice
//! over HTTP (the `PaddleHub` `hubserving` `ocr_system` shape). This is the
//! production OCR path for the photo-import pipeline: a raw receipt image goes
//! in as bytes, the service returns recognised text + quad regions, and this
//! adapter maps that vendor JSON into the port's vendor-neutral [`OcrResult`].
//!
//! ## Layering
//!
//! This crate is **composition-root-only**: it is wired in by `bin/*` or the
//! Dioxus server context and **never** imported by a feature crate. It is the
//! one place `reqwest` and the raw `PaddleOCR` response JSON are allowed to live —
//! those vendor types **die here**; only [`OcrResult`] / [`PhoskError`] cross
//! the seam. It depends on the L2 port ([`phosk_adapter_ocr`]) and the
//! foundation ([`phosk_core`]), nothing upward.
//!
//! ## Wire shape
//!
//! `PaddleOCR`'s hubserving API accepts a JSON body `{"images": ["<base64>"]}`
//! posted to `/predict/ocr_system` and returns:
//!
//! ```json
//! {
//!   "msg": "",
//!   "results": [[
//!     {"confidence": 0.99, "text": "MIGROS", "text_region": [[40,30],[360,30],[360,74],[40,74]]}
//!   ]],
//!   "status": "000"
//! }
//! ```
//!
//! `text_region` is a 4-point quad (clockwise from top-left) in image pixels;
//! we reduce it to the port's axis-aligned `(x, y, width, height)` bbox. The
//! outer `results` array has one entry per input image — we send exactly one.
//!
//! ## Config
//!
//! The service URL comes from `PHOSK_OCR_URL` (default
//! `http://localhost:8868`); the endpoint path `/predict/ocr_system` is fixed by
//! the hubserving convention. Errors map to [`PhoskError`]: an unreachable or
//! erroring service, a non-success HTTP status, or unparseable JSON all become
//! [`PhoskError::Invalid`] (the photo pipeline treats a failed OCR stage as a
//! retriable/flagged input, not a crash). The adapter **never** panics.

use async_trait::async_trait;
use base64::Engine as _;
use phosk_adapter_ocr::{OcrAdapter, OcrRegion, OcrResult};
use phosk_core::error::PhoskError;
use serde::Deserialize;

/// Default `PaddleOCR` microservice base URL (`PaddleHub` serving default port).
pub const DEFAULT_OCR_URL: &str = "http://localhost:8868";

/// Environment variable that overrides the service base URL.
pub const OCR_URL_ENV: &str = "PHOSK_OCR_URL";

/// The fixed hubserving endpoint path for the `ocr_system` module.
const OCR_ENDPOINT: &str = "/predict/ocr_system";

/// Concrete [`OcrAdapter`] backed by a `PaddleOCR` HTTP microservice.
///
/// Holds a reusable [`reqwest::Client`] and the resolved base URL. Construct
/// with [`PaddleOcr::new`] (explicit URL) or [`PaddleOcr::from_env`] (reads
/// `PHOSK_OCR_URL`, falling back to [`DEFAULT_OCR_URL`]).
#[derive(Debug, Clone)]
pub struct PaddleOcr {
    client: reqwest::Client,
    base_url: String,
}

impl PaddleOcr {
    /// Build an adapter pointed at `base_url` (e.g. `http://localhost:8868`).
    ///
    /// A trailing slash on `base_url` is tolerated. The endpoint path
    /// (`/predict/ocr_system`) is appended internally.
    ///
    /// # Errors
    ///
    /// Returns [`PhoskError::Invalid`] if the reqwest client cannot be built
    /// (e.g. the TLS backend fails to initialise).
    pub fn new(base_url: impl Into<String>) -> Result<Self, PhoskError> {
        let client = reqwest::Client::builder()
            .build()
            .map_err(|e| PhoskError::Invalid(format!("paddle ocr: client build failed: {e}")))?;
        Ok(Self {
            client,
            base_url: base_url.into(),
        })
    }

    /// Build an adapter reading the base URL from `PHOSK_OCR_URL`, defaulting to
    /// [`DEFAULT_OCR_URL`] when the variable is unset or empty.
    ///
    /// # Errors
    ///
    /// Propagates a client-build failure from [`PaddleOcr::new`] as
    /// [`PhoskError::Invalid`].
    pub fn from_env() -> Result<Self, PhoskError> {
        let url = std::env::var(OCR_URL_ENV)
            .ok()
            .filter(|s| !s.trim().is_empty())
            .unwrap_or_else(|| DEFAULT_OCR_URL.to_owned());
        Self::new(url)
    }

    /// The resolved base URL this adapter posts to.
    #[must_use]
    pub fn base_url(&self) -> &str {
        &self.base_url
    }

    /// The fully-qualified predict endpoint (`<base>/predict/ocr_system`).
    fn endpoint(&self) -> String {
        format!("{}{OCR_ENDPOINT}", self.base_url.trim_end_matches('/'))
    }

    /// Liveness probe: is the `PaddleOCR` service reachable at all?
    ///
    /// Used by the gated live integration test to skip cleanly when the service
    /// is not installed. A reachable service returns `Ok(true)`; any transport
    /// failure returns `Ok(false)` (never an `Err`) so callers can branch on a
    /// plain bool without unwrapping a transport error.
    pub async fn is_reachable(&self) -> bool {
        // A bare GET to the base URL; we only care that *something* answered,
        // not the status code (the predict endpoint is POST-only).
        (self.client.get(&self.base_url).send().await).is_ok()
    }
}

/// One recognised box in the `PaddleOCR` hubserving response.
#[derive(Debug, Deserialize)]
struct PaddleBox {
    text: String,
    confidence: f64,
    /// Four `[x, y]` corner points, image pixels, clockwise from top-left.
    text_region: Vec<[f64; 2]>,
}

/// The top-level `PaddleOCR` hubserving response envelope.
#[derive(Debug, Deserialize)]
struct PaddleResponse {
    /// One inner `Vec<PaddleBox>` per input image; we send exactly one image.
    #[serde(default)]
    results: Vec<Vec<PaddleBox>>,
    /// Hubserving status string; `"000"` is success. Optional — some builds omit it.
    #[serde(default)]
    status: Option<String>,
    /// Human-readable error message on failure.
    #[serde(default)]
    msg: Option<String>,
}

/// Reduce a 4-point quad to an axis-aligned `(x, y, width, height)` bbox in
/// pixels, clamping negatives to 0 and rounding to `u32`. An empty/degenerate
/// quad yields a zero box rather than an error (the text still maps through).
fn quad_to_bbox(quad: &[[f64; 2]]) -> (u32, u32, u32, u32) {
    if quad.is_empty() {
        return (0, 0, 0, 0);
    }
    let mut min_x = f64::MAX;
    let mut min_y = f64::MAX;
    let mut max_x = f64::MIN;
    let mut max_y = f64::MIN;
    for &[x, y] in quad {
        min_x = min_x.min(x);
        min_y = min_y.min(y);
        max_x = max_x.max(x);
        max_y = max_y.max(y);
    }
    let clamp = |v: f64| -> u32 {
        // Pre-clamp into `[0, u32::MAX]` so the cast is in-range and lossless on
        // the integer part; negatives/NaN/inf become 0 or u32::MAX. The cast is
        // then a deliberate, bounded f64->u32 (truncation of the already-rounded
        // value is exact in-range) — annotated like the other money/geo casts in
        // the workspace.
        let r = v.round();
        if !r.is_finite() || r <= 0.0 {
            0
        } else if r >= f64::from(u32::MAX) {
            u32::MAX
        } else {
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            {
                r as u32
            }
        }
    };
    let x = clamp(min_x);
    let y = clamp(min_y);
    let w = clamp(max_x).saturating_sub(x);
    let h = clamp(max_y).saturating_sub(y);
    (x, y, w, h)
}

/// Map a parsed `PaddleOCR` envelope into the port's [`OcrResult`].
///
/// Pure (no I/O) so it is unit-testable with a static JSON stub. A non-`"000"`
/// status (when present) is surfaced as [`PhoskError::Invalid`] carrying the
/// service `msg`. `full_text` is the per-region text joined top-to-bottom with
/// newlines, matching the port's `full_text`/`regions` consistency contract.
fn map_response(resp: PaddleResponse) -> Result<OcrResult, PhoskError> {
    if let Some(status) = resp.status.as_deref().filter(|s| *s != "000") {
        let msg = resp.msg.unwrap_or_default();
        return Err(PhoskError::Invalid(format!(
            "paddle ocr: service status {status}: {msg}"
        )));
    }

    // We sent one image, so we read the first inner result list (if any).
    let boxes = resp.results.into_iter().next().unwrap_or_default();

    let regions: Vec<OcrRegion> = boxes
        .into_iter()
        .map(|b| OcrRegion {
            confidence: b.confidence,
            bbox: quad_to_bbox(&b.text_region),
            text: b.text,
        })
        .collect();

    let full_text = regions
        .iter()
        .map(|r| r.text.as_str())
        .collect::<Vec<_>>()
        .join("\n");

    Ok(OcrResult { full_text, regions })
}

#[async_trait]
impl OcrAdapter for PaddleOcr {
    async fn extract(&self, image: &[u8]) -> Result<OcrResult, PhoskError> {
        if image.is_empty() {
            return Err(PhoskError::Invalid(
                "paddle ocr: empty image bytes".to_owned(),
            ));
        }

        let b64 = base64::engine::general_purpose::STANDARD.encode(image);
        let body = serde_json::json!({ "images": [b64] });
        let url = self.endpoint();

        tracing::debug!(%url, bytes = image.len(), "PaddleOcr: POST predict");

        let resp = self
            .client
            .post(&url)
            .json(&body)
            .send()
            .await
            .map_err(|e| PhoskError::Invalid(format!("paddle ocr: request failed: {e}")))?;

        let status = resp.status();
        if !status.is_success() {
            return Err(PhoskError::Invalid(format!(
                "paddle ocr: HTTP {}",
                status.as_u16()
            )));
        }

        let parsed: PaddleResponse = resp
            .json()
            .await
            .map_err(|e| PhoskError::Invalid(format!("paddle ocr: bad response JSON: {e}")))?;

        map_response(parsed)
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::float_cmp)]
mod tests {
    use super::*;

    /// A realistic `PaddleOCR` hubserving success envelope for a small receipt.
    fn stub_json() -> serde_json::Value {
        serde_json::json!({
            "msg": "",
            "status": "000",
            "results": [[
                {
                    "text": "MIGROS GENEVE",
                    "confidence": 0.992,
                    "text_region": [[40, 30], [360, 30], [360, 74], [40, 74]]
                },
                {
                    "text": "Bananes Bio 2.45",
                    "confidence": 0.971,
                    "text_region": [[40, 120], [480, 120], [480, 152], [40, 152]]
                },
                {
                    "text": "TOTAL CHF 5.65",
                    "confidence": 0.990,
                    "text_region": [[40, 240], [490, 240], [490, 276], [40, 276]]
                }
            ]]
        })
    }

    fn parse(v: &serde_json::Value) -> PaddleResponse {
        serde_json::from_value(v.clone()).expect("stub parses")
    }

    #[test]
    fn maps_success_envelope_to_result() {
        let out = map_response(parse(&stub_json())).expect("maps");
        assert_eq!(out.regions.len(), 3);
        assert_eq!(out.regions[0].text, "MIGROS GENEVE");
        assert_eq!(out.regions[0].confidence, 0.992);
        assert!(out.full_text.starts_with("MIGROS GENEVE"));
        assert!(out.full_text.contains("TOTAL CHF 5.65"));
        // full_text is newline-joined regions, in order.
        assert_eq!(
            out.full_text,
            "MIGROS GENEVE\nBananes Bio 2.45\nTOTAL CHF 5.65"
        );
    }

    #[test]
    fn quad_reduces_to_axis_aligned_bbox() {
        let out = map_response(parse(&stub_json())).expect("maps");
        // First quad [[40,30],[360,30],[360,74],[40,74]] -> (40,30,320,44).
        assert_eq!(out.regions[0].bbox, (40, 30, 320, 44));
    }

    #[test]
    fn quad_to_bbox_handles_negative_and_empty() {
        assert_eq!(quad_to_bbox(&[]), (0, 0, 0, 0));
        // Negative corner clamps to 0 on origin; width measured to max.
        let b = quad_to_bbox(&[[-5.0, -5.0], [10.0, -5.0], [10.0, 8.0], [-5.0, 8.0]]);
        assert_eq!(b, (0, 0, 10, 8));
    }

    #[test]
    fn non_success_status_is_an_error() {
        let v = serde_json::json!({
            "msg": "model not loaded",
            "status": "101",
            "results": []
        });
        let err = map_response(parse(&v)).expect_err("non-000 is error");
        assert!(matches!(err, PhoskError::Invalid(_)));
        assert!(err.to_string().contains("model not loaded"));
    }

    #[test]
    fn missing_status_is_tolerated() {
        // Some `PaddleOCR` builds omit `status`; treat presence of results as ok.
        let v = serde_json::json!({
            "results": [[
                {"text": "X", "confidence": 0.5, "text_region": [[0,0],[2,0],[2,2],[0,2]]}
            ]]
        });
        let out = map_response(parse(&v)).expect("no status => ok");
        assert_eq!(out.regions.len(), 1);
        assert_eq!(out.regions[0].bbox, (0, 0, 2, 2));
    }

    #[test]
    fn empty_results_yields_empty_result() {
        let v = serde_json::json!({ "status": "000", "results": [[]] });
        let out = map_response(parse(&v)).expect("maps");
        assert!(out.regions.is_empty());
        assert!(out.full_text.is_empty());
    }

    #[tokio::test]
    async fn empty_image_is_rejected_without_network() {
        let ocr = PaddleOcr::new("http://127.0.0.1:9").expect("client builds");
        let err = ocr.extract(&[]).await.expect_err("empty rejected");
        assert!(matches!(err, PhoskError::Invalid(_)));
        assert_eq!(err.http_status(), 400);
    }

    #[test]
    fn from_env_builds_a_usable_adapter() {
        // We cannot mutate env here (`unsafe_code = forbid` forbids the edition-2024
        // `unsafe { remove_var }`), so assert the resolution invariant instead:
        // from_env yields the default base URL UNLESS PHOSK_OCR_URL overrides it.
        let ocr = PaddleOcr::from_env().expect("builds");
        match std::env::var(OCR_URL_ENV)
            .ok()
            .filter(|s| !s.trim().is_empty())
        {
            Some(set) => assert_eq!(ocr.base_url(), set),
            None => assert_eq!(ocr.base_url(), DEFAULT_OCR_URL),
        }
    }

    #[test]
    fn endpoint_tolerates_trailing_slash() {
        let a = PaddleOcr::new("http://h:8868").expect("ok");
        let b = PaddleOcr::new("http://h:8868/").expect("ok");
        assert_eq!(a.endpoint(), "http://h:8868/predict/ocr_system");
        assert_eq!(b.endpoint(), "http://h:8868/predict/ocr_system");
    }

    #[test]
    fn is_object_safe_behind_arc_dyn() {
        let ocr = PaddleOcr::new(DEFAULT_OCR_URL).expect("ok");
        let _adapter: std::sync::Arc<dyn OcrAdapter> = std::sync::Arc::new(ocr);
    }
}
