//! Integration tests for `phosk_ai` (ai_spine + ai_features).
//!
//! These tests pin the AI panel read-model (feed + chat + status from the LLM
//! port), the chat clear, the feed-dismiss write, and the dashboard insight against
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
//! against the PORT only. The structural LlmAdapter/tool-registry
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

use phosk_adapter_llm::FakeLlm;
use phosk_ai::ai_features::{COMPUTED_SOURCE, InsightDto, dashboard_insight, dismiss_feed_item};
use phosk_ai::ai_spine::{
    AiChatMsgDto, AiFeedItemDto, AiPanelDto, AiStatusDto, ai_panel, ai_status, clear_chat,
};
use phosk_core::error::PhoskError;
use phosk_core::money::Money;
use phosk_db_memory::MemoryDb;
use phosk_model::BudgetConfig;

/// A healthy fake model under a recognisable id.
fn llm() -> FakeLlm {
    FakeLlm::with_model("test-model:7b")
}

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
    let panel: AiPanelDto = ai_panel(&db, &llm(), "OLLAMA")
        .await
        .expect("ai_panel composes from the port");
    // Smoke: the three sub-models are present.
    let _ = (&panel.feed, &panel.msgs, &panel.status);
}

#[tokio::test]
async fn ai_panel_status_is_the_real_model_and_its_health() {
    let db = seeded();
    let panel = ai_panel(&db, &llm(), "OLLAMA").await.expect("ai_panel ok");
    let want = AiStatusDto {
        online: true,
        model: "test-model:7b".to_owned(),
        engine: "OLLAMA".to_owned(),
        location: "LOCAL".to_owned(),
    };
    assert_eq!(
        panel.status, want,
        "the status line names the adapter's model and its health"
    );
}

#[tokio::test]
async fn ai_status_is_offline_when_the_model_server_is_down() {
    let status = ai_status(&llm().reachable(false), "OLLAMA").await;
    assert!(!status.online, "an unreachable server is offline");
    assert_eq!(
        status.model, "test-model:7b",
        "the label is still the real id"
    );
}

#[tokio::test]
async fn ai_status_is_offline_when_the_model_is_not_installed() {
    let status = ai_status(&llm().healthy(false), "OLLAMA").await;
    assert!(
        !status.online,
        "a reachable server without the model is offline"
    );
}

#[tokio::test]
async fn a_down_model_does_not_fail_the_panel_read() {
    let db = seeded();
    let panel = ai_panel(&db, &llm().reachable(false), "OLLAMA")
        .await
        .expect("the feed and chat still load");
    assert!(!panel.status.online);
    assert_eq!(panel.feed.len(), 3);
}

#[tokio::test]
async fn ai_panel_feed_has_three_seeded_items_in_order() {
    let db = seeded();
    let panel = ai_panel(&db, &llm(), "OLLAMA").await.expect("ai_panel ok");
    assert_eq!(panel.feed.len(), 3, "seed has exactly 3 feed items");

    // Item order + every non-id field is pinned to seed_feed_items().
    let kinds: Vec<&str> = panel.feed.iter().map(|f| f.kind.as_str()).collect();
    assert_eq!(kinds, vec!["categorize", "detect", "reprocess"]);
}

#[tokio::test]
async fn ai_panel_feed_first_item_is_the_categorize_line() {
    let db = seeded();
    let panel = ai_panel(&db, &llm(), "OLLAMA").await.expect("ai_panel ok");
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
    let panel = ai_panel(&db, &llm(), "OLLAMA").await.expect("ai_panel ok");
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
    let panel = ai_panel(&db, &llm(), "OLLAMA").await.expect("ai_panel ok");
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
    let panel = ai_panel(&db, &llm(), "OLLAMA").await.expect("ai_panel ok");
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
    let panel = ai_panel(&db, &llm(), "OLLAMA").await.expect("ai_panel ok");
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
// clear_chat — empty the latest transcript
// ─────────────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn clear_chat_empties_the_transcript() {
    let db = seeded();
    assert_eq!(
        ai_panel(&db, &llm(), "OLLAMA")
            .await
            .expect("before")
            .msgs
            .len(),
        2
    );

    clear_chat(&db)
        .await
        .expect("clear_chat resolves the latest chat and clears it");

    let after = ai_panel(&db, &llm(), "OLLAMA").await.expect("after");
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
    assert!(
        ai_panel(&db, &llm(), "OLLAMA")
            .await
            .expect("panel")
            .msgs
            .is_empty()
    );
}

// ─────────────────────────────────────────────────────────────────────────────
// dismiss_feed_item — a WRITE that mutates feed state via the PORT
// (the "write tool enqueues/marks, never silently corrupts" invariant)
// ─────────────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn dismiss_feed_item_removes_only_the_targeted_item() {
    let db = seeded();
    let panel = ai_panel(&db, &llm(), "OLLAMA").await.expect("panel before");
    let target = panel.feed[1].id.clone(); // the iCloud+ detect candidate

    dismiss_feed_item(&db, &target)
        .await
        .expect("dismiss the targeted feed item");

    let after = ai_panel(&db, &llm(), "OLLAMA").await.expect("panel after");
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
// dashboard_insight — computed from the data, labelled as computed
// ─────────────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn dashboard_insight_is_labelled_computed_not_a_model() {
    let db = seeded();
    let ins: InsightDto = dashboard_insight(&db, as_of())
        .await
        .expect("dashboard_insight composes from the data");
    assert_eq!(ins.source, COMPUTED_SOURCE, "no model wrote this line");
    assert!(!ins.text.is_empty(), "the insight carries a sentence");
    assert!(
        !ins.text.contains("Coffee runs are up 28%"),
        "the canned sentence is gone"
    );
    assert!(
        ins.estimated_savings >= Money::ZERO,
        "a saving is never negative"
    );
}

#[tokio::test]
async fn dashboard_insight_says_so_when_there_is_no_spend() {
    let empty = MemoryDb::new(
        Vec::new(),
        Vec::new(),
        BudgetConfig {
            monthly_budget: Money::ZERO,
            savings_target: Money::ZERO,
        },
    );
    let ins = dashboard_insight(&empty, as_of())
        .await
        .expect("insight ok");
    assert_eq!(ins.source, COMPUTED_SOURCE);
    assert!(
        ins.text.starts_with("No spending recorded this cycle"),
        "{}",
        ins.text
    );
    assert_eq!(ins.estimated_savings, Money::ZERO);
}

#[tokio::test]
async fn dashboard_insight_serializes_savings_as_camelcase_i64_centimes() {
    let db = seeded();
    let ins = dashboard_insight(&db, as_of()).await.expect("insight ok");
    let v = serde_json::to_value(&ins).expect("InsightDto serializes");
    // camelCase key + exact integer centimes (NOT a CHF float).
    assert_eq!(
        v.get("estimatedSavings"),
        Some(&serde_json::json!(ins.estimated_savings.centimes())),
        "money serializes as exact i64 centimes under the camelCase key"
    );
    assert!(v["estimatedSavings"].is_i64());
    assert_eq!(v.get("source"), Some(&serde_json::json!(COMPUTED_SOURCE)));
    assert!(
        v.get("model").is_none(),
        "no model badge on a computed line"
    );
    assert!(v.get("text").is_some(), "text key is present");
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
    let panel = ai_panel(&db, &llm(), "OLLAMA").await.expect("panel ok");
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
