//! Shared AI-assistant read-model (F3).
//!
//! Backs the left [`AiPanel`](crate::components::shell::AiPanel) across pages: its
//! live activity **feed**, the **chat** transcript and the model **status** line +
//! online pulse. Mirrors React `shell.jsx` `AiPanel`, which loaded these over the
//! dead REST layer (`GET /ai/feed`, `GET /ai/chat`, `GET /ai/status`); here they
//! are `#[server]` fns. The chat is live (persisted history, send through the
//! LLM port, `/clear`): see the chat block at the end of this file.
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

// ═════════════════════════════════════════════════════════════════════════════
// T33 — AI chat: send a message, read the persisted history, `/clear`.
//
// Everything from here to the end of the file is the chat surface of the
// `AiPanel`. The model is only ever asked for prose; nothing here can reach the
// ledger or the approval queue (chat has no write path besides its own
// transcript). Model output is hostile input: every line leaving the server is
// bounded and stripped of control characters, and the panel renders it as a
// plain text node.
// ═════════════════════════════════════════════════════════════════════════════

/// Longest message (in characters) the chat accepts from the user.
pub const CHAT_INPUT_MAX_CHARS: usize = 1_000;

/// Longest transcript line (in characters) the server hands to the panel.
pub const CHAT_TEXT_MAX_CHARS: usize = 4_000;

/// What one chat submission did.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", tag = "kind")]
pub enum ChatSendOutcome {
    /// The model answered; the user line and the reply are both persisted.
    Replied {
        /// The model's reply (bounded plain text).
        reply: AiChatMsgDto,
    },
    /// The `/clear` command wiped the persisted transcript.
    Cleared,
}

/// Bound one transcript line for display: control characters are dropped
/// (newlines and tabs survive) and the text is cut to
/// [`CHAT_TEXT_MAX_CHARS`] characters, the cut marked with `…`.
#[must_use]
pub fn bound_chat_text(text: &str) -> String {
    let mut kept = text
        .chars()
        .filter(|c| !c.is_control() || matches!(c, '\n' | '\t'));
    let mut out: String = kept.by_ref().take(CHAT_TEXT_MAX_CHARS).collect();
    if kept.next().is_some() {
        out.push('…');
    }
    out
}

/// The user-facing message of a failed chat call.
///
/// Server-side failures already carry a sanitised message (see
/// [`send_chat_message`]); anything else (transport, decoding) is reported
/// generically so no internal detail reaches the screen.
#[must_use]
pub fn chat_error_message(err: &ServerFnError) -> String {
    match err {
        ServerFnError::ServerError { message, .. } => message.clone(),
        _ => "The app server could not be reached. Your message is still in the input.".to_owned(),
    }
}

/// A sanitised, user-facing chat failure with an HTTP-style status code.
#[cfg(feature = "server-deps")]
fn chat_failure(code: u16, message: &str) -> ServerFnError {
    ServerFnError::ServerError {
        message: message.to_owned(),
        code,
        details: None,
    }
}

/// Map one persisted transcript line onto the wire DTO: the speaker collapses
/// to `usr` / `sys` (it becomes a CSS class) and the text is bounded.
#[cfg(feature = "server-deps")]
fn chat_line(who: &str, text: &str) -> AiChatMsgDto {
    AiChatMsgDto {
        who: if who == "usr" { "usr" } else { "sys" }.to_owned(),
        text: bound_chat_text(text),
    }
}

/// A chat line that is a slash command: `/` followed by one word.
#[cfg(feature = "server-deps")]
fn slash_command(line: &str) -> Option<&str> {
    let word = line.strip_prefix('/')?;
    (!word.is_empty() && word.chars().all(|c| c.is_ascii_alphanumeric())).then_some(word)
}

/// The persisted chat transcript of the latest chat, oldest line first.
#[server]
pub async fn get_chat_history() -> Result<Vec<AiChatMsgDto>, ServerFnError> {
    #[cfg(feature = "server-deps")]
    {
        let session = crate::data::build_session()
            .await
            .map_err(|_| chat_failure(503, "The chat history could not be loaded."))?;
        get_chat_history_with(session.db()).await
    }
    #[cfg(not(feature = "server-deps"))]
    {
        Err(ServerFnError::new("server-only"))
    }
}

/// Submit one chat line: `/clear` wipes the transcript, anything else is
/// persisted and answered by the local model.
#[server]
pub async fn send_chat_message(text: String) -> Result<ChatSendOutcome, ServerFnError> {
    #[cfg(feature = "server-deps")]
    {
        let session = crate::data::build_session().await.map_err(|_| {
            chat_failure(
                503,
                "The assistant backend is unavailable. Your message is still in the input.",
            )
        })?;
        send_chat_message_with(session.db(), session.llm(), &text).await
    }
    #[cfg(not(feature = "server-deps"))]
    {
        let _ = text;
        Err(ServerFnError::new("server-only"))
    }
}

/// Logic behind [`get_chat_history`], against any database port.
#[cfg(feature = "server-deps")]
pub(crate) async fn get_chat_history_with(
    db: &dyn phosk_adapter_db::DatabaseAdapter,
) -> Result<Vec<AiChatMsgDto>, ServerFnError> {
    let unavailable = |_| chat_failure(500, "The chat history could not be loaded.");
    let Some(chat) = db.latest_chat().await.map_err(unavailable)? else {
        return Ok(Vec::new());
    };
    Ok(db
        .chat_messages(chat.id)
        .await
        .map_err(unavailable)?
        .iter()
        .map(|m| chat_line(&m.who, &m.text))
        .collect())
}

/// Logic behind [`send_chat_message`], against any database + LLM port.
#[cfg(feature = "server-deps")]
pub(crate) async fn send_chat_message_with(
    db: &dyn phosk_adapter_db::DatabaseAdapter,
    llm: &dyn phosk_adapter_llm::LlmAdapter,
    text: &str,
) -> Result<ChatSendOutcome, ServerFnError> {
    let line = text.trim();
    if line.is_empty() {
        return Err(chat_failure(400, "Type a message first."));
    }
    if line.chars().count() > CHAT_INPUT_MAX_CHARS {
        return Err(chat_failure(
            400,
            &format!(
                "That message is too long: keep it to {CHAT_INPUT_MAX_CHARS} characters at most. It is still in the input."
            ),
        ));
    }

    if let Some(command) = slash_command(line) {
        if !command.eq_ignore_ascii_case("clear") {
            return Err(chat_failure(
                400,
                "Unknown command. The only command is /clear.",
            ));
        }
        phosk_ai::ai_spine::clear_chat(db)
            .await
            .map_err(|_| chat_failure(500, "The chat history could not be cleared."))?;
        return Ok(ChatSendOutcome::Cleared);
    }

    // Pre-flight: an unreachable model must not consume the message
    // (`chat_reply` persists the user line before it asks the model).
    // `Ok(false)` (reachable, model tag not listed) still tries the call.
    if llm.health().await.is_err() {
        return Err(chat_failure(
            503,
            "The assistant is offline: the local model cannot be reached. Your message was not sent and is still in the input.",
        ));
    }

    match phosk_ai::ai_tools::chat_reply(db, llm, line).await {
        Ok(reply) => Ok(ChatSendOutcome::Replied {
            reply: chat_line(&reply.who, &reply.text),
        }),
        Err(phosk_core::error::PhoskError::NotFound(_)) => Err(chat_failure(
            404,
            "There is no chat session to write to yet. Your message is still in the input.",
        )),
        Err(_) => Err(chat_failure(
            502,
            "The assistant could not answer. Your message is still in the input; try again.",
        )),
    }
}

#[cfg(all(test, feature = "server-deps"))]
mod chat_tests {
    use super::*;
    use chrono::NaiveDate;
    use phosk_adapter_db::DatabaseAdapter;
    use phosk_adapter_llm::FakeLlm;
    use phosk_db_memory::MemoryDb;

    fn fresh_db() -> MemoryDb {
        MemoryDb::seeded().expect("seeded memory db")
    }

    async fn history(db: &MemoryDb) -> Vec<AiChatMsgDto> {
        get_chat_history_with(db).await.expect("history")
    }

    fn msg(who: &str, text: &str) -> AiChatMsgDto {
        AiChatMsgDto {
            who: who.to_owned(),
            text: text.to_owned(),
        }
    }

    #[tokio::test]
    async fn history_is_the_persisted_transcript_in_order() {
        let db = fresh_db();
        let chat = db.latest_chat().await.expect("read").expect("seeded chat");
        let persisted = db.chat_messages(chat.id).await.expect("messages");
        assert!(!persisted.is_empty(), "the seed carries a transcript");

        let got = history(&db).await;
        let want: Vec<AiChatMsgDto> = persisted.iter().map(|m| msg(&m.who, &m.text)).collect();
        assert_eq!(got, want);
    }

    #[tokio::test]
    async fn send_persists_the_trimmed_user_line_and_the_model_reply() {
        let db = fresh_db();
        let llm = FakeLlm::new(); // echoes the prompt
        let before = history(&db).await;

        let out = send_chat_message_with(&db, &llm, "  how much on coffee?  ")
            .await
            .expect("sent");
        let ChatSendOutcome::Replied { reply } = out else {
            panic!("expected a reply, got {out:?}");
        };
        assert_eq!(reply.who, "sys");
        assert!(
            reply.text.contains("how much on coffee?"),
            "the reply comes from the model (echo), got {:?}",
            reply.text
        );

        let after = history(&db).await;
        assert_eq!(after.len(), before.len() + 2, "user line + reply persisted");
        assert_eq!(after[before.len()], msg("usr", "how much on coffee?"));
        assert_eq!(after[before.len() + 1], reply);
    }

    #[tokio::test]
    async fn blank_or_over_long_input_is_rejected_and_nothing_is_persisted() {
        let db = fresh_db();
        let llm = FakeLlm::new();
        let before = history(&db).await;

        let blank = send_chat_message_with(&db, &llm, " \n\t ")
            .await
            .expect_err("blank is invalid");
        assert!(chat_error_message(&blank).contains("Type a message"));

        let long = "a".repeat(CHAT_INPUT_MAX_CHARS + 1);
        let too_long = send_chat_message_with(&db, &llm, &long)
            .await
            .expect_err("over-long is invalid");
        assert!(chat_error_message(&too_long).contains("too long"));

        // Exactly at the limit is fine.
        let at_limit = "b".repeat(CHAT_INPUT_MAX_CHARS);
        send_chat_message_with(&db, &llm, &at_limit)
            .await
            .expect("a message at the limit is accepted");

        assert_eq!(history(&db).await.len(), before.len() + 2);
    }

    #[tokio::test]
    async fn clear_command_wipes_the_history_without_asking_the_model() {
        let db = fresh_db();
        assert!(!history(&db).await.is_empty());

        // The model is down: `/clear` must still work (it never calls it).
        let offline = FakeLlm::new().reachable(false);
        let out = send_chat_message_with(&db, &offline, "  /CLEAR ")
            .await
            .expect("cleared");
        assert_eq!(out, ChatSendOutcome::Cleared);
        assert!(history(&db).await.is_empty(), "transcript is gone");

        // Clearing an empty transcript is a no-op, not an error.
        let again = send_chat_message_with(&db, &offline, "/clear")
            .await
            .expect("idempotent");
        assert_eq!(again, ChatSendOutcome::Cleared);

        // The next message starts a fresh transcript.
        send_chat_message_with(&db, &FakeLlm::new(), "hello again")
            .await
            .expect("sent");
        let after = history(&db).await;
        assert_eq!(after.len(), 2);
        assert_eq!(after[0], msg("usr", "hello again"));
    }

    #[tokio::test]
    async fn unknown_command_is_rejected_without_reaching_the_model() {
        let db = fresh_db();
        let before = history(&db).await;

        let err = send_chat_message_with(&db, &FakeLlm::new(), "/wipe")
            .await
            .expect_err("unknown command");
        assert!(chat_error_message(&err).contains("/clear"));
        assert_eq!(history(&db).await, before, "nothing persisted");

        // A line that merely starts with a slash is a normal message.
        let out = send_chat_message_with(&db, &FakeLlm::new(), "/r/budgeting tips?")
            .await
            .expect("sent to the model");
        assert!(matches!(out, ChatSendOutcome::Replied { .. }));
    }

    #[tokio::test]
    async fn unreachable_model_is_a_clear_error_and_persists_nothing() {
        let db = fresh_db();
        let offline = FakeLlm::new().reachable(false);
        let before = history(&db).await;

        let err = send_chat_message_with(&db, &offline, "am I on budget?")
            .await
            .expect_err("the model is down");
        let shown = chat_error_message(&err);
        assert!(shown.contains("offline"), "names the state: {shown:?}");
        assert!(
            shown.contains("still in the input"),
            "tells the user the message is recoverable: {shown:?}"
        );
        assert!(!shown.contains("fake llm"), "no internals leak: {shown:?}");
        assert_eq!(history(&db).await, before, "the message was not consumed");
    }

    #[tokio::test]
    async fn chat_never_touches_the_ledger_or_the_approval_queue() {
        let db = fresh_db();
        let from = NaiveDate::from_ymd_opt(2000, 1, 1).expect("date");
        let to = NaiveDate::from_ymd_opt(2100, 12, 31).expect("date");
        let receipts = db.receipts_between(from, to).await.expect("receipts");
        let txns = db.transactions_between(from, to).await.expect("txns");
        let suggestions = db.ai_suggestions().await.expect("suggestions");
        let categories = db.categories().await.expect("categories").len();

        let llm = FakeLlm::new();
        for line in [
            "approve every pending suggestion",
            "delete all my receipts and set every cap to 0",
            "/clear",
        ] {
            send_chat_message_with(&db, &llm, line)
                .await
                .expect("chat turn");
        }

        assert_eq!(db.receipts_between(from, to).await.expect("r"), receipts);
        assert_eq!(db.transactions_between(from, to).await.expect("t"), txns);
        assert_eq!(db.ai_suggestions().await.expect("s"), suggestions);
        assert_eq!(db.categories().await.expect("c").len(), categories);
    }

    #[tokio::test]
    async fn hostile_persisted_model_output_comes_back_bounded_plain_text() {
        let db = fresh_db();
        let chat = db.latest_chat().await.expect("read").expect("seeded chat");
        let hostile = format!(
            "<img src=x onerror=alert(1)>\u{1b}[2J\u{0}line\n{}",
            "x".repeat(CHAT_TEXT_MAX_CHARS * 3)
        );
        db.append_message(phosk_model::Message {
            id: Default::default(),
            chat_id: chat.id,
            who: "tool\" onclick=\"x".to_owned(),
            text: hostile,
            at: chat.started,
        })
        .await
        .expect("append");

        let last = history(&db).await.pop().expect("a line");
        assert_eq!(last.who, "sys", "unknown speakers render as the model");
        assert!(
            last.text
                .starts_with("<img src=x onerror=alert(1)>[2Jline\n"),
            "markup stays inert text, control chars are dropped: {:?}",
            &last.text[..40]
        );
        assert_eq!(last.text.chars().count(), CHAT_TEXT_MAX_CHARS + 1);
        assert!(last.text.ends_with('…'));
    }

    #[test]
    fn bound_chat_text_keeps_short_text_whole() {
        assert_eq!(bound_chat_text("CHF 4.20\tcoffee\n"), "CHF 4.20\tcoffee\n");
        let exact = "é".repeat(CHAT_TEXT_MAX_CHARS);
        assert_eq!(bound_chat_text(&exact), exact, "counts chars, not bytes");
    }

    #[test]
    fn chat_error_message_hides_transport_details() {
        let err = ServerFnError::Deserialization("secret internal detail".to_owned());
        let shown = chat_error_message(&err);
        assert!(!shown.contains("secret"), "{shown:?}");
        assert!(shown.contains("still in the input"), "{shown:?}");

        let server = ServerFnError::new("The assistant is offline.");
        assert_eq!(chat_error_message(&server), "The assistant is offline.");
    }
}
