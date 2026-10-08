//! `ai_spine` — the shared **`AiPanel`** read-model + chat spine.
//!
//! Backs the left AI panel that rides every page: its live activity **feed**, the
//! **chat** transcript and the model **status** line + online pulse. Mirrors the
//! Dioxus `ai.rs` view DTOs (`AiFeedItemDto` / `AiChatMsgDto` / `AiStatusDto` /
//! `AiPanelDto`), composed here from the PORT instead of seeded inline.
//!
//! Feed and transcript come from the database PORT; the status line asks the
//! LLM PORT which model it runs and whether that model answers right now. Chat
//! turns go through [`crate::ai_tools::chat_reply`] (the model), never through a
//! canned reply.

use serde::{Deserialize, Serialize};

use phosk_adapter_db::DatabaseAdapter;
use phosk_adapter_llm::LlmAdapter;
use phosk_core::error::PhoskError;

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

/// AI status line + online pulse (`/ai/status`), read from the LLM PORT: the
/// configured model id and whether it answers a health probe. Mirrors
/// `ai.rs::AiStatusDto`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AiStatusDto {
    /// The model server is reachable AND lists the configured model → green pulse.
    pub online: bool,
    /// The configured model id, as the LLM adapter reports it (e.g.
    /// `"qwen3.6:35b-custom"`).
    pub model: String,
    /// Inference engine, as named by the composition root (e.g. `"OLLAMA"`).
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

/// Where the model runs. The composition root only ever wires a model server on
/// this machine (Ollama on localhost), never a remote service.
const LOCATION: &str = "LOCAL";

/// The model status line: the adapter's model id, and `online` only when the
/// health probe says the model server is up and has that model. An unreachable
/// server and a missing model both read as offline; the probe's own timeout
/// (a few seconds in the Ollama adapter) bounds the wait.
#[tracing::instrument(level = "debug", skip_all)]
pub async fn ai_status(llm: &dyn LlmAdapter, engine: &str) -> AiStatusDto {
    let online = matches!(llm.health().await, Ok(true));
    AiStatusDto {
        online,
        model: llm.model().to_owned(),
        engine: engine.to_owned(),
        location: LOCATION.to_owned(),
    }
}

/// The shared assistant read (`/ai/feed` + `/ai/chat` + `/ai/status`): the
/// feed items and latest chat transcript from the database PORT, the status
/// line from the LLM PORT ([`ai_status`]). `engine` names the inference engine
/// the composition root wired (the LLM port does not know it).
///
/// # Errors
/// Propagates any [`PhoskError`] from the adapter reads. A down model is not an
/// error: it shows as `status.online == false`.
#[tracing::instrument(level = "debug", skip_all)]
pub async fn ai_panel(
    db: &dyn DatabaseAdapter,
    llm: &dyn LlmAdapter,
    engine: &str,
) -> Result<AiPanelDto, PhoskError> {
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
        status: ai_status(llm, engine).await,
    })
}

/// A non-empty, render-ready date label for a feed item (`18 Jun`).
fn relative_time(at: chrono::NaiveDate) -> String {
    at.format("%d %b").to_string()
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
