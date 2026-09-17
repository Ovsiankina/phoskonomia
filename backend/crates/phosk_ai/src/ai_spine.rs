//! `ai_spine` — the shared **`AiPanel`** read-model + chat spine.
//!
//! Backs the left AI panel that rides every page: its live activity **feed**, the
//! **chat** transcript and the model **status** line + online pulse. Mirrors the
//! Dioxus `ai.rs` view DTOs (`AiFeedItemDto` / `AiChatMsgDto` / `AiStatusDto` /
//! `AiPanelDto`), composed here from the PORT instead of seeded inline.
//!
//! No LLM wiring yet: the read-models are composed from the PORT, while the real
//! Ollama/GEMMA4 spine lands later (see `backend-features-todo.md` §7).

use serde::{Deserialize, Serialize};

use phosk_adapter_db::DatabaseAdapter;
use phosk_core::error::PhoskError;
use phosk_id::MessageId;
use phosk_model::Message;

/// One AI-feed activity item (`/ai/feed` element). Shapes the panel's `FeedItem`
/// prop. Mirrors `dioxus-app/src/data/ai.rs::AiFeedItemDto`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AiFeedItemDto {
    /// Stable id (feed key + track/dismiss target).
    pub id: String,
    /// `categorize` | `reprocess` | `suggest` | `detect` — picks the icon.
    pub kind: String,
    /// Activity text.
    pub text: String,
    /// Optional confidence `0..1` (rendered `CONF NN%`).
    pub conf: Option<f64>,
    /// Optional state (`running` shows the RUNNING… chip).
    pub state: Option<String>,
    /// Relative time label.
    pub time: String,
    /// Action button labels (first is primary).
    pub actions: Vec<String>,
    /// Candidate flag — first action is the primary "track" affordance.
    pub cand: bool,
}

/// One chat transcript line (`/ai/chat` element). `who` is `"usr"` (user) or
/// `"sys"` (model). Mirrors `ai.rs::AiChatMsgDto`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AiChatMsgDto {
    /// `"usr"` or `"sys"`.
    pub who: String,
    /// Message text.
    pub text: String,
}

/// AI status line + online pulse (`/ai/status`). Seeded for now (`online`, model
/// `"GEMMA4"`, engine `"OLLAMA"`, location `"LOCAL"`). Mirrors `ai.rs::AiStatusDto`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AiStatusDto {
    /// Model reachable / loaded → green pulse.
    pub online: bool,
    /// Model badge, e.g. `"GEMMA4"`.
    pub model: String,
    /// Inference engine, e.g. `"OLLAMA"`.
    pub engine: String,
    /// Where it runs, e.g. `"LOCAL"`.
    pub location: String,
}

/// The full assistant payload one page fetch carries (feed + chat + status), so a
/// page wires a single read for the whole panel. Mirrors `ai.rs::AiPanelDto`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AiPanelDto {
    /// Live activity feed.
    pub feed: Vec<AiFeedItemDto>,
    /// Chat transcript so far.
    pub msgs: Vec<AiChatMsgDto>,
    /// Status line + pulse.
    pub status: AiStatusDto,
}

/// The shared assistant read (`/ai/feed` + `/ai/chat` + `/ai/status`): composes
/// the panel from the PORT (feed items, latest chat transcript, seeded status).
///
/// # Errors
/// Propagates any [`PhoskError`] from the adapter reads.
#[tracing::instrument(level = "debug", skip_all)]
pub async fn ai_panel(db: &dyn DatabaseAdapter) -> Result<AiPanelDto, PhoskError> {
    let feed = db
        .feed_items()
        .await?
        .into_iter()
        .map(|f| AiFeedItemDto {
            id: f.id.to_string(),
            kind: f.kind,
            text: f.text,
            conf: f.conf,
            state: f.state,
            time: relative_time(f.at),
            actions: f.actions,
            cand: f.cand,
        })
        .collect();

    let msgs = match db.latest_chat().await? {
        Some(chat) => db
            .chat_messages(chat.id)
            .await?
            .into_iter()
            .map(|m| AiChatMsgDto {
                who: m.who,
                text: m.text,
            })
            .collect(),
        None => Vec::new(),
    };

    Ok(AiPanelDto {
        feed,
        msgs,
        status: status(),
    })
}

/// The seeded local-model status pulse (GEMMA4 on OLLAMA, LOCAL, online).
fn status() -> AiStatusDto {
    AiStatusDto {
        online: true,
        model: "GEMMA4".to_owned(),
        engine: "OLLAMA".to_owned(),
        location: "LOCAL".to_owned(),
    }
}

/// A non-empty relative-time label derived from a feed item's date. The seed has
/// no wall clock, so the date string is a stable, render-ready stand-in.
fn relative_time(at: chrono::NaiveDate) -> String {
    at.format("%d %b").to_string()
}

/// Produce the model's (canned) reply to a user message. Real GEMMA4 inference
/// lands later; this keeps the spine deterministic for tests.
fn canned_reply(_text: &str) -> String {
    "I tracked that. Ask me about a category, signal, or your cycle pace.".to_owned()
}

/// Append a user message to the latest chat and return the model's reply (a canned
/// reply for now; real GEMMA4 inference lands later).
///
/// # Errors
/// Propagates any [`PhoskError`] from the adapter reads/writes.
#[tracing::instrument(level = "debug", skip_all)]
pub async fn send_message(
    db: &dyn DatabaseAdapter,
    text: &str,
) -> Result<AiChatMsgDto, PhoskError> {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return Err(PhoskError::Invalid("empty chat message".to_owned()));
    }

    let Some(chat) = db.latest_chat().await? else {
        return Err(PhoskError::NotFound("no chat to append to".to_owned()));
    };

    // Persist the user turn verbatim, then the canned model reply. `at` mirrors
    // the chat's start date (the seed has no wall clock); ordering is by append.
    let user = Message {
        id: MessageId::new(),
        chat_id: chat.id,
        who: "usr".to_owned(),
        text: text.to_owned(),
        at: chat.started,
    };
    db.append_message(user).await?;

    let reply_text = canned_reply(text);
    let reply = Message {
        id: MessageId::new(),
        chat_id: chat.id,
        who: "sys".to_owned(),
        text: reply_text.clone(),
        at: chat.started,
    };
    db.append_message(reply).await?;

    Ok(AiChatMsgDto {
        who: "sys".to_owned(),
        text: reply_text,
    })
}

/// Clear the latest chat transcript.
///
/// # Errors
/// Propagates any [`PhoskError`] from the adapter writes.
#[tracing::instrument(level = "debug", skip_all)]
pub async fn clear_chat(db: &dyn DatabaseAdapter) -> Result<(), PhoskError> {
    // No chat ⇒ nothing to clear (idempotent no-op, never an error).
    if let Some(chat) = db.latest_chat().await? {
        db.clear_chat(chat.id).await?;
    }
    Ok(())
}
