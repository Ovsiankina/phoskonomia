//! Tests for the LLM-backed AI surfaces + the effect-typed approval gate
//! (`phosk_ai::ai_tools`), driven hermetically by `phosk_adapter_llm::FakeLlm`.
//!
//! These pin the rewire from inline canned text to the `LlmAdapter` PORT:
//!   - chat reply now comes from `llm.complete` (read tool, persists transcript);
//!   - the narrative insight one-liner comes from `llm.complete`, model badge is
//!     the live model id, estimated saving stays the seeded centimes;
//!   - auto-categorize / suggest are WRITE tools: they call the model's
//!     constrained-output path, schema-check it, and ENQUEUE a `ProposedWrite`
//!     (status "open"), and must NOT mutate the DB (the "never auto-write" gate);
//!   - sub-0.7 confidence is flagged, not dropped.
//!
//! Test-only lint relaxations (the workspace denies these in prod code).
#![allow(
    clippy::expect_used,
    clippy::panic,
    clippy::doc_markdown,
    clippy::missing_const_for_fn,
    clippy::float_cmp
)]

use phosk_adapter_db::DatabaseAdapter;
use phosk_adapter_llm::{FakeLlm, LlmAdapter};
use phosk_ai::ai_spine::ai_panel;
use phosk_ai::ai_tools::{
    CHAT_REPLY_MAX_CHARS, CONFIDENCE_THRESHOLD, ProposedWrite, ToolEffect, auto_categorize,
    chat_reply, narrative_insight, suggest,
};
use phosk_core::error::PhoskError;
use phosk_core::money::Money;
use phosk_db_memory::MemoryDb;

fn db() -> MemoryDb {
    MemoryDb::seeded().expect("seed the deterministic Swiss MemoryDb")
}

fn as_of() -> chrono::NaiveDate {
    chrono::NaiveDate::from_ymd_opt(2026, 6, 18).expect("valid demo date 2026-06-18")
}

// ── chat_reply: READ tool, reply from llm.complete, persisted ────────────────

#[tokio::test]
async fn chat_reply_uses_the_model_completion() {
    let db = db();
    let llm = FakeLlm::new(); // echo-only
    let reply = chat_reply(&db, &llm, "how much on coffee?")
        .await
        .expect("chat_reply produces a model reply");
    assert_eq!(reply.who, "sys");
    // FakeLlm echoes the prompt; the reply must reflect the model, not a canned const.
    assert!(
        reply.text.contains("echo:"),
        "the reply came from llm.complete (FakeLlm echo), got {:?}",
        reply.text
    );
}

#[tokio::test]
async fn chat_reply_persists_user_then_reply() {
    let db = db();
    let llm = FakeLlm::with_model("qwen3.6:35b-custom").with_reply(
        "You are Phoskonomia's budgeting assistant. Answer the user concisely.\nUser: hi",
        "Hello!",
    );
    let before = ai_panel(&db).await.expect("before").msgs.len();
    let reply = chat_reply(&db, &llm, "hi").await.expect("chat_reply ok");
    assert_eq!(
        reply.text, "Hello!",
        "scripted model reply is returned verbatim"
    );

    let after = ai_panel(&db).await.expect("after").msgs;
    assert_eq!(
        after.len(),
        before + 2,
        "user line + model reply both persisted"
    );
    assert_eq!(after[after.len() - 2].who, "usr");
    assert_eq!(after[after.len() - 2].text, "hi");
    assert_eq!(after[after.len() - 1].who, "sys");
    assert_eq!(after[after.len() - 1].text, "Hello!");
}

#[tokio::test]
async fn chat_reply_empty_text_is_invalid_and_persists_nothing() {
    let db = db();
    let llm = FakeLlm::new();
    let before = ai_panel(&db).await.expect("before").msgs.len();
    match chat_reply(&db, &llm, "   ").await {
        Err(PhoskError::Invalid(_)) => {}
        other => panic!("expected Invalid for blank text, got {other:?}"),
    }
    let after = ai_panel(&db).await.expect("after").msgs.len();
    assert_eq!(after, before, "a rejected turn must not be persisted");
}

#[tokio::test]
async fn chat_reply_model_failure_persists_nothing() {
    let db = db();
    let before = ai_panel(&db).await.expect("before").msgs;
    for llm in [
        FakeLlm::new().reachable(false),       // model down
        FakeLlm::new().fail_completions(true), // healthy, then fails to answer
    ] {
        assert!(
            chat_reply(&db, &llm, "am I on budget?").await.is_err(),
            "a failed completion is an error"
        );
        assert_eq!(
            ai_panel(&db).await.expect("after").msgs,
            before,
            "neither the user line nor a reply is saved when the model fails"
        );
    }
}

#[tokio::test]
async fn chat_reply_bounds_the_saved_model_reply() {
    let db = db();
    let prompt =
        "You are Phoskonomia's budgeting assistant. Answer the user concisely.\nUser: essay";
    let llm = FakeLlm::new().with_reply(prompt, "é".repeat(CHAT_REPLY_MAX_CHARS * 3));
    let reply = chat_reply(&db, &llm, "essay").await.expect("chat_reply ok");
    assert_eq!(reply.text.chars().count(), CHAT_REPLY_MAX_CHARS + 1);
    assert!(reply.text.ends_with('…'), "the cut is marked");
    let saved = ai_panel(&db).await.expect("after").msgs;
    assert_eq!(
        saved.last().map(|m| &m.text),
        Some(&reply.text),
        "the bounded reply is what is saved"
    );

    // A reply at the limit is kept whole.
    let db = crate::db();
    let exact = "x".repeat(CHAT_REPLY_MAX_CHARS);
    let llm = FakeLlm::new().with_reply(prompt, exact.clone());
    let reply = chat_reply(&db, &llm, "essay").await.expect("chat_reply ok");
    assert_eq!(reply.text, exact);
}

// ── narrative_insight: READ tool, sentence from the model, live model badge ───

#[tokio::test]
async fn narrative_insight_text_comes_from_the_model() {
    let db = db();
    let llm = FakeLlm::with_model("gemma4:26b-custom").with_reply(
        "Summarise this spending cycle in one short, plain sentence of budgeting advice.",
        "Spend less on coffee.",
    );
    let ins = narrative_insight(&db, &llm, as_of())
        .await
        .expect("narrative_insight composes from the model");
    assert_eq!(
        ins.text, "Spend less on coffee.",
        "the sentence is the model completion"
    );
    assert_eq!(
        ins.model, "gemma4:26b-custom",
        "the badge is the live model id (provenance)"
    );
    // Estimated saving stays the seeded exact centimes (planning's value, not the model's).
    assert_eq!(ins.estimated_savings, Money::from_centimes(4_200));
}

// ── auto_categorize: WRITE tool — ENQUEUE a proposal, never mutate the DB ─────

#[tokio::test]
async fn auto_categorize_enqueues_a_proposal_and_does_not_write() {
    let db = db();
    let suggestions_before = db.ai_suggestions().await.expect("read suggestions").len();

    let llm = FakeLlm::new(); // unscripted structured -> synthesized schema-shaped object
    let proposal: ProposedWrite = auto_categorize(&db, &llm, "Bananes Bio 2.45")
        .await
        .expect("auto_categorize proposes a category");

    // The effect is a WRITE that was ENQUEUED, not applied.
    assert_eq!(proposal.effect(), ToolEffect::Write);
    assert_eq!(
        proposal.suggestion.status, "open",
        "an enqueued proposal is always open"
    );
    assert_eq!(
        proposal.model,
        llm.model(),
        "proposal records the authoring model (provenance)"
    );

    // CRITICAL: no suggestion was persisted to the DB — the gate never auto-writes.
    let suggestions_after = db.ai_suggestions().await.expect("read suggestions").len();
    assert_eq!(
        suggestions_after, suggestions_before,
        "auto_categorize must ENQUEUE only — never persist to the store"
    );
}

#[tokio::test]
async fn auto_categorize_empty_line_is_invalid() {
    let db = db();
    let llm = FakeLlm::new();
    match auto_categorize(&db, &llm, "  ").await {
        Err(PhoskError::Invalid(_)) => {}
        other => panic!("expected Invalid for blank line, got {other:?}"),
    }
}

#[tokio::test]
async fn auto_categorize_uses_the_models_structured_pick() {
    let db = db();
    // Script the exact structured answer the model returns for this prompt.
    let cats: Vec<String> = db
        .categories()
        .await
        .expect("categories")
        .into_iter()
        .map(|c| c.name)
        .collect();
    let prompt = format!(
        "Pick the best category for the receipt line \"Pain complet 3.20\" from: {}.",
        cats.join(", ")
    );
    let llm = FakeLlm::new().with_structured(
        &prompt,
        serde_json::json!({ "category": "Groceries", "confidence": 0.91 }),
    );
    let proposal = auto_categorize(&db, &llm, "Pain complet 3.20")
        .await
        .expect("auto_categorize ok");
    assert_eq!(proposal.suggestion.target.as_deref(), Some("Groceries"));
    assert_eq!(proposal.suggestion.confidence, 0.91);
    assert!(!proposal.low_confidence, "0.91 is above the 0.7 threshold");
    assert!(proposal.suggestion.text.contains("Groceries"));
}

#[tokio::test]
async fn auto_categorize_low_confidence_is_flagged_not_dropped() {
    let db = db();
    let cats: Vec<String> = db
        .categories()
        .await
        .expect("categories")
        .into_iter()
        .map(|c| c.name)
        .collect();
    let prompt = format!(
        "Pick the best category for the receipt line \"mystery item\" from: {}.",
        cats.join(", ")
    );
    let llm = FakeLlm::new().with_structured(
        &prompt,
        serde_json::json!({ "category": "Other", "confidence": 0.42 }),
    );
    let proposal = auto_categorize(&db, &llm, "mystery item")
        .await
        .expect("auto_categorize ok");
    assert!(proposal.suggestion.confidence < CONFIDENCE_THRESHOLD);
    assert!(
        proposal.low_confidence,
        "a sub-0.7 proposal is coral-flagged, still enqueued (never dropped)"
    );
    assert_eq!(proposal.suggestion.status, "open");
}

// ── suggest: WRITE tool — structured suggestion, ENQUEUED with centimes ───────

#[tokio::test]
async fn suggest_enqueues_with_exact_centimes_and_no_db_write() {
    let db = db();
    let before = db.ai_suggestions().await.expect("read").len();
    let llm = FakeLlm::new().with_structured(
        "Propose one concrete budgeting action for this cycle as structured JSON.",
        serde_json::json!({
            "text": "Cap going-out at CHF 70",
            "confidence": 0.81,
            "target": "Going out",
            "estimatedSavingsCentimes": 4_200
        }),
    );
    let proposal = suggest(&db, &llm, as_of()).await.expect("suggest ok");
    assert_eq!(proposal.suggestion.text, "Cap going-out at CHF 70");
    assert_eq!(proposal.suggestion.target.as_deref(), Some("Going out"));
    assert_eq!(
        proposal.suggestion.estimated_savings,
        Some(Money::from_centimes(4_200)),
        "savings cross as exact i64 centimes"
    );
    assert_eq!(proposal.suggestion.status, "open");
    assert!(!proposal.low_confidence);

    let after = db.ai_suggestions().await.expect("read").len();
    assert_eq!(after, before, "suggest must ENQUEUE only, never persist");
}

#[tokio::test]
async fn suggest_missing_text_is_invalid() {
    let db = db();
    let llm = FakeLlm::new().with_structured(
        "Propose one concrete budgeting action for this cycle as structured JSON.",
        serde_json::json!({ "confidence": 0.5 }),
    );
    match suggest(&db, &llm, as_of()).await {
        Err(PhoskError::Invalid(_)) => {}
        other => panic!("expected Invalid when model omits `text`, got {other:?}"),
    }
}
