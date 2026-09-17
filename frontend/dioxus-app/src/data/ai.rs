//! Shared AI-assistant read-model (F3).
//!
//! Backs the left [`AiPanel`](crate::components::shell::AiPanel) across pages: its
//! live activity **feed**, the **chat** transcript and the model **status** line +
//! online pulse. Mirrors React `shell.jsx` `AiPanel`, which loaded these over the
//! dead REST layer (`GET /ai/feed`, `GET /ai/chat`, `GET /ai/status`); here they
//! are seeded `#[server]` fns. The real Ollama/GEMMA4 wiring lands later.
//!
//! These DTOs are serde view structs (so they cross the `#[server]` boundary);
//! pages map them onto the panel's `FeedItem`/`ChatMsg` props.

use dioxus::prelude::*;
use serde::{Deserialize, Serialize};

/// One AI-feed activity item (`GET /ai/feed` element). Shapes the panel's
/// `FeedItem` prop.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AiFeedItemDto {
    /// Stable id (feed key + track/dismiss target).
    pub id: String,
    /// `categorize` | `reprocess` | `suggest` | other — picks the icon.
    pub kind: String,
    /// Activity text.
    pub text: String,
    /// Optional confidence 0..1 (rendered `CONF NN%`).
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

/// One chat transcript line (`GET /ai/chat` element). Shapes the panel's
/// `ChatMsg` prop. `who` is `"usr"` (user) or `"sys"` (model).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AiChatMsgDto {
    /// `"usr"` or `"sys"`.
    pub who: String,
    /// Message text.
    pub text: String,
}

/// AI status line + online pulse (`GET /ai/status`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
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

/// The full assistant payload one page fetch carries (feed + chat + status),
/// so a page wires a single `use_resource` for the whole panel.
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

/// The shared assistant read (`/ai/feed` + `/ai/chat` + `/ai/status`).
///
/// REAL: composes `phosk_ai::ai_spine::ai_panel` — the live activity feed and
/// chat transcript come from the PORT (seeded), the status line is the
/// GEMMA4-on-Ollama pulse. The real local-model inference lands with the LLM
/// adapter; the feed/chat reads here are already live against the adapter.
#[server]
pub async fn get_ai_panel() -> Result<AiPanelDto, ServerFnError> {
    #[cfg(feature = "server-deps")]
    {
        let session = crate::data::build_session().await?;
        let p = phosk_ai::ai_spine::ai_panel(session.db())
            .await
            .map_err(|e| ServerFnError::new(e.to_string()))?;
        Ok(AiPanelDto {
            feed: p
                .feed
                .into_iter()
                .map(|f| AiFeedItemDto {
                    id: f.id,
                    kind: f.kind,
                    text: f.text,
                    conf: f.conf,
                    state: f.state,
                    time: f.time,
                    actions: f.actions,
                    cand: f.cand,
                })
                .collect(),
            msgs: p
                .msgs
                .into_iter()
                .map(|m| AiChatMsgDto {
                    who: m.who,
                    text: m.text,
                })
                .collect(),
            status: AiStatusDto {
                online: p.status.online,
                model: p.status.model,
                engine: p.status.engine,
                location: p.status.location,
            },
        })
    }
    #[cfg(not(feature = "server-deps"))]
    {
        Err(ServerFnError::new("server-only"))
    }
}
