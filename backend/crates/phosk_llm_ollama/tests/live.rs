//! Live smoke test against a *running* Ollama at `localhost:11434`.
//!
//! These tests are **reachability-gated**: each first probes `/api/tags` with a
//! short timeout, and if Ollama is not reachable it returns early (the test
//! passes vacuously). CI / a dev box with Ollama down must never see a red
//! suite from here. When Ollama *is* up, the tests exercise the real model:
//! [`OllamaLlm::complete`] must return non-empty text and
//! [`OllamaLlm::generate_structured`] must yield a schema-shaped JSON object.
//!
//! The generation timeout inside the adapter is generous (the ~23 GB model
//! loads slowly on a cold first hit), so these can take a while the first run.

#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]

use phosk_adapter_llm::LlmAdapter;
use phosk_llm_ollama::OllamaLlm;
use serde_json::json;

/// Build the env-default adapter and confirm Ollama is reachable AND our model
/// is present. Returns `None` (⇒ skip) on any not-ready / unreachable state.
async fn live_adapter_or_skip(reason: &str) -> Option<OllamaLlm> {
    let llm = OllamaLlm::from_env().expect("adapter builds");
    match llm.health().await {
        Ok(true) => Some(llm),
        Ok(false) => {
            eprintln!("SKIP {reason}: ollama reachable but model not ready/listed");
            None
        }
        Err(e) => {
            eprintln!("SKIP {reason}: ollama unreachable ({e})");
            None
        }
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn live_complete_returns_nonempty() {
    let Some(llm) = live_adapter_or_skip("live_complete_returns_nonempty").await else {
        return;
    };

    let out = llm
        .complete("Reply with the single word: pong.")
        .await
        .expect("complete succeeds against live ollama");
    eprintln!("live complete -> {out:?}");
    assert!(
        !out.trim().is_empty(),
        "live model returned empty completion"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn live_structured_yields_schema_valid_json() {
    let Some(llm) = live_adapter_or_skip("live_structured_yields_schema_valid_json").await else {
        return;
    };

    // A tiny, unambiguous schema: classify a receipt line item.
    let schema = json!({
        "type": "object",
        "properties": {
            "category": { "type": "string" },
            "confidence": { "type": "number" }
        },
        "required": ["category", "confidence"]
    });

    let value = llm
        .generate_structured(
            "Classify the receipt line 'Bananes Bio 2.45' into a budget category. \
             Return category and a confidence between 0 and 1.",
            &schema,
        )
        .await
        .expect("generate_structured succeeds against live ollama");

    eprintln!("live structured -> {value}");

    // Adapter guarantees an object; check the schema's required fields exist
    // with the declared JSON types.
    let obj = value.as_object().expect("structured output is an object");
    let category = obj
        .get("category")
        .expect("has `category`")
        .as_str()
        .expect("`category` is a string");
    assert!(!category.trim().is_empty(), "category must be non-empty");
    assert!(
        obj.get("confidence").expect("has `confidence`").is_number(),
        "`confidence` must be a number"
    );
}
