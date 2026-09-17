//! `data::ai`: the shared assistant panel (feed + chat + status).

use super::support::{assert_maps_except, fresh_db};
use crate::data::ai::{get_ai_panel, AiChatMsgDto, AiStatusDto};

#[tokio::test]
async fn get_ai_panel_serves_the_seeded_feed_chat_and_status() {
    let p = get_ai_panel().await.expect("panel");

    let kinds: Vec<&str> = p.feed.iter().map(|f| f.kind.as_str()).collect();
    assert_eq!(kinds, ["categorize", "detect", "reprocess"]);
    assert_eq!(p.feed[1].conf, Some(0.72));
    assert!(p.feed[1].cand);
    assert_eq!(p.feed[1].actions, ["CONFIRM", "DISMISS"]);
    assert_eq!(p.feed[2].state.as_deref(), Some("running"));
    assert!(p
        .feed
        .iter()
        .all(|f| !f.id.is_empty() && !f.time.is_empty()));

    assert_eq!(
        p.msgs,
        [
            AiChatMsgDto {
                who: "usr".into(),
                text: "How am I doing this cycle?".into(),
            },
            AiChatMsgDto {
                who: "sys".into(),
                text: "You're at 78% of your going-out cap with 12 days left.".into(),
            },
        ]
    );
    assert_eq!(
        p.status,
        AiStatusDto {
            online: true,
            model: "GEMMA4".into(),
            engine: "OLLAMA".into(),
            location: "LOCAL".into(),
        }
    );
}

#[tokio::test]
async fn get_ai_panel_mirrors_the_backend_panel() {
    let wire = get_ai_panel().await.expect("panel");
    let backend = phosk_ai::ai_spine::ai_panel(&fresh_db())
        .await
        .expect("backend");
    // Feed ids are minted per store, so two stores never share them.
    assert_maps_except(&wire, &backend, "id");
}
