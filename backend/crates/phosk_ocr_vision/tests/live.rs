//! GATED live integration test for the Ollama vision-LLM OCR adapter.
//!
//! The test asks Ollama (capability-based, via `/api/show`) for an installed
//! vision-capable model and runs against it; it skips cleanly (printing a skip
//! line) only when Ollama is down OR no installed model reports the `vision`
//! capability. When a vision model is present it sends a tiny PNG and asserts the
//! adapter returns a typed result/error (never a panic) — content is not asserted
//! because a general vision model's transcription of a synthetic image is
//! non-deterministic. Override the model with `PHOSK_OCR_VISION_MODEL` and the
//! URL with `PHOSK_OCR_VISION_URL`.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use phosk_adapter_ocr::OcrAdapter;
use phosk_ocr_vision::OllamaVisionOcr;

/// Minimal valid 1x1 PNG (same hand-verified bytes as the paddle live test).
const TINY_PNG: &[u8] = &[
    0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0x00, 0x00, 0x00, 0x0D, b'I', b'H', b'D', b'R',
    0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x02, 0x00, 0x00, 0x00, 0x90, 0x77, 0x53,
    0xDE, 0x00, 0x00, 0x00, 0x0C, b'I', b'D', b'A', b'T', 0x08, 0xD7, 0x63, 0xF8, 0xCF, 0xC0, 0x00,
    0x00, 0x03, 0x01, 0x01, 0x00, 0x18, 0xDD, 0x8D, 0xB4, 0x00, 0x00, 0x00, 0x00, b'I', b'E', b'N',
    b'D', 0xAE, 0x42, 0x60, 0x82,
];

#[tokio::test]
async fn live_vision_extract_when_model_present() {
    let probe = OllamaVisionOcr::from_env().expect("client builds");

    // Capability-based discovery: pick whatever vision model is actually installed
    // (respecting an explicit PHOSK_OCR_VISION_MODEL override if it is vision-capable).
    let model = match probe.model_has_vision(probe.model()).await {
        Ok(true) => Some(probe.model().to_owned()),
        Ok(false) => match probe.detect_vision_model().await {
            Ok(found) => found,
            Err(e) => {
                eprintln!(
                    "SKIP live_vision_extract_when_model_present: Ollama unreachable at {} ({e})",
                    probe.base_url()
                );
                return;
            }
        },
        Err(e) => {
            eprintln!(
                "SKIP live_vision_extract_when_model_present: Ollama unreachable at {} ({e})",
                probe.base_url()
            );
            return;
        }
    };

    let Some(model) = model else {
        eprintln!(
            "SKIP live_vision_extract_when_model_present: no installed model reports the `vision` capability"
        );
        return;
    };

    let ocr = OllamaVisionOcr::new(probe.base_url(), &model).expect("client builds");
    eprintln!("live vision OCR against installed vision model '{model}'");
    match ocr.extract(TINY_PNG).await {
        Ok(result) => {
            // full_text is the newline join of region texts; the two views agree.
            assert_eq!(
                result.full_text.lines().filter(|l| !l.is_empty()).count(),
                result.regions.iter().filter(|r| !r.text.is_empty()).count(),
            );
        }
        Err(e) => {
            // The round-trip reached the model; a general vision model may emit
            // empty/non-JSON for a synthetic 1x1 image. That is acceptable here.
            eprintln!("live vision OCR returned an error (acceptable on a 1x1 image): {e}");
        }
    }
}
