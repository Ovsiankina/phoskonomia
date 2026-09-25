//! `phosk_ai` — the AI read-slice + spine SERVICE crate (ADR-002 L5).
//!
//! Composes the shared **`AiPanel`** read-model (live feed + chat transcript +
//! model status — [`ai_spine`]) and the AI feature surfaces (feed dismiss +
//! dashboard insight — [`ai_features`]) from the PORT, and exposes the chat /
//! feed write paths, plus the approval service ([`ai_approval`]) — the only
//! path from a model proposal to the ledger.
//!
//! **Layering (ADR-010).** Every service fn takes a `&dyn DatabaseAdapter` (the
//! PORT) and depends only on the PORT trait crate + the foundation crates
//! (`phosk_core`, `phosk_model`, `phosk_id`) — never on a concrete adapter
//! (`phosk_db_memory` is a *dev*-dependency, tests only).
//!
//! **Money is exact centimes (centimes-everywhere).** [`InsightDto`]'s
//! `estimatedSavings` serializes as its lossless i64 centime count via
//! [`phosk_model::money_centimes`]. CHF `f64` is render-only and never on the wire.
//!
//! This crate's DTOs + signatures form the locked wire contract; the service
//! bodies compute the read-models from the PORT.

pub mod ai_approval;
pub mod ai_features;
pub mod ai_spine;
pub mod ai_tools;

pub use ai_approval::{
    ApprovalOutcome, PendingGroup, PendingSuggestion, approve_receipt, approve_suggestion,
    pending_suggestions, reject_suggestion, validate_proposal,
};
pub use ai_features::{InsightDto, dashboard_insight, dismiss_feed_item};
pub use ai_spine::{
    AiChatMsgDto, AiFeedItemDto, AiPanelDto, AiStatusDto, ai_panel, clear_chat, send_message,
};
pub use ai_tools::{
    CHAT_REPLY_MAX_CHARS, CONFIDENCE_THRESHOLD, ProposedWrite, ToolEffect, auto_categorize,
    chat_reply, narrative_insight, suggest,
};
