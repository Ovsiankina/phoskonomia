//! `categories` — the ledger-side category breakdown of spend, and the
//! category list's write side (create · rename · delete-if-empty · merge ·
//! split).
//!
//! The *read* half owns the observed per-category spend distribution derived
//! from receipts — the filter option list the Transactions page shows and the
//! per-category roll-up the receipt list groups on.
//!
//! The *write* half owns the category records themselves. The stored record is
//! [`CategoryCap`] (name · cap · glyph · note · provenance) — the one the
//! Budgets and Categories pages edit, and the one receipts, line items,
//! subscriptions and signals reference **by name** (ADR-008: the name is the
//! identity). `phosk_planning` owns the *budget* meaning of that record (caps,
//! alerts, history); this module owns its life cycle, because creating,
//! renaming and deleting a category is a ledger-integrity operation: a rename
//! must carry every historical reference with it, and a delete may only ever
//! remove a category nothing points at. [`merge_categories`] and
//! [`split_category`] move history *between* categories — wholesale, or a
//! hand-picked set of line items — each as one all-or-nothing operation.
//!
//! Everything here validates its input before it touches the store (names are
//! trimmed, bounded and control-character free) and stamps [`Provenance`]:
//! [`Source::UserEntered`] on create, [`Source::UserModified`] on rename, on a
//! merge target, and on every line a split re-points.
//!
//! [`Source::UserEntered`]: phosk_model::Source::UserEntered
//! [`Source::UserModified`]: phosk_model::Source::UserModified

use std::collections::BTreeSet;
use std::collections::HashMap;
use std::collections::HashSet;

use phosk_adapter_db::DatabaseAdapter;
use phosk_core::cycle::Period;
use phosk_core::error::PhoskError;
use phosk_core::money::Money;
use phosk_id::{CategoryId, LineItemId};
use phosk_model::{CategoryCap, Provenance};
use serde::{Deserialize, Serialize};

/// One category's observed spend across a cycle window (ledger view).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CategorySpendDto {
    /// Category name (identity for UI/AI).
    pub category: String,
    /// Total spent in this category in the window (exact centimes).
    #[serde(with = "phosk_model::money_centimes")]
    pub total: Money,
    /// Number of receipts in this category in the window.
    pub txns: u32,
}

/// The observed per-category spend distribution for the current cycle, ranked
/// by total spend descending (ties broken on name ascending).
#[tracing::instrument(level = "debug", skip_all)]
pub async fn category_spend(
    db: &dyn DatabaseAdapter,
    as_of: chrono::NaiveDate,
) -> Result<Vec<CategorySpendDto>, PhoskError> {
    let window = Period::Month.resolve(as_of)?;
    let receipts = db.receipts_between(window.start, window.end).await?;

    let mut totals: HashMap<String, (Money, u32)> = HashMap::new();
    for r in receipts {
        let entry = totals.entry(r.category).or_insert((Money::ZERO, 0));
        entry.0 = entry.0.checked_add(r.amount)?;
        entry.1 = entry.1.saturating_add(1);
    }

    let mut ranked: Vec<CategorySpendDto> = totals
        .into_iter()
        .map(|(category, (total, txns))| CategorySpendDto {
            category,
            total,
            txns,
        })
        .collect();
    ranked.sort_by(|a, b| {
        b.total
            .cmp(&a.total)
            .then_with(|| a.category.cmp(&b.category))
    });
    Ok(ranked)
}

/// The distinct category names present in the seed (filter-dropdown options),
/// sorted ascending.
#[tracing::instrument(level = "debug", skip_all)]
pub async fn available_categories(db: &dyn DatabaseAdapter) -> Result<Vec<String>, PhoskError> {
    let names: BTreeSet<String> = db
        .all_receipts()
        .await?
        .into_iter()
        .map(|r| r.category)
        .collect();
    Ok(names.into_iter().collect())
}

// ── Write side: create · rename · delete-if-empty ───────────────────────────

/// Longest accepted category name, in characters. Long enough for
/// `"Health insurance"` and friends, short enough to stay a label.
const MAX_NAME_CHARS: usize = 48;
/// Longest accepted display glyph, in characters (one symbol, possibly
/// composed — `"☕"`, `"▤"`).
const MAX_GLYPH_CHARS: usize = 4;
/// Longest accepted AI-guidance note, in characters.
const MAX_NOTE_CHARS: usize = 200;
/// Glyph used when the caller supplies none.
const DEFAULT_GLYPH: &str = "◆";

/// What the caller supplies to create a category. The id, slug and provenance
/// are derived by [`create_category`] — never taken from the caller.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NewCategory {
    /// Display name — the category's identity. Trimmed before use.
    pub name: String,
    /// Optional cycle cap in exact centimes; `None` is an unlimited category.
    #[serde(with = "phosk_model::opt_money_centimes")]
    pub cap: Option<Money>,
    /// Whether this is a fixed (non-discretionary) envelope.
    pub fixed: bool,
    /// Display glyph; empty falls back to [`DEFAULT_GLYPH`].
    pub glyph: String,
    /// Free-text guidance line shown next to the envelope.
    pub note: String,
}

/// Create a spending category and return the stored record.
///
/// The name is trimmed and validated, the `slug` is derived from it (made
/// unique against the existing categories), and the record is stamped
/// [`Source::UserEntered`](phosk_model::Source::UserEntered). Category names
/// are compared **case-insensitively** for uniqueness: `"groceries"` and
/// `"Groceries"` are the same envelope to a human, so they must not coexist.
///
/// # Errors
/// - [`PhoskError::Invalid`] if the name is blank, too long or contains control
///   characters, if the glyph/note exceed their bounds, if the cap is negative,
///   or if a category with that name already exists.
/// - Propagates any [`PhoskError`] from the adapter.
#[tracing::instrument(level = "debug", skip_all)]
pub async fn create_category(
    db: &dyn DatabaseAdapter,
    new: NewCategory,
) -> Result<CategoryCap, PhoskError> {
    let existing = db.category_caps().await?;
    let record = validated_record(&new, &existing)?;
    db.insert_category(record.clone()).await?;
    tracing::debug!("created category");
    Ok(record)
}

/// Turn caller input into the record the port stores: name/glyph/note cleaned,
/// cap sanity-checked, name unique (case-insensitively) against `existing`,
/// slug derived and made unique, provenance stamped. Writes nothing — the two
/// callers ([`create_category`] and [`split_category`]) hand the result to a
/// different port method.
fn validated_record(
    new: &NewCategory,
    existing: &[CategoryCap],
) -> Result<CategoryCap, PhoskError> {
    let name = clean_text(&new.name, "category name", MAX_NAME_CHARS)?;
    let glyph = if new.glyph.trim().is_empty() {
        DEFAULT_GLYPH.to_owned()
    } else {
        clean_text(&new.glyph, "category glyph", MAX_GLYPH_CHARS)?
    };
    let note = if new.note.trim().is_empty() {
        String::new()
    } else {
        clean_text(&new.note, "category note", MAX_NOTE_CHARS)?
    };
    if new.cap.is_some_and(|cap| cap.centimes() < 0) {
        return Err(PhoskError::Invalid(
            "category cap cannot be negative".to_owned(),
        ));
    }
    if let Some(clash) = existing.iter().find(|c| same_name(&c.name, &name)) {
        return Err(PhoskError::Invalid(format!(
            "category {} already exists",
            clash.name
        )));
    }

    Ok(CategoryCap {
        id: CategoryId::new(),
        slug: unique_slug(&name, existing),
        name,
        cap: new.cap,
        fixed: new.fixed,
        glyph,
        note,
        provenance: Provenance::user_entered(),
    })
}

/// Rename a category, carrying every historical reference with it.
///
/// The adapter re-points the receipts, line items, subscriptions and signals
/// that named `from` (see
/// [`DatabaseAdapter::rename_category`](phosk_adapter_db::DatabaseAdapter::rename_category)),
/// so no spend is ever stranded on a name that no longer exists. The record's
/// id and slug are stable; its provenance becomes `UserModified`. Renaming a
/// category to the name it already has is a no-op success; changing only the
/// case of its own name is a real rename.
///
/// # Errors
/// - [`PhoskError::NotFound`] if no category is named `from`.
/// - [`PhoskError::Invalid`] if `to` is blank/too long/has control characters,
///   or if a different category already carries it.
#[tracing::instrument(level = "debug", skip_all)]
pub async fn rename_category(
    db: &dyn DatabaseAdapter,
    from: &str,
    to: &str,
) -> Result<(), PhoskError> {
    let to = clean_text(to, "category name", MAX_NAME_CHARS)?;
    let caps = db.category_caps().await?;
    let current = caps
        .iter()
        .find(|c| c.name == from)
        .ok_or_else(|| PhoskError::NotFound(format!("category {from}")))?;

    if current.name == to {
        tracing::debug!("rename to the same name is a no-op");
        return Ok(());
    }
    if let Some(clash) = caps
        .iter()
        .find(|c| c.name != current.name && same_name(&c.name, &to))
    {
        return Err(PhoskError::Invalid(format!(
            "category {} already exists",
            clash.name
        )));
    }

    db.rename_category(&current.name, &to).await
}

/// How many stored records still reference `category` by name.
///
/// Counts receipts, their line items, subscriptions and signals — everything
/// the rename/delete rules care about. Drives the "N entries" hint next to a
/// category the UI cannot delete yet.
///
/// # Errors
/// Propagates any [`PhoskError`] from the adapter's reads.
#[tracing::instrument(level = "debug", skip_all)]
pub async fn category_usage(db: &dyn DatabaseAdapter, category: &str) -> Result<u32, PhoskError> {
    let receipts = db.all_receipts().await?;
    let mut uses: u32 = 0;
    for r in &receipts {
        if r.category == category {
            uses = uses.saturating_add(1);
        }
        let lines = db.line_items(r.id).await?;
        let on_lines = lines.iter().filter(|l| l.category == category).count();
        uses = uses.saturating_add(u32::try_from(on_lines).unwrap_or(u32::MAX));
    }
    let subs = db
        .subscriptions()
        .await?
        .iter()
        .filter(|s| s.category == category)
        .count();
    let signals = db
        .signals()
        .await?
        .iter()
        .filter(|s| s.parent == category)
        .count();
    uses = uses.saturating_add(u32::try_from(subs).unwrap_or(u32::MAX));
    uses = uses.saturating_add(u32::try_from(signals).unwrap_or(u32::MAX));
    Ok(uses)
}

/// Delete a category — **only** if nothing references it any more.
///
/// A category carrying history is not deletable: dropping it would strand that
/// spend on a name no envelope answers for. The caller is told how many records
/// still point at it so the UI can offer a rename (or, later, a merge) instead.
/// The adapter enforces the same rule, whoever calls it.
///
/// # Errors
/// - [`PhoskError::NotFound`] if no category carries that `name`.
/// - [`PhoskError::Invalid`] if the category is still referenced.
#[tracing::instrument(level = "debug", skip_all)]
pub async fn delete_category(db: &dyn DatabaseAdapter, name: &str) -> Result<(), PhoskError> {
    let caps = db.category_caps().await?;
    if !caps.iter().any(|c| c.name == name) {
        return Err(PhoskError::NotFound(format!("category {name}")));
    }
    let uses = category_usage(db, name).await?;
    if uses > 0 {
        return Err(PhoskError::Invalid(format!(
            "category {name} is still used by {uses} record(s) — rename it instead of deleting it"
        )));
    }
    db.delete_category(name).await
}

// ── Write side: merge · split ───────────────────────────────────────────────

/// Fold the category `from` into `into`; returns how many stored rows were
/// re-pointed.
///
/// The way out of the corner [`delete_category`] leaves a user in: a category
/// that carries history cannot be deleted, but it *can* be merged away. Every
/// receipt, line item, subscription and signal that named `from` names `into`
/// afterwards, and only then does the `from` record go — one operation, so no
/// spend is ever stranded (see
/// [`DatabaseAdapter::merge_categories`](phosk_adapter_db::DatabaseAdapter::merge_categories)).
///
/// The target keeps its own cap: two envelopes becoming one does not mean their
/// budgets add up, and deciding the new cap is the user's call on the Budgets
/// page. The source's prior-cycle history rows stay with the deleted id.
///
/// Both names are matched case-insensitively, the way category names are
/// unique in the first place.
///
/// # Errors
/// - [`PhoskError::NotFound`] if either category does not exist.
/// - [`PhoskError::Invalid`] if `from` and `into` name the same category
///   (`"Coffee"` and `"coffee"` are one envelope).
#[tracing::instrument(level = "debug", skip_all)]
pub async fn merge_categories(
    db: &dyn DatabaseAdapter,
    from: &str,
    into: &str,
) -> Result<u32, PhoskError> {
    // Both ends are resolved case-insensitively: names are unique that way
    // (`create_category`), so the match is unambiguous, and it is what makes a
    // case-only self-merge ("Coffee" into "coffee") land on the *same* record
    // and be refused rather than reported as an unknown category.
    let caps = db.category_caps().await?;
    let source = caps
        .iter()
        .find(|c| same_name(&c.name, from))
        .ok_or_else(|| PhoskError::NotFound(format!("category {from}")))?;
    let target = caps
        .iter()
        .find(|c| same_name(&c.name, into))
        .ok_or_else(|| PhoskError::NotFound(format!("category {into}")))?;
    if same_name(&source.name, &target.name) {
        return Err(PhoskError::Invalid(format!(
            "category {from} cannot be merged into itself"
        )));
    }

    let moved = db.merge_categories(&source.name, &target.name).await?;
    tracing::debug!(moved, "merged category");
    Ok(moved)
}

/// What the caller supplies to split a category: the source, the category to
/// carve out of it, and the line items that move.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CategorySplit {
    /// The category the picked lines currently belong to.
    pub from: String,
    /// The category to create for them. Validated exactly like
    /// [`create_category`]'s input.
    pub into: NewCategory,
    /// The line items to move. Repeats are the same line, not extra ones.
    pub lines: Vec<LineItemId>,
}

/// Carve a new category out of `split.from` by moving the picked line items
/// into it; returns the created record.
///
/// The counterpart to a merge, at item granularity: "these five lines were
/// never really Groceries". The new category is created and the lines are
/// re-pointed as one operation, so a rejected split leaves nothing behind —
/// not even an empty category. The receipts the lines hang off keep their own
/// category and their amounts; only the lines move, and each moved line is
/// stamped `UserModified`, because a human just re-judged where it belongs.
///
/// # Errors
/// - [`PhoskError::Invalid`] if no line is picked, if the new category's
///   name/glyph/note/cap fail validation, if that name is already taken, or if
///   a picked line is not currently in `split.from`.
/// - [`PhoskError::NotFound`] if `split.from` does not exist, or if a picked id
///   names no stored line item.
#[tracing::instrument(level = "debug", skip_all)]
pub async fn split_category(
    db: &dyn DatabaseAdapter,
    split: CategorySplit,
) -> Result<CategoryCap, PhoskError> {
    // The same line listed twice is one line; keep the caller's order so the
    // error naming a bad id is the first one they wrote.
    let mut seen = HashSet::new();
    let lines: Vec<LineItemId> = split
        .lines
        .into_iter()
        .filter(|id| seen.insert(*id))
        .collect();
    if lines.is_empty() {
        return Err(PhoskError::Invalid(
            "a split needs at least one line item".to_owned(),
        ));
    }

    let caps = db.category_caps().await?;
    let source = caps
        .iter()
        .find(|c| same_name(&c.name, &split.from))
        .ok_or_else(|| PhoskError::NotFound(format!("category {}", split.from)))?;
    let source_name = source.name.clone();
    let record = validated_record(&split.into, &caps)?;

    db.split_category(&source_name, record.clone(), &lines)
        .await?;
    tracing::debug!(moved = lines.len(), "split category");
    Ok(record)
}

/// Trim `raw`, then accept it only if it is non-empty, within `max` characters
/// and free of control characters (tabs/newlines/escapes have no business in a
/// label and are how a display string smuggles in something else).
fn clean_text(raw: &str, what: &str, max: usize) -> Result<String, PhoskError> {
    let text = raw.trim();
    if text.is_empty() {
        return Err(PhoskError::Invalid(format!("{what} cannot be empty")));
    }
    if text.chars().count() > max {
        return Err(PhoskError::Invalid(format!(
            "{what} is longer than {max} characters"
        )));
    }
    if text.chars().any(char::is_control) {
        return Err(PhoskError::Invalid(format!(
            "{what} cannot contain control characters"
        )));
    }
    Ok(text.to_owned())
}

/// Whether two category names denote the same envelope to a human.
const fn same_name(a: &str, b: &str) -> bool {
    a.eq_ignore_ascii_case(b)
}

/// A URL/UI-safe slug derived from a category name: lowercase ASCII
/// alphanumerics, every other run collapsed to a single `-`.
fn slugify(name: &str) -> String {
    let mut slug = String::with_capacity(name.len());
    for ch in name.chars() {
        if ch.is_ascii_alphanumeric() {
            slug.push(ch.to_ascii_lowercase());
        } else if !slug.ends_with('-') {
            slug.push('-');
        }
    }
    slug.trim_matches('-').to_owned()
}

/// [`slugify`] the name, then make it unique against `existing` by appending
/// `-2`, `-3`, … The slug is the UI's stable handle, so two categories may
/// never share one.
fn unique_slug(name: &str, existing: &[CategoryCap]) -> String {
    let base = slugify(name);
    let base = if base.is_empty() {
        "category".to_owned()
    } else {
        base
    };
    let taken = |s: &str| existing.iter().any(|c| c.slug == s);
    if !taken(&base) {
        return base;
    }
    // Bounded by the number of existing categories plus one, so it always ends.
    for n in 2..=existing.len().saturating_add(2) {
        let candidate = format!("{base}-{n}");
        if !taken(&candidate) {
            return candidate;
        }
    }
    base
}
