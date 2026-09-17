//! Budgets page (route `/budgets`). Faithful port of React `pages/Budgets.jsx`
//! (`BudgetPage`). The envelope console: tune category caps (channel thresholds)
//! against the monthly budget, watch the projection, inspect any channel. Reuses
//! the shared shell (AI panel left, inspector dock right).
//!
//! Shape preserved 1:1 with the JSX: the `.pk` root + `ScannerBg`, the
//! `.app-shell.swap` with the left `AiPanel`, the `.app-main` (TopBar + scroll),
//! the `.bud-wrap` (top header + controls, the four-card KPI band, the allocation
//! console, the ENVELOPES section + grid/rows), the docked or drawered
//! `BudgetInspector`, and the tweaks panel (replaced by the inline SORT/LAYOUT
//! buttons + a local toggle for the AI panel — the React tweaks panel is
//! authoring tooling, `@ds-adherence-ignore`, and is NOT ported).
//!
//! Data: where React fanned out `useGet` over the dead REST layer, this fans out
//! `use_resource` over the F3 `#[server]` fns (`get_cycle`, `get_categories`,
//! `get_budget_totals`, `get_allocation`, and — on demand for the inspector —
//! `get_category_detail` / `get_category_transactions`). Money crosses as exact
//! `Money` and renders through `chf`/`chf2` (Pilowlava by CSS); geometry (meter
//! widths, bar percentages, SVG positions) is computed in presentation `f64` CHF
//! via `Money::as_chf_f64()`, faithful to the JSX which worked in raw numbers.
//!
//! Cap edits: clicking the cap value (on a card, a row or in the inspector) opens
//! an inline CHF field. SAVE (or Enter) sends the raw text to the
//! `set_category_cap` server fn, which parses and validates it; the page shows
//! its message on failure and, on success, refetches the envelopes, totals,
//! allocation and inspector detail. The −/+ stepper is still a LOCAL preview (a
//! `use_signal<HashMap<String,i64>>` of centimes deltas on top of the fetched
//! cap) and is not persisted; a successful save clears that category's delta.
//!
//! JSX idioms → RSX (per the playbook):
//!   * `useState` → `use_signal`; `useEffect`(resize) → `use_effect` + `document::eval`.
//!   * `useGet(...)` → `use_resource(...)`; on-demand (`/categories/{name}`) →
//!     `use_resource` over the `sel` signal so it refetches when selection changes.
//!   * `.map(...)` → `for x in iter` (with `{ ... rsx!{} }` blocks for per-item locals).
//!   * conditional render `a ? b : c` → `if/else` in rsx, or a pre-computed `let`.
//!   * inline `style={{...}}` objects → `style: "k:v;k:v"` strings (token vars verbatim).
//!   * compound display strings hoisted to `let`s above `rsx!` (format-segment parser is strict).

use std::collections::HashMap;

use dioxus::prelude::*;
use phosk_core::money::Money;

use crate::components::prims::{Dot, ScannerBg};
use crate::components::shell::{AiPanel, TopBar};
use crate::components::states::{Awaiting, InlineStatus};
use crate::data::budgets::{
    cap_error_text, cap_input_text, get_allocation, get_budget_totals, get_categories,
    get_category_detail, get_category_transactions, set_category_cap, AllocationDto,
    BudgetTotalsDto, CategoryDetailDto, CategoryDto, CategoryTxnDto,
};
use crate::data::cycle::{get_cycle, CycleDto};
use crate::data::{chf, chf2};

// ── status mapper ────────────────────────────────────────────────────────────

/// `budgetStatus` key — the channel's tone/label class.
#[derive(Clone, Copy, PartialEq, Eq)]
enum StKey {
    None,
    Fixed,
    Unused,
    Over,
    WillExceed,
    Tight,
    OnTrack,
}

/// Pure status mapper: a category's `{cap, spent, proj, fixed}` → `{key, label, tone}`.
/// Faithful to React `budgetStatus`, but `budget` is the (possibly stepped) cap and
/// `proj` is passed in (the inspector overrides it with the detail-endpoint figure).
fn budget_status(
    spent: Money,
    proj: Money,
    cap: Money,
    fixed: bool,
) -> (StKey, String, &'static str) {
    if fixed {
        return (StKey::Fixed, "FIXED".to_string(), "blue");
    }
    if cap.centimes() == 0 {
        return (StKey::None, "NO CAP".to_string(), "blue");
    }
    if spent.centimes() == 0 {
        return (StKey::Unused, "UNUSED".to_string(), "blue");
    }
    let p = spent.as_chf_f64() / cap.as_chf_f64();
    let pj = proj.as_chf_f64() / cap.as_chf_f64();
    if p > 1.0 {
        let over = ((p - 1.0) * 100.0).round() as i64;
        return (StKey::Over, format!("{over}% OVER"), "coral");
    }
    if pj > 1.0 {
        return (StKey::WillExceed, "ON PACE OVER".to_string(), "coral");
    }
    if p >= 0.85 {
        return (StKey::Tight, "TIGHT".to_string(), "blue");
    }
    (StKey::OnTrack, "ON TRACK".to_string(), "blue")
}

/// The `pct` CSS modifier (`alert`/`warn`/`ok`) for a status key (React inline ternary).
fn pct_cls(key: StKey) -> &'static str {
    match key {
        StKey::Over => "alert",
        StKey::WillExceed | StKey::Tight => "warn",
        _ => "ok",
    }
}

/// The stepped cap of a category = its fetched budget + any local delta override.
fn cap_of(c: &CategoryDto, overrides: &HashMap<String, i64>) -> Money {
    let base = c.budget.centimes();
    let delta = overrides.get(&c.name).copied().unwrap_or(0);
    Money::from_centimes((base + delta).max(0))
}

/// `Math.abs(remaining)` as `Money` (centimes).
fn abs_money(m: Money) -> Money {
    Money::from_centimes(m.centimes().abs())
}

/// The status-0 (backend-offline) hint line states.jsx renders below the message
/// (`start it: cargo run -p phosk_api`). `errored` is the `#[server]` analog of a
/// failed request; `None` keeps the JSX behaviour of showing it only when offline.
const OFFLINE_HINT: &str = "start it: cargo run -p phosk_api";
fn offline_message(errored: bool) -> (Option<String>, Option<String>) {
    if errored {
        (
            Some("Backend offline".to_string()),
            Some(OFFLINE_HINT.to_string()),
        )
    } else {
        (None, None)
    }
}

// ════════════════════════════════════════════════════════════════════════════

/// The composed Phoskonomia budgets console.
#[component]
pub fn BudgetsPage() -> Element {
    // ---- backend data loads (was: useGet) ----
    let cycle = use_resource(get_cycle);
    let mut cats = use_resource(get_categories);
    let mut totals = use_resource(get_budget_totals);
    let mut alloc = use_resource(get_allocation);

    // ---- UI state (was: useState / useTweaks) ----
    // The React tweaks panel (authoring tooling) is not ported; its defaults are
    // mirrored as plain UI signals: layout cards|rows, sort order|used|over,
    // projection markers on, AI panel open.
    let mut ai_collapsed = use_signal(|| false);
    let mut sel = use_signal(|| Option::<String>::None);
    let mut drawer = use_signal(|| false);
    let mut env_layout = use_signal(|| "cards".to_string());
    let mut sort = use_signal(|| "order".to_string());
    let show_proj = use_signal(|| true);
    // Local cap-step overrides (centimes), keyed by category name: the −/+
    // stepper's unsaved preview. Saved edits go through `set_category_cap`.
    let mut cap_overrides = use_signal(HashMap::<String, i64>::new);

    // Responsive dock vs drawer: narrow (<1280px) drawers the inspector.
    let narrow = use_signal(|| false);
    use_effect(move || {
        let mut set_narrow = narrow;
        let mut eval = document::eval(
            r#"
            function on(){ dioxus.send(window.innerWidth < 1280 ? 1 : 0); }
            on();
            window.addEventListener("resize", on);
            "#,
        );
        spawn(async move {
            while let Ok(v) = eval.recv::<i32>().await {
                set_narrow.set(v == 1);
            }
        });
    });
    let dockable = !narrow();

    // Selected channel detail + recent txns — fetched on demand when something is
    // selected (React guarded the path; here we skip the read when None).
    let mut sel_detail = use_resource(move || async move {
        match sel() {
            Some(name) => Some(get_category_detail(name).await),
            None => None,
        }
    });

    // A cap was saved: drop that category's unsaved stepper delta (the saved
    // value is absolute) and refetch every read the cap feeds. `restart` keeps
    // the previous value until the new one lands, so nothing unmounts meanwhile.
    let on_cap_saved = use_callback(move |name: String| {
        cap_overrides.write().remove(&name);
        cats.restart();
        totals.restart();
        alloc.restart();
        sel_detail.restart();
    });
    let sel_txns = use_resource(move || async move {
        match sel() {
            Some(name) => Some(get_category_transactions(name).await),
            None => None,
        }
    });

    // ---- read the resources into local snapshots (clone out of the borrow) ----
    let c = match &*cycle.read() {
        Some(Ok(v)) => v.clone(),
        _ => empty_cycle(),
    };
    let cats_v = cats.read().as_ref().and_then(|r| r.as_ref().ok()).cloned();
    let totals_v = totals
        .read()
        .as_ref()
        .and_then(|r| r.as_ref().ok())
        .cloned();
    let alloc_v = alloc.read().as_ref().and_then(|r| r.as_ref().ok()).cloned();

    // loading flags (resource not yet resolved at all) — feeds <Awaiting loading/>.
    let cats_loading = cats.read().is_none();
    let alloc_loading = alloc.read().is_none();

    // resource-error flags: a resolved `Err(ServerFnError)` is the `#[server]`
    // analog of REST status 0 (backend offline) — feeds the status-driven message
    // + the `cargo run` hint into <Awaiting/>, faithful to states.jsx.
    let cats_err = matches!(&*cats.read(), Some(Err(_)));
    let alloc_err = matches!(&*alloc.read(), Some(Err(_)));

    let categories: Vec<CategoryDto> = cats_v.clone().unwrap_or_default();
    let cats_ready = cats_v.is_some() && !categories.is_empty();
    let overrides = cap_overrides();

    // ---- KPI figures: read straight off /budget/totals ----
    let totals_dto: Option<BudgetTotalsDto> = totals_v.clone();
    let days_left = c.days_left;

    // ---- sort the FETCHED category list per the tweak ----
    let sort_mode = sort();
    let ordered: Vec<CategoryDto> = {
        let mut arr = categories.clone();
        let rank = |c: &CategoryDto| -> i32 {
            let (k, _, _) = budget_status(c.spent, c.proj, cap_of(c, &overrides), c.fixed);
            match k {
                StKey::Over => 0,
                StKey::WillExceed => 1,
                _ => 2,
            }
        };
        let used = |c: &CategoryDto| -> f64 {
            let cap = cap_of(c, &overrides);
            if cap.centimes() > 0 {
                c.spent.as_chf_f64() / cap.as_chf_f64()
            } else {
                0.0
            }
        };
        if sort_mode == "used" {
            arr.sort_by(|a, b| {
                used(b)
                    .partial_cmp(&used(a))
                    .unwrap_or(std::cmp::Ordering::Equal)
            });
        } else if sort_mode == "over" {
            arr.sort_by(|a, b| {
                rank(a).cmp(&rank(b)).then_with(|| {
                    let pa = a.proj.as_chf_f64() / cap_of(a, &overrides).as_chf_f64().max(1.0);
                    let pb = b.proj.as_chf_f64() / cap_of(b, &overrides).as_chf_f64().max(1.0);
                    pb.partial_cmp(&pa).unwrap_or(std::cmp::Ordering::Equal)
                })
            });
        }
        arr
    };

    let sel_id = sel();
    let sel_cat: Option<CategoryDto> = sel_id
        .as_ref()
        .and_then(|name| categories.iter().find(|c| &c.name == name).cloned());
    let sel_cap = sel_cat
        .as_ref()
        .map_or(Money::ZERO, |c| cap_of(c, &overrides));
    let show_drawer = !dockable && drawer() && sel_cat.is_some();
    let cycle_label = c.label.clone();

    // ---- pre-computed compound display strings (format-segment parser is strict) ----
    const DASH: &str = "—";
    let budget = totals_dto.as_ref().map(|t| t.budget);
    let allocated = totals_dto.as_ref().map(|t| t.allocated);
    let spent = totals_dto.as_ref().map(|t| t.spent);
    let projected = totals_dto.as_ref().map(|t| t.projected);
    let remaining = totals_dto.as_ref().map(|t| t.remaining);
    let over_alloc = totals_dto.as_ref().map(|t| t.over_allocated);
    let unallocated = totals_dto.as_ref().map(|t| t.unallocated);
    let envelope_count: Option<u32> = totals_dto.as_ref().map(|t| t.envelope_count).or_else(|| {
        if cats_ready {
            Some(u32::try_from(categories.len()).unwrap_or(0))
        } else {
            None
        }
    });
    let spent_pct: Option<i64> = match (spent, budget) {
        (Some(s), Some(b)) if b.centimes() != 0 => {
            Some(((s.as_chf_f64() / b.as_chf_f64()) * 100.0).round() as i64)
        }
        _ => None,
    };
    let proj_over: Option<Money> = match (projected, budget) {
        (Some(p), Some(b)) => p.checked_sub(b).ok(),
        _ => None,
    };
    let proj_is_over = proj_over.is_some_and(|m| m.centimes() > 0);

    // header summary line bits
    let env_count_str = envelope_count.map_or(DASH.to_string(), |n| n.to_string());
    let budget_str = budget.map_or(DASH.to_string(), |m| chf(m, 0));
    let spent_pct_str = spent_pct.map_or(DASH.to_string(), |p| format!("{p}% spent"));
    let cats_count_str = if cats_ready {
        categories.len().to_string()
    } else {
        DASH.to_string()
    };

    // KPI band strings
    let kpi_budget = budget.map_or(DASH.to_string(), |m| chf(m, 0));
    let kpi_budget_sub = {
        let cyc = if c.days != 0 {
            format!("{}-day cycle · ", c.days)
        } else {
            String::new()
        };
        format!("{cyc}{days_left} days left")
    };
    let alloc_flag = if over_alloc.is_some_and(|m| m.centimes() > 0) {
        "OVER"
    } else {
        "OK"
    };
    let kpi_allocated = allocated.map_or(DASH.to_string(), |m| chf(m, 0));
    let kpi_allocated_sub = if over_alloc.is_some_and(|m| m.centimes() > 0) {
        format!("CHF {} over budget", chf(over_alloc.unwrap(), 0))
    } else if let Some(u) = unallocated {
        format!("CHF {} unallocated", chf(u, 0))
    } else {
        DASH.to_string()
    };
    let kpi_spent_pct = spent_pct.map_or(DASH.to_string(), |p| format!("{p}%"));
    let kpi_spent = spent.map_or(DASH.to_string(), |m| chf(m, 0));
    let kpi_spent_sub = remaining.map_or(DASH.to_string(), |m| {
        format!("CHF {} left of monthly budget", chf(m, 0))
    });
    let kpi_projected = projected.map_or(DASH.to_string(), |m| chf(m, 0));
    let kpi_projected_sub = match proj_over {
        Some(po) if po.centimes() > 0 => format!("CHF {} over at current pace", chf(po, 0)),
        Some(po) => format!("CHF {} under at current pace", chf(abs_money(po), 0)),
        None => DASH.to_string(),
    };
    let kpi_proj_big_style = if proj_is_over {
        "color:var(--neon);text-shadow:var(--glow-text)"
    } else {
        ""
    };
    let kpi_proj_sub_style = if proj_is_over {
        "color:var(--neon-hot)"
    } else {
        ""
    };

    // TopBar date label.
    let topbar_date = if c.days == 0 {
        String::new()
    } else {
        format!("{} · DAY {}/{}", c.label, c.day, c.days)
    };

    // budget as f64 CHF for the allocation domain geometry (None → 0).
    let budget_chf = budget.map_or(0.0, |m| m.as_chf_f64());

    rsx! {
        div { class: "pk", style: "height:100vh;min-height:0",
            // Budgets field — the molten signal sits LOWER-RIGHT, with a faint
            // envelope mass upper-left and a light wire scaffold across the top.
            ScannerBg {
                class: "pk-bg".to_string(),
                seed: 63,
                shapes: r#"[
                    { char: "4", cx: .8, cy: .77, scale: .44, style: "red", morph: "vein", live: true, fill: .5 },
                    { char: "8", cx: .14, cy: .3, scale: .31, style: "faint", morph: "blob", live: false, fill: .5 },
                    { char: "5", cx: .48, cy: .12, scale: .18, style: "wire", morph: "vein", live: false, fill: .36 },
                    { char: "2", cx: .92, cy: .18, scale: .14, style: "wire", morph: "vein", live: false, fill: .3 }
                ]"#.to_string(),
            }

            div { class: "app-shell swap",
                AiPanel { collapsed: ai_collapsed(), on_toggle: move |()| ai_collapsed.toggle() }

                div { class: "app-main",
                    TopBar { active: "BUDGETS".to_string(), date_text: topbar_date }
                    div { class: "app-scroll", "data-screen-label": "BUDGETS",
                        div { class: "bud-wrap",

                            // ===================== TOP HEADER + CONTROLS =====================
                            div { class: "bud-top",
                                div {
                                    div { class: "ttl", "Budgets" }
                                    div { class: "sum",
                                        b { "{env_count_str}" }
                                        " envelopes · "
                                        b { "CHF {budget_str}" }
                                        " monthly ·"
                                        span { class: "coral", " {spent_pct_str}" }
                                        if !c.label.is_empty() {
                                            " · {c.label}"
                                        }
                                        if c.days != 0 {
                                            " · day {c.day}/{c.days}"
                                        }
                                    }
                                }
                                div { class: "bud-controls",
                                    div { class: "modes",
                                        span { class: "mlbl", "SORT" }
                                        button {
                                            class: if sort() == "order" { "m on" } else { "m" },
                                            onclick: move |_| sort.set("order".to_string()),
                                            "ORDER"
                                        }
                                        button {
                                            class: if sort() == "used" { "m on" } else { "m" },
                                            onclick: move |_| sort.set("used".to_string()),
                                            "USED"
                                        }
                                        button {
                                            class: if sort() == "over" { "m on" } else { "m" },
                                            onclick: move |_| sort.set("over".to_string()),
                                            "OVER"
                                        }
                                    }
                                    div { class: "modes",
                                        button {
                                            class: if env_layout() == "cards" { "m on" } else { "m" },
                                            onclick: move |_| env_layout.set("cards".to_string()),
                                            "▦ CARDS"
                                        }
                                        button {
                                            class: if env_layout() == "rows" { "m on" } else { "m" },
                                            onclick: move |_| env_layout.set("rows".to_string()),
                                            "≡ ROWS"
                                        }
                                    }
                                }
                            }

                            // ===================== KPI BAND =====================
                            div { class: "bud-kpis",
                                div { class: "bud-kpi",
                                    div { class: "lbl",
                                        span { "MONTHLY BUDGET" }
                                        span { "{c.label}" }
                                    }
                                    div { class: "big",
                                        span { class: "cur", "CHF" }
                                        "{kpi_budget}"
                                    }
                                    div { class: "sub", "{kpi_budget_sub}" }
                                }
                                div { class: "bud-kpi blue",
                                    div { class: "lbl",
                                        span { "ALLOCATED" }
                                        span { "{alloc_flag}" }
                                    }
                                    div { class: "big",
                                        span { class: "cur", "CHF" }
                                        "{kpi_allocated}"
                                    }
                                    div { class: "sub", "{kpi_allocated_sub}" }
                                }
                                div { class: "bud-kpi accent",
                                    div { class: "lbl",
                                        span { "SPENT" }
                                        span { "{kpi_spent_pct}" }
                                    }
                                    div { class: "big",
                                        span { class: "cur", "CHF" }
                                        "{kpi_spent}"
                                    }
                                    div { class: "sub", "{kpi_spent_sub}" }
                                }
                                div { class: "bud-kpi",
                                    div { class: "lbl",
                                        span { "PROJECTED" }
                                        span { "{c.end_date}" }
                                    }
                                    div { class: "big", style: "{kpi_proj_big_style}",
                                        span { class: "cur", "CHF" }
                                        "{kpi_projected}"
                                    }
                                    div { class: "sub", style: "{kpi_proj_sub_style}", "{kpi_projected_sub}" }
                                }
                            }

                            // ===================== ALLOCATION CONSOLE =====================
                            AllocationBar {
                                alloc: alloc_v.clone(),
                                totals: totals_dto.clone(),
                                budget_chf,
                                loading: alloc_loading,
                                errored: alloc_err,
                            }

                            // ===================== ENVELOPES SECTION =====================
                            div { class: "bud-sec",
                                span { class: "lbl", "⊞ ENVELOPES" }
                                span { class: "ct", "{cats_count_str}" }
                                span { class: "rule" }
                                span { class: "meta", "CAP = THRESHOLD · ━ SIGNAL · ┊ PROJECTION · CLICK TO INSPECT" }
                            }

                            if !cats_ready {
                                {
                                    let (msg, hint) = offline_message(cats_err);
                                    rsx! {
                                        Awaiting {
                                            label: "ENVELOPES".to_string(),
                                            loading: cats_loading,
                                            message: msg,
                                            hint,
                                        }
                                    }
                                }
                            } else if env_layout() == "rows" {
                                div { class: "env-rows",
                                    for cc in ordered.iter() {
                                        {
                                            let cap = cap_of(cc, &overrides);
                                            rsx! {
                                                EnvRow {
                                                    key: "{cc.name}",
                                                    c: cc.clone(),
                                                    cap,
                                                    active: sel_id.as_deref() == Some(cc.name.as_str()),
                                                    show_proj: show_proj(),
                                                    on_select: move |name: String| {
                                                        sel.set(Some(name));
                                                        if !dockable { drawer.set(true); }
                                                    },
                                                    on_step: move |(name, d): (String, i64)| step(&mut cap_overrides, &name, d),
                                                    on_saved: on_cap_saved,
                                                }
                                            }
                                        }
                                    }
                                }
                            } else {
                                div { class: "env-grid",
                                    for cc in ordered.iter() {
                                        {
                                            let cap = cap_of(cc, &overrides);
                                            rsx! {
                                                EnvCard {
                                                    key: "{cc.name}",
                                                    c: cc.clone(),
                                                    cap,
                                                    days_left,
                                                    active: sel_id.as_deref() == Some(cc.name.as_str()),
                                                    show_proj: show_proj(),
                                                    on_select: move |name: String| {
                                                        sel.set(Some(name));
                                                        if !dockable { drawer.set(true); }
                                                    },
                                                    on_step: move |(name, d): (String, i64)| step(&mut cap_overrides, &name, d),
                                                    on_saved: on_cap_saved,
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }

                // docked inspector (wide layouts)
                if dockable {
                    BudgetInspector {
                        cat: sel_cat.clone(),
                        cap: sel_cap,
                        days_left,
                        cycle_label: cycle_label.clone(),
                        detail: detail_of(&sel_detail),
                        txns: txns_of(&sel_txns),
                        variant: None,
                        on_close: move |()| sel.set(None),
                        on_step: move |(name, d): (String, i64)| step(&mut cap_overrides, &name, d),
                        on_saved: on_cap_saved,
                    }
                }
            }

            // drawer inspector (narrow layouts)
            if show_drawer {
                div { class: "sig-drawer-back", onclick: move |_| drawer.set(false),
                    div { class: "sig-drawer", onclick: move |e: Event<MouseData>| e.stop_propagation(),
                        BudgetInspector {
                            cat: sel_cat.clone(),
                            cap: sel_cap,
                            days_left,
                            cycle_label: cycle_label.clone(),
                            detail: detail_of(&sel_detail),
                            txns: txns_of(&sel_txns),
                            variant: Some("drawer".to_string()),
                            on_close: move |()| drawer.set(false),
                            on_step: move |(name, d): (String, i64)| step(&mut cap_overrides, &name, d),
                            on_saved: on_cap_saved,
                        }
                    }
                }
            }
        }
    }
}

/// Apply a ±delta (in whole CHF, like the React `+10`/`-10`) to a category's local
/// cap override. The override is centimes; `d` is whole CHF so we scale by 100.
fn step(cap_overrides: &mut Signal<HashMap<String, i64>>, name: &str, d: i64) {
    let mut m = cap_overrides.write();
    let entry = m.entry(name.to_string()).or_insert(0);
    *entry += d * 100;
}

/// Read the on-demand `CategoryDetailDto` out of its resource (None until green).
fn detail_of(
    res: &Resource<Option<Result<CategoryDetailDto, ServerFnError>>>,
) -> Option<CategoryDetailDto> {
    match &*res.read() {
        Some(Some(Ok(d))) => Some(d.clone()),
        _ => None,
    }
}

/// Read the on-demand category txns out of its resource.
fn txns_of(
    res: &Resource<Option<Result<Vec<CategoryTxnDto>, ServerFnError>>>,
) -> Vec<CategoryTxnDto> {
    match &*res.read() {
        Some(Some(Ok(t))) => t.clone(),
        _ => Vec::new(),
    }
}

/// `CycleDto` empty default — the page renders "—"/empty labels until it lands.
fn empty_cycle() -> CycleDto {
    CycleDto {
        label: String::new(),
        day: 0,
        days: 0,
        days_left: 0,
        as_of: String::new(),
        start_date: String::new(),
        end_date: String::new(),
    }
}

// ── EnvMeter — the channel level meter (signal vs threshold/cap vs projection) ──

/// Faithful port of React `EnvMeter`. Geometry only (presentation `f64` CHF).
#[component]
fn EnvMeter(
    c: CategoryDto,
    cap: Money,
    #[props(default = true)] show_proj: bool,
    #[props(default = 9)] h: i32,
) -> Element {
    let spent = c.spent.as_chf_f64();
    let proj = c.proj.as_chf_f64();
    let cap_v = {
        let v = cap.as_chf_f64();
        if v > 0.0 {
            v
        } else if spent > 0.0 {
            spent
        } else {
            1.0
        }
    };
    let domain = {
        let d = cap_v.max(proj).max(spent) * 1.06;
        if d > 0.0 {
            d
        } else {
            1.0
        }
    };
    let pc = |v: f64| (v / domain) * 100.0;
    let over = spent > cap_v && cap_v > 0.0;
    // base colour by status against THIS cap.
    let (key, _, _) = budget_status(c.spent, c.proj, cap, c.fixed);
    let base_col = match key {
        StKey::Over => "var(--neon)",
        StKey::WillExceed | StKey::Tight => "var(--warn)",
        _ => "var(--indigo)",
    };
    let cap_part = spent.min(cap_v);
    let sig_w = pc(cap_part);
    let sig_style = format!("width:{sig_w}%;background:{base_col};box-shadow:0 0 7px {base_col}");
    let cap_cm = cap.centimes();
    let over_left = pc(cap_v);
    let over_w = pc(spent - cap_v);
    let thresh_left = pc(cap_v);
    let proj_left = pc(proj);
    let proj_hot = proj > cap_v;
    rsx! {
        div { class: "env-meter", style: "height:{h}px",
            div { class: "env-sig", style: "{sig_style}" }
            if over {
                div { class: "env-sig over", style: "left:{over_left}%;width:{over_w}%" }
            }
            if cap_cm > 0 {
                div { class: "env-thresh", style: "left:{thresh_left}%" }
            }
            if show_proj && cap_cm > 0 && proj > spent {
                div {
                    class: if proj_hot { "env-proj hot" } else { "env-proj" },
                    style: "left:{proj_left}%",
                }
            }
        }
    }
}

// ── CapStepper — tune the threshold ───────────────────────────────────────────

/// Faithful port of React `CapStepper`. `disabled` → the FIXED CHARGE label.
///
/// The −/+ buttons step the local preview (`on_step`). The cap value itself is a
/// button that swaps the control for an inline CHF field: SAVE or Enter sends
/// the raw text to `set_category_cap` (the server parses and validates it),
/// CANCEL or Escape drops the draft. While the save runs the field is locked
/// and an [`InlineStatus`] says so; a rejection keeps the field open with the
/// server's message. On success the field closes and `on_saved` gets the name
/// so the page can refetch. `inspector` switches to the larger dock styling
/// (no `CAP` label).
#[component]
fn CapStepper(
    name: String,
    value: Money,
    disabled: bool,
    on_step: EventHandler<i64>,
    on_saved: EventHandler<String>,
    #[props(default = false)] inspector: bool,
) -> Element {
    // Hooks first, unconditionally (rules of hooks), before any early return.
    let mut editing = use_signal(|| false);
    let mut draft = use_signal(String::new);
    let mut pending = use_signal(|| false);
    let mut error = use_signal(|| Option::<String>::None);

    let save_name = name.clone();
    let save = use_callback(move |()| {
        if pending() {
            return;
        }
        let name = save_name.clone();
        let amount = draft();
        pending.set(true);
        error.set(None);
        spawn(async move {
            let result = set_category_cap(name.clone(), amount).await;
            pending.set(false);
            match result {
                Ok(()) => {
                    editing.set(false);
                    on_saved.call(name);
                }
                Err(e) => error.set(Some(cap_error_text(&e))),
            }
        });
    });
    let mut cancel = move || {
        if !pending() {
            editing.set(false);
            error.set(None);
        }
    };

    if disabled {
        return rsx! { span { class: "cap-fixed", "FIXED CHARGE" } };
    }
    let wrap_cls = if inspector { "insp-step" } else { "cap-step" };

    if editing() {
        let busy = pending();
        let field_label = format!("New cap for {name}, in CHF");
        return rsx! {
            div {
                class: "cap-edit",
                onclick: move |e: Event<MouseData>| e.stop_propagation(),
                div { class: "{wrap_cls}",
                    if !inspector {
                        span { class: "cl", "CAP" }
                    }
                    label { class: "cap-in",
                        span { class: "cur", "CHF" }
                        input {
                            r#type: "text",
                            inputmode: "decimal",
                            autocomplete: "off",
                            spellcheck: "false",
                            aria_label: "{field_label}",
                            value: "{draft}",
                            disabled: busy,
                            onmounted: move |e: Event<MountedData>| async move {
                                let _ = e.set_focus(true).await;
                            },
                            oninput: move |e: Event<FormData>| {
                                draft.set(e.value());
                                // A stale rejection should not sit next to new text.
                                if error.peek().is_some() {
                                    error.set(None);
                                }
                            },
                            onkeydown: move |e: Event<KeyboardData>| {
                                // Keep keys away from the card's Enter/Space select.
                                e.stop_propagation();
                                match e.key() {
                                    Key::Enter => {
                                        e.prevent_default();
                                        save.call(());
                                    }
                                    Key::Escape => {
                                        e.prevent_default();
                                        cancel();
                                    }
                                    _ => {}
                                }
                            },
                        }
                    }
                    button {
                        class: "cap-act ok",
                        r#type: "button",
                        disabled: busy,
                        onclick: move |_| save.call(()),
                        "SAVE"
                    }
                    button {
                        class: "cap-act",
                        r#type: "button",
                        disabled: busy,
                        onclick: move |_| cancel(),
                        "CANCEL"
                    }
                }
                InlineStatus {
                    pending: busy,
                    error: error(),
                    pending_label: "Saving cap…".to_string(),
                }
            }
        };
    }

    let v = chf(value, 0);
    rsx! {
        div {
            class: "{wrap_cls}",
            onclick: move |e: Event<MouseData>| e.stop_propagation(),
            if !inspector {
                span { class: "cl", "CAP" }
            }
            button { class: "cs", title: "Lower cap CHF 10", onclick: move |_| on_step.call(-10), "−" }
            button {
                class: "cv cap-val",
                r#type: "button",
                title: "Edit cap",
                onclick: move |_| {
                    draft.set(cap_input_text(value));
                    error.set(None);
                    editing.set(true);
                },
                "CHF {v}"
            }
            button { class: "cs", title: "Raise cap CHF 10", onclick: move |_| on_step.call(10), "+" }
        }
    }
}

// ── EnvCard — the primary envelope unit ───────────────────────────────────────

/// Faithful port of React `EnvCard`.
#[component]
fn EnvCard(
    c: CategoryDto,
    cap: Money,
    days_left: u32,
    active: bool,
    show_proj: bool,
    on_select: EventHandler<String>,
    on_step: EventHandler<(String, i64)>,
    on_saved: EventHandler<String>,
) -> Element {
    let (key, label, tone) = budget_status(c.spent, c.proj, cap, c.fixed);
    let spent = c.spent;
    let proj = c.proj;
    let p = if cap.centimes() > 0 {
        spent.as_chf_f64() / cap.as_chf_f64()
    } else {
        0.0
    };
    let remaining = c.remaining;
    let rem_cm = remaining.centimes();
    let per_day = if rem_cm > 0 && days_left > 0 {
        Money::from_centimes(rem_cm / i64::from(days_left))
    } else {
        Money::ZERO
    };
    let items = c.items;

    let card_cls = {
        let mut s = format!("env osc-bkt {tone}");
        if active {
            s.push_str(" on");
        }
        s
    };
    let tag_cls = if c.fixed { "env-tag fix" } else { "env-tag" };
    let tag_txt = if c.fixed { "FIXED" } else { "VARIABLE" };
    let cap_txt = if cap.centimes() == 0 {
        DASH.to_string()
    } else {
        chf(cap, 0)
    };
    let pct_txt = if cap.centimes() > 0 {
        format!("{}%", (p * 100.0).round() as i64)
    } else {
        DASH.to_string()
    };
    let pct_amt_cls = format!("pct {}", pct_cls(key));
    let spent_txt = chf(spent, 0);
    let rem_lbl = if rem_cm >= 0 { "LEFT" } else { "OVER" };
    let rem_txt = chf(abs_money(remaining), 0);
    let proj_over_cap = proj.centimes() > cap.centimes();
    let proj_b_cls = if proj_over_cap { "coral" } else { "" };
    let proj_txt = chf(proj, 0);
    let per_day_txt = chf(per_day, 0);
    let items_word = if items == 1 { "ENTRY" } else { "ENTRIES" };
    let name = c.name.clone();
    let name2 = c.name.clone();
    let nm = c.name.clone();
    let cap_name = c.name.clone();

    rsx! {
        div {
            class: "{card_cls}",
            role: "button",
            tabindex: 0,
            onclick: move |_| on_select.call(name.clone()),
            onkeydown: move |e: Event<KeyboardData>| {
                if e.key() == Key::Enter || e.key() == Key::Character(" ".to_string()) {
                    e.prevent_default();
                    on_select.call(name2.clone());
                }
            },
            span { class: "osc-leg", "{label}" }
            div { class: "env-h",
                span { class: "env-nm", "{nm}" }
                span { class: "{tag_cls}", "{tag_txt}" }
            }
            div { class: "env-amt",
                span { class: "sp", "CHF {spent_txt}" }
                span { class: "cap", "/ {cap_txt}" }
                span { class: "{pct_amt_cls}", "{pct_txt}" }
            }
            EnvMeter { c: c.clone(), cap, show_proj }
            div { class: "env-meta",
                span {
                    i { "{rem_lbl}" }
                    " CHF {rem_txt}"
                }
                if show_proj && !c.fixed && cap.centimes() > 0 {
                    span {
                        i { "PROJ" }
                        " "
                        b { class: "{proj_b_cls}", "CHF {proj_txt}" }
                    }
                }
                if !c.fixed && rem_cm > 0 {
                    span {
                        i { "/DAY" }
                        " CHF {per_day_txt}"
                    }
                }
                if c.fixed && !c.next.is_empty() {
                    span {
                        i { "NEXT" }
                        " {c.next}"
                    }
                }
            }
            div { class: "env-foot",
                span { class: "env-items", "{items} {items_word}" }
                CapStepper {
                    name: cap_name,
                    value: cap,
                    disabled: c.fixed,
                    on_step: move |d: i64| on_step.call((c.name.clone(), d)),
                    on_saved,
                }
            }
        }
    }
}

// ── EnvRow — the compact row variant ──────────────────────────────────────────

/// Faithful port of React `EnvRow`.
#[component]
fn EnvRow(
    c: CategoryDto,
    cap: Money,
    active: bool,
    show_proj: bool,
    on_select: EventHandler<String>,
    on_step: EventHandler<(String, i64)>,
    on_saved: EventHandler<String>,
) -> Element {
    let (key, label, tone) = budget_status(c.spent, c.proj, cap, c.fixed);
    let spent = c.spent;
    let p = if cap.centimes() > 0 {
        spent.as_chf_f64() / cap.as_chf_f64()
    } else {
        0.0
    };

    let row_cls = if active { "envrow on" } else { "envrow" };
    let nm_cls = if c.fixed { "er-nm fix" } else { "er-nm" };
    let stat_cls = format!("er-stat {tone}");
    let amt_cap = if cap.centimes() == 0 {
        DASH.to_string()
    } else {
        chf(cap, 0)
    };
    let spent_txt = chf(spent, 0);
    let pct_cls_str = format!("er-pct {}", pct_cls(key));
    let pct_txt = if cap.centimes() > 0 {
        format!("{}%", (p * 100.0).round() as i64)
    } else {
        DASH.to_string()
    };
    let name = c.name.clone();
    let nm = c.name.clone();
    let cap_name = c.name.clone();

    rsx! {
        div { class: "{row_cls}", onclick: move |_| on_select.call(name.clone()),
            span { class: "{nm_cls}", "{nm}" }
            span { class: "{stat_cls}", "{label}" }
            div { class: "er-meter",
                EnvMeter { c: c.clone(), cap, show_proj, h: 7 }
            }
            span { class: "er-amt",
                "CHF "
                b { "{spent_txt}" }
                " "
                i { "/ {amt_cap}" }
            }
            span { class: "{pct_cls_str}", "{pct_txt}" }
            CapStepper {
                name: cap_name,
                value: cap,
                disabled: c.fixed,
                on_step: move |d: i64| on_step.call((c.name.clone(), d)),
                on_saved,
            }
        }
    }
}

// ── AllocationBar — channel-mix bar vs the monthly budget threshold ──────────

/// Faithful port of React `AllocationBar`. Geometry in presentation `f64` CHF;
/// the displayed figures (allocated / over-/unallocated) are exact `Money`.
#[component]
fn AllocationBar(
    alloc: Option<AllocationDto>,
    totals: Option<BudgetTotalsDto>,
    budget_chf: f64,
    loading: bool,
    #[props(default = false)] errored: bool,
) -> Element {
    let segments = alloc
        .as_ref()
        .map(|a| a.segments.clone())
        .unwrap_or_default();
    let advice = alloc.as_ref().map(|a| a.ai_advice.clone());

    if alloc.is_none() || segments.is_empty() {
        let (msg, hint) = offline_message(errored);
        return rsx! {
            Awaiting { label: "ALLOCATION".to_string(), loading, message: msg, hint }
        };
    }

    let t = totals.as_ref();
    let allocated = t.map(|t| t.allocated);
    let over_alloc = t.map(|t| t.over_allocated);
    let unallocated = t.map(|t| t.unallocated);

    // geometry only: a denominator to draw the bars/marker against.
    let cap_sum: f64 = segments.iter().map(|s| s.cap.as_chf_f64()).sum();
    let alloc_chf = allocated.map_or(cap_sum, |m| m.as_chf_f64());
    let domain = {
        let d = alloc_chf.max(budget_chf) * 1.02;
        if d > 0.0 {
            d
        } else {
            1.0
        }
    };

    // is-over: prefer overAllocated, else unallocated → not over, else unknown.
    let is_over: Option<bool> = match over_alloc {
        Some(m) => Some(m.centimes() > 0),
        None => unallocated.map(|_| false),
    };

    let flag_cls = if is_over == Some(true) {
        "alloc-flag over"
    } else {
        "alloc-flag ok"
    };
    let flag_txt = match is_over {
        None => DASH.to_string(),
        Some(true) => format!(
            "▲ CHF {} OVER-ALLOCATED",
            chf(over_alloc.unwrap_or(Money::ZERO), 0)
        ),
        Some(false) => format!(
            "✓ CHF {} UNALLOCATED",
            chf(unallocated.unwrap_or(Money::ZERO), 0)
        ),
    };

    let shades = [
        "rgba(143,125,255,.42)",
        "rgba(106,95,192,.5)",
        "rgba(120,104,210,.4)",
        "rgba(90,72,191,.5)",
    ];

    let budget_left = (budget_chf / domain) * 100.0;
    let budget_lbl = chf(Money::from_centimes((budget_chf * 100.0).round() as i64), 0);
    // foot figures
    let foot_budget = chf(Money::from_centimes((budget_chf * 100.0).round() as i64), 0);
    let alloc_b_cls = if is_over == Some(true) {
        "coral"
    } else {
        "blue"
    };
    let foot_alloc = allocated.map_or(DASH.to_string(), |m| format!("CHF {}", chf(m, 0)));
    let foot_env_count = segments.iter().filter(|s| s.cap.centimes() > 0).count();
    let advice_model = advice.as_ref().map_or("GEMMA4".to_string(), |a| {
        if a.model.is_empty() {
            "GEMMA4".to_string()
        } else {
            a.model.clone()
        }
    });
    let advice_text = advice.as_ref().map(|a| a.text.clone()).unwrap_or_default();
    let advice_line = format!("{advice_model} · {advice_text}");
    let has_advice = advice.as_ref().is_some_and(|a| !a.text.is_empty());

    rsx! {
        div { class: "alloc osc-bkt blue",
            span { class: "osc-leg", "ALLOCATION" }
            div { class: "alloc-h",
                span { class: "hud", "CHANNEL MIX · CAPS vs MONTHLY BUDGET" }
                span { class: "{flag_cls}", "{flag_txt}" }
            }
            div { class: "alloc-track",
                for (i , seg) in segments.iter().enumerate() {
                    {
                        let cap_chf = seg.cap.as_chf_f64();
                        if cap_chf <= 0.0 {
                            rsx! {}
                        } else {
                            // Presence test, not value test: `seg.share != null` in the JSX.
                            // A present `Some(0.0)` renders 0% width; only an absent share
                            // (`None`) falls back to cap/domain.
                            let w = seg.share.map_or((cap_chf / domain) * 100.0, |s| s * 100.0);
                            let bg = if seg.fixed { "rgba(143,125,255,.22)".to_string() } else { shades[i % shades.len()].to_string() };
                            let seg_cls = if seg.fixed { "alloc-seg fix" } else { "alloc-seg" };
                            let seg_style = format!("width:{w}%;background:{bg}");
                            let title = format!("{} · CHF {}", seg.name, chf(seg.cap, 0));
                            let first_word = seg.name.split(' ').next().unwrap_or("").to_string();
                            rsx! {
                                span { key: "{seg.name}", class: "{seg_cls}", style: "{seg_style}", title: "{title}",
                                    if w > 9.0 {
                                        span { class: "alloc-lbl", "{first_word}" }
                                    }
                                }
                            }
                        }
                    }
                }
                div { class: "alloc-thresh", style: "left:{budget_left}%",
                    span { class: "alloc-thresh-lbl", "BUDGET · CHF {budget_lbl}" }
                }
            }
            div { class: "alloc-foot",
                span {
                    i { "MONTHLY BUDGET" }
                    " CHF {foot_budget}"
                }
                span {
                    i { "ALLOCATED" }
                    " "
                    b { class: "{alloc_b_cls}", "{foot_alloc}" }
                }
                span {
                    i { "ENVELOPES" }
                    " {foot_env_count}"
                }
                span { class: "spacer" }
                if has_advice {
                    span { class: "alloc-ai",
                        Dot { tone: "blue".to_string(), size: 6 }
                        " {advice_line}"
                    }
                }
            }
        }
    }
}

// ── HistBars — six-cycle history mini-chart for the inspector ─────────────────

/// Faithful port of React `HistBars`: SVG bars vs a dashed cap line.
///
/// `proj` is `Option<f64>` mirroring the JSX `proj != null` test: when present
/// (even a zero/under-cap projection) it is appended as the last bar — the dashed
/// `var(--indigo-neon)` projection bar — exactly as React's `[...series, proj]`.
/// The value is never used as a presence gate, so a fixed/zero-proj channel still
/// draws its (possibly zero-height) projection bar.
#[component]
fn HistBars(
    hist: Vec<f64>,
    proj: Option<f64>,
    cap: f64,
    #[props(default = 300.0)] w: f64,
    #[props(default = 110.0)] h: f64,
) -> Element {
    let mut data = hist.clone();
    let has_proj = proj.is_some();
    if let Some(p) = proj {
        data.push(p);
    }
    if data.is_empty() {
        return rsx! {};
    }
    let labels = [
        "DEC", "JAN", "FEB", "MAR", "APR", "MAY", "JUN", "JUL", "AUG", "SEP", "OCT", "NOV",
    ];
    let cap_v = cap.max(0.0);
    let max = {
        let m = data.iter().copied().fold(cap_v, f64::max) * 1.12;
        if m > 0.0 {
            m
        } else {
            1.0
        }
    };
    let pad_b = 16.0;
    let pad_t = 8.0;
    let n = data.len() as f64;
    let bw = (w / n) * 0.56;
    let y = |v: f64| h - pad_b - (v / max) * (h - pad_t - pad_b);
    let cap_y = y(cap_v);
    let cap_text_y = cap_y - 4.0;
    let view_box = format!("0 0 {w} {h}");

    rsx! {
        svg { width: "100%", height: "{h}", view_box: "{view_box}", preserve_aspect_ratio: "none", style: "display:block",
            line {
                x1: "0",
                y1: "{cap_y}",
                x2: "{w}",
                y2: "{cap_y}",
                stroke: "var(--neon-dim)",
                stroke_width: "1",
                stroke_dasharray: "4 4",
                opacity: ".8",
            }
            text {
                x: "{w}",
                y: "{cap_text_y}",
                text_anchor: "end",
                fill: "var(--neon-dim)",
                font_size: "8",
                font_family: "var(--font-body)",
                letter_spacing: ".1em",
                "CAP"
            }
            for (i , v) in data.iter().enumerate() {
                {
                    let v = *v;
                    let x = (i as f64 + 0.5) * (w / n);
                    let is_proj = has_proj && i == data.len() - 1;
                    let col = if v > cap_v {
                        "var(--neon)"
                    } else if is_proj {
                        "rgba(143,125,255,.5)"
                    } else {
                        "rgba(132,116,222,.55)"
                    };
                    let ry = y(v);
                    let rh = h - pad_b - ry;
                    let rx = x - bw / 2.0;
                    let rect_stroke = if is_proj { "var(--indigo-neon)" } else { "none" };
                    let rect_dash = if is_proj { "3 2" } else { "0" };
                    let rect_style = if v > cap_v { "filter:drop-shadow(0 0 4px var(--neon))" } else { "" };
                    let txt_fill = if is_proj { "var(--indigo-neon)" } else { "var(--ink-3)" };
                    let label = labels.get(i).copied().unwrap_or("");
                    let txt_y = h - 4.0;
                    rsx! {
                        g { key: "{i}",
                            rect {
                                x: "{rx}",
                                y: "{ry}",
                                width: "{bw}",
                                height: "{rh}",
                                fill: "{col}",
                                stroke: "{rect_stroke}",
                                stroke_dasharray: "{rect_dash}",
                                style: "{rect_style}",
                            }
                            text {
                                x: "{x}",
                                y: "{txt_y}",
                                text_anchor: "middle",
                                fill: "{txt_fill}",
                                font_size: "7.5",
                                font_family: "var(--font-body)",
                                letter_spacing: ".08em",
                                "{label}"
                            }
                        }
                    }
                }
            }
        }
    }
}

// ── BudgetInspector (right dock) ──────────────────────────────────────────────

/// Faithful port of React `BudgetInspector` + `BudgetInspectorBody`. Renders the
/// empty state until a channel is selected, then the inspector body. `detail`/
/// `txns` are the on-demand reads (passed in by the page, fetched over the `sel`
/// signal — replacing React's self-fetching `useGet` in the body component).
#[component]
fn BudgetInspector(
    cat: Option<CategoryDto>,
    cap: Money,
    days_left: u32,
    cycle_label: String,
    detail: Option<CategoryDetailDto>,
    txns: Vec<CategoryTxnDto>,
    variant: Option<String>,
    on_close: EventHandler<()>,
    on_step: EventHandler<(String, i64)>,
    on_saved: EventHandler<String>,
) -> Element {
    let panel_cls = match &variant {
        Some(v) => format!("sig-panel bud-insp {v}"),
        None => "sig-panel bud-insp".to_string(),
    };

    let Some(cat) = cat else {
        return rsx! {
            aside { class: "{panel_cls}",
                div { class: "sig-empty",
                    span { class: "mk", "⊞" }
                    div { class: "tx",
                        "No envelope selected."
                        br {}
                        "Click any "
                        b { "budget channel" }
                        " to inspect its cycle history, projection and AI cap guidance."
                    }
                }
            }
        };
    };

    // Prefer detail-endpoint fields; fall back to the list record fields.
    let spent = cat.spent;
    let proj = detail.as_ref().map_or(cat.proj, |d| d.projected_spend);
    let hist = cat.hist.clone();
    let (key, _, tone) = budget_status(spent, proj, cap, cat.fixed);
    let p = if cap.centimes() > 0 {
        spent.as_chf_f64() / cap.as_chf_f64()
    } else {
        0.0
    };
    let remaining = cat.remaining;
    let rem_cm = remaining.centimes();
    let hist_avg = detail.as_ref().map(|d| d.hist_avg);
    let guidance = detail
        .as_ref()
        .map(|d| d.guidance.clone())
        .filter(|g| !g.is_empty())
        .unwrap_or_else(|| cat.note.clone());

    // header
    let kls_txt = if cat.fixed {
        "⊞ BUDGET CHANNEL · FIXED"
    } else {
        "⊞ BUDGET CHANNEL · VARIABLE"
    };
    let name = cat.name.clone();

    // delta
    let big_cls = if key == StKey::Over {
        "big up"
    } else {
        "big down"
    };
    let big_style = if tone == "blue" && key != StKey::Over {
        "color:var(--ink)"
    } else {
        ""
    };
    let big_txt = if cap.centimes() > 0 {
        format!("{}%", (p * 100.0).round() as i64)
    } else {
        DASH.to_string()
    };
    let vs_txt = format!("of cap used · {days_left} days left");

    // stats
    let cap_txt = chf(cap, 0);
    let spent_txt = chf(spent, 0);
    let rem_k = if rem_cm >= 0 { "Remaining" } else { "Over by" };
    let rem_v_style = if rem_cm < 0 {
        "color:var(--neon)"
    } else {
        "color:var(--ink)"
    };
    let rem_v_txt = chf(abs_money(remaining), 0);
    let proj_v_style = if proj.centimes() > cap.centimes() {
        "color:var(--neon)"
    } else {
        "color:var(--ink)"
    };
    let proj_v_txt = chf(proj, 0);
    let items_txt = cat.items.to_string();
    let hist_len = hist.len();
    let hist_avg_k = format!("{hist_len}-cyc avg");
    let hist_avg_v = hist_avg.map_or(DASH.to_string(), |m| format!("CHF {}", chf(m, 0)));

    // chart inputs (presentation f64). React always passes `proj` (line 279), so
    // the dashed indigo-neon projection bar is ALWAYS the last bar — gate on
    // presence (`Some`), never on value, so a fixed/zero-proj channel still draws it.
    let hist_f64 = hist.clone();
    let proj_bar = Some(proj.as_chf_f64());
    let cap_f64 = cap.as_chf_f64();
    let axis_cycles = format!("{hist_len} CYCLES");
    let axis_proj = format!("PROJECTED · {cycle_label}");

    // recent txns header
    let recent_h = format!("This cycle · {}", cat.name);
    let recent_txns: Vec<CategoryTxnDto> = txns.iter().take(5).cloned().collect();

    // footer guidance composition
    let over_cap_amt = detail.as_ref().map(|d| d.over_cap_amount);
    let over_cap_tail = match over_cap_amt {
        Some(m) if m.centimes() > 0 => format!("Over cap by CHF {}. ", chf(m, 0)),
        _ => String::new(),
    };
    let foot_kind = match key {
        StKey::Over | StKey::WillExceed => 0,
        StKey::Fixed => 1,
        _ => 2,
    };
    let name_for_step = cat.name.clone();
    let step_name = cat.name.clone();

    rsx! {
        aside { class: "{panel_cls}",
            div { class: "sig-head",
                div { class: "kls", "{kls_txt}" }
                div { class: "nm", "{cat.name}" }
                div { class: "ds", "{guidance}" }
                span { class: "x", title: "Close", onclick: move |_| on_close.call(()), "✕" }
            }

            div { class: "sig-delta",
                span { class: "{big_cls}", style: "{big_style}", "{big_txt}" }
                span { class: "vs", "{vs_txt}" }
            }

            if !hist.is_empty() {
                div { class: "sig-chart",
                    HistBars { hist: hist_f64, proj: proj_bar, cap: cap_f64 }
                    div { class: "axis",
                        span { "{axis_cycles}" }
                        span { "{axis_proj}" }
                    }
                }
            }

            div { class: "sig-stats",
                div { class: "st",
                    div { class: "k", "Cap" }
                    div { class: "v", "CHF {cap_txt}" }
                }
                div { class: "st",
                    div { class: "k", "Spent" }
                    div { class: "v coral", "CHF {spent_txt}" }
                }
                div { class: "st",
                    div { class: "k", "{rem_k}" }
                    div { class: "v", style: "{rem_v_style}", "CHF {rem_v_txt}" }
                }
                div { class: "st",
                    div { class: "k", "Projected" }
                    div { class: "v", style: "{proj_v_style}", "CHF {proj_v_txt}" }
                }
                div { class: "st",
                    div { class: "k", "Entries" }
                    div { class: "v", "{items_txt}" }
                }
                div { class: "st",
                    div { class: "k", "{hist_avg_k}" }
                    div { class: "v", style: "font-size:16px", "{hist_avg_v}" }
                }
            }

            if !recent_txns.is_empty() {
                div { class: "sig-recent",
                    div { class: "h", "{recent_h}" }
                    for t in recent_txns.iter() {
                        {
                            let amt = chf2(t.amount);
                            rsx! {
                                div { class: "sig-occ", key: "{t.id}",
                                    span { class: "dt", "{t.date}" }
                                    span { class: "no", "{t.shop}" }
                                    span { class: "pr", "CHF {amt}" }
                                }
                            }
                        }
                    }
                }
            }

            if !cat.fixed {
                div { class: "insp-cap",
                    div { class: "h", "Tune cap" }
                    // Keyed by name: a new selection starts with a closed editor.
                    CapStepper {
                        key: "{name_for_step}",
                        name: name_for_step.clone(),
                        value: cap,
                        disabled: false,
                        inspector: true,
                        on_step: move |d: i64| on_step.call((step_name.clone(), d)),
                        on_saved,
                    }
                }
            }

            div { class: "sig-foot",
                div { class: "tx",
                    match foot_kind {
                        0 => rsx! {
                            b { class: "coral", "⚠ {name}" }
                            " — {guidance} {over_cap_tail}Raise the cap or trim spend before cycle close."
                        },
                        1 => rsx! {
                            "Fixed charge. {guidance} Not tunable from here."
                        },
                        _ => rsx! {
                            "{guidance} The AI keeps this channel under watch and flags drift early."
                        },
                    }
                }
            }
        }
    }
}

/// Em-dash default used in pre-computed strings.
const DASH: &str = "—";
