//! The Phoskonomia Categories page (`/categories`): list, create, rename,
//! delete-if-empty, merge (with a confirm step naming how many line items move)
//! and a colour picker seeded from Oscillocore tokens — the user picks a swatch,
//! never types a colour.
//!
//! Chrome follows the other pages: `.app-shell.swap` with the left `AiPanel`,
//! `.app-main` (TopBar + scroll). The body reuses the `/config` panel classes
//! (`cfg-*`). The merge panel is the view's one coral moment.

use dioxus::prelude::*;

use crate::components::prims::ScannerBg;
use crate::components::shell::{AiPanel, TopBar};
use crate::components::states::{Awaiting, InlineStatus};
use crate::data::categories::{
    create_category, delete_category, error_text, list_categories, merge_categories, merge_preview,
    rename_category, set_category_colour, CategoriesDto, CategoryRowDto, MergePreviewDto,
    DEFAULT_COLOUR,
};
use crate::data::cycle::get_cycle;

/// One write against the ledger.
#[derive(Clone, PartialEq, Debug)]
enum Op {
    Create { name: String, colour: String },
    Rename { from: String, to: String },
    Colour { name: String, colour: String },
    Delete(String),
    Merge { from: String, into: String },
}

/// Where an op's "saving" state and error show: a panel, or one category's
/// row. Typed, so no category name can collide with a panel.
#[derive(Clone, PartialEq, Debug)]
enum Target {
    Create,
    Merge,
    Row(String),
}

impl Op {
    /// Where the op's status belongs.
    fn target(&self) -> Target {
        match self {
            Self::Create { .. } => Target::Create,
            Self::Rename { from, .. } => Target::Row(from.clone()),
            Self::Colour { name, .. } | Self::Delete(name) => Target::Row(name.clone()),
            Self::Merge { .. } => Target::Merge,
        }
    }
}

impl Target {
    /// Whether a successful op on this target closes the rename draft opened
    /// on `draft_from`: only the row's own draft, never another row's.
    fn closes_draft(&self, draft_from: &str) -> bool {
        matches!(self, Self::Row(name) if name == draft_from)
    }
}

/// The `/categories` page.
#[component]
pub fn CategoriesPage() -> Element {
    let mut ai_collapsed = use_signal(|| true);
    let cycle = use_resource(get_cycle);
    let list = use_resource(list_categories);

    // Every successful write answers with the refreshed view, which then wins
    // over the initial read.
    let live = use_signal(|| None::<CategoriesDto>);
    let busy = use_signal(|| None::<Target>);
    let mut err = use_signal(|| None::<(Target, String)>);

    let mut new_name = use_signal(String::new);
    let mut new_colour = use_signal(|| DEFAULT_COLOUR.to_string());
    let mut renaming = use_signal(|| None::<(String, String)>);
    let mut merge_from = use_signal(String::new);
    let mut merge_into = use_signal(String::new);
    let mut preview = use_signal(|| None::<MergePreviewDto>);

    let run = use_callback(move |op: Op| {
        let mut live = live;
        let mut busy = busy;
        let target = op.target();
        busy.set(Some(target.clone()));
        err.set(None);
        spawn(async move {
            let res = match op {
                Op::Create { name, colour } => create_category(name, String::new(), colour).await,
                Op::Rename { from, to } => rename_category(from, to).await,
                Op::Colour { name, colour } => set_category_colour(name, colour).await,
                Op::Delete(name) => delete_category(name).await,
                Op::Merge { from, into } => merge_categories(from, into).await,
            };
            match res {
                Ok(view) => {
                    live.set(Some(view));
                    match &target {
                        Target::Create => new_name.set(String::new()),
                        Target::Merge => {
                            preview.set(None);
                            merge_from.set(String::new());
                            merge_into.set(String::new());
                        }
                        Target::Row(_) => {
                            let closes = renaming
                                .read()
                                .as_ref()
                                .is_some_and(|(f, _)| target.closes_draft(f));
                            if closes {
                                renaming.set(None);
                            }
                        }
                    }
                }
                Err(e) => err.set(Some((target, error_text(&e)))),
            }
            busy.set(None);
        });
    });

    let review_merge = move |_| {
        let (from, into) = (merge_from(), merge_into());
        let mut busy = busy;
        busy.set(Some(Target::Merge));
        err.set(None);
        spawn(async move {
            match merge_preview(from, into).await {
                Ok(p) => preview.set(Some(p)),
                Err(e) => err.set(Some((Target::Merge, error_text(&e)))),
            }
            busy.set(None);
        });
    };

    let view: Option<CategoriesDto> = live
        .read()
        .clone()
        .or_else(|| list.read().as_ref().and_then(|r| r.as_ref().ok()).cloned());
    let loading = list.read().is_none() && view.is_none();
    let load_error = matches!(&*list.read(), Some(Err(_))) && view.is_none();

    let topbar_date = match &*cycle.read() {
        Some(Ok(c)) if c.days > 0 => format!("{} · DAY {}/{}", c.label, c.day, c.days),
        _ => String::new(),
    };
    let busy_now = busy();
    let busy_any = busy_now.is_some();
    let err_for = |t: &Target| {
        err.read()
            .as_ref()
            .filter(|(target, _)| target == t)
            .map(|(_, m)| m.clone())
    };

    let body = match view {
        None => {
            let (message, hint) = if load_error {
                (
                    Some("Categories unavailable".to_string()),
                    Some("The ledger did not answer · reload to retry".to_string()),
                )
            } else {
                (None, None)
            };
            rsx! { Awaiting { label: "CATEGORIES".to_string(), loading, message, hint } }
        }
        Some(v) => {
            let total = v.rows.len().to_string();
            let used = v.rows.iter().filter(|r| r.uses > 0).count().to_string();
            let names: Vec<String> = v.rows.iter().map(|r| r.name.clone()).collect();
            let palette = v.palette.clone();
            let create_err = err_for(&Target::Create);
            let merge_err = err_for(&Target::Merge);
            let row_target = |name: &str| Target::Row(name.to_owned());
            rsx! {
                div { class: "cfg-top",
                    div {
                        div { class: "ttl", "Categories" }
                        div { class: "sum",
                            b { "{total}" }
                            " categories · "
                            b { "{used}" }
                            " carry history · renames and merges re-point every entry"
                        }
                    }
                }
                div { class: "cfg-grid",
                    section { class: "cfg-panel span-2",
                        div { class: "cfg-legend",
                            span { class: "gl", "◆" }
                            span { class: "nm", "Categories" }
                        }
                        if v.rows.is_empty() {
                            Awaiting { label: "CATEGORIES".to_string(), message: Some("No categories yet".to_string()) }
                        }
                        for r in v.rows.iter() {
                            CategoryRow {
                                key: "{r.slug}",
                                row: r.clone(),
                                palette: palette.clone(),
                                disabled: busy_any,
                                saving: busy_now.as_ref() == Some(&row_target(&r.name)),
                                error: err_for(&row_target(&r.name)),
                                renaming: renaming.read().as_ref().filter(|(f, _)| *f == r.name).map(|(_, d)| d.clone()),
                                on_rename_start: move |name: String| renaming.set(Some((name.clone(), name))),
                                on_rename_draft: move |d: String| {
                                    let from = renaming.read().as_ref().map(|(f, _)| f.clone());
                                    if let Some(from) = from {
                                        renaming.set(Some((from, d)));
                                    }
                                },
                                on_rename_cancel: move |()| renaming.set(None),
                                on_op: move |op: Op| run.call(op),
                            }
                        }
                    }

                    section { class: "cfg-panel",
                        div { class: "cfg-legend",
                            span { class: "gl", "+" }
                            span { class: "nm", "New category" }
                        }
                        div { class: "cfg-panel-sub", "A name and a colour from the palette. Set its cap on Budgets." }
                        if let Some(n) = v.notice.clone() {
                            div { class: "cfg-panel-sub", role: "status", "{n}" }
                        }
                        div { class: "cfg-row",
                            div { class: "rl", div { class: "lab", "Name" } }
                            div { class: "rc cfg-text",
                                input {
                                    r#type: "text",
                                    "aria-label": "New category name",
                                    placeholder: "e.g. Pets",
                                    value: "{new_name}",
                                    disabled: busy_any,
                                    oninput: move |e| new_name.set(e.value()),
                                }
                            }
                        }
                        div { class: "cfg-row",
                            div { class: "rl", div { class: "lab", "Colour" } }
                            div { class: "rc",
                                Swatches {
                                    palette: palette.clone(),
                                    value: new_colour(),
                                    disabled: busy_any,
                                    on_pick: move |c: String| new_colour.set(c),
                                }
                            }
                        }
                        div { class: "cfg-row",
                            div { class: "rl",
                                InlineStatus { pending: busy_now == Some(Target::Create), error: create_err }
                            }
                            div { class: "rc",
                                button {
                                    class: "cfg-reset",
                                    r#type: "button",
                                    disabled: busy_any || new_name().trim().is_empty(),
                                    onclick: move |_| run.call(Op::Create { name: new_name(), colour: new_colour() }),
                                    "+ Create"
                                }
                            }
                        }
                    }

                    section { class: "cfg-panel acc-coral",
                        div { class: "cfg-legend",
                            span { class: "gl", "⇢" }
                            span { class: "nm", "Merge" }
                        }
                        div { class: "cfg-panel-sub", "Fold one category into another. Every entry moves; the first one is removed; the target keeps its cap." }
                        div { class: "cfg-row",
                            div { class: "rl", div { class: "lab", "Merge" } }
                            div { class: "rc cfg-select",
                                select {
                                    "aria-label": "Category to merge away",
                                    disabled: busy_any || preview.read().is_some(),
                                    onchange: move |e| merge_from.set(e.value()),
                                    option { value: "", selected: merge_from().is_empty(), "— pick —" }
                                    for n in names.iter() {
                                        option { value: "{n}", selected: merge_from() == *n, "{n}" }
                                    }
                                }
                            }
                        }
                        div { class: "cfg-row",
                            div { class: "rl", div { class: "lab", "Into" } }
                            div { class: "rc cfg-select",
                                select {
                                    "aria-label": "Category to merge into",
                                    disabled: busy_any || preview.read().is_some(),
                                    onchange: move |e| merge_into.set(e.value()),
                                    option { value: "", selected: merge_into().is_empty(), "— pick —" }
                                    for n in names.iter() {
                                        option { value: "{n}", selected: merge_into() == *n, "{n}" }
                                    }
                                }
                            }
                        }
                        if let Some(p) = preview() {
                            MergeConfirm {
                                preview: p,
                                disabled: busy_any,
                                on_confirm: move |(from, into): (String, String)| run.call(Op::Merge { from, into }),
                                on_cancel: move |()| preview.set(None),
                            }
                        } else {
                            div { class: "cfg-row",
                                div { class: "rl" }
                                div { class: "rc",
                                    button {
                                        class: "cfg-reset",
                                        r#type: "button",
                                        disabled: busy_any || merge_from().is_empty() || merge_into().is_empty(),
                                        onclick: review_merge,
                                        "Review merge"
                                    }
                                }
                            }
                        }
                        InlineStatus { pending: busy_now == Some(Target::Merge), error: merge_err }
                    }
                }
            }
        }
    };

    rsx! {
        div { class: "pk", style: "height:100vh;min-height:0",
            ScannerBg {
                class: "pk-bg".to_string(),
                seed: 41,
                shapes: r#"[
                    { char: "3", cx: .9, cy: .8, scale: .36, style: "red", morph: "vein", live: true, fill: .46 },
                    { char: "6", cx: .1, cy: .26, scale: .26, style: "faint", morph: "blob", live: false, fill: .42 },
                    { char: "9", cx: .5, cy: .92, scale: .14, style: "wire", morph: "vein", live: false, fill: .3 }
                ]"#.to_string(),
            }
            div { class: "app-shell swap",
                AiPanel { collapsed: ai_collapsed(), on_toggle: move |()| ai_collapsed.toggle() }
                div { class: "app-main",
                    TopBar { active: "CATEGORIES".to_string(), date_text: topbar_date }
                    div { class: "app-scroll", "data-screen-label": "CATEGORIES",
                        div { class: "cfg-wrap", {body} }
                    }
                }
            }
        }
    }
}

/// One category: glyph in its colour, name (or the rename field), usage, the
/// colour swatches and the row actions.
#[component]
fn CategoryRow(
    row: CategoryRowDto,
    palette: Vec<String>,
    disabled: bool,
    saving: bool,
    error: Option<String>,
    renaming: Option<String>,
    on_rename_start: EventHandler<String>,
    on_rename_draft: EventHandler<String>,
    on_rename_cancel: EventHandler<()>,
    on_op: EventHandler<Op>,
) -> Element {
    let glyph_style = format!(
        "font-family:var(--font-display);font-size:var(--t-h3);margin-right:var(--s-2);color:var(--{})",
        row.colour
    );
    let uses = row.uses.to_string();
    let entries = if row.uses == 1 { "entry" } else { "entries" };
    let (name_colour, name_rename, name_delete, name_save) = (
        row.name.clone(),
        row.name.clone(),
        row.name.clone(),
        row.name.clone(),
    );
    rsx! {
        div { class: "cfg-row", "aria-busy": "{saving}",
            div { class: "rl",
                if let Some(draft) = renaming.clone() {
                    div { class: "cfg-text cfg-pref",
                        input {
                            r#type: "text",
                            "aria-label": "New name",
                            value: "{draft}",
                            disabled,
                            oninput: move |e| on_rename_draft.call(e.value()),
                        }
                        button {
                            class: "cfg-reset",
                            r#type: "button",
                            disabled: disabled || draft.trim().is_empty(),
                            onclick: move |_| on_op.call(Op::Rename { from: name_save.clone(), to: draft.clone() }),
                            "Save"
                        }
                        button {
                            class: "cfg-reset",
                            r#type: "button",
                            disabled,
                            onclick: move |_| on_rename_cancel.call(()),
                            "Cancel"
                        }
                    }
                } else {
                    div { class: "lab",
                        span { style: "{glyph_style}", "{row.glyph}" }
                        "{row.name}"
                    }
                }
                div { class: "hint",
                    span { class: "num", "{uses}" }
                    " {entries}"
                    if row.fixed { " · FIXED" }
                }
                InlineStatus { pending: saving, error }
            }
            div { class: "rc cfg-pref",
                Swatches {
                    palette,
                    value: row.colour.clone(),
                    disabled,
                    on_pick: move |c: String| on_op.call(Op::Colour { name: name_colour.clone(), colour: c }),
                }
                if renaming.is_none() {
                    button {
                        class: "cfg-reset",
                        r#type: "button",
                        disabled,
                        onclick: move |_| on_rename_start.call(name_rename.clone()),
                        "Rename"
                    }
                }
                if row.uses == 0 {
                    button {
                        class: "cfg-reset",
                        r#type: "button",
                        disabled,
                        onclick: move |_| on_op.call(Op::Delete(name_delete.clone())),
                        "Delete"
                    }
                }
            }
        }
    }
}

/// The colour picker: one swatch per palette token, filled with that token.
/// The chosen one carries a check mark; there is no free-form input.
#[component]
fn Swatches(
    palette: Vec<String>,
    value: String,
    disabled: bool,
    on_pick: EventHandler<String>,
) -> Element {
    rsx! {
        div { class: "cfg-pref", role: "radiogroup", "aria-label": "Colour",
            for t in palette.into_iter() {
                {
                    let on = t == value;
                    let style = format!(
                        "width:var(--s-4);height:var(--s-4);padding:0;border:0;border-radius:var(--r-1);\
                         background:var(--{t});color:var(--bg);font-size:var(--t-xs);line-height:var(--lh-tight);\
                         cursor:pointer"
                    );
                    let pick = t.clone();
                    rsx! {
                        button {
                            key: "{t}",
                            r#type: "button",
                            role: "radio",
                            title: "{t}",
                            "aria-label": "{t}",
                            "aria-checked": "{on}",
                            style: "{style}",
                            disabled,
                            onclick: move |_| on_pick.call(pick.clone()),
                            if on { "✓" }
                        }
                    }
                }
            }
        }
    }
}

/// The merge confirm step: states how many line items (and rows in total)
/// will be re-pointed before anything is written.
#[component]
fn MergeConfirm(
    preview: MergePreviewDto,
    disabled: bool,
    on_confirm: EventHandler<(String, String)>,
    on_cancel: EventHandler<()>,
) -> Element {
    let lines = preview.line_items.to_string();
    let records = preview.records.to_string();
    let noun = if preview.line_items == 1 {
        "line item"
    } else {
        "line items"
    };
    let (from, into) = (preview.from.clone(), preview.into.clone());
    rsx! {
        div { class: "cfg-row", role: "alertdialog", "aria-label": "Confirm merge",
            div { class: "rl",
                div { class: "lab",
                    "Merge {preview.from} into {preview.into}?"
                }
                div { class: "hint",
                    span { class: "num", "{lines}" }
                    " {noun} will be re-pointed ("
                    span { class: "num", "{records}" }
                    " entries in total). {preview.from} is removed. This cannot be undone."
                }
            }
            div { class: "rc cfg-pref",
                button {
                    class: "cfg-reset",
                    r#type: "button",
                    disabled,
                    onclick: move |_| on_confirm.call((from.clone(), into.clone())),
                    "Confirm merge"
                }
                button {
                    class: "cfg-reset",
                    r#type: "button",
                    disabled,
                    onclick: move |_| on_cancel.call(()),
                    "Cancel"
                }
            }
        }
    }
}

#[cfg(test)]
mod target_tests {
    use super::*;

    #[test]
    fn panel_targets_never_collide_with_a_category_name() {
        for name in ["+", "⇢"] {
            let t = Op::Delete(name.to_owned()).target();
            assert_eq!(t, Target::Row(name.to_owned()));
            assert_ne!(t, Target::Create);
            assert_ne!(t, Target::Merge);
        }
        let create = Op::Create {
            name: "+".to_owned(),
            colour: DEFAULT_COLOUR.to_owned(),
        };
        assert_eq!(create.target(), Target::Create);
    }

    #[test]
    fn a_row_op_closes_only_its_own_rename_draft() {
        let colour = Op::Colour {
            name: "Rent".to_owned(),
            colour: DEFAULT_COLOUR.to_owned(),
        };
        assert!(colour.target().closes_draft("Rent"));
        assert!(!colour.target().closes_draft("Groceries"));
        assert!(!Target::Create.closes_draft("Rent"));
        assert!(!Target::Merge.closes_draft("Rent"));
    }
}
