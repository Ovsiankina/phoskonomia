//! `/categories` data: the category list, and create / rename / delete-if-empty
//! / merge through `phosk_ledger::categories`, plus a per-category colour.
//!
//! ## Colour
//!
//! A category's colour is an Oscillocore **token name** (`"indigo-neon"`), never
//! a raw hex or free text: the wire value is checked server-side against
//! `COLOUR_TOKENS`, and the page renders it as `var(--<token>)`. Left out
//! on purpose: the coral family and the warm `neon-magenta` (coral is the
//! view's single signal moment), and `ok` / `warn` (they mean status).
//!
//! `CategoryCap` has no colour field, so the token is stored as the preference
//! `category_colour.<slug>` (the slug is stable across renames). That keeps the
//! port and both adapters untouched; `/config` lists only its own rule-table
//! keys and `phosk_settings::settings_summary` skips the prefix, so these rows
//! never show up there. A stored value that is not in the palette reads back as
//! [`DEFAULT_COLOUR`]. When a delete or merge makes a category vanish, its row
//! is reset to an empty, non-user value (the port has no removal), so a slug
//! reused later never inherits the old colour.
//!
//! Every write returns the refreshed [`CategoriesDto`], so the page never needs
//! a second round-trip. Testable inner fns are `*_with(db, …)` (see
//! `data/tests/categories.rs`).

use dioxus::prelude::*;
use serde::{Deserialize, Serialize};

/// The colour tokens a category may carry, in picker order. Every entry is a
/// custom property in `colors_and_type.css` (without the leading `--`).
/// Server-only: the page gets the list through [`CategoriesDto::palette`].
#[cfg(feature = "server-deps")]
pub const COLOUR_TOKENS: &[&str] = &[
    "indigo",
    "indigo-2",
    "indigo-3",
    "indigo-neon",
    "text-blue",
    "ink-2",
    "ink-3",
];

/// The colour of a category nobody picked one for.
pub const DEFAULT_COLOUR: &str = "indigo";

/// One row of the category list.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CategoryRowDto {
    /// Display name — the category's identity.
    pub name: String,
    /// Stable slug (the colour preference key hangs off it).
    pub slug: String,
    /// Display glyph.
    pub glyph: String,
    /// Colour token from `COLOUR_TOKENS`.
    pub colour: String,
    /// Whether this is a fixed (non-discretionary) envelope.
    pub fixed: bool,
    /// Receipts, line items, subscriptions and signals naming it.
    pub uses: u32,
}

/// The `/categories` view.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CategoriesDto {
    /// Every category, by name.
    pub rows: Vec<CategoryRowDto>,
    /// The colour tokens the picker offers (the server's allow-list).
    pub palette: Vec<String>,
    /// A write that half-succeeded (the category exists, a side detail was not
    /// saved): shown as a hint, not an error.
    pub notice: Option<String>,
}

/// What a merge would do — shown in the confirm step before it runs.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MergePreviewDto {
    /// The category that disappears (stored spelling).
    pub from: String,
    /// The category that absorbs it (stored spelling).
    pub into: String,
    /// Line items that will be re-pointed.
    pub line_items: u32,
    /// Every row that will be re-pointed (receipts, line items,
    /// subscriptions, signals) — what the merge reports back.
    pub records: u32,
}

/// Page-safe messages the data layer itself produces.
#[cfg(feature = "server-deps")]
mod msg {
    pub(super) const BAD_COLOUR: &str = "pick a colour from the palette";
    pub(super) const UNKNOWN: &str = "this category no longer exists";
    pub(super) const FAILED: &str = "could not save the change, try again";
    pub(super) const COLOUR_NOT_SAVED: &str =
        "category created, but its colour was not saved · pick it again";
}

/// Shown when a call fails before the server could answer.
const UNREACHABLE: &str = "could not reach the server, nothing was changed";

/// The text to show for a failed call: the server's own (page-safe) message,
/// or a generic line when the request never got an answer.
pub fn error_text(err: &ServerFnError) -> String {
    match err {
        ServerFnError::ServerError { message, .. } if !message.is_empty() => message.clone(),
        _ => UNREACHABLE.to_string(),
    }
}

// ═══ server fns ══════════════════════════════════════════════════════════════

/// The category list. REAL: `DatabaseAdapter::category_caps` + one usage pass
/// over the store + the colour preferences.
#[server]
pub async fn list_categories() -> Result<CategoriesDto, ServerFnError> {
    #[cfg(feature = "server-deps")]
    {
        let session = crate::data::build_session().await?;
        list_categories_with(session.db()).await
    }
    #[cfg(not(feature = "server-deps"))]
    {
        Err(ServerFnError::new("server-only"))
    }
}

/// Create a category with a palette colour. REAL:
/// `phosk_ledger::categories::create_category`.
#[server]
pub async fn create_category(
    name: String,
    glyph: String,
    colour: String,
) -> Result<CategoriesDto, ServerFnError> {
    #[cfg(feature = "server-deps")]
    {
        let session = crate::data::build_session().await?;
        create_category_with(session.db(), &name, &glyph, &colour).await
    }
    #[cfg(not(feature = "server-deps"))]
    {
        let _ = (name, glyph, colour);
        Err(ServerFnError::new("server-only"))
    }
}

/// Rename a category. REAL: `phosk_ledger::categories::rename_category`.
#[server]
pub async fn rename_category(from: String, to: String) -> Result<CategoriesDto, ServerFnError> {
    #[cfg(feature = "server-deps")]
    {
        let session = crate::data::build_session().await?;
        rename_category_with(session.db(), &from, &to).await
    }
    #[cfg(not(feature = "server-deps"))]
    {
        let _ = (from, to);
        Err(ServerFnError::new("server-only"))
    }
}

/// Change a category's colour token.
#[server]
pub async fn set_category_colour(
    name: String,
    colour: String,
) -> Result<CategoriesDto, ServerFnError> {
    #[cfg(feature = "server-deps")]
    {
        let session = crate::data::build_session().await?;
        set_category_colour_with(session.db(), &name, &colour).await
    }
    #[cfg(not(feature = "server-deps"))]
    {
        let _ = (name, colour);
        Err(ServerFnError::new("server-only"))
    }
}

/// Delete a category nothing references. REAL:
/// `phosk_ledger::categories::delete_category`.
#[server]
pub async fn delete_category(name: String) -> Result<CategoriesDto, ServerFnError> {
    #[cfg(feature = "server-deps")]
    {
        let session = crate::data::build_session().await?;
        delete_category_with(session.db(), &name).await
    }
    #[cfg(not(feature = "server-deps"))]
    {
        let _ = name;
        Err(ServerFnError::new("server-only"))
    }
}

/// What merging `from` into `into` would re-point (read-only).
#[server]
pub async fn merge_preview(from: String, into: String) -> Result<MergePreviewDto, ServerFnError> {
    #[cfg(feature = "server-deps")]
    {
        let session = crate::data::build_session().await?;
        merge_preview_with(session.db(), &from, &into).await
    }
    #[cfg(not(feature = "server-deps"))]
    {
        let _ = (from, into);
        Err(ServerFnError::new("server-only"))
    }
}

/// Fold `from` into `into`. REAL: `phosk_ledger::categories::merge_categories`.
#[server]
pub async fn merge_categories(from: String, into: String) -> Result<CategoriesDto, ServerFnError> {
    #[cfg(feature = "server-deps")]
    {
        let session = crate::data::build_session().await?;
        merge_categories_with(session.db(), &from, &into)
            .await
            .map(|(_, view)| view)
    }
    #[cfg(not(feature = "server-deps"))]
    {
        let _ = (from, into);
        Err(ServerFnError::new("server-only"))
    }
}

// ═══ inner fns (server-only, testable against any adapter) ═══════════════════

#[cfg(feature = "server-deps")]
use std::collections::HashMap;

#[cfg(feature = "server-deps")]
use phosk_adapter_db::DatabaseAdapter;
#[cfg(feature = "server-deps")]
use phosk_core::error::PhoskError;
#[cfg(feature = "server-deps")]
use phosk_model::CategoryCap;

/// The preference key holding a category's colour token.
#[cfg(feature = "server-deps")]
fn colour_key(slug: &str) -> String {
    format!("{}{slug}", phosk_settings::CATEGORY_COLOUR_PREFIX)
}

/// Reset the colour row of every category in `before` that no longer exists,
/// so a slug reused later (e.g. by a split) starts from the default. Best
/// effort: the delete/merge already happened, and a leftover row is only
/// cosmetic, so a failed reset does not turn that success into an error.
#[cfg(feature = "server-deps")]
async fn clear_vanished_colours(db: &dyn DatabaseAdapter, before: &[CategoryCap]) {
    let Ok(after) = db.category_caps().await else {
        return;
    };
    for gone in before
        .iter()
        .filter(|b| after.iter().all(|a| a.slug != b.slug))
    {
        let _ = db.reset_preference(&colour_key(&gone.slug), "").await;
    }
}

/// Every stored row naming each category, in one pass over the store — the
/// same four sources `phosk_ledger::categories::category_usage` counts.
#[cfg(feature = "server-deps")]
async fn usage_by_name(db: &dyn DatabaseAdapter) -> Result<HashMap<String, u32>, PhoskError> {
    let mut uses: HashMap<String, u32> = HashMap::new();
    let mut bump = |name: &str| {
        let n = uses.entry(name.to_owned()).or_default();
        *n = n.saturating_add(1);
    };
    for r in db.all_receipts().await? {
        bump(&r.category);
        for l in db.line_items(r.id).await? {
            bump(&l.category);
        }
    }
    for s in db.subscriptions().await? {
        bump(&s.category);
    }
    for s in db.signals().await? {
        bump(&s.parent);
    }
    Ok(uses)
}

/// Mark a freshly created view when the category exists but its colour write
/// failed: retrying would only hit "already exists", so this is a notice, not
/// an error.
#[cfg(feature = "server-deps")]
pub(crate) fn with_colour_notice(mut view: CategoriesDto, colour_saved: bool) -> CategoriesDto {
    if !colour_saved {
        view.notice = Some(msg::COLOUR_NOT_SAVED.to_owned());
    }
    view
}

/// Accept only an exact palette token.
#[cfg(feature = "server-deps")]
fn palette_token(colour: &str) -> Result<&'static str, ServerFnError> {
    COLOUR_TOKENS
        .iter()
        .find(|t| **t == colour)
        .copied()
        .ok_or_else(|| ServerFnError::new(msg::BAD_COLOUR))
}

/// Map a store failure to a page-safe error. The ledger's own validation
/// messages (`"category … already exists"`, `"… cannot be empty"`) name only
/// what the user typed and pass through; any other `Invalid` may carry engine
/// detail (paths, driver text) and becomes a generic line.
#[cfg(feature = "server-deps")]
pub(crate) fn store_error(err: PhoskError) -> ServerFnError {
    match err {
        PhoskError::NotFound(_) => ServerFnError::new(msg::UNKNOWN),
        PhoskError::Invalid(m) if m.starts_with("category ") => ServerFnError::new(m),
        PhoskError::Invalid(_) | PhoskError::InvalidDate(_) | PhoskError::Overflow(_) => {
            ServerFnError::new(msg::FAILED)
        }
    }
}

/// Build the list view: every category with its usage and colour, by name.
#[cfg(feature = "server-deps")]
pub(crate) async fn list_categories_with(
    db: &dyn DatabaseAdapter,
) -> Result<CategoriesDto, ServerFnError> {
    let caps = db.category_caps().await.map_err(store_error)?;
    let prefs = db.preferences().await.map_err(store_error)?;
    let usage = usage_by_name(db).await.map_err(store_error)?;
    let mut rows = Vec::with_capacity(caps.len());
    for c in caps {
        let key = colour_key(&c.slug);
        let colour = prefs
            .iter()
            .find(|p| p.key == key)
            .and_then(|p| COLOUR_TOKENS.iter().find(|t| **t == p.value))
            .copied()
            .unwrap_or(DEFAULT_COLOUR);
        let uses = usage.get(&c.name).copied().unwrap_or(0);
        rows.push(CategoryRowDto {
            name: c.name,
            slug: c.slug,
            glyph: c.glyph,
            colour: colour.to_owned(),
            fixed: c.fixed,
            uses,
        });
    }
    rows.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    Ok(CategoriesDto {
        rows,
        palette: COLOUR_TOKENS.iter().map(|t| (*t).to_owned()).collect(),
        notice: None,
    })
}

/// Validate the colour first (a bad token creates nothing), then create the
/// category and store its colour. Once the category exists the create has
/// succeeded: a failed colour write comes back as a [`CategoriesDto::notice`].
#[cfg(feature = "server-deps")]
pub(crate) async fn create_category_with(
    db: &dyn DatabaseAdapter,
    name: &str,
    glyph: &str,
    colour: &str,
) -> Result<CategoriesDto, ServerFnError> {
    let colour = palette_token(colour)?;
    let created = phosk_ledger::categories::create_category(
        db,
        phosk_ledger::categories::NewCategory {
            name: name.to_owned(),
            cap: None,
            fixed: false,
            glyph: glyph.to_owned(),
            note: String::new(),
        },
    )
    .await
    .map_err(store_error)?;
    let colour_saved = db
        .set_preference(&colour_key(&created.slug), colour)
        .await
        .is_ok();
    Ok(with_colour_notice(
        list_categories_with(db).await?,
        colour_saved,
    ))
}

/// Rename; the colour follows because it is keyed by the stable slug.
#[cfg(feature = "server-deps")]
pub(crate) async fn rename_category_with(
    db: &dyn DatabaseAdapter,
    from: &str,
    to: &str,
) -> Result<CategoriesDto, ServerFnError> {
    phosk_ledger::categories::rename_category(db, from, to)
        .await
        .map_err(store_error)?;
    list_categories_with(db).await
}

/// Store a palette token for an existing category.
#[cfg(feature = "server-deps")]
pub(crate) async fn set_category_colour_with(
    db: &dyn DatabaseAdapter,
    name: &str,
    colour: &str,
) -> Result<CategoriesDto, ServerFnError> {
    let colour = palette_token(colour)?;
    let cat = db.category_cap_by_name(name).await.map_err(store_error)?;
    db.set_preference(&colour_key(&cat.slug), colour)
        .await
        .map_err(store_error)?;
    list_categories_with(db).await
}

/// Delete-if-empty; a used category is refused with the ledger's count. The
/// deleted category's colour row is cleared.
#[cfg(feature = "server-deps")]
pub(crate) async fn delete_category_with(
    db: &dyn DatabaseAdapter,
    name: &str,
) -> Result<CategoriesDto, ServerFnError> {
    let before = db.category_caps().await.map_err(store_error)?;
    phosk_ledger::categories::delete_category(db, name)
        .await
        .map_err(store_error)?;
    clear_vanished_colours(db, &before).await;
    list_categories_with(db).await
}

/// Resolve both ends the way `merge_categories` does (case-insensitively,
/// refusing a self-merge), then count what would move. Writes nothing.
#[cfg(feature = "server-deps")]
pub(crate) async fn merge_preview_with(
    db: &dyn DatabaseAdapter,
    from: &str,
    into: &str,
) -> Result<MergePreviewDto, ServerFnError> {
    let caps = db.category_caps().await.map_err(store_error)?;
    let find = |n: &str| {
        caps.iter()
            .find(|c| c.name.eq_ignore_ascii_case(n))
            .map(|c| c.name.clone())
            .ok_or_else(|| ServerFnError::new(msg::UNKNOWN))
    };
    let (source, target) = (find(from)?, find(into)?);
    if source == target {
        return Err(ServerFnError::new(format!(
            "category {from} cannot be merged into itself"
        )));
    }
    let mut line_items: u32 = 0;
    for r in db.all_receipts().await.map_err(store_error)? {
        let lines = db.line_items(r.id).await.map_err(store_error)?;
        let n = lines.iter().filter(|l| l.category == source).count();
        line_items = line_items.saturating_add(u32::try_from(n).unwrap_or(u32::MAX));
    }
    let records = phosk_ledger::categories::category_usage(db, &source)
        .await
        .map_err(store_error)?;
    Ok(MergePreviewDto {
        from: source,
        into: target,
        line_items,
        records,
    })
}

/// Run the merge; returns how many rows moved, and the refreshed view. The
/// source's colour row is cleared; the target keeps its own.
#[cfg(feature = "server-deps")]
pub(crate) async fn merge_categories_with(
    db: &dyn DatabaseAdapter,
    from: &str,
    into: &str,
) -> Result<(u32, CategoriesDto), ServerFnError> {
    let before = db.category_caps().await.map_err(store_error)?;
    let moved = phosk_ledger::categories::merge_categories(db, from, into)
        .await
        .map_err(store_error)?;
    clear_vanished_colours(db, &before).await;
    Ok((moved, list_categories_with(db).await?))
}
