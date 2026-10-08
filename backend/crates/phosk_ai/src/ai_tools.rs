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
//! Low-confidence (`< 0.7`, or NaN/out-of-range — see [`is_low_confidence`])
//! proposals are still returned, but carry [`ProposedWrite::low_confidence`]
//! `== true` so the approval UI can coral-flag them (ADR confidence rule).

use serde::{Deserialize, Serialize};
use serde_json::json;

use chrono::{Datelike, NaiveDate};
use phosk_adapter_db::DatabaseAdapter;
use phosk_adapter_llm::LlmAdapter;
use phosk_core::error::PhoskError;
use phosk_core::money::Money;
use phosk_id::{MessageId, SuggestionId};
use phosk_model::{AiSuggestion, Message, is_low_confidence};

use crate::ai_features::InsightDto;
use crate::ai_spine::AiChatMsgDto;

/// The ADR confidence threshold: proposals below this are flagged (coral),
/// never silently dropped. A re-export, not a second definition — the
/// canonical value lives at [`phosk_model::LOW_CONFIDENCE_THRESHOLD`].
pub use phosk_model::LOW_CONFIDENCE_THRESHOLD as CONFIDENCE_THRESHOLD;

/// Longest model chat reply (in characters) that [`chat_reply`] saves and
/// returns; a longer completion is cut and the cut marked with `…`.
pub const CHAT_REPLY_MAX_CHARS: usize = 4_000;

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
    let low_confidence = is_low_confidence(suggestion.confidence);
    ProposedWrite {
        suggestion,
        model: model.to_owned(),
        low_confidence,
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// READ tools — call the model, return prose / a DTO, never mutate
// ─────────────────────────────────────────────────────────────────────────────

/// Chat turn (READ tool): ask the model for a reply via the PORT, then persist
/// the user line and the reply, and return the reply.
///
/// The reply always comes from `llm.complete`. The model's prose is trimmed and
/// cut to [`CHAT_REPLY_MAX_CHARS`]; an empty completion is an error (nothing is
/// saved), never a stand-in sentence.
///
/// Nothing is persisted until the model has answered, so a failed turn (model
/// down, not loaded, timed out) leaves the transcript untouched and can be
/// retried without saving the user line twice.
///
/// # Errors
/// [`PhoskError::Invalid`] for empty `text` or a blank model reply;
/// [`PhoskError::NotFound`] if there is no chat to append to; otherwise
/// propagates adapter / model errors.
#[tracing::instrument(level = "debug", skip_all)]
pub async fn chat_reply(
    db: &dyn DatabaseAdapter,
    llm: &dyn LlmAdapter,
    text: &str,
    as_of: NaiveDate,
) -> Result<AiChatMsgDto, PhoskError> {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return Err(PhoskError::Invalid("empty chat message".to_owned()));
    }

    let Some(chat) = db.latest_chat().await? else {
        return Err(PhoskError::NotFound("no chat to append to".to_owned()));
    };

    // The model only sees what we hand it: a read-only snapshot of the cycle
    // and the last few turns. It cannot write anything from here.
    let context = chat_context(db, as_of).await?;
    let history = db.chat_messages(chat.id).await?;
    let recent = &history[history.len().saturating_sub(CHAT_HISTORY_TURNS)..];

    // Ask the model first (the only place the chat reply comes from now): a
    // failure here must not leave a saved question without an answer.
    let raw = llm
        .complete(&chat_prompt(&context, recent, trimmed))
        .await?;
    let reply_text = normalize_reply(&raw)?;

    // Persist the user turn verbatim, then the reply.
    db.append_message(Message {
        id: MessageId::new(),
        chat_id: chat.id,
        who: "usr".to_owned(),
        text: text.to_owned(),
        at: as_of,
    })
    .await?;

    db.append_message(Message {
        id: MessageId::new(),
        chat_id: chat.id,
        who: "sys".to_owned(),
        text: reply_text.clone(),
        at: as_of,
    })
    .await?;

    Ok(AiChatMsgDto {
        who: "sys".to_owned(),
        text: reply_text,
    })
}

/// A model-written narrative insight (READ tool): the model reads the same
/// data snapshot the chat gets ([`chat_context`]) and writes one sentence.
///
/// `source` is the live model id (provenance). No saving is estimated here
/// (`estimated_savings` is zero): money figures come from arithmetic, not from
/// the model. The dashboard uses the computed
/// [`crate::ai_features::dashboard_insight`] instead; this stays for a surface
/// that explicitly asks the model.
///
/// # Errors
/// Propagates adapter / model errors; a blank completion is
/// [`PhoskError::Invalid`] (never a stand-in sentence).
#[tracing::instrument(level = "debug", skip_all, fields(as_of = %as_of))]
pub async fn narrative_insight(
    db: &dyn DatabaseAdapter,
    llm: &dyn LlmAdapter,
    as_of: chrono::NaiveDate,
) -> Result<InsightDto, PhoskError> {
    let context = chat_context(db, as_of).await?;
    let raw = llm
        .complete(&format!("DATA\n{context}\n{INSIGHT_PROMPT}"))
        .await?;
    let text = normalize_insight(&raw)?;
    Ok(InsightDto {
        source: llm.model().to_owned(),
        text,
        estimated_savings: Money::ZERO,
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
/// `db` is READ context only: the model sees the [`chat_context`] snapshot.
/// The returned [`ProposedWrite`] is a candidate awaiting approval;
/// `estimatedSavings` is carried as exact i64 centimes.
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
    let context = chat_context(db, as_of).await?;
    let schema = suggestion_schema();
    let out = llm
        .generate_structured(&format!("DATA\n{context}\n{SUGGEST_PROMPT}"), &schema)
        .await?;

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

/// The narrative-insight prompt (const, per the prompts-as-`const`s rule). It
/// follows the data snapshot.
pub const INSIGHT_PROMPT: &str = "Using only the data above, summarise this spending cycle in one short, plain sentence of budgeting advice. If there is no spending yet, say so.";

/// The suggestion prompt (const). It follows the data snapshot.
pub const SUGGEST_PROMPT: &str = "Using only the data above, propose one concrete budgeting action for this cycle as structured JSON.";

/// How many earlier chat messages the model sees with each new turn.
const CHAT_HISTORY_TURNS: usize = 8;

/// How many of the latest transactions the chat snapshot lists.
const CHAT_RECENT_TX: usize = 15;

/// A plain-text, read-only snapshot of the user's money for the chat model:
/// the calendar month containing `as_of` (budget, savings target, spend per
/// category against its cap) plus the latest transactions of the last 90
/// days. Amounts are rendered from exact centimes. Spend per category is
/// item-level: a receipt's lines count under their own category
/// (`phosk_adapter_db::spend`).
///
/// # Errors
/// Propagates adapter errors.
pub async fn chat_context(
    db: &dyn DatabaseAdapter,
    as_of: NaiveDate,
) -> Result<String, PhoskError> {
    use std::fmt::Write as _;

    let month_start = as_of.with_day(1).unwrap_or(as_of);
    let window_start = as_of - chrono::Days::new(90);
    let mut txs = db.transactions_between(window_start, as_of).await?;
    txs.sort_by_key(|t| std::cmp::Reverse(t.date));

    let month: Vec<_> = txs.iter().filter(|t| t.date >= month_start).collect();
    let spent = Money::sum(month.iter().map(|t| t.amount))?;

    let mut out = String::new();
    let _ = writeln!(out, "Today: {as_of}. Currency CHF.");
    if let Ok(cfg) = db.budget_config().await {
        let _ = writeln!(
            out,
            "Monthly budget: {} · savings target: {} (0 means not set).",
            cfg.monthly_budget, cfg.savings_target
        );
    }
    let _ = writeln!(out, "Spent this month ({month_start} to {as_of}): {spent}.");

    let caps = db.category_caps().await?;
    let receipts = db.receipts_between(month_start, as_of).await?;
    let parts = phosk_adapter_db::spend::receipt_parts(db, &receipts).await?;
    let mut names: Vec<String> = caps.iter().map(|c| c.name.clone()).collect();
    for p in &parts {
        if !names.contains(&p.category) {
            names.push(p.category.clone());
        }
    }
    let _ = writeln!(out, "By category this month:");
    for name in &names {
        let cat_spent = phosk_adapter_db::spend::spent_in(&parts, name)?;
        let cap = caps
            .iter()
            .find(|c| &c.name == name)
            .and_then(|c| c.cap)
            .map_or_else(|| "no cap".to_owned(), |c| format!("cap {c}"));
        let _ = writeln!(out, "- {name}: {cat_spent} ({cap})");
    }

    let _ = writeln!(out, "Latest transactions (newest first):");
    if txs.is_empty() {
        let _ = writeln!(out, "- none recorded yet");
    }
    for t in txs.iter().take(CHAT_RECENT_TX) {
        let _ = writeln!(out, "- {} {} [{}] {}", t.date, t.shop, t.category, t.amount);
    }
    Ok(out)
}

/// Build the chat prompt for a user turn: the data snapshot, the recent
/// conversation, then the new question.
fn chat_prompt(context: &str, recent: &[Message], user_text: &str) -> String {
    let mut history = String::new();
    for m in recent {
        let who = if m.who == "usr" { "User" } else { "Assistant" };
        history.push_str(who);
        history.push_str(": ");
        history.push_str(&m.text);
        history.push('\n');
    }
    format!(
        "You are Phoskonomia's budgeting assistant for one person in Switzerland. \
         Answer concisely in plain text (no markdown tables), using only the data below; \
         if the data does not answer the question, say so. You cannot change any data.\n\n\
         DATA\n{context}\nCONVERSATION\n{history}User: {user_text}\nAssistant:"
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

/// Normalize a model chat completion: trim and cut to [`CHAT_REPLY_MAX_CHARS`]
/// (the cut marked with `…`).
///
/// # Errors
/// [`PhoskError::Invalid`] when the model returned nothing usable: an empty
/// answer is reported, never replaced by a sentence the model did not write.
fn normalize_reply(raw: &str) -> Result<String, PhoskError> {
    let t = raw.trim();
    if t.is_empty() {
        return Err(PhoskError::Invalid(
            "the model returned an empty reply".to_owned(),
        ));
    }
    let mut chars = t.chars();
    let mut out: String = chars.by_ref().take(CHAT_REPLY_MAX_CHARS).collect();
    if chars.next().is_some() {
        out.push('…');
    }
    Ok(out)
}

/// Normalize a model insight completion (trim; blank is an error).
///
/// # Errors
/// [`PhoskError::Invalid`] when the model returned nothing usable.
fn normalize_insight(raw: &str) -> Result<String, PhoskError> {
    let t = raw.trim();
    if t.is_empty() {
        Err(PhoskError::Invalid(
            "the model returned an empty insight".to_owned(),
        ))
    } else {
        Ok(t.to_owned())
    }
}
