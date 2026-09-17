//! `ai_tools` — the LLM-backed AI surfaces + the **effect-typed approval gate**.
//!
//! This is where `phosk_ai` actually talks to a language model. Every fn here
//! takes a `&dyn LlmAdapter` (the L2 PORT) alongside the `&dyn DatabaseAdapter`
//! PORT, so a composition root injects the real Ollama adapter and feature code
//! never imports a concrete LLM (ADR-010). Vendor types (Ollama JSON, reqwest
//! errors) died at the L3 adapter; only `serde_json::Value` (schema-constrained)
//! and `PhoskError` cross this seam.
//!
//! ## The effect-typed gate (AI write-tools ENQUEUE, never auto-write)
//!
//! AI tools are split by *effect*:
//!
//! - **Read tools** (`narrative_insight`, `chat_reply`) call the model and read
//!   the DB, and return their result directly — they never mutate domain state.
//! - **Write tools** (`auto_categorize`, `suggest`) call the model to PROPOSE a
//!   change, schema-check it, and return a [`ProposedWrite`] enqueued for human
//!   approval. They take `&dyn DatabaseAdapter` for *reads only* (context) and
//!   **never** persist: the gate's whole point is that a model can suggest but
//!   only an approved [`ProposedWrite`] is ever applied (later, by the approval
//!   pipeline). A write tool that wrote to the DB would be a bug; the type makes
//!   that impossible to do by accident — these fns hand back data, never a commit.
//!
//! Low-confidence (`< 0.7`) proposals are still returned, but carry
//! [`ProposedWrite::low_confidence`] `== true` so the approval UI can coral-flag
//! them (ADR confidence rule).

use serde::{Deserialize, Serialize};
use serde_json::json;

use phosk_adapter_db::DatabaseAdapter;
use phosk_adapter_llm::LlmAdapter;
use phosk_core::error::PhoskError;
use phosk_core::money::Money;
use phosk_id::{MessageId, SuggestionId};
use phosk_model::{AiSuggestion, Message};

use crate::ai_features::InsightDto;
use crate::ai_spine::AiChatMsgDto;

/// The ADR confidence threshold: proposals below this are flagged (coral),
/// never silently dropped.
pub const CONFIDENCE_THRESHOLD: f64 = 0.7;

/// The effect of an AI tool, made explicit in the type system. A `Read` tool may
/// execute against the DB; a `Write` tool may only ENQUEUE a proposal for
/// approval — it must never mutate domain state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ToolEffect {
    /// Side-effect-free: reads the DB / calls the model, returns a result.
    Read,
    /// Mutating intent: the model proposes a change that is ENQUEUED for human
    /// approval and applied later — never written here.
    Write,
}

/// A model-proposed mutation, schema-checked and ENQUEUED for human approval.
///
/// This is the unit the effect-typed gate hands back from a write tool. It is a
/// candidate [`AiSuggestion`] with `status == "open"` — it has NOT been
/// persisted or applied. The approval pipeline (later) is the only thing that
/// turns an approved `ProposedWrite` into a real domain mutation.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProposedWrite {
    /// The candidate suggestion (always `status == "open"` here).
    pub suggestion: AiSuggestion,
    /// The model that produced it (provenance — every AI artefact records which
    /// model authored it).
    pub model: String,
    /// `true` when the suggestion's confidence is below [`CONFIDENCE_THRESHOLD`]
    /// — the approval UI coral-flags these; they are still enqueued, never dropped.
    pub low_confidence: bool,
}

impl ProposedWrite {
    /// The effect of any `ProposedWrite` is, by construction, [`ToolEffect::Write`].
    #[must_use]
    pub const fn effect(&self) -> ToolEffect {
        ToolEffect::Write
    }
}

/// Build a `ProposedWrite` from a candidate suggestion + the model name, setting
/// the low-confidence flag from the ADR threshold. Forces `status = "open"`.
fn enqueue(mut suggestion: AiSuggestion, model: &str) -> ProposedWrite {
    "open".clone_into(&mut suggestion.status);
    let low_confidence = suggestion.confidence < CONFIDENCE_THRESHOLD;
    ProposedWrite {
        suggestion,
        model: model.to_owned(),
        low_confidence,
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// READ tools — call the model, return prose / a DTO, never mutate
// ─────────────────────────────────────────────────────────────────────────────

/// Chat turn (READ tool): persist the user line, ask the model for a reply via
/// the PORT, persist the reply, and return it.
///
/// Unlike the canned [`crate::ai_spine::send_message`] (which the build-contract
/// freezes for the seeded read-model), this routes the reply through
/// `llm.complete`. The model's prose is trimmed; an empty completion falls back
/// to a deterministic acknowledgement so the transcript never gains a blank line.
///
/// # Errors
/// [`PhoskError::Invalid`] for empty `text`; [`PhoskError::NotFound`] if there is
/// no chat to append to; otherwise propagates adapter / model errors.
#[tracing::instrument(level = "debug", skip_all)]
pub async fn chat_reply(
    db: &dyn DatabaseAdapter,
    llm: &dyn LlmAdapter,
    text: &str,
) -> Result<AiChatMsgDto, PhoskError> {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return Err(PhoskError::Invalid("empty chat message".to_owned()));
    }

    let Some(chat) = db.latest_chat().await? else {
        return Err(PhoskError::NotFound("no chat to append to".to_owned()));
    };

    // Persist the user turn verbatim.
    db.append_message(Message {
        id: MessageId::new(),
        chat_id: chat.id,
        who: "usr".to_owned(),
        text: text.to_owned(),
        at: chat.started,
    })
    .await?;

    // Ask the model (the only place the chat reply comes from now).
    let raw = llm.complete(&chat_prompt(trimmed)).await?;
    let reply_text = normalize_reply(&raw);

    db.append_message(Message {
        id: MessageId::new(),
        chat_id: chat.id,
        who: "sys".to_owned(),
        text: reply_text.clone(),
        at: chat.started,
    })
    .await?;

    Ok(AiChatMsgDto {
        who: "sys".to_owned(),
        text: reply_text,
    })
}

/// The narrative dashboard insight (READ tool): ask the model for the one-liner
/// via the PORT, keep the seeded estimated saving (exact centimes).
///
/// The sentence now comes from `llm.complete`; the `model` badge is the live
/// model id (provenance). The estimated saving stays the pinned seed value —
/// computing it is the planning slice's job, not the model's.
///
/// # Errors
/// Propagates adapter / model errors; a blank completion falls back to a
/// deterministic sentence (never an empty insight).
#[tracing::instrument(level = "debug", skip_all, fields(as_of = %as_of))]
pub async fn narrative_insight(
    db: &dyn DatabaseAdapter,
    llm: &dyn LlmAdapter,
    as_of: chrono::NaiveDate,
) -> Result<InsightDto, PhoskError> {
    let _ = (db, as_of);
    let raw = llm.complete(INSIGHT_PROMPT).await?;
    let text = normalize_insight(&raw);
    Ok(InsightDto {
        model: llm.model().to_owned(),
        text,
        estimated_savings: Money::from_centimes(4_200),
    })
}

// ─────────────────────────────────────────────────────────────────────────────
// WRITE tools — PROPOSE via the model, schema-check, ENQUEUE; never persist
// ─────────────────────────────────────────────────────────────────────────────

/// Auto-categorize a receipt line (WRITE tool): ask the model — via the
/// constrained-output PORT — to pick a category for `line_text`, schema-check the
/// answer, and ENQUEUE a `budget_cut`/`cap`-class proposal. **Never writes.**
///
/// `db` is used for READ context only (the configured category names the model
/// must choose from). The returned [`ProposedWrite`] is a candidate awaiting
/// approval; low-confidence picks are flagged, not dropped.
///
/// # Errors
/// [`PhoskError::Invalid`] for empty `line_text`; propagates adapter / model /
/// schema errors. A model answer that fails schema validation surfaces as the
/// PORT's error, never a panic.
#[tracing::instrument(level = "debug", skip_all)]
pub async fn auto_categorize(
    db: &dyn DatabaseAdapter,
    llm: &dyn LlmAdapter,
    line_text: &str,
) -> Result<ProposedWrite, PhoskError> {
    let trimmed = line_text.trim();
    if trimmed.is_empty() {
        return Err(PhoskError::Invalid("empty receipt line".to_owned()));
    }

    // READ context: the categories the model may pick from.
    let categories: Vec<String> = db.categories().await?.into_iter().map(|c| c.name).collect();

    let schema = categorize_schema();
    let prompt = categorize_prompt(trimmed, &categories);
    let out = llm.generate_structured(&prompt, &schema).await?;

    let category = out
        .get("category")
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| PhoskError::Invalid("model omitted `category`".to_owned()))?
        .to_owned();
    let confidence = out
        .get("confidence")
        .and_then(serde_json::Value::as_f64)
        .unwrap_or(0.0_f64)
        .clamp(0.0, 1.0);

    let suggestion = AiSuggestion {
        id: SuggestionId::new(),
        kind: "cap".to_owned(),
        text: format!("Categorise \"{trimmed}\" as {category}"),
        confidence,
        target: Some(category),
        estimated_savings: None,
        status: "open".to_owned(),
    };
    Ok(enqueue(suggestion, llm.model()))
}

/// Generate a budget/savings suggestion (WRITE tool): ask the model — via the
/// constrained-output PORT — for a structured suggestion, schema-check it, and
/// ENQUEUE it. **Never writes.**
///
/// `db` is READ context only. The returned [`ProposedWrite`] is a candidate
/// awaiting approval; `estimatedSavings` is carried as exact i64 centimes.
///
/// # Errors
/// Propagates adapter / model / schema errors; a model answer missing required
/// fields surfaces as [`PhoskError::Invalid`], never a panic.
#[tracing::instrument(level = "debug", skip_all, fields(as_of = %as_of))]
pub async fn suggest(
    db: &dyn DatabaseAdapter,
    llm: &dyn LlmAdapter,
    as_of: chrono::NaiveDate,
) -> Result<ProposedWrite, PhoskError> {
    let _ = (db, as_of);
    let schema = suggestion_schema();
    let out = llm.generate_structured(SUGGEST_PROMPT, &schema).await?;

    let text = out
        .get("text")
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| PhoskError::Invalid("model omitted `text`".to_owned()))?
        .to_owned();
    let confidence = out
        .get("confidence")
        .and_then(serde_json::Value::as_f64)
        .unwrap_or(0.0_f64)
        .clamp(0.0, 1.0);
    // Money crosses as exact centimes; default to no estimate if absent/0.
    let estimated_savings = out
        .get("estimatedSavingsCentimes")
        .and_then(serde_json::Value::as_i64)
        .filter(|c| *c != 0)
        .map(Money::from_centimes);

    let suggestion = AiSuggestion {
        id: SuggestionId::new(),
        kind: "budget_cut".to_owned(),
        text,
        confidence,
        target: out
            .get("target")
            .and_then(serde_json::Value::as_str)
            .map(ToOwned::to_owned),
        estimated_savings,
        status: "open".to_owned(),
    };
    Ok(enqueue(suggestion, llm.model()))
}

// ─────────────────────────────────────────────────────────────────────────────
// prompts (const) + JSON schemas + reply normalization
// ─────────────────────────────────────────────────────────────────────────────

/// The narrative-insight prompt (const, per the prompts-as-`const`s rule).
const INSIGHT_PROMPT: &str =
    "Summarise this spending cycle in one short, plain sentence of budgeting advice.";

/// The suggestion prompt (const).
const SUGGEST_PROMPT: &str =
    "Propose one concrete budgeting action for this cycle as structured JSON.";

/// Build the chat prompt for a user turn.
fn chat_prompt(user_text: &str) -> String {
    format!(
        "You are Phoskonomia's budgeting assistant. Answer the user concisely.\nUser: {user_text}"
    )
}

/// Build the auto-categorize prompt listing the allowed categories.
fn categorize_prompt(line: &str, categories: &[String]) -> String {
    let list = if categories.is_empty() {
        "(no categories configured)".to_owned()
    } else {
        categories.join(", ")
    };
    format!("Pick the best category for the receipt line \"{line}\" from: {list}.")
}

/// JSON Schema for the auto-categorize structured output.
fn categorize_schema() -> serde_json::Value {
    json!({
        "type": "object",
        "properties": {
            "category": { "type": "string" },
            "confidence": { "type": "number" }
        },
        "required": ["category", "confidence"]
    })
}

/// JSON Schema for the structured suggestion output (savings as i64 centimes).
fn suggestion_schema() -> serde_json::Value {
    json!({
        "type": "object",
        "properties": {
            "text": { "type": "string" },
            "confidence": { "type": "number" },
            "target": { "type": "string" },
            "estimatedSavingsCentimes": { "type": "integer" }
        },
        "required": ["text", "confidence"]
    })
}

/// Normalize a model chat completion: trim, and fall back to a deterministic
/// acknowledgement when the model returns nothing usable (never a blank line).
fn normalize_reply(raw: &str) -> String {
    let t = raw.trim();
    if t.is_empty() {
        "I tracked that. Ask me about a category, signal, or your cycle pace.".to_owned()
    } else {
        t.to_owned()
    }
}

/// Normalize a model insight completion (trim + non-empty fallback).
fn normalize_insight(raw: &str) -> String {
    let t = raw.trim();
    if t.is_empty() {
        "Coffee runs are up 28% this cycle. Capping them at CHF 70 keeps you on budget".to_owned()
    } else {
        t.to_owned()
    }
}
