#![allow(
    // Test-only: the workspace denies these in production, but `clippy.toml`'s
    // allow-in-tests only covers `#[test]` bodies, not integration-test helpers
    // or module docs, so the exemption is made explicit crate-wide (mirrors
    // `tests/personal_ious.rs`).
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::doc_markdown,
    clippy::float_cmp
)]
//! Integration tests for the personal-IOU WRITE path
//! (`phosk_debts::iou_write`): create · edit · delete · record payment · settle.
//!
//! They drive the service through `phosk_db_memory::MemoryDb::seeded` (the
//! deterministic Swiss seed: four IOUs, `i1..i4`) and assert the observable
//! effect on the read side (`personal_ious::list_personal_ious` / `iou_stats`),
//! never on internals.
//!
//! Seed baseline: owedToYou = 16500, youOwe = 26000, net = −9500, 2 in / 2 out.

use chrono::NaiveDate;
use phosk_adapter_db::DatabaseAdapter;
use phosk_core::error::PhoskError;
use phosk_core::money::Money;
use phosk_db_memory::MemoryDb;
use phosk_debts::iou_write::{
    NewPersonalIou, PersonalIouEdit, create_personal_iou, delete_personal_iou, edit_personal_iou,
    record_iou_payment, settle_personal_iou,
};
use phosk_debts::personal_ious::{PersonalIouDto, iou_stats, list_personal_ious};

// ── helpers ───────────────────────────────────────────────────────────────────

/// A seeded in-memory adapter.
fn db() -> MemoryDb {
    MemoryDb::seeded().expect("seed is valid")
}

/// A date, or a panic naming it (test-only).
const fn day(y: i32, m: u32, d: u32) -> NaiveDate {
    NaiveDate::from_ymd_opt(y, m, d).expect("valid test date")
}

/// A valid inbound IOU: Robin owes you CHF 80.00 for a shared taxi.
fn robin() -> NewPersonalIou {
    NewPersonalIou {
        dir: "in".to_owned(),
        person: "Robin Tell".to_owned(),
        initials: String::new(),
        amount: Money::from_centimes(8_000),
        reason: "Shared taxi".to_owned(),
        since: day(2026, 6, 2),
    }
}

/// The IOU with `slug`, or `None`.
async fn find(db: &dyn DatabaseAdapter, slug: &str) -> Option<PersonalIouDto> {
    list_personal_ious(db)
        .await
        .expect("list")
        .into_iter()
        .find(|i| i.id == slug)
}

/// The IOU with `slug`; panics when it is gone (test-only).
async fn get(db: &dyn DatabaseAdapter, slug: &str) -> PersonalIouDto {
    find(db, slug).await.expect("the IOU is listed")
}

// ── create ────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn create_adds_an_iou_outstanding_in_full() {
    let db = db();
    let slug = create_personal_iou(&db, robin()).await.expect("create");

    let iou = get(&db, &slug).await;
    assert_eq!(iou.dir, "in");
    assert_eq!(iou.person, "Robin Tell");
    // A fresh IOU is outstanding in full: nothing repaid yet.
    assert_eq!(iou.amount, Money::from_centimes(8_000));
    assert_eq!(iou.of, Money::from_centimes(8_000));
    assert_eq!(iou.repaid_pct, 0.0);
    assert_eq!(iou.reason, "Shared taxi");
    assert_eq!(iou.since, "02 JUN 2026");
    // Blank initials are derived from the person's name.
    assert_eq!(iou.initials, "RT");

    assert_eq!(list_personal_ious(&db).await.expect("list").len(), 5);
}

#[tokio::test]
async fn create_counts_towards_the_matching_side_of_the_net_position() {
    let db = db();
    create_personal_iou(&db, robin()).await.expect("create in");
    create_personal_iou(
        &db,
        NewPersonalIou {
            dir: "out".to_owned(),
            person: "Nadia".to_owned(),
            amount: Money::from_centimes(2_500),
            reason: "Cinema".to_owned(),
            ..robin()
        },
    )
    .await
    .expect("create out");

    let stats = iou_stats(&db).await.expect("stats");
    assert_eq!(stats.owed_to_you, Money::from_centimes(16_500 + 8_000));
    assert_eq!(stats.you_owe, Money::from_centimes(26_000 + 2_500));
    assert_eq!(stats.net, Money::from_centimes(24_500 - 28_500));
    assert_eq!(stats.count_in, 3);
    assert_eq!(stats.count_out, 3);
}

#[tokio::test]
async fn create_gives_a_second_iou_with_the_same_person_its_own_slug() {
    let db = db();
    let first = create_personal_iou(&db, robin()).await.expect("first");
    let second = create_personal_iou(
        &db,
        NewPersonalIou {
            reason: "Concert".to_owned(),
            ..robin()
        },
    )
    .await
    .expect("second");

    assert_eq!(first, "robin-tell");
    assert_ne!(first, second, "the same person may owe twice");
    assert_eq!(get(&db, &second).await.reason, "Concert");
    assert_eq!(get(&db, &first).await.reason, "Shared taxi");
}

#[tokio::test]
async fn create_rejects_unusable_input() {
    let db = db();
    let cases = [
        (
            "blank person",
            NewPersonalIou {
                person: "   ".to_owned(),
                ..robin()
            },
        ),
        (
            "person with no usable slug",
            NewPersonalIou {
                person: "***".to_owned(),
                ..robin()
            },
        ),
        (
            "unknown direction",
            NewPersonalIou {
                dir: "sideways".to_owned(),
                ..robin()
            },
        ),
        (
            "zero amount",
            NewPersonalIou {
                amount: Money::ZERO,
                ..robin()
            },
        ),
        (
            "negative amount",
            NewPersonalIou {
                amount: Money::from_centimes(-100),
                ..robin()
            },
        ),
    ];
    for (what, input) in cases {
        assert!(
            matches!(
                create_personal_iou(&db, input).await,
                Err(PhoskError::Invalid(_))
            ),
            "{what} must be rejected"
        );
    }
    // A rejected create leaves the store untouched.
    assert_eq!(list_personal_ious(&db).await.expect("list").len(), 4);
}

#[tokio::test]
async fn create_accepts_a_direction_in_any_case() {
    let db = db();
    let slug = create_personal_iou(
        &db,
        NewPersonalIou {
            dir: " OUT ".to_owned(),
            ..robin()
        },
    )
    .await
    .expect("create");
    assert_eq!(get(&db, &slug).await.dir, "out");
}

// ── edit ──────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn edit_changes_only_the_supplied_fields() {
    let db = db();
    edit_personal_iou(
        &db,
        "i2",
        PersonalIouEdit {
            person: Some("Marco Bellini".to_owned()),
            reason: Some("Split ski cabin + lift pass".to_owned()),
            ..PersonalIouEdit::default()
        },
    )
    .await
    .expect("edit");

    let iou = get(&db, "i2").await;
    assert_eq!(iou.person, "Marco Bellini");
    assert_eq!(iou.reason, "Split ski cabin + lift pass");
    // Untouched fields keep their seeded values, slug included.
    assert_eq!(iou.id, "i2");
    assert_eq!(iou.dir, "in");
    assert_eq!(iou.amount, Money::from_centimes(4_500));
    assert_eq!(iou.of, Money::from_centimes(9_000));
    assert_eq!(iou.since, "01 FEB 2026");
}

#[tokio::test]
async fn editing_the_original_amount_keeps_what_was_already_repaid() {
    let db = db();
    // i2: of 9000, outstanding 4500 → 4500 already repaid. Correcting the
    // original to 10000 leaves the repaid 4500 a fact: 5500 is still owed.
    edit_personal_iou(
        &db,
        "i2",
        PersonalIouEdit {
            of: Some(Money::from_centimes(10_000)),
            ..PersonalIouEdit::default()
        },
    )
    .await
    .expect("edit");

    let iou = get(&db, "i2").await;
    assert_eq!(iou.of, Money::from_centimes(10_000));
    assert_eq!(iou.amount, Money::from_centimes(5_500));
    assert_eq!(iou.repaid_pct, 0.45);
}

#[tokio::test]
async fn edit_rejects_an_original_below_what_was_already_repaid() {
    let db = db();
    // i2 has 4500 repaid; an original of 4000 would mean a negative balance.
    let err = edit_personal_iou(
        &db,
        "i2",
        PersonalIouEdit {
            of: Some(Money::from_centimes(4_000)),
            ..PersonalIouEdit::default()
        },
    )
    .await;
    assert!(matches!(err, Err(PhoskError::Invalid(_))));

    let iou = get(&db, "i2").await;
    assert_eq!(iou.of, Money::from_centimes(9_000));
    assert_eq!(iou.amount, Money::from_centimes(4_500));
}

#[tokio::test]
async fn edit_rejects_an_unknown_direction_and_a_blank_person() {
    let db = db();
    for edit in [
        PersonalIouEdit {
            dir: Some("both".to_owned()),
            ..PersonalIouEdit::default()
        },
        PersonalIouEdit {
            person: Some("  ".to_owned()),
            ..PersonalIouEdit::default()
        },
    ] {
        assert!(matches!(
            edit_personal_iou(&db, "i1", edit).await,
            Err(PhoskError::Invalid(_))
        ));
    }
    assert_eq!(get(&db, "i1").await.person, "Léa");
    assert_eq!(get(&db, "i1").await.dir, "in");
}

#[tokio::test]
async fn edit_flips_the_direction_and_the_net_position_follows() {
    let db = db();
    // i1 (in, 12000) becomes an "out": owedToYou loses it, youOwe gains it.
    edit_personal_iou(
        &db,
        "i1",
        PersonalIouEdit {
            dir: Some("out".to_owned()),
            ..PersonalIouEdit::default()
        },
    )
    .await
    .expect("edit");

    let stats = iou_stats(&db).await.expect("stats");
    assert_eq!(stats.owed_to_you, Money::from_centimes(4_500));
    assert_eq!(stats.you_owe, Money::from_centimes(38_000));
    assert_eq!(stats.net, Money::from_centimes(-33_500));
    assert_eq!(stats.count_in, 1);
    assert_eq!(stats.count_out, 3);
}

#[tokio::test]
async fn edit_reports_an_unknown_slug() {
    let db = db();
    let err = edit_personal_iou(&db, "nope", PersonalIouEdit::default()).await;
    assert!(matches!(err, Err(PhoskError::NotFound(_))));
}

// ── delete ────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn delete_removes_the_iou_from_the_list_and_the_stats() {
    let db = db();
    delete_personal_iou(&db, "i4").await.expect("delete");

    assert!(find(&db, "i4").await.is_none());
    assert_eq!(list_personal_ious(&db).await.expect("list").len(), 3);

    let stats = iou_stats(&db).await.expect("stats");
    assert_eq!(stats.you_owe, Money::from_centimes(6_000));
    assert_eq!(stats.owed_to_you, Money::from_centimes(16_500));
    assert_eq!(stats.net, Money::from_centimes(10_500));
    assert_eq!(stats.count_out, 1);
}

#[tokio::test]
async fn delete_reports_an_unknown_slug_and_cannot_run_twice() {
    let db = db();
    assert!(matches!(
        delete_personal_iou(&db, "nope").await,
        Err(PhoskError::NotFound(_))
    ));
    delete_personal_iou(&db, "i3").await.expect("delete");
    assert!(matches!(
        delete_personal_iou(&db, "i3").await,
        Err(PhoskError::NotFound(_))
    ));
}

// ── record payment ────────────────────────────────────────────────────────────

#[tokio::test]
async fn a_partial_payment_reduces_the_outstanding_amount_only() {
    let db = db();
    // i4: you owe 20000 of an original 50000. Pay back 7500.
    let left = record_iou_payment(&db, "i4", Money::from_centimes(7_500))
        .await
        .expect("payment");
    assert_eq!(left, Money::from_centimes(12_500));

    let iou = get(&db, "i4").await;
    assert_eq!(iou.amount, Money::from_centimes(12_500));
    assert_eq!(
        iou.of,
        Money::from_centimes(50_000),
        "the original is fixed"
    );
    assert_eq!(iou.repaid_pct, 0.75);

    let stats = iou_stats(&db).await.expect("stats");
    assert_eq!(stats.you_owe, Money::from_centimes(18_500));
    assert_eq!(stats.count_out, 2, "a partial payment keeps the IOU open");
}

#[tokio::test]
async fn paying_the_whole_outstanding_amount_settles_the_iou() {
    let db = db();
    let left = record_iou_payment(&db, "i2", Money::from_centimes(4_500))
        .await
        .expect("payment");
    assert_eq!(left, Money::ZERO);

    let iou = get(&db, "i2").await;
    assert_eq!(iou.amount, Money::ZERO);
    assert_eq!(iou.repaid_pct, 1.0);
    assert_eq!(
        iou_stats(&db).await.expect("stats").owed_to_you,
        Money::from_centimes(12_000)
    );
}

#[tokio::test]
async fn a_payment_beyond_the_outstanding_amount_is_rejected() {
    let db = db();
    let err = record_iou_payment(&db, "i2", Money::from_centimes(4_501)).await;
    assert!(matches!(err, Err(PhoskError::Invalid(_))));
    assert_eq!(get(&db, "i2").await.amount, Money::from_centimes(4_500));
}

#[tokio::test]
async fn a_non_positive_payment_is_rejected() {
    let db = db();
    for cents in [0, -1] {
        let err = record_iou_payment(&db, "i2", Money::from_centimes(cents)).await;
        assert!(
            matches!(err, Err(PhoskError::Invalid(_))),
            "{cents} centimes"
        );
    }
    assert_eq!(get(&db, "i2").await.amount, Money::from_centimes(4_500));
}

#[tokio::test]
async fn a_payment_against_an_unknown_slug_reports_not_found() {
    let db = db();
    let err = record_iou_payment(&db, "nope", Money::from_centimes(100)).await;
    assert!(matches!(err, Err(PhoskError::NotFound(_))));
}

// ── settle ────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn settle_clears_the_remaining_balance_and_reports_what_it_cleared() {
    let db = db();
    let cleared = settle_personal_iou(&db, "i4").await.expect("settle");
    assert_eq!(cleared, Money::from_centimes(20_000));

    let iou = get(&db, "i4").await;
    assert_eq!(iou.amount, Money::ZERO);
    assert_eq!(
        iou.of,
        Money::from_centimes(50_000),
        "the original is fixed"
    );
    assert_eq!(iou.repaid_pct, 1.0);
    assert_eq!(
        iou_stats(&db).await.expect("stats").you_owe,
        Money::from_centimes(6_000)
    );
}

#[tokio::test]
async fn settling_a_settled_iou_clears_nothing_more() {
    let db = db();
    settle_personal_iou(&db, "i1").await.expect("first settle");
    let again = settle_personal_iou(&db, "i1").await.expect("second settle");
    assert_eq!(again, Money::ZERO);
    assert_eq!(get(&db, "i1").await.amount, Money::ZERO);
}

#[tokio::test]
async fn settle_reports_an_unknown_slug() {
    let db = db();
    let err = settle_personal_iou(&db, "nope").await;
    assert!(matches!(err, Err(PhoskError::NotFound(_))));
}
