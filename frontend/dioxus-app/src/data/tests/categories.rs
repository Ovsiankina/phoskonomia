//! `data::categories`: the `/categories` list, create / rename / delete /
//! merge through `phosk_ledger::categories`, and the token-only colour picker.
//! Writes run on a fresh store, never the global one.

use phosk_adapter_db::DatabaseAdapter;
use phosk_core::error::PhoskError;

use super::support::{fresh_db, server_error};
use crate::data::categories::{
    create_category_with, delete_category_with, list_categories, list_categories_with,
    merge_categories_with, merge_preview_with, rename_category_with, set_category_colour_with,
    store_error, with_colour_notice, CategoriesDto, CategoryRowDto, COLOUR_TOKENS, DEFAULT_COLOUR,
};

fn row<'a>(view: &'a CategoriesDto, name: &str) -> &'a CategoryRowDto {
    view.rows
        .iter()
        .find(|r| r.name == name)
        .unwrap_or_else(|| panic!("row `{name}` present"))
}

/// Line items stored under `category`, counted straight from the store.
async fn lines_in(db: &dyn DatabaseAdapter, category: &str) -> u32 {
    let mut n = 0;
    for r in db.all_receipts().await.expect("receipts") {
        let lines = db.line_items(r.id).await.expect("lines");
        n += u32::try_from(lines.iter().filter(|l| l.category == category).count()).expect("fits");
    }
    n
}

#[tokio::test]
async fn list_shows_every_stored_category_with_usage_and_the_palette() {
    let db = fresh_db();
    let view = list_categories_with(&db).await.expect("read");

    let stored = db.category_caps().await.expect("caps");
    let mut names: Vec<&str> = view.rows.iter().map(|r| r.name.as_str()).collect();
    let mut want: Vec<&str> = stored.iter().map(|c| c.name.as_str()).collect();
    names.sort_unstable();
    want.sort_unstable();
    assert_eq!(names, want, "one row per stored category");

    for r in &view.rows {
        let uses = phosk_ledger::categories::category_usage(&db, &r.name)
            .await
            .expect("usage");
        assert_eq!(
            r.uses, uses,
            "one-pass usage matches the ledger for {}",
            r.name
        );
    }
    assert_eq!(view.notice, None);

    let groceries = row(&view, "Groceries");
    assert_eq!(groceries.slug, "groceries");
    assert_eq!(groceries.glyph, "▤");
    assert_eq!(groceries.colour, DEFAULT_COLOUR, "no colour chosen yet");
    let uses = phosk_ledger::categories::category_usage(&db, "Groceries")
        .await
        .expect("usage");
    assert!(uses > 0, "the seed has groceries history");
    assert_eq!(groceries.uses, uses);

    assert_eq!(
        view.palette, COLOUR_TOKENS,
        "the picker offers the allow-list"
    );
    assert!(COLOUR_TOKENS.contains(&DEFAULT_COLOUR));
}

#[tokio::test]
async fn the_server_fn_reads_the_global_stack() {
    let view = list_categories().await.expect("read");
    assert!(view.rows.iter().any(|r| r.name == "Groceries"));
}

#[tokio::test]
async fn create_stores_the_category_and_its_colour_token() {
    let db = fresh_db();
    let view = create_category_with(&db, "  Pets ", "", "text-blue")
        .await
        .expect("created");
    let pets = row(&view, "Pets");
    assert_eq!(pets.colour, "text-blue");
    assert_eq!(pets.uses, 0);
    assert_eq!(pets.glyph, "◆", "blank glyph falls back to the default");
    assert!(db.category_cap_by_name("Pets").await.is_ok());
}

#[tokio::test]
async fn create_refuses_anything_but_a_palette_token_and_writes_nothing() {
    for bad in ["#ff0000", "red", "var(--ok)", "--ok", "neon", "", "ok;x"] {
        let db = fresh_db();
        let before = db.category_caps().await.expect("caps").len();
        let msg = server_error(create_category_with(&db, "Pets", "", bad).await, 500);
        assert_eq!(msg, "pick a colour from the palette", "input {bad:?}");
        assert_eq!(db.category_caps().await.expect("caps").len(), before);
    }
}

#[tokio::test]
async fn create_surfaces_the_ledger_validation_message() {
    let db = fresh_db();
    let msg = server_error(
        create_category_with(&db, "groceries", "", "indigo-2").await,
        500,
    );
    assert_eq!(msg, "category Groceries already exists");
    let msg = server_error(create_category_with(&db, "   ", "", "indigo-2").await, 500);
    assert_eq!(msg, "category name cannot be empty");
}

#[tokio::test]
async fn set_colour_accepts_tokens_only_and_survives_a_rename() {
    let db = fresh_db();
    let msg = server_error(
        set_category_colour_with(&db, "Transport", "#123456").await,
        500,
    );
    assert_eq!(msg, "pick a colour from the palette");
    assert_eq!(
        row(&list_categories_with(&db).await.expect("read"), "Transport").colour,
        DEFAULT_COLOUR
    );

    set_category_colour_with(&db, "Transport", "indigo-neon")
        .await
        .expect("set");
    let view = rename_category_with(&db, "Transport", "Travel")
        .await
        .expect("renamed");
    assert!(view.rows.iter().all(|r| r.name != "Transport"));
    assert_eq!(
        row(&view, "Travel").colour,
        "indigo-neon",
        "colour follows the slug"
    );
}

#[tokio::test]
async fn set_colour_on_an_unknown_category_is_not_found() {
    let db = fresh_db();
    let msg = server_error(set_category_colour_with(&db, "Nope", "indigo").await, 500);
    assert_eq!(msg, "this category no longer exists");
}

#[tokio::test]
async fn a_stored_colour_outside_the_palette_reads_as_the_default() {
    let db = fresh_db();
    db.set_preference("category_colour.rent", "#ff0000")
        .await
        .expect("raw write");
    let view = list_categories_with(&db).await.expect("read");
    assert_eq!(row(&view, "Rent").colour, DEFAULT_COLOUR);
}

#[tokio::test]
async fn rename_carries_history_and_reports_clashes() {
    let db = fresh_db();
    let uses = row(&list_categories_with(&db).await.expect("read"), "Groceries").uses;
    let view = rename_category_with(&db, "Groceries", "Food")
        .await
        .expect("renamed");
    assert_eq!(row(&view, "Food").uses, uses);

    let msg = server_error(rename_category_with(&db, "Food", "rent").await, 500);
    assert_eq!(msg, "category Rent already exists");
}

#[tokio::test]
async fn merge_preview_counts_the_line_items_the_merge_repoints() {
    let db = fresh_db();
    let from_lines = lines_in(&db, "Coffee & snacks").await;
    let into_lines = lines_in(&db, "Groceries").await;
    assert!(from_lines > 0, "the seed has coffee line items");

    // Names resolve case-insensitively, as in the ledger.
    let preview = merge_preview_with(&db, "coffee & snacks", "Groceries")
        .await
        .expect("preview");
    assert_eq!(preview.from, "Coffee & snacks");
    assert_eq!(preview.into, "Groceries");
    assert_eq!(preview.line_items, from_lines);
    let usage = phosk_ledger::categories::category_usage(&db, "Coffee & snacks")
        .await
        .expect("usage");
    assert_eq!(preview.records, usage);

    let (moved, view) = merge_categories_with(&db, "Coffee & snacks", "Groceries")
        .await
        .expect("merged");
    assert_eq!(moved, preview.records, "the preview told the truth");
    assert!(view.rows.iter().all(|r| r.name != "Coffee & snacks"));
    assert_eq!(lines_in(&db, "Coffee & snacks").await, 0);
    assert_eq!(lines_in(&db, "Groceries").await, into_lines + from_lines);
}

#[tokio::test]
async fn merge_into_itself_or_an_unknown_category_is_refused_before_writing() {
    let db = fresh_db();
    let msg = server_error(merge_preview_with(&db, "Rent", "rent").await, 500);
    assert_eq!(msg, "category Rent cannot be merged into itself");
    let msg = server_error(merge_preview_with(&db, "Rent", "Nope").await, 500);
    assert_eq!(msg, "this category no longer exists");
    let msg = server_error(merge_categories_with(&db, "Rent", "Rent").await, 500);
    assert_eq!(msg, "category Rent cannot be merged into itself");
    assert!(db.category_cap_by_name("Rent").await.is_ok());
}

#[tokio::test]
async fn delete_removes_an_unused_category_and_refuses_a_used_one() {
    let db = fresh_db();
    create_category_with(&db, "Pets", "", "ink-2")
        .await
        .expect("created");
    let view = delete_category_with(&db, "Pets").await.expect("deleted");
    assert!(view.rows.iter().all(|r| r.name != "Pets"));

    let msg = server_error(delete_category_with(&db, "Groceries").await, 500);
    assert!(
        msg.starts_with("category Groceries is still used by"),
        "{msg}"
    );
}

#[test]
fn store_errors_never_leak_adapter_detail() {
    let engine = PhoskError::Invalid("surreal file engine at /some/path: boom".to_owned());
    let msg = server_error::<()>(Err(store_error(engine)), 500);
    assert_eq!(msg, "could not save the change, try again");
    let overflow = PhoskError::Overflow("x".to_owned());
    assert_eq!(
        server_error::<()>(Err(store_error(overflow)), 500),
        "could not save the change, try again"
    );
}

#[test]
fn the_palette_leaves_out_status_and_warm_accent_tokens() {
    // `ok` / `warn` mean status, `neon-magenta` is a warm red-pink next to the
    // coral signal; none of them may tag a category.
    for t in ["ok", "warn", "neon-magenta", "neon", "coral"] {
        assert!(
            !COLOUR_TOKENS.contains(&t),
            "{t} must not be a category colour"
        );
    }
}

#[tokio::test]
async fn a_colour_stored_before_the_palette_shrank_reads_as_the_default() {
    let db = fresh_db();
    db.set_preference("category_colour.rent", "warn")
        .await
        .expect("raw write");
    let view = list_categories_with(&db).await.expect("read");
    assert_eq!(row(&view, "Rent").colour, DEFAULT_COLOUR);
    let msg = server_error(set_category_colour_with(&db, "Rent", "warn").await, 500);
    assert_eq!(msg, "pick a colour from the palette");
}

/// The colour row a vanished category leaves behind: cleared, not user-set.
async fn assert_colour_cleared(db: &dyn DatabaseAdapter, slug: &str) {
    let pref = db
        .preference(&format!("category_colour.{slug}"))
        .await
        .expect("the row stays, reset");
    assert!(
        !COLOUR_TOKENS.contains(&pref.value.as_str()),
        "a reused slug must not inherit {:?}",
        pref.value
    );
    assert_eq!(pref.provenance.source, phosk_model::Source::RuleGenerated);
}

#[tokio::test]
async fn delete_clears_the_vanished_category_colour() {
    let db = fresh_db();
    let before = phosk_settings::settings_summary(&db)
        .await
        .expect("summary");
    let view = create_category_with(&db, "Pets", "", "indigo-3")
        .await
        .expect("created");
    let slug = row(&view, "Pets").slug.clone();
    delete_category_with(&db, "Pets").await.expect("deleted");
    assert_colour_cleared(&db, &slug).await;
    let after = phosk_settings::settings_summary(&db)
        .await
        .expect("summary");
    assert_eq!(after.total_preferences, before.total_preferences);
    assert_eq!(after.changed_count, before.changed_count);
}

#[tokio::test]
async fn merge_clears_the_source_colour_and_keeps_the_target_colour() {
    let db = fresh_db();
    set_category_colour_with(&db, "Coffee & snacks", "ink-3")
        .await
        .expect("set");
    let view = set_category_colour_with(&db, "Groceries", "indigo-2")
        .await
        .expect("set");
    let slug = row(&view, "Coffee & snacks").slug.clone();
    let (_, view) = merge_categories_with(&db, "coffee & snacks", "Groceries")
        .await
        .expect("merged");
    assert_colour_cleared(&db, &slug).await;
    assert_eq!(row(&view, "Groceries").colour, "indigo-2");
}

#[tokio::test]
async fn a_colour_write_failing_after_create_is_a_notice_not_an_error() {
    let db = fresh_db();
    let view = list_categories_with(&db).await.expect("read");
    assert_eq!(with_colour_notice(view.clone(), true).notice, None);
    let noticed = with_colour_notice(view.clone(), false);
    assert_eq!(
        noticed.notice.as_deref(),
        Some("category created, but its colour was not saved · pick it again")
    );
    assert_eq!(noticed.rows, view.rows, "the category is still listed");
}
