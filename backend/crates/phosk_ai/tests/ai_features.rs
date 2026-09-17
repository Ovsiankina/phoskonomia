//! RED integration tests for `phosk_ai` — the AI read-slice + spine SERVICE crate.
//!
//! Drives the five public service fns (`ai_panel`, `send_message`, `clear_chat`,
//! `dismiss_feed_item`, `dashboard_insight`) against the deterministic Swiss
//! May/June-2026 seed (`phosk_db_memory::MemoryDb::seeded`). Every body is
//! `todo!()` today, so each `#[tokio::test]` COMPILES then PANICS at runtime —
//! these are the red tests the green phase turns true.
//!
//! Scope (per the AI feature checklist): activity feed (+ dismiss), narrative
//! dashboard insight, chat history + send + clear (persistence), AI status
//! (model/engine/online), and the suggestion/candidate/provenance shapes carried
//! on feed items. Money is asserted in EXACT i64 centimes; CHF float is never a
//! wire form.
//!
//! Test-only: `expect("msg")` is allowed (the workspace denies it in prod);
//! never bare `unwrap()`.
#![allow(
    clippy::expect_used,
    clippy::doc_markdown,
    clippy::missing_const_for_fn
)]

use phosk_core::money::Money;
use phosk_db_memory::MemoryDb;

use phosk_ai::ai_features::{InsightDto, dashboard_insight, dismiss_feed_item};
use phosk_ai::ai_spine::{
    AiChatMsgDto, AiFeedItemDto, AiPanelDto, AiStatusDto, ai_panel, clear_chat, send_message,
};

/// The seeded in-memory adapter, behind the PORT.
fn db() -> MemoryDb {
    MemoryDb::seeded().expect("the deterministic Swiss seed builds")
}

/// The pinned demo clock: day 18 of the 30-day June 2026 cycle.
fn as_of() -> chrono::NaiveDate {
    chrono::NaiveDate::from_ymd_opt(2026, 6, 18).expect("2026-06-18 is a valid date")
}

// ── ai_panel: composed read-model (feed + chat + status) ──────────────────────

#[tokio::test]
async fn ai_panel_composes_feed_chat_and_status() {
    let db = db();
    let panel: AiPanelDto = ai_panel(&db).await.expect("ai_panel reads the seed");

    // The panel carries all three sub-models.
    assert!(!panel.feed.is_empty(), "the seed has activity-feed items");
    assert!(!panel.msgs.is_empty(), "the seed has a chat transcript");
    assert_eq!(panel.status.model, "GEMMA4");
}

#[tokio::test]
async fn ai_panel_feed_matches_seed_count_and_first_item() {
    let db = db();
    let panel = ai_panel(&db).await.expect("ai_panel reads the seed");

    // Seed: 3 feed items (categorize, detect-candidate, reprocess-running).
    assert_eq!(
        panel.feed.len(),
        3,
        "seed_feed_items has exactly three entries"
    );

    let first: &AiFeedItemDto = &panel.feed[0];
    assert_eq!(first.kind, "categorize");
    assert_eq!(first.text, "Categorised 4 lines on the Migros receipt");
    assert_eq!(first.conf, Some(0.93));
    assert_eq!(first.state, None);
    assert_eq!(first.actions, vec!["VIEW".to_owned()]);
    assert!(!first.cand, "the categorize item is not a candidate");
}

#[tokio::test]
async fn ai_panel_feed_surfaces_candidate_with_confidence() {
    let db = db();
    let panel = ai_panel(&db).await.expect("ai_panel reads the seed");

    // The detect item is a low-confidence candidate (0.72) with CONFIRM/DISMISS.
    let cand = panel
        .feed
        .iter()
        .find(|f| f.cand)
        .expect("the seed has one candidate feed item");
    assert_eq!(cand.kind, "detect");
    assert_eq!(cand.conf, Some(0.72));
    assert_eq!(
        cand.actions,
        vec!["CONFIRM".to_owned(), "DISMISS".to_owned()]
    );
}

#[tokio::test]
async fn ai_panel_feed_running_item_has_no_confidence() {
    let db = db();
    let panel = ai_panel(&db).await.expect("ai_panel reads the seed");

    // The reprocess item is in-flight: state "running", no confidence, no actions.
    let running = panel
        .feed
        .iter()
        .find(|f| f.state.as_deref() == Some("running"))
        .expect("the seed has one running feed item");
    assert_eq!(running.kind, "reprocess");
    assert_eq!(running.conf, None);
    assert!(running.actions.is_empty());
    assert!(!running.cand);
}

#[tokio::test]
async fn ai_panel_feed_item_carries_a_relative_time_label() {
    let db = db();
    let panel = ai_panel(&db).await.expect("ai_panel reads the seed");

    // Each feed item renders a relative-time label string (derived from `at`).
    for item in &panel.feed {
        assert!(!item.time.is_empty(), "every feed item has a time label");
    }
}

#[tokio::test]
async fn ai_panel_chat_matches_seed_transcript() {
    let db = db();
    let panel = ai_panel(&db).await.expect("ai_panel reads the seed");

    // Seed: a 2-line transcript (usr question, sys reply).
    assert_eq!(panel.msgs.len(), 2, "the seeded chat has two messages");
    assert_eq!(panel.msgs[0].who, "usr");
    assert_eq!(panel.msgs[0].text, "How am I doing this cycle?");
    assert_eq!(panel.msgs[1].who, "sys");
    assert_eq!(
        panel.msgs[1].text,
        "You're at 78% of your going-out cap with 12 days left."
    );
}

#[tokio::test]
async fn ai_panel_status_is_the_seeded_local_gemma_pulse() {
    let db = db();
    let panel = ai_panel(&db).await.expect("ai_panel reads the seed");

    let status: AiStatusDto = panel.status;
    assert!(status.online, "the seeded model is online");
    assert_eq!(status.model, "GEMMA4");
    assert_eq!(status.engine, "OLLAMA");
    assert_eq!(status.location, "LOCAL");
}

#[tokio::test]
async fn ai_panel_serializes_feed_item_as_camelcase() {
    let db = db();
    let panel = ai_panel(&db).await.expect("ai_panel reads the seed");

    let v = serde_json::to_value(&panel.feed[0]).expect("feed item serializes");
    // camelCase contract + the optional `state`/`conf` keys present.
    assert!(v.get("kind").is_some());
    assert!(v.get("conf").is_some());
    assert!(v.get("state").is_some());
    assert!(v.get("actions").is_some());
    assert!(v.get("cand").is_some());
    // No snake_case leakage.
    assert!(v.get("is_candidate").is_none());
}

// ── send_message: chat history + send + persistence + canned reply ────────────

#[tokio::test]
async fn send_message_returns_a_system_reply() {
    let db = db();
    let reply: AiChatMsgDto = send_message(&db, "anything I could cut?")
        .await
        .expect("send_message produces a reply");

    // The model reply is authored by "sys" and is non-empty.
    assert_eq!(reply.who, "sys");
    assert!(!reply.text.is_empty(), "the canned reply has content");
}

#[tokio::test]
async fn send_message_persists_both_the_user_line_and_the_reply() {
    let db = db();
    let before = ai_panel(&db)
        .await
        .expect("read transcript before")
        .msgs
        .len();

    let _ = send_message(&db, "how much on subscriptions?")
        .await
        .expect("send_message persists");

    let after = ai_panel(&db).await.expect("read transcript after").msgs;
    // The user message AND the system reply are appended (transcript grows by 2).
    assert_eq!(after.len(), before + 2, "both turns are persisted");

    // The newly-appended user line carries the exact text we sent.
    let user_line = after
        .iter()
        .rev()
        .find(|m| m.who == "usr")
        .expect("the user line was persisted");
    assert_eq!(user_line.text, "how much on subscriptions?");
}

#[tokio::test]
async fn send_message_appends_to_the_latest_chat() {
    let db = db();
    // Drive two turns; the transcript should accumulate, not reset.
    let _ = send_message(&db, "first").await.expect("first turn");
    let _ = send_message(&db, "second").await.expect("second turn");

    let msgs = ai_panel(&db).await.expect("read transcript").msgs;
    // 2 seeded + 2 user + 2 sys = 6.
    assert_eq!(msgs.len(), 6, "two turns append four lines onto the seed");
}

// ── clear_chat: persistence reset ─────────────────────────────────────────────

#[tokio::test]
async fn clear_chat_empties_the_transcript() {
    let db = db();
    clear_chat(&db)
        .await
        .expect("clear_chat resolves the latest chat");

    let msgs = ai_panel(&db)
        .await
        .expect("read transcript after clear")
        .msgs;
    assert!(msgs.is_empty(), "the transcript is empty after clear");
}

#[tokio::test]
async fn clear_chat_then_send_starts_a_fresh_pair() {
    let db = db();
    clear_chat(&db).await.expect("clear first");
    let _ = send_message(&db, "fresh start")
        .await
        .expect("send after clear");

    let msgs = ai_panel(&db).await.expect("read transcript").msgs;
    // Only the new user line + the reply remain.
    assert_eq!(msgs.len(), 2, "exactly the new turn survives a prior clear");
    assert_eq!(msgs[0].who, "usr");
    assert_eq!(msgs[0].text, "fresh start");
    assert_eq!(msgs[1].who, "sys");
}

// ── dismiss_feed_item: activity-feed write ────────────────────────────────────

#[tokio::test]
async fn dismiss_feed_item_removes_it_from_the_panel() {
    let db = db();
    let panel = ai_panel(&db).await.expect("read feed before dismiss");
    let target = panel.feed[0].id.clone();
    let before = panel.feed.len();

    dismiss_feed_item(&db, &target)
        .await
        .expect("dismiss the first feed item");

    let after = ai_panel(&db).await.expect("read feed after dismiss").feed;
    assert_eq!(after.len(), before - 1, "the feed shrinks by one");
    assert!(
        !after.iter().any(|f| f.id == target),
        "the dismissed id is gone from the feed"
    );
}

#[tokio::test]
async fn dismiss_unknown_feed_item_is_not_found() {
    let db = db();
    let err = dismiss_feed_item(&db, "no-such-feed-item")
        .await
        .expect_err("dismissing an unknown id is an error");

    assert!(
        matches!(err, phosk_core::error::PhoskError::NotFound(_)),
        "unknown feed id maps to PhoskError::NotFound, got {err:?}"
    );
}

// ── dashboard_insight: narrative one-liner + estimated saving (centimes) ──────

#[tokio::test]
async fn dashboard_insight_is_the_gemma_one_liner() {
    let db = db();
    let insight: InsightDto = dashboard_insight(&db, as_of())
        .await
        .expect("dashboard_insight composes the cycle insight");

    assert_eq!(insight.model, "GEMMA4");
    assert!(!insight.text.is_empty(), "the insight sentence has content");
}

#[tokio::test]
async fn dashboard_insight_estimated_savings_is_exact_centimes() {
    let db = db();
    let insight = dashboard_insight(&db, as_of())
        .await
        .expect("dashboard_insight composes the cycle insight");

    // Pinned seed value: CHF 42.00 = 4_200 centimes (matches dashboard.rs::get_insight).
    assert_eq!(
        insight.estimated_savings,
        Money::from_centimes(4_200),
        "the estimated saving is exact i64 centimes, never a CHF float"
    );
}

#[tokio::test]
async fn dashboard_insight_serializes_savings_as_i64_centimes() {
    let db = db();
    let insight = dashboard_insight(&db, as_of())
        .await
        .expect("dashboard_insight composes the cycle insight");

    let v = serde_json::to_value(&insight).expect("insight serializes");
    // camelCase key + EXACT centime integer on the wire (not 42.0 CHF).
    assert_eq!(
        v.get("estimatedSavings")
            .expect("camelCase money key present"),
        &serde_json::json!(4_200),
        "estimatedSavings serializes as the i64 centime count"
    );
    assert_eq!(v.get("model").expect("model key"), "GEMMA4");
    // No snake_case / CHF-float leakage.
    assert!(v.get("estimated_savings").is_none());
}

#[tokio::test]
async fn dashboard_insight_round_trips_through_json() {
    let db = db();
    let insight = dashboard_insight(&db, as_of())
        .await
        .expect("dashboard_insight composes the cycle insight");

    let json = serde_json::to_string(&insight).expect("serialize");
    let back: InsightDto = serde_json::from_str(&json).expect("deserialize");
    assert_eq!(back, insight, "the InsightDto round-trips losslessly");
}
