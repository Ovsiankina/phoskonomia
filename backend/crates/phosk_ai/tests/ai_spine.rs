//! Integration tests for `phosk_ai` (ai_spine + ai_features).
//!
//! These tests pin the AI panel read-model (feed + chat + status), the chat
//! spine (send/clear), the feed-dismiss write, and the dashboard insight against
//! the deterministic Swiss seed in `phosk_db_memory::MemoryDb::seeded()` at the
//! demo clock `as_of = 2026-06-18`.
//!
//! Each `#[tokio::test]` drives the implemented service, which composes the
//! DTOs from the PORT and is asserted against the seed.
//!
//! Spec sources (field shapes & expected values):
//!   - frontend/dioxus-app/src/data/ai.rs        (AiFeedItemDto/AiChatMsgDto/AiStatusDto/AiPanelDto)
//!   - frontend/dioxus-app/src/data/dashboard.rs (InsightDto: model/text/estimatedSavings)
//!   - backend/crates/phosk_db_memory/src/seed.rs (seed_feed_items / seed_chats_and_messages)
//!   - backend/documentation/build-contract.md §5.7
//!
//! NOTE (signature gap, for the GREEN phase): the build-contract task scope also
//! demands a `LlmAdapter` port (constrained/structured generation) + a FAKE
//! adapter, a tool registry/manifest with an effect-typed gate (read=execute,
//! write=enqueue-approval, never direct write), and a per-receipt approval queue
//! (bulk approve) + audit log. NONE of those types/fns exist in the `phosk_ai`
//! skeleton today (only the 5 service fns + 5 DTOs). The "write enqueues, never
//! writes" invariant is therefore tested indirectly here via `dismiss_feed_item`
//! / `send_message` against the PORT only. The structural LlmAdapter/tool-registry
//! tests cannot be written until those public items are added to the skeleton —
//! flagged in the agent return.

// Test-only: the workspace denies `panic!`/`expect`/`unwrap` in production code,
// but integration tests assert failure paths with `panic!`/`expect` (per the
// build-contract: "Tests MAY use expect"). Relax the restriction + pedantic/nursery
// style lints for this test target only.
#![allow(
    clippy::panic,
    clippy::expect_used,
    clippy::doc_markdown,
    clippy::needless_collect,
    clippy::map_unwrap_or,
    clippy::missing_const_for_fn
)]

use phosk_ai::ai_features::{InsightDto, dashboard_insight, dismiss_feed_item};
use phosk_ai::ai_spine::{
    AiChatMsgDto, AiFeedItemDto, AiPanelDto, AiStatusDto, ai_panel, clear_chat, send_message,
};
use phosk_core::error::PhoskError;
use phosk_core::money::Money;
use phosk_db_memory::MemoryDb;

/// The demo clock the contract pins every derived field to (June cycle, day 18).
fn as_of() -> chrono::NaiveDate {
    chrono::NaiveDate::from_ymd_opt(2026, 6, 18).expect("valid demo date 2026-06-18")
}

fn seeded() -> MemoryDb {
    MemoryDb::seeded().expect("seed the deterministic Swiss MemoryDb")
}

// ─────────────────────────────────────────────────────────────────────────────
// ai_panel — composes feed + chat transcript + status from the PORT
// ─────────────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn ai_panel_returns_ok() {
    let db = seeded();
    let panel: AiPanelDto = ai_panel(&db)
        .await
        .expect("ai_panel composes from the port");
    // Smoke: the three sub-models are present.
    let _ = (&panel.feed, &panel.msgs, &panel.status);
}

#[tokio::test]
async fn ai_panel_status_is_seeded_gemma_on_ollama_local() {
    let db = seeded();
    let panel = ai_panel(&db).await.expect("ai_panel ok");
    let want = AiStatusDto {
        online: true,
        model: "GEMMA4".to_owned(),
        engine: "OLLAMA".to_owned(),
        location: "LOCAL".to_owned(),
    };
    assert_eq!(
        panel.status, want,
        "status line is the seeded GEMMA4/OLLAMA/LOCAL pulse"
    );
}

#[tokio::test]
async fn ai_panel_feed_has_three_seeded_items_in_order() {
    let db = seeded();
    let panel = ai_panel(&db).await.expect("ai_panel ok");
    assert_eq!(panel.feed.len(), 3, "seed has exactly 3 feed items");

    // Item order + every non-id field is pinned to seed_feed_items().
    let kinds: Vec<&str> = panel.feed.iter().map(|f| f.kind.as_str()).collect();
    assert_eq!(kinds, vec!["categorize", "detect", "reprocess"]);
}

#[tokio::test]
async fn ai_panel_feed_first_item_is_the_categorize_line() {
    let db = seeded();
    let panel = ai_panel(&db).await.expect("ai_panel ok");
    let f0 = &panel.feed[0];
    assert_eq!(f0.kind, "categorize");
    assert_eq!(f0.text, "Categorised 4 lines on the Migros receipt");
    assert_eq!(f0.conf, Some(0.93));
    assert_eq!(f0.state, None);
    assert_eq!(f0.actions, vec!["VIEW".to_owned()]);
    assert!(!f0.cand, "categorize line is not a candidate");
    assert!(
        !f0.id.is_empty(),
        "every feed item carries a stable id for dismiss"
    );
    assert!(
        !f0.time.is_empty(),
        "feed item carries a relative-time label"
    );
}

#[tokio::test]
async fn ai_panel_feed_second_item_is_the_low_conf_detect_candidate() {
    let db = seeded();
    let panel = ai_panel(&db).await.expect("ai_panel ok");
    let f1 = &panel.feed[1];
    assert_eq!(f1.kind, "detect");
    assert_eq!(f1.text, "Detected a possible recurring charge: iCloud+");
    assert_eq!(f1.conf, Some(0.72));
    assert_eq!(f1.actions, vec!["CONFIRM".to_owned(), "DISMISS".to_owned()]);
    assert!(
        f1.cand,
        "detect line is a tracking candidate (cand == true)"
    );
}

#[tokio::test]
async fn ai_panel_feed_low_confidence_item_is_below_threshold_but_present() {
    // The < 0.7 coral-flag rule: lines below 0.7 are flagged, NOT dropped from the
    // feed. The seed's lowest-confidence feed item is the iCloud+ detect at 0.72,
    // which is NOT below 0.7 — assert it survives and is the candidate. (A genuine
    // sub-0.7 line would still appear; the panel never filters by confidence.)
    let db = seeded();
    let panel = ai_panel(&db).await.expect("ai_panel ok");
    let flagged: Vec<&AiFeedItemDto> = panel
        .feed
        .iter()
        .filter(|f| f.conf.is_some_and(phosk_model::is_low_confidence))
        .collect();
    // No seeded item is < 0.7, but the running (conf == None) item must still be present.
    assert!(
        flagged.is_empty(),
        "no seeded feed item is below 0.7; the panel must not invent one"
    );
    assert!(
        panel
            .feed
            .iter()
            .any(|f| f.conf.is_none() && f.state.as_deref() == Some("running")),
        "the running re-process item (no confidence) is kept in the feed"
    );
}

#[tokio::test]
async fn ai_panel_feed_running_item_has_running_state_and_no_conf() {
    let db = seeded();
    let panel = ai_panel(&db).await.expect("ai_panel ok");
    let f2 = &panel.feed[2];
    assert_eq!(f2.kind, "reprocess");
    assert_eq!(f2.text, "Re-processing the latest import…");
    assert_eq!(f2.conf, None, "running item has no confidence");
    assert_eq!(f2.state.as_deref(), Some("running"));
    assert!(
        f2.actions.is_empty(),
        "running item exposes no action buttons"
    );
    assert!(!f2.cand);
}

#[tokio::test]
async fn ai_panel_chat_transcript_matches_seed_oldest_first() {
    let db = seeded();
    let panel = ai_panel(&db).await.expect("ai_panel ok");
    assert_eq!(panel.msgs.len(), 2, "seeded chat has 2 messages");
    assert_eq!(
        panel.msgs,
        vec![
            AiChatMsgDto {
                who: "usr".to_owned(),
                text: "How am I doing this cycle?".to_owned(),
            },
            AiChatMsgDto {
                who: "sys".to_owned(),
                text: "You're at 78% of your going-out cap with 12 days left.".to_owned(),
            },
        ],
        "transcript is oldest to newest, who is usr or sys"
    );
}

// ─────────────────────────────────────────────────────────────────────────────
// send_message — append a user line, return the model's (canned) reply
// ─────────────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn send_message_returns_a_sys_reply() {
    let db = seeded();
    let reply: AiChatMsgDto = send_message(&db, "how much on coffee?")
        .await
        .expect("send_message returns the model reply");
    assert_eq!(
        reply.who, "sys",
        "the returned message is the model's reply, not the echo"
    );
    assert!(!reply.text.is_empty(), "the canned reply is non-empty");
}

#[tokio::test]
async fn send_message_persists_user_then_reply_into_transcript() {
    let db = seeded();
    // Baseline transcript length from the seed.
    let before = ai_panel(&db).await.expect("panel before").msgs.len();
    assert_eq!(before, 2);

    let reply = send_message(&db, "anything I could cut?")
        .await
        .expect("send_message ok");

    let after = ai_panel(&db).await.expect("panel after").msgs;
    // The user line AND the sys reply are both appended (transcript grows by 2).
    assert_eq!(
        after.len(),
        before + 2,
        "user message + sys reply are both persisted"
    );

    let user_line = &after[after.len() - 2];
    let sys_line = &after[after.len() - 1];
    assert_eq!(user_line.who, "usr");
    assert_eq!(
        user_line.text, "anything I could cut?",
        "the user's text is stored verbatim"
    );
    assert_eq!(sys_line.who, "sys");
    assert_eq!(
        *sys_line, reply,
        "the persisted reply equals the value returned to the caller"
    );
}

#[tokio::test]
async fn send_message_empty_text_is_rejected_as_invalid() {
    let db = seeded();
    // An empty user message is not a valid chat turn — must map to PhoskError::Invalid,
    // never panic, and must NOT grow the transcript.
    let before = ai_panel(&db).await.expect("panel before").msgs.len();
    let res = send_message(&db, "").await;
    match res {
        Err(PhoskError::Invalid(_)) => {}
        Err(other) => panic!("expected PhoskError::Invalid for empty text, got {other:?}"),
        Ok(_) => panic!("empty chat text must be rejected, not accepted"),
    }
    let after = ai_panel(&db).await.expect("panel after").msgs.len();
    assert_eq!(after, before, "a rejected message must not be persisted");
}

// ─────────────────────────────────────────────────────────────────────────────
// clear_chat — empty the latest transcript
// ─────────────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn clear_chat_empties_the_transcript() {
    let db = seeded();
    assert_eq!(ai_panel(&db).await.expect("before").msgs.len(), 2);

    clear_chat(&db)
        .await
        .expect("clear_chat resolves the latest chat and clears it");

    let after = ai_panel(&db).await.expect("after");
    assert!(
        after.msgs.is_empty(),
        "the transcript is empty after clear_chat"
    );
    // Clearing the chat must not disturb the feed or status.
    assert_eq!(after.feed.len(), 3, "clearing chat leaves the feed intact");
    assert!(
        after.status.online,
        "clearing chat leaves the status pulse intact"
    );
}

#[tokio::test]
async fn clear_chat_is_idempotent() {
    let db = seeded();
    clear_chat(&db).await.expect("first clear ok");
    // A second clear on an already-empty transcript must still succeed (no panic).
    clear_chat(&db)
        .await
        .expect("second clear is a no-op, not an error");
    assert!(ai_panel(&db).await.expect("panel").msgs.is_empty());
}

// ─────────────────────────────────────────────────────────────────────────────
// dismiss_feed_item — a WRITE that mutates feed state via the PORT
// (the "write tool enqueues/marks, never silently corrupts" invariant)
// ─────────────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn dismiss_feed_item_removes_only_the_targeted_item() {
    let db = seeded();
    let panel = ai_panel(&db).await.expect("panel before");
    let target = panel.feed[1].id.clone(); // the iCloud+ detect candidate

    dismiss_feed_item(&db, &target)
        .await
        .expect("dismiss the targeted feed item");

    let after = ai_panel(&db).await.expect("panel after");
    assert!(
        after.feed.iter().all(|f| f.id != target),
        "the dismissed item no longer appears in the feed"
    );
    assert_eq!(after.feed.len(), 2, "exactly one item was dismissed");
    // The other two survive untouched.
    assert!(after.feed.iter().any(|f| f.kind == "categorize"));
    assert!(after.feed.iter().any(|f| f.kind == "reprocess"));
}

#[tokio::test]
async fn dismiss_feed_item_unknown_id_is_not_found() {
    let db = seeded();
    let res = dismiss_feed_item(&db, "no-such-feed-item").await;
    match res {
        Err(PhoskError::NotFound(_)) => {}
        Err(other) => panic!("expected PhoskError::NotFound for unknown id, got {other:?}"),
        Ok(()) => panic!("dismissing an unknown feed item must be NotFound, not Ok"),
    }
}

#[tokio::test]
async fn dismiss_feed_item_empty_id_is_not_found() {
    let db = seeded();
    // Defensive: an empty id resolves to nothing → NotFound (never a panic, never a
    // silent success that would mask a frontend bug).
    let res = dismiss_feed_item(&db, "").await;
    assert!(
        matches!(res, Err(PhoskError::NotFound(_))),
        "empty feed id must be NotFound, got {res:?}"
    );
}

// ─────────────────────────────────────────────────────────────────────────────
// dashboard_insight — GEMMA4 one-liner + estimatedSavings in EXACT centimes
// ─────────────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn dashboard_insight_model_is_gemma4() {
    let db = seeded();
    let ins: InsightDto = dashboard_insight(&db, as_of())
        .await
        .expect("dashboard_insight composes the GEMMA4 line");
    assert_eq!(
        ins.model, "GEMMA4",
        "the insight badge is the local model name"
    );
    assert!(
        !ins.text.is_empty(),
        "the insight carries a non-empty sentence"
    );
}

#[tokio::test]
async fn dashboard_insight_estimated_savings_is_exact_centimes() {
    let db = seeded();
    let ins = dashboard_insight(&db, as_of()).await.expect("insight ok");
    // The dashboard spec (dashboard.rs::get_insight) pins the saving at CHF 42.00.
    // Money is i64 CENTIMES — the wire form is exact, never a CHF float.
    assert_eq!(
        ins.estimated_savings,
        Money::from_centimes(4_200),
        "estimatedSavings is exactly 4200 centimes (CHF 42.00)"
    );
}

#[tokio::test]
async fn dashboard_insight_text_matches_the_coffee_cap_line() {
    let db = seeded();
    let ins = dashboard_insight(&db, as_of()).await.expect("insight ok");
    assert_eq!(
        ins.text, "Coffee runs are up 28% this cycle. Capping them at CHF 70 keeps you on budget",
        "the seeded dashboard insight sentence is pinned to the wire spec"
    );
}

#[tokio::test]
async fn dashboard_insight_serializes_savings_as_camelcase_i64_centimes() {
    let db = seeded();
    let ins = dashboard_insight(&db, as_of()).await.expect("insight ok");
    let v = serde_json::to_value(&ins).expect("InsightDto serializes");
    // camelCase key + exact integer centimes (NOT a CHF float like 42.0).
    assert_eq!(
        v.get("estimatedSavings"),
        Some(&serde_json::json!(4_200)),
        "money serializes as exact i64 centimes under the camelCase key"
    );
    assert_eq!(v.get("model"), Some(&serde_json::json!("GEMMA4")));
    assert!(v.get("text").is_some(), "text key is present");
    // Defensive: the float form must NOT leak onto the wire.
    assert_ne!(
        v.get("estimatedSavings"),
        Some(&serde_json::json!(42.0)),
        "CHF float form must never appear on the wire"
    );
}

#[tokio::test]
async fn dashboard_insight_roundtrips_through_json() {
    let db = seeded();
    let ins = dashboard_insight(&db, as_of()).await.expect("insight ok");
    let json = serde_json::to_string(&ins).expect("serialize");
    let back: InsightDto = serde_json::from_str(&json).expect("deserialize");
    assert_eq!(back, ins, "InsightDto round-trips losslessly via centimes");
}

// ─────────────────────────────────────────────────────────────────────────────
// DTO serialization shape guards (compile + assert the camelCase contract)
// ─────────────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn ai_panel_serializes_with_camelcase_keys() {
    let db = seeded();
    let panel = ai_panel(&db).await.expect("panel ok");
    let v = serde_json::to_value(&panel).expect("AiPanelDto serializes");
    assert!(v.get("feed").is_some(), "panel has a feed array");
    assert!(v.get("msgs").is_some(), "panel has a msgs array");
    assert!(v.get("status").is_some(), "panel has a status object");

    let f0 = &v["feed"][0];
    // camelCase / contract keys on a feed item.
    for key in [
        "id", "kind", "text", "conf", "state", "time", "actions", "cand",
    ] {
        assert!(f0.get(key).is_some(), "feed item exposes the `{key}` key");
    }
    let s = &v["status"];
    for key in ["online", "model", "engine", "location"] {
        assert!(s.get(key).is_some(), "status exposes the `{key}` key");
    }
}
