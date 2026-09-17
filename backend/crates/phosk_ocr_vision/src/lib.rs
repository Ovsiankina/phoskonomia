//! `phosk_ocr_vision` — the **concrete L3 vision-LLM OCR adapter** (ADR-010).
//!
//! An [`OcrAdapter`] impl that transcribes a receipt image using a
//! **vision-capable** model served by local **Ollama**. It is the swappable
//! alternative to the [`phosk_ocr_paddle`](../phosk_ocr_paddle/index.html)
//! microservice: same port, different technology. The pipeline picks one at the
//! composition root; feature code never knows which.
//!
//! ## How it works
//!
//! It POSTs to Ollama's `/api/generate` with the image base64-encoded in the
//! `images[]` field and a fixed prompt instructing the model to return **only**
//! JSON of the shape `{"regions":[{"text":..,"confidence":..,"bbox":[x,y,w,h]}]}`.
//! `format: "json"` is set so Ollama constrains the output to valid JSON
//! (ADR-008). The adapter then parses that JSON into the port's [`OcrResult`].
//! Because a model can still wrap JSON in prose or fences despite `format:json`,
//! the parser is defensive: it slices to the outer `{...}` before deserialising.
//!
//! ## Layering
//!
//! Composition-root-only; **never** imported by a feature crate. `reqwest` and
//! the raw Ollama response JSON live **only** here and die at this adapter —
//! only [`OcrResult`] / [`PhoskError`] cross the seam. Depends on the L2 port
//! ([`phosk_adapter_ocr`]) + foundation ([`phosk_core`]) and nothing upward.
//!
//! ## Config & gating
//!
//! The Ollama base URL comes from `PHOSK_OCR_VISION_URL` (default
//! `http://localhost:11434`); the model from `PHOSK_OCR_VISION_MODEL` (default
//! `llava`). [`OllamaVisionOcr::detect_vision_model`] lists installed models and
//! asks `/api/show` for each one's reported capabilities, returning the first
//! that advertises `vision`. Detection is **capability-based, not name-based**, so
//! a custom-named model such as `qwen3.6:35b-custom` is correctly recognised; the
//! gated live test runs against whatever vision-capable model is installed and
//! skips cleanly only when Ollama is down or no model reports `vision`.
//! Every failure maps to [`PhoskError`]; the adapter never panics.

use async_trait::async_trait;
use base64::Engine as _;
use phosk_adapter_ocr::{OcrAdapter, OcrRegion, OcrResult};
use phosk_core::error::PhoskError;
use serde::Deserialize;

/// Default Ollama base URL.
pub const DEFAULT_OLLAMA_URL: &str = "http://localhost:11434";

/// Environment variable overriding the Ollama base URL.
pub const OLLAMA_URL_ENV: &str = "PHOSK_OCR_VISION_URL";

/// Environment variable selecting the vision model.
pub const VISION_MODEL_ENV: &str = "PHOSK_OCR_VISION_MODEL";

/// Default vision model name (only used if the env var is unset).
pub const DEFAULT_VISION_MODEL: &str = "llava";

/// The instruction sent to the vision model. Kept as a `const` per the
/// prompts-as-Rust-consts rule; asks for strict JSON the parser expects.
const OCR_PROMPT: &str = "\
You are an OCR engine. Transcribe ALL text visible in this receipt image. \
Respond with ONLY a JSON object, no prose, no markdown fences, of the exact shape:\n\
{\"regions\":[{\"text\":\"<line text>\",\"confidence\":<0.0-1.0>,\"bbox\":[x,y,width,height]}]}\n\
Each region is one line or token of text. bbox is in image pixels, integers, \
top-left origin. If you cannot estimate a box, use [0,0,0,0]. Preserve reading order.";

/// Concrete [`OcrAdapter`] backed by an Ollama vision model.
#[derive(Debug, Clone)]
pub struct OllamaVisionOcr {
    client: reqwest::Client,
    base_url: String,
    model: String,
}

impl OllamaVisionOcr {
    /// Build an adapter for `base_url` + `model`.
    ///
    /// # Errors
    ///
    /// [`PhoskError::Invalid`] if the reqwest client cannot be built.
    pub fn new(base_url: impl Into<String>, model: impl Into<String>) -> Result<Self, PhoskError> {
        let client = reqwest::Client::builder()
            .build()
            .map_err(|e| PhoskError::Invalid(format!("vision ocr: client build failed: {e}")))?;
        Ok(Self {
            client,
            base_url: base_url.into(),
            model: model.into(),
        })
    }

    /// Build from the environment: `PHOSK_OCR_VISION_URL` (default
    /// [`DEFAULT_OLLAMA_URL`]) and `PHOSK_OCR_VISION_MODEL` (default
    /// [`DEFAULT_VISION_MODEL`]).
    ///
    /// # Errors
    ///
    /// Propagates a client-build failure as [`PhoskError::Invalid`].
    pub fn from_env() -> Result<Self, PhoskError> {
        let url = std::env::var(OLLAMA_URL_ENV)
            .ok()
            .filter(|s| !s.trim().is_empty())
            .unwrap_or_else(|| DEFAULT_OLLAMA_URL.to_owned());
        let model = std::env::var(VISION_MODEL_ENV)
            .ok()
            .filter(|s| !s.trim().is_empty())
            .unwrap_or_else(|| DEFAULT_VISION_MODEL.to_owned());
        Self::new(url, model)
    }

    /// The model name this adapter requests.
    #[must_use]
    pub fn model(&self) -> &str {
        &self.model
    }

    /// The Ollama base URL.
    #[must_use]
    pub fn base_url(&self) -> &str {
        &self.base_url
    }

    /// Whether *any* installed Ollama model is vision-capable.
    ///
    /// Convenience wrapper over [`Self::detect_vision_model`]; `Ok(false)` means
    /// Ollama is up but no installed model reports the `vision` capability (the
    /// gated test then skips), and `Err` means Ollama is unreachable — letting
    /// the live test distinguish "no model" from "no Ollama".
    ///
    /// # Errors
    ///
    /// [`PhoskError::Invalid`] if `/api/tags` cannot be reached or parsed.
    pub async fn has_vision_model(&self) -> Result<bool, PhoskError> {
        Ok(self.detect_vision_model().await?.is_some())
    }

    /// The name of the first installed model that `/api/show` reports as
    /// `vision`-capable, or `None` if none is. **Capability-based**, so a
    /// custom-named vision model (e.g. `qwen3.6:35b-custom`) is recognised where
    /// a name-substring match would miss it.
    ///
    /// # Errors
    ///
    /// [`PhoskError::Invalid`] if `/api/tags` cannot be reached or parsed.
    pub async fn detect_vision_model(&self) -> Result<Option<String>, PhoskError> {
        for name in self.list_models().await? {
            if self.model_has_vision(&name).await? {
                return Ok(Some(name));
            }
        }
        Ok(None)
    }

    /// Whether `model` advertises the `vision` capability via `/api/show`.
    /// A non-success status (e.g. unknown model) yields `Ok(false)`, not `Err`.
    ///
    /// # Errors
    ///
    /// [`PhoskError::Invalid`] if `/api/show` is unreachable or returns
    /// unparseable JSON.
    pub async fn model_has_vision(&self, model: &str) -> Result<bool, PhoskError> {
        let url = format!("{}/api/show", self.base_url.trim_end_matches('/'));
        let resp = self
            .client
            .post(&url)
            .json(&serde_json::json!({ "model": model }))
            .send()
            .await
            .map_err(|e| PhoskError::Invalid(format!("vision ocr: /api/show failed: {e}")))?;
        if !resp.status().is_success() {
            return Ok(false);
        }
        let show: ShowResponse = resp
            .json()
            .await
            .map_err(|e| PhoskError::Invalid(format!("vision ocr: bad /api/show JSON: {e}")))?;
        Ok(show.is_vision_capable())
    }

    /// List installed model names via `/api/tags`.
    async fn list_models(&self) -> Result<Vec<String>, PhoskError> {
        let url = format!("{}/api/tags", self.base_url.trim_end_matches('/'));
        let resp = self
            .client
            .get(&url)
            .send()
            .await
            .map_err(|e| PhoskError::Invalid(format!("vision ocr: /api/tags failed: {e}")))?;
        let tags: TagsResponse = resp
            .json()
            .await
            .map_err(|e| PhoskError::Invalid(format!("vision ocr: bad /api/tags JSON: {e}")))?;
        Ok(tags.model_names())
    }

    fn generate_endpoint(&self) -> String {
        format!("{}/api/generate", self.base_url.trim_end_matches('/'))
    }
}

/// `/api/tags` response (only the model names matter).
#[derive(Debug, Deserialize)]
struct TagsResponse {
    #[serde(default)]
    models: Vec<TagModel>,
}

#[derive(Debug, Deserialize)]
struct TagModel {
    #[serde(default)]
    name: String,
    #[serde(default)]
    model: String,
}

impl TagsResponse {
    /// Installed model identifiers (prefer `name`, fall back to `model`).
    fn model_names(&self) -> Vec<String> {
        self.models
            .iter()
            .map(|m| {
                if m.name.is_empty() {
                    m.model.clone()
                } else {
                    m.name.clone()
                }
            })
            .filter(|s| !s.is_empty())
            .collect()
    }
}

/// `/api/show` response — only the advertised `capabilities` matter here.
#[derive(Debug, Deserialize)]
struct ShowResponse {
    #[serde(default)]
    capabilities: Vec<String>,
}

impl ShowResponse {
    /// Whether the model advertises the `vision` capability (case-insensitive).
    fn is_vision_capable(&self) -> bool {
        self.capabilities
            .iter()
            .any(|c| c.eq_ignore_ascii_case("vision"))
    }
}

/// `/api/generate` (non-streamed) response envelope; we read `response`.
#[derive(Debug, Deserialize)]
struct GenerateResponse {
    #[serde(default)]
    response: String,
}

/// The model's transcription JSON we instruct it to emit.
#[derive(Debug, Deserialize)]
struct VisionTranscription {
    #[serde(default)]
    regions: Vec<VisionRegion>,
}

#[derive(Debug, Deserialize)]
struct VisionRegion {
    #[serde(default)]
    text: String,
    #[serde(default = "default_conf")]
    confidence: f64,
    /// `[x, y, width, height]`; tolerated missing/short — padded with zeros.
    #[serde(default)]
    bbox: Vec<i64>,
}

const fn default_conf() -> f64 {
    0.5
}

/// Extract the outermost balanced `{...}` JSON object from a model reply that
/// may carry leading/trailing prose or Markdown code fences. Returns `None` if
/// no brace-balanced object is found.
fn extract_json_object(raw: &str) -> Option<&str> {
    let start = raw.find('{')?;
    let bytes = raw.as_bytes();
    let mut depth = 0_i32;
    let mut in_str = false;
    let mut escaped = false;
    for (i, &b) in bytes.iter().enumerate().skip(start) {
        if in_str {
            if escaped {
                escaped = false;
            } else if b == b'\\' {
                escaped = true;
            } else if b == b'"' {
                in_str = false;
            }
            continue;
        }
        match b {
            b'"' => in_str = true,
            b'{' => depth += 1,
            b'}' => {
                depth -= 1;
                if depth == 0 {
                    return raw.get(start..=i);
                }
            }
            _ => {}
        }
    }
    None
}

/// Reduce a model `bbox` vec to the port's `(x, y, w, h)`, clamping negatives to
/// 0 and missing trailing fields to 0. Pure / unit-testable.
fn bbox_from_vec(v: &[i64]) -> (u32, u32, u32, u32) {
    let g = |i: usize| -> u32 {
        let val = v.get(i).copied().unwrap_or(0);
        u32::try_from(val.max(0)).unwrap_or(u32::MAX)
    };
    (g(0), g(1), g(2), g(3))
}

/// Parse a raw model reply string into an [`OcrResult`]. Pure (no I/O), so the
/// whole mapping is unit-testable against a stub string with no live model.
///
/// # Errors
///
/// [`PhoskError::Invalid`] if no JSON object can be located or it fails to
/// deserialise into the expected `{"regions":[...]}` shape.
fn parse_vision_reply(raw: &str) -> Result<OcrResult, PhoskError> {
    let json = extract_json_object(raw).ok_or_else(|| {
        PhoskError::Invalid("vision ocr: model reply contained no JSON object".to_owned())
    })?;
    let parsed: VisionTranscription = serde_json::from_str(json)
        .map_err(|e| PhoskError::Invalid(format!("vision ocr: unparseable transcription: {e}")))?;

    let regions: Vec<OcrRegion> = parsed
        .regions
        .into_iter()
        .map(|r| OcrRegion {
            confidence: r.confidence.clamp(0.0, 1.0),
            bbox: bbox_from_vec(&r.bbox),
            text: r.text,
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
impl OcrAdapter for OllamaVisionOcr {
    async fn extract(&self, image: &[u8]) -> Result<OcrResult, PhoskError> {
        if image.is_empty() {
            return Err(PhoskError::Invalid(
                "vision ocr: empty image bytes".to_owned(),
            ));
        }

        let b64 = base64::engine::general_purpose::STANDARD.encode(image);
        let body = serde_json::json!({
            "model": self.model,
            "prompt": OCR_PROMPT,
            "images": [b64],
            "format": "json",
            "stream": false,
        });
        let url = self.generate_endpoint();

        tracing::debug!(%url, model = %self.model, bytes = image.len(), "OllamaVisionOcr: generate");

        let resp = self
            .client
            .post(&url)
            .json(&body)
            .send()
            .await
            .map_err(|e| PhoskError::Invalid(format!("vision ocr: request failed: {e}")))?;

        let status = resp.status();
        if !status.is_success() {
            return Err(PhoskError::Invalid(format!(
                "vision ocr: HTTP {}",
                status.as_u16()
            )));
        }

        let generated: GenerateResponse = resp
            .json()
            .await
            .map_err(|e| PhoskError::Invalid(format!("vision ocr: bad /api/generate JSON: {e}")))?;

        parse_vision_reply(&generated.response)
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::float_cmp)]
mod tests {
    use super::*;

    #[test]
    fn parses_clean_transcription_json() {
        let raw = r#"{"regions":[
            {"text":"MIGROS GENEVE","confidence":0.98,"bbox":[40,30,320,44]},
            {"text":"TOTAL CHF 5.65","confidence":0.99,"bbox":[40,240,450,36]}
        ]}"#;
        let out = parse_vision_reply(raw).expect("parses");
        assert_eq!(out.regions.len(), 2);
        assert_eq!(out.regions[0].text, "MIGROS GENEVE");
        assert_eq!(out.regions[0].bbox, (40, 30, 320, 44));
        assert_eq!(out.full_text, "MIGROS GENEVE\nTOTAL CHF 5.65");
    }

    #[test]
    fn strips_prose_and_code_fences() {
        let raw = "Sure! Here is the JSON:\n```json\n{\"regions\":[{\"text\":\"X\",\"confidence\":0.7,\"bbox\":[1,2,3,4]}]}\n```\nHope that helps.";
        let out = parse_vision_reply(raw).expect("parses despite wrapping");
        assert_eq!(out.regions.len(), 1);
        assert_eq!(out.regions[0].text, "X");
        assert_eq!(out.regions[0].bbox, (1, 2, 3, 4));
    }

    #[test]
    fn defaults_and_clamps_missing_fields() {
        // Missing confidence -> default 0.5; short bbox padded; out-of-range conf clamped.
        let raw = r#"{"regions":[
            {"text":"A","bbox":[5,6]},
            {"text":"B","confidence":1.7,"bbox":[1,2,3,4]},
            {"text":"C","confidence":-0.2,"bbox":[-9,0,0,0]}
        ]}"#;
        let out = parse_vision_reply(raw).expect("parses");
        assert_eq!(out.regions[0].confidence, 0.5);
        assert_eq!(out.regions[0].bbox, (5, 6, 0, 0)); // padded
        assert_eq!(out.regions[1].confidence, 1.0); // clamped down
        assert_eq!(out.regions[2].confidence, 0.0); // clamped up
        assert_eq!(out.regions[2].bbox, (0, 0, 0, 0)); // negative clamped
    }

    #[test]
    fn no_json_is_an_error() {
        let err =
            parse_vision_reply("I cannot read this image, sorry.").expect_err("no json => error");
        assert!(matches!(err, PhoskError::Invalid(_)));
    }

    #[test]
    fn malformed_json_is_an_error() {
        let err =
            parse_vision_reply("{\"regions\": [ {\"text\": ] }").expect_err("bad json => error");
        assert!(matches!(err, PhoskError::Invalid(_)));
    }

    #[test]
    fn empty_regions_yields_empty_result() {
        let out = parse_vision_reply("{\"regions\":[]}").expect("parses");
        assert!(out.regions.is_empty());
        assert!(out.full_text.is_empty());
    }

    #[test]
    fn extract_json_object_handles_braces_in_strings() {
        // A `}` inside a string must not close the object early.
        let raw = "{\"regions\":[{\"text\":\"a}b{c\",\"confidence\":0.9,\"bbox\":[0,0,1,1]}]}";
        let out = parse_vision_reply(raw).expect("parses");
        assert_eq!(out.regions[0].text, "a}b{c");
    }

    #[test]
    fn tags_response_lists_model_names() {
        let v = serde_json::json!({
            "models": [
                {"name": "qwen3.6:35b-custom", "model": "qwen3.6:35b-custom"},
                {"name": "", "model": "mistral:7b"}
            ]
        });
        let tags: TagsResponse = serde_json::from_value(v).expect("parses");
        assert_eq!(tags.model_names(), vec!["qwen3.6:35b-custom", "mistral:7b"]);
    }

    #[test]
    fn show_response_detects_vision_capability() {
        // Exactly what /api/show reports for qwen3.6:35b-custom on this machine.
        let v = serde_json::json!({
            "capabilities": ["completion", "vision", "tools", "thinking"]
        });
        let show: ShowResponse = serde_json::from_value(v).expect("parses");
        assert!(show.is_vision_capable());
    }

    #[test]
    fn show_response_text_only_is_not_vision() {
        let v = serde_json::json!({ "capabilities": ["completion", "tools"] });
        let show: ShowResponse = serde_json::from_value(v).expect("parses");
        assert!(!show.is_vision_capable());
    }

    #[tokio::test]
    async fn empty_image_is_rejected_without_network() {
        let ocr = OllamaVisionOcr::new("http://127.0.0.1:9", "llava").expect("client builds");
        let err = ocr.extract(&[]).await.expect_err("empty rejected");
        assert!(matches!(err, PhoskError::Invalid(_)));
        assert_eq!(err.http_status(), 400);
    }

    #[test]
    fn from_env_builds_a_usable_adapter() {
        // Cannot mutate env (`unsafe_code = forbid`); assert the resolution
        // invariant: defaults unless the override vars are set.
        let ocr = OllamaVisionOcr::from_env().expect("builds");
        match std::env::var(OLLAMA_URL_ENV)
            .ok()
            .filter(|s| !s.trim().is_empty())
        {
            Some(set) => assert_eq!(ocr.base_url(), set),
            None => assert_eq!(ocr.base_url(), DEFAULT_OLLAMA_URL),
        }
        match std::env::var(VISION_MODEL_ENV)
            .ok()
            .filter(|s| !s.trim().is_empty())
        {
            Some(set) => assert_eq!(ocr.model(), set),
            None => assert_eq!(ocr.model(), DEFAULT_VISION_MODEL),
        }
    }

    #[test]
    fn is_object_safe_behind_arc_dyn() {
        let ocr = OllamaVisionOcr::new(DEFAULT_OLLAMA_URL, "llava").expect("ok");
        let _adapter: std::sync::Arc<dyn OcrAdapter> = std::sync::Arc::new(ocr);
    }
}
