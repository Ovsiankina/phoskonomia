#![allow(
    // Test-only: the workspace denies these in production, but `clippy.toml`'s
    // allow-in-tests only covers `#[test]` bodies, not integration-test helpers
    // or module docs, so the exemption is made explicit crate-wide (mirrors the
    // sibling adapter / feature crates).
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::doc_markdown,
    clippy::missing_const_for_fn,
    clippy::float_cmp
)]
//! Tests for [`phosk_adapter_llm::FakeLlm`] — the deterministic test double for
//! the [`LlmAdapter`](phosk_adapter_llm::LlmAdapter) port. These also pin the
//! port's contract (empty-prompt rejection, schema-shaped structured output) so
//! a future real adapter has an executable spec to match.

use phosk_adapter_llm::{FakeLlm, LlmAdapter};
use phosk_core::error::PhoskError;
use serde_json::json;

#[tokio::test]
async fn complete_echoes_unscripted_prompts() {
    let llm = FakeLlm::new();
    let out = llm.complete("hello").await.expect("echo should succeed");
    assert_eq!(out, "echo: hello");
}

#[tokio::test]
async fn complete_returns_scripted_reply_on_exact_match() {
    let llm = FakeLlm::new().with_reply("ping", "pong");
    assert_eq!(llm.complete("ping").await.expect("scripted"), "pong");
    // A non-matching prompt still falls back to echo.
    assert_eq!(
        llm.complete("pong").await.expect("echo"),
        "echo: pong",
        "only the exact scripted key is overridden"
    );
}

#[tokio::test]
async fn complete_rejects_empty_prompt() {
    let llm = FakeLlm::new();
    let err = llm.complete("").await.expect_err("empty prompt is invalid");
    assert!(matches!(err, PhoskError::Invalid(_)));
}

#[tokio::test]
async fn model_defaults_and_overrides() {
    assert_eq!(FakeLlm::new().model(), FakeLlm::DEFAULT_MODEL);
    assert_eq!(
        FakeLlm::with_model("gemma4:26b-custom").model(),
        "gemma4:26b-custom"
    );
}

#[tokio::test]
async fn health_reports_configured_state() {
    assert!(FakeLlm::new().health().await.expect("healthy by default"));
    assert!(
        !FakeLlm::new()
            .healthy(false)
            .health()
            .await
            .expect("health call succeeds even when not ready")
    );
}

#[tokio::test]
async fn structured_output_matches_schema_properties() {
    let llm = FakeLlm::new();
    let schema = json!({
        "type": "object",
        "properties": {
            "category": { "type": "string" },
            "confidence": { "type": "number" },
            "count": { "type": "integer" },
            "flagged": { "type": "boolean" },
            "tags": { "type": "array", "items": { "type": "string" } },
            "meta": {
                "type": "object",
                "properties": { "shop": { "type": "string" } }
            }
        }
    });

    let v = llm
        .generate_structured("classify this receipt", &schema)
        .await
        .expect("structured output");

    // Keys are exactly the declared properties, with type-correct placeholders.
    assert_eq!(v["category"], json!(""));
    assert_eq!(v["confidence"], json!(0.0));
    assert_eq!(v["count"], json!(0));
    assert_eq!(v["flagged"], json!(false));
    assert_eq!(v["tags"], json!([""]));
    assert_eq!(v["meta"], json!({ "shop": "" }));
    // No stray keys leak in.
    assert_eq!(
        v.as_object().expect("object").len(),
        6,
        "exactly the schema's six properties"
    );
}

#[tokio::test]
async fn structured_output_scripted_reply_overrides_shape() {
    let canned = json!({ "category": "Groceries", "confidence": 0.92 });
    let llm = FakeLlm::new().with_structured("classify x", canned.clone());
    let schema = json!({ "type": "object", "properties": { "category": { "type": "string" } } });

    let v = llm
        .generate_structured("classify x", &schema)
        .await
        .expect("scripted structured");
    assert_eq!(v, canned);
}

#[tokio::test]
async fn structured_output_rejects_empty_prompt_and_non_object_schema() {
    let llm = FakeLlm::new();
    let schema = json!({ "type": "object", "properties": {} });

    let empty = llm
        .generate_structured("", &schema)
        .await
        .expect_err("empty prompt invalid");
    assert!(matches!(empty, PhoskError::Invalid(_)));

    let bad_schema = llm
        .generate_structured("hi", &json!("not-an-object"))
        .await
        .expect_err("non-object schema invalid");
    assert!(matches!(bad_schema, PhoskError::Invalid(_)));
}

/// The port must be object-safe: feature crates hold it as a trait object.
#[tokio::test]
async fn adapter_is_object_safe_behind_arc() {
    use std::sync::Arc;
    let llm: Arc<dyn LlmAdapter> = Arc::new(FakeLlm::new());
    assert_eq!(llm.complete("x").await.expect("dyn call"), "echo: x");
}
