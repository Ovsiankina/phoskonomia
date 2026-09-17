//! AI: activity feed, chat transcript, suggestion queue.

use phosk_adapter_db::DatabaseAdapter;
use phosk_core::money::Money;
use phosk_id::{ChatId, FeedItemId, MessageId, SuggestionId};
use phosk_model::{AiSuggestion, Chat, Message};

use crate::support::{Failure, Outcome, date, ensure, ensure_eq, ensure_not_found};

/// The seeded (latest) chat.
async fn seeded_chat(db: &dyn DatabaseAdapter) -> Result<Chat, Failure> {
    Ok(db.latest_chat().await?.ok_or("the seed has a chat")?)
}

/// A feed item is dismissed by its stringified id, and only once.
pub async fn dismiss_feed_item_removes_it(db: &dyn DatabaseAdapter) -> Outcome {
    let feed = db.feed_items().await?;
    ensure_eq(&feed.len(), &3, "feed item count")?;
    let (target, rest) = feed.split_first().ok_or("no feed items")?;
    let key = target.id.to_string();

    db.dismiss_feed_item(&key).await?;
    let after = db.feed_items().await?;
    ensure_eq(&after.len(), &rest.len(), "count after dismiss")?;
    let kept = rest.iter().all(|f| after.contains(f));
    ensure(kept, "other feed items untouched")?;
    ensure_not_found(db.dismiss_feed_item(&key).await, "dismiss again")?;
    let unknown = FeedItemId::new().to_string();
    ensure_not_found(db.dismiss_feed_item(&unknown).await, "dismiss unknown")?;
    ensure_not_found(db.dismiss_feed_item("not-an-id").await, "dismiss garbage")
}

/// The seeded chat is the latest one and holds the seeded transcript; an
/// unknown chat has no messages.
pub async fn latest_chat_holds_the_seeded_transcript(db: &dyn DatabaseAdapter) -> Outcome {
    let chat = seeded_chat(db).await?;
    ensure_eq(&chat.started, &date(2026, 6, 18)?, "chat start")?;
    let msgs = db.chat_messages(chat.id).await?;
    ensure_eq(&msgs.len(), &2, "seeded message count")?;
    let scoped = msgs.iter().all(|m| m.chat_id == chat.id);
    ensure(scoped, "messages carry their chat id")?;
    let mut who: Vec<&str> = msgs.iter().map(|m| m.who.as_str()).collect();
    who.sort_unstable();
    ensure_eq(
        &who,
        &vec!["sys", "usr"],
        "one user and one assistant message",
    )?;
    let none = db.chat_messages(ChatId::new()).await?;
    ensure(none.is_empty(), "unknown chat has no messages")
}

/// Appended messages are stored verbatim and read back oldest→newest
/// (they fall on distinct days; see the crate docs on same-day order).
pub async fn append_message_reads_back_oldest_first(db: &dyn DatabaseAdapter) -> Outcome {
    let chat = seeded_chat(db).await?;
    let seeded = db.chat_messages(chat.id).await?;
    let mut appended = Vec::new();
    for (day, who) in [(19, "usr"), (20, "sys"), (21, "usr")] {
        let m = Message {
            id: MessageId::new(),
            chat_id: chat.id,
            who: who.to_owned(),
            text: format!("conformance message {day}"),
            at: date(2026, 6, day)?,
        };
        db.append_message(m.clone()).await?;
        appended.push(m);
    }
    let msgs = db.chat_messages(chat.id).await?;
    ensure_eq(&msgs.len(), &(seeded.len() + 3), "count after append")?;
    ensure(
        seeded.iter().all(|m| msgs.contains(m)),
        "seeded messages kept",
    )?;
    let tail = msgs.get(seeded.len()..).unwrap_or_default();
    ensure_eq(&tail, &appended.as_slice(), "appended, oldest→newest, last")
}

/// Clearing a chat empties its transcript only; the chat itself stays.
pub async fn clear_chat_empties_only_that_chat(db: &dyn DatabaseAdapter) -> Outcome {
    let chat = seeded_chat(db).await?;
    let seeded = db.chat_messages(chat.id).await?;
    db.clear_chat(ChatId::new()).await?;
    let kept = db.chat_messages(chat.id).await?;
    ensure_eq(
        &kept,
        &seeded,
        "clearing another chat keeps this transcript",
    )?;
    db.clear_chat(chat.id).await?;
    let cleared = db.chat_messages(chat.id).await?;
    ensure(cleared.is_empty(), "transcript cleared")?;
    ensure_eq(
        &db.latest_chat().await?,
        &Some(chat),
        "the chat itself remains",
    )
}

/// A proposal is appended to the queue verbatim and its id returned.
pub async fn enqueue_suggestion_appends_it(db: &dyn DatabaseAdapter) -> Outcome {
    let before = db.ai_suggestions().await?;
    ensure_eq(&before.len(), &2, "seeded suggestion count")?;
    let s = AiSuggestion {
        id: SuggestionId::new(),
        kind: "cap".to_owned(),
        text: "Conformance proposal".to_owned(),
        confidence: 0.5,
        target: Some("groceries".to_owned()),
        estimated_savings: Some(Money::from_centimes(1_500)),
        status: "open".to_owned(),
    };
    let id = db.enqueue_suggestion(s.clone()).await?;
    ensure_eq(&id, &s.id, "enqueue returns the id")?;
    let after = db.ai_suggestions().await?;
    ensure_eq(&after.len(), &(before.len() + 1), "count after enqueue")?;
    ensure(after.contains(&s), "the proposal is listed verbatim")?;
    let kept = before.iter().all(|b| after.contains(b));
    ensure(kept, "existing suggestions kept")
}

/// A status update changes only its target suggestion's status.
pub async fn update_suggestion_status_changes_only_the_target(db: &dyn DatabaseAdapter) -> Outcome {
    let before = db.ai_suggestions().await?;
    let (target, rest) = before.split_first().ok_or("no suggestions")?;
    db.update_suggestion_status(target.id, "accepted").await?;
    let after = db.ai_suggestions().await?;
    ensure_eq(
        &after.len(),
        &before.len(),
        "no suggestion added or removed",
    )?;
    let want = AiSuggestion {
        status: "accepted".to_owned(),
        ..target.clone()
    };
    ensure(after.contains(&want), "target status updated")?;
    let kept = rest.iter().all(|r| after.contains(r));
    ensure(kept, "other suggestions untouched")?;
    let unknown = db.update_suggestion_status(SuggestionId::new(), "dismissed");
    ensure_not_found(unknown.await, "update_suggestion_status(unknown)")
}
