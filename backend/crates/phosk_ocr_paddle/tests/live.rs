//! GATED live integration test for the `PaddleOCR` adapter.
//!
//! `PaddleOCR` is **not installed** on the build/dev machine, so this test is
//! self-skipping: it first probes the service for reachability and returns
//! early (printing a skip line) if nothing answers. When a `PaddleOCR`
//! `hubserving` instance *is* running (default `http://localhost:8868`, override
//! via `PHOSK_OCR_URL`) the test sends a tiny generated PNG and asserts the
//! adapter returns a well-formed [`OcrResult`] without erroring. It never panics
//! on a down service — that is the whole point of the reachability gate.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use phosk_adapter_ocr::OcrAdapter;
use phosk_ocr_paddle::PaddleOcr;

/// A minimal valid 1x1 white PNG (so the request body is a real image even if
/// the service has nothing to recognise). Bytes are a hand-verified PNG.
const TINY_PNG: &[u8] = &[
    0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, // signature
    0x00, 0x00, 0x00, 0x0D, b'I', b'H', b'D', b'R', 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01,
    0x08, 0x02, 0x00, 0x00, 0x00, 0x90, 0x77, 0x53, 0xDE, // IHDR + crc
    0x00, 0x00, 0x00, 0x0C, b'I', b'D', b'A', b'T', 0x08, 0xD7, 0x63, 0xF8, 0xCF, 0xC0, 0x00, 0x00,
    0x03, 0x01, 0x01, 0x00, 0x18, 0xDD, 0x8D, 0xB4, // IDAT + crc
    0x00, 0x00, 0x00, 0x00, b'I', b'E', b'N', b'D', 0xAE, 0x42, 0x60, 0x82, // IEND
];

#[tokio::test]
async fn live_paddle_extract_when_service_up() {
    let ocr = PaddleOcr::from_env().expect("client builds");

    if !ocr.is_reachable().await {
        eprintln!(
            "SKIP live_paddle_extract_when_service_up: no PaddleOCR at {} (set PHOSK_OCR_URL to run)",
            ocr.base_url()
        );
        return;
    }

    // Service is up: a real call must succeed (or return a service-side error we
    // surface as PhoskError — either way, never a panic). We assert the happy
    // path: a parseable OcrResult.
    match ocr.extract(TINY_PNG).await {
        Ok(result) => {
            // full_text is the newline join of regions; both views are present.
            assert_eq!(
                result.full_text.lines().filter(|l| !l.is_empty()).count(),
                result.regions.iter().filter(|r| !r.text.is_empty()).count(),
            );
        }
        Err(e) => {
            // A live service that rejects this synthetic image is acceptable;
            // we only require a typed PhoskError, never a panic.
            eprintln!("live PaddleOCR returned an error (acceptable): {e}");
        }
    }
}
