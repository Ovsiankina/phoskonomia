//! Dashboard page (route `/dashboard`). Faithful port of React `pages/Dashboard.jsx`
//! (`DashFull`). This is the GOLD-STANDARD reference the other six page agents copy.
//!
//! Shape preserved 1:1 with the JSX: the `.pk` root + `ScannerBg`, the
//! `.app-shell.swap` with the left `AiPanel`, the `.app-main` (TopBar + scroll),
//! the CONSOLE HERO (`c-dock` glass + `c-main` screen/channels/watch), the
//! item-signal `SignalStrip`, the scroll seam, the TERMINAL grid (left KPI rail,
//! mid category-budgets + recent, right alerts/recurring/insight), and the docked
//! or drawered `SignalPanel`.
//!
//! Data: where React fanned out `useGet` over the dead REST layer, this fans out
//! `use_resource` over the F3 `#[server]` fns. Each `use_resource` is matched as
//! `Some(Ok(_))` (green) / `Some(Err(_))` (awaiting backend) / `None` (loading) —
//! the exact tri-state React's `ok200(status)` / `loading` distinguished. Money
//! crosses as exact `Money`; presentation series (chart/spark) are `f64` CHF.
//!
//! JSX idioms → RSX:
//!   * `useState` → `use_signal`; `useEffect`(resize) → `use_effect` + `document::eval`.
//!   * `useGet(...)` → `use_resource(move || server_fn())`; on-demand fetch
//!     (`sel`) → `use_resource` over the signal so it refetches when `sel` changes.
//!   * `.map(...)` → `for x in iter` (with `{ ... rsx!{} }` blocks for per-item locals).
//!   * conditional render `a ? b : c` → `if/else` in rsx, or a pre-computed `match`.
//!   * inline `style={{...}}` objects → `style: "k:v;k:v"` strings (token vars verbatim).
//!   * SVG/charts are owned by F2 primitives (`PhoskChart`, `SavingsDial`, `Spark`).

use dioxus::prelude::*;
use phosk_core::money::Money;

use crate::components::comps::{Alert, AlertAction, AlertItem, Cat, CatRows, Rec, RecRow, Txn, TxnTape};
use crate::components::prims::{pct_tone, Dot, PhoskChart, SavingsDial, ScannerBg, Spark};
use crate::components::shell::{Sig, SigOcc, SignalPanel, SignalStrip, TopBar};
use crate::components::states::{Awaiting, InlineStatus};
use crate::data::budgets::{get_categories, CategoryDto};
use crate::data::dashboard::{
    act_on_alert, get_alerts, get_insight, get_recurring, get_spend_series, get_top_shops,
    get_totals, AlertDto, RecurringDto, ALERT_ACTION_FAILED,
};
use crate::data::signals::{
    dismiss_signal, get_signal, get_signal_candidates, get_signals, track_signal, SignalDetailDto,
    SignalDto,
};
use crate::data::transactions::{list_transactions, TransactionDto, TxnFilter};
use crate::data::{chf, chf2, cycle::get_cycle, cycle::CycleDto};
use crate::Route;

// ── tiny presentation helpers (faithful to the JSX's chf0/pct/ok200) ─────────

/// Whole-CHF (0 decimals), the big console numerals (`chf0` in the JSX).
fn chf0(m: Money) -> String {
    chf(m, 0)
}

/// A 0–1 ratio as a rounded percent string, em-dash when absent (`pct` in JSX).
fn pct_str(r: Option<f64>) -> String {
    match r {
        Some(v) if v.is_finite() => format!("{}%", (v * 100.0).round() as i64),
        _ => "—".to_string(),
    }
}

/// `Money` series → presentation `f64` CHF, for the chart/spark primitives.
fn money_chf_series(v: &[Money]) -> Vec<f64> {
    v.iter().map(|m| m.as_chf_f64()).collect()
}

// ── DTO → F2 component-struct mappers ────────────────────────────────────────

/// `CategoryDto` → the `Cat` row consumed by `CatRows`.
fn cat_of(c: &CategoryDto) -> Cat {
    Cat {
        name: c.name.clone(),
        spent: c.spent,
        budget: c.budget,
        items: i64::from(c.items),
        fixed: c.fixed,
    }
}

/// `TransactionDto` → the `Txn` row consumed by `TxnTape`.
fn txn_of(t: &TransactionDto) -> Txn {
    Txn {
        id: t.id.clone(),
        date: t.date.clone(),
        shop: t.shop.clone(),
        category: t.category.clone(),
        amount: t.amount,
        flag: t.low_conf_count > 0,
        fixed: t.fixed,
    }
}

/// `RecurringDto` → the `Rec` row consumed by `RecRow`. `RecurringDto` carries
/// only a `days_until`, so the status tone (`due`/`soon`/clean) is derived from
/// it (faithful to how the React recurring rows coloured themselves).
fn rec_of(r: &RecurringDto) -> Rec {
    let status = if r.days_until <= 0 {
        "due"
    } else if r.days_until <= 7 {
        "soon"
    } else {
        "clean"
    };
    Rec {
        name: r.name.clone(),
        cycle: "MONTHLY".to_string(),
        amount: r.amount,
        status: status.to_string(),
        next: r.next.clone(),
        src: "user".to_string(),
    }
}

/// The text to show for a failed signal track/dismiss press: the server's own
/// message (`track_signal_with`/`dismiss_signal_with` already keep it
/// page-safe — see their tests) or a generic line if the call failed before
/// the server could answer.
fn signal_action_error_text(err: &ServerFnError) -> String {
    match err {
        ServerFnError::ServerError { message, .. } if !message.is_empty() => message.clone(),
        _ => "could not reach the server, try again".to_string(),
    }
}

/// `AlertDto` → the `Alert` row consumed by `AlertItem`. The per-tone action
/// buttons (`VIEW` / `RAISE CAP` / `DISMISS` / `SNOOZE` / `MARK PAID`, each with
/// its backend verb `kind`) carry through from the read so the `.acts` button
/// row renders; the page wires `on_action` (the React DISMISS/SNOOZE/APPLY/VIEW
/// round-trip), sending each button's `kind`, never its label.
fn alert_of(a: &AlertDto) -> Alert {
    let tone = if a.tone == "info" {
        "llm".to_string()
    } else {
        a.tone.clone()
    };
    Alert {
        id: a.id.clone(),
        tone,
        tag: a.tag.clone(),
        head: a.head.clone(),
        body: a.body.clone(),
        actions: a
            .actions
            .iter()
            .map(|act| AlertAction {
                label: act.label.clone(),
                kind: act.kind.clone(),
            })
            .collect(),
    }
}

/// `SignalDto` → the `Sig` the strip/cards render. The list view does not need
/// `avg_unit`/`recent`/`conf`, so they are zero/empty; the selected-signal panel
/// uses [`sig_of_detail`] which fills them.
fn sig_of_list(s: &SignalDto) -> Sig {
    Sig {
        id: s.id.clone(),
        label: s.label.clone(),
        parent: s.parent.clone(),
        desc: s.desc.clone(),
        delta_pct: if s.candidate {
            None
        } else {
            Some(i64::from(s.delta_pct))
        },
        series: s.series.clone(),
        cycle_qty: s.cycle_qty,
        unit: s.unit.clone(),
        cycle_spend: s.cycle_spend,
        avg_unit: Money::ZERO,
        txns: i64::from(s.txns),
        since: s.since.clone(),
        conf: None,
        recent: Vec::new(),
        candidate: s.candidate,
    }
}

/// `SignalDetailDto` → the fully-populated `Sig` the inspector panel renders
/// (recent occurrences, avg/unit, confidence).
fn sig_of_detail(d: &SignalDetailDto) -> Sig {
    let s = &d.signal;
    let avg_unit = if s.cycle_qty > 0.0 {
        Money::from_centimes(((s.cycle_spend.centimes() as f64) / s.cycle_qty).round() as i64)
    } else {
        Money::ZERO
    };
    Sig {
        id: s.id.clone(),
        label: s.label.clone(),
        parent: s.parent.clone(),
        desc: s.desc.clone(),
        delta_pct: if s.candidate {
            None
        } else {
            Some(i64::from(s.delta_pct))
        },
        series: s.series.clone(),
        cycle_qty: s.cycle_qty,
        unit: s.unit.clone(),
        cycle_spend: s.cycle_spend,
        avg_unit,
        txns: i64::from(d.all_time_txns),
        since: s.since.clone(),
        conf: None,
        recent: d
            .recent
            .iter()
            .map(|o| SigOcc {
                date: o.date.clone(),
                note: s.label.clone(),
                shop: o.shop.clone(),
                total: o.amount,
            })
            .collect(),
        candidate: s.candidate,
    }
}

// ── small DTO fallbacks (so reads compose without unwrapping a borrow) ───────

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

// ════════════════════════════════════════════════════════════════════════════

/// The composed Phoskonomia dashboard.
#[component]
pub fn DashboardPage() -> Element {
    // ---- backend data loads (was: useGet) ----
    let cycle = use_resource(get_cycle);
    let totals = use_resource(get_totals);
    let series = use_resource(get_spend_series);
    let shops = use_resource(get_top_shops);
    let insight = use_resource(get_insight);
    let cats = use_resource(get_categories);
    let txns = use_resource(move || list_transactions(TxnFilter::default()));
    let recurring = use_resource(get_recurring);
    let alerts = use_resource(get_alerts);
    let signals = use_resource(get_signals);
    let candidates = use_resource(get_signal_candidates);

    // ---- UI state (was: useState) ----
    let mut ai_collapsed = use_signal(|| false);
    let mut sel = use_signal(|| Option::<String>::None);
    let mut drawer_sig = use_signal(|| false);
    // The fixed, page-safe message from a rejected alert-action press; cleared
    // on the next attempt or once it succeeds.
    let alert_error = use_signal(|| Option::<String>::None);
    // Same, for a rejected signal track/dismiss press from the signal panel.
    let signal_error = use_signal(|| Option::<String>::None);
    // Programmatic navigation for the alert VIEW deep-link (React's navigate()).
    let nav = use_navigator();

    // Responsive dock vs drawer: narrow (<1280px) drawers the signal panel.
    // Faithful to React's window.innerWidth resize listener.
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

    // Selected item-signal detail — fetched on demand; only when something is
    // selected (React guarded the path; here we simply skip the read when None).
    let sig_detail = use_resource(move || async move {
        match sel() {
            Some(id) => Some(get_signal(id).await),
            None => None,
        }
    });

    // ---- read the resources into local snapshots (clone out of the borrow) ----
    let c = match &*cycle.read() {
        Some(Ok(v)) => v.clone(),
        _ => empty_cycle(),
    };
    let totals_v = totals
        .read()
        .as_ref()
        .and_then(|r| r.as_ref().ok())
        .cloned();
    let series_v = series
        .read()
        .as_ref()
        .and_then(|r| r.as_ref().ok())
        .cloned();
    let shops_v = shops.read().as_ref().and_then(|r| r.as_ref().ok()).cloned();
    let insight_v = insight
        .read()
        .as_ref()
        .and_then(|r| r.as_ref().ok())
        .cloned();
    let cats_v = cats.read().as_ref().and_then(|r| r.as_ref().ok()).cloned();
    let txns_v = txns.read().as_ref().and_then(|r| r.as_ref().ok()).cloned();
    let recurring_v = recurring
        .read()
        .as_ref()
        .and_then(|r| r.as_ref().ok())
        .cloned();
    let alerts_v = alerts
        .read()
        .as_ref()
        .and_then(|r| r.as_ref().ok())
        .cloned();
    let signals_v = signals
        .read()
        .as_ref()
        .and_then(|r| r.as_ref().ok())
        .cloned();
    let candidates_v = candidates
        .read()
        .as_ref()
        .and_then(|r| r.as_ref().ok())
        .cloned();

    // loading flags (resource not yet resolved at all) — feeds <Awaiting loading/>.
    let totals_loading = totals.read().is_none();
    let series_loading = series.read().is_none();
    let shops_loading = shops.read().is_none();
    let cats_loading = cats.read().is_none();
    let txns_loading = txns.read().is_none();
    let recurring_loading = recurring.read().is_none();
    let alerts_loading = alerts.read().is_none();
    let insight_loading = insight.read().is_none();

    // tracked signals + AI candidates (candidates carry candidate=true already).
    let signal_list: Vec<Sig> = {
        let mut v: Vec<Sig> = signals_v
            .as_ref()
            .map(|l| l.iter().map(sig_of_list).collect())
            .unwrap_or_default();
        if let Some(cs) = &candidates_v {
            v.extend(cs.iter().map(sig_of_list));
        }
        v
    };

    // selected signal detail → Sig for the panel.
    let sig_obj: Option<Sig> = match &*sig_detail.read() {
        Some(Some(Ok(d))) => Some(sig_of_detail(d)),
        _ => None,
    };
    let sel_id = sel();

    // upcoming recurring charges, soonest first (hero NEXT dock).
    let next_due: Vec<RecurringDto> = match &recurring_v {
        Some(r) => {
            let mut nd: Vec<RecurringDto> = r.recurring.clone();
            nd.sort_by_key(|x| x.days_until);
            nd.into_iter().take(3).collect()
        }
        None => Vec::new(),
    };
    let recurring_ok = recurring_v.is_some();
    let recurring_list = recurring_v
        .as_ref()
        .map(|r| r.recurring.clone())
        .unwrap_or_default();
    let monthly_total = recurring_v.as_ref().map(|r| r.monthly_total);

    // WATCH strip — top 3 alerts.
    let alert_list: Vec<AlertDto> = alerts_v.clone().unwrap_or_default();
    let watch_items: Vec<AlertDto> = alert_list.iter().take(3).cloned().collect();

    // category lists.
    let categories: Vec<CategoryDto> = cats_v.clone().unwrap_or_default();
    let channels: Vec<CategoryDto> = categories.iter().take(5).cloned().collect();
    let cat_rows: Vec<Cat> = categories.iter().map(cat_of).collect();

    // recent txns.
    let txn_list: Vec<TransactionDto> = txns_v
        .as_ref()
        .map(|t| t.transactions.clone())
        .unwrap_or_default();
    let txn_rows: Vec<Txn> = txn_list.iter().take(9).map(txn_of).collect();
    let txn_count = txn_list.len();

    // Pre-formatted strings. The rsx `"{...}"` format-segment parser only accepts
    // a bare ident / simple expression — it rejects `{ if .. { "+" } else { "" } }`
    // and `map_or("—".to_string(), ..)` (string literals + closures inside a
    // segment). So every compound / dash-defaulted display string is built here.
    const DASH: &str = "—";
    let vs_last_str = totals_v.as_ref().map(|t| {
        let sign = if t.vs_last_cycle_pct > 0 { "+" } else { "" };
        format!("{sign}{}%", t.vs_last_cycle_pct)
    });
    // Terminal LEFT-rail KPI numerals + sub-lines.
    let kpi_budget = totals_v
        .as_ref()
        .map_or(DASH.to_string(), |t| chf0(t.budget));
    let kpi_budget_sub = if c.days == 0 {
        DASH.to_string()
    } else {
        format!("{}-day cycle · day {}", c.days, c.day)
    };
    let kpi_spent_pct = totals_v.as_ref().map_or(DASH.to_string(), |t| {
        pct_str(Some(f64::from(t.spent_pct) / 100.0))
    });
    let kpi_spent = totals_v
        .as_ref()
        .map_or(DASH.to_string(), |t| chf0(t.spent));
    let kpi_spent_sub = totals_v
        .as_ref()
        .map_or(DASH.to_string(), |t| format!("CHF {}", chf2(t.spent)));
    let kpi_remaining = totals_v
        .as_ref()
        .map_or(DASH.to_string(), |t| chf0(t.remaining));
    let kpi_remaining_sub = totals_v.as_ref().map_or(DASH.to_string(), |t| {
        format!(
            "CHF {}/day to stay on budget",
            chf0(t.per_day_to_stay_on_budget)
        )
    });
    // Header counts + insight model + monthly run-rate.
    let cats_count_str = if cats_v.is_some() {
        categories.len().to_string()
    } else {
        String::new()
    };
    let alerts_count_str = if alerts_v.is_some() {
        alert_list.len().to_string()
    } else {
        DASH.to_string()
    };
    let monthly_total_str =
        monthly_total.map_or(DASH.to_string(), |m| format!("CHF {}/MO", chf0(m)));
    let insight_model = insight_v
        .as_ref()
        .map_or("GEMMA4".to_string(), |i| i.model.clone());
    // Spend-trace HUD numerals.
    let trace_hud = totals_v.as_ref().map_or(DASH.to_string(), |t| {
        format!(
            "CHF {} / {} · {}",
            chf0(t.spent),
            chf0(t.budget),
            pct_str(Some(f64::from(t.spent_pct) / 100.0))
        )
    });

    // cycle convenience.
    let cycle_label = c.label.clone();
    let days_left = c.days_left;
    let topbar_date = if c.days == 0 {
        String::new()
    } else {
        format!("{} · DAY {}/{}", c.label, c.day, c.days)
    };

    rsx! {
        div { class: "pk", style: "height:100vh;min-height:0",
            ScannerBg {
                class: "pk-bg".to_string(),
                seed: 31,
                shapes: r#"[
                    { char: "8", cx: .14, cy: .5, scale: .3, style: "wire", morph: "vein", live: false, fill: .42 },
                    { char: "8", cx: .93, cy: .82, scale: .14, style: "wire", live: false, fill: .24 },
                    { char: "e", cx: .8, cy: .26, scale: .18, style: "faint", morph: "blob", live: false, fill: .46 }
                ]"#.to_string(),
            }

            div { class: "app-shell swap",
                AiPanelDash {
                    collapsed: ai_collapsed(),
                    on_toggle: move |()| ai_collapsed.toggle(),
                    on_track: move |id: String| {
                        sel.set(Some(id));
                        if !dockable {
                            drawer_sig.set(true);
                        }
                    },
                }

                div { class: "app-main",
                    TopBar { active: "DASHBOARD".to_string(), date_text: topbar_date }
                    div { class: "app-scroll",

                        // ===================== CONSOLE HERO =====================
                        section { class: "pk-hero dash-c", "data-screen-label": "HERO",
                            div { class: "c-dock glass",
                                // hero remaining
                                div { class: "c-hero",
                                    div { class: "lbl", "REMAINING · {cycle_label}" }
                                    if let Some(t) = &totals_v {
                                        div { class: "big",
                                            span { class: "cur", "CHF" }
                                            "{chf0(t.remaining)}"
                                        }
                                        div { class: "sub",
                                            "of CHF {chf0(t.budget)} budget · {pct_str(Some(f64::from(t.spent_pct) / 100.0))} spent · {days_left} days left"
                                        }
                                    } else {
                                        div { class: "big",
                                            span { class: "cur", "CHF" }
                                            "—"
                                        }
                                    }
                                }
                                // savings dial
                                div { style: "display:flex;justify-content:center;padding:4px 0",
                                    if let Some(t) = &totals_v {
                                        SavingsDial {
                                            size: 150.0,
                                            saved: t.saved.as_chf_f64(),
                                            target: t.savings_target.as_chf_f64(),
                                            projected: t.savings_projected.as_chf_f64(),
                                        }
                                    } else {
                                        Awaiting { label: "SAVINGS".to_string(), loading: totals_loading }
                                    }
                                }
                                // snapshot ministats
                                div {
                                    div { class: "hud sm", style: "margin-bottom:6px", "SNAPSHOT" }
                                    if let Some(t) = &totals_v {
                                        div { class: "ministat",
                                            span { class: "k", "Spent" }
                                            span { class: "v coral", "CHF {chf0(t.spent)}" }
                                        }
                                        div { class: "ministat",
                                            span { class: "k", "Saved" }
                                            span { class: "v", "CHF {chf0(t.saved)}" }
                                        }
                                        div { class: "ministat",
                                            span { class: "k", "Savings rate" }
                                            span { class: "v", "{pct_str(Some(t.savings_rate))}" }
                                        }
                                        div { class: "ministat",
                                            span { class: "k", "vs last cycle" }
                                            span { class: "v", style: "color:var(--ok)",
                                                "{vs_last_str.clone().unwrap_or_default()}"
                                            }
                                        }
                                    } else {
                                        Awaiting { label: "SNAPSHOT".to_string(), loading: totals_loading }
                                    }
                                }
                                // NEXT dock
                                div {
                                    class: "osc-bkt coral",
                                    style: "margin-top:auto;border:1px solid var(--hairline-warm);background:rgba(22,8,16,.4);padding:14px 13px 11px",
                                    span { class: "osc-leg", "NEXT" }
                                    if !recurring_ok {
                                        Awaiting { label: "NEXT DUE".to_string(), loading: recurring_loading, tone: "coral".to_string() }
                                    } else if next_due.is_empty() {
                                        div { class: "dim", style: "font-size:11px;padding:8px 0;letter-spacing:.04em", "No upcoming charges." }
                                    } else {
                                        for (i , n) in next_due.iter().enumerate() {
                                            {
                                                let row_style = if i == 0 {
                                                    String::new()
                                                } else {
                                                    "margin-top:9px;padding-top:9px;border-top:1px solid var(--hairline)".to_string()
                                                };
                                                let name_style = if i == 0 {
                                                    "font-size:17px;color:var(--ink);min-width:0;overflow:hidden;text-overflow:ellipsis;white-space:nowrap"
                                                } else {
                                                    "font-size:14px;color:var(--ink);min-width:0;overflow:hidden;text-overflow:ellipsis;white-space:nowrap"
                                                };
                                                let amt_style = if i == 0 {
                                                    "font-size:19px;white-space:nowrap"
                                                } else {
                                                    "font-size:15px;white-space:nowrap"
                                                };
                                                let due_tail = if n.days_until <= 0 {
                                                    "TODAY".to_string()
                                                } else if n.days_until == 1 {
                                                    format!("{} DAY", n.days_until)
                                                } else {
                                                    format!("{} DAYS", n.days_until)
                                                };
                                                rsx! {
                                                    div { key: "{n.id}", style: "{row_style}",
                                                        div { style: "display:flex;justify-content:space-between;align-items:baseline;gap:10px",
                                                            span { class: "num", style: "{name_style}", "{n.name}" }
                                                            span { class: "num coral", style: "{amt_style}", "CHF {chf2(n.amount)}" }
                                                        }
                                                        div { class: "dim", style: "font-size:9.5px;letter-spacing:.12em;margin-top:3px",
                                                            "DUE {n.next} · {due_tail}"
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            }

                            div { class: "c-main",
                                // spend-trace screen
                                div { class: "c-screen",
                                    ScannerBg {
                                        class: "c-screen-bg".to_string(),
                                        seed: 91,
                                        bg: false,
                                        grid: false,
                                        dish: false,
                                        shapes: r#"[{ char: "8", cx: .5, cy: .56, scale: .52, r: .92, style: "redneg", morph: "mass", live: true, fill: .6 }]"#.to_string(),
                                    }
                                    div { class: "scr-hud",
                                        span { class: "hud", "SPEND TRACE · {cycle_label}" }
                                        span { class: "hud", style: "color:var(--neon-dim)", "{trace_hud}" }
                                    }
                                    if let Some(s) = &series_v {
                                        PhoskChart {
                                            width: 1150.0,
                                            height: 300.0,
                                            show_bars: false,
                                            show_pace: true,
                                            show_area: true,
                                            show_last: true,
                                            pad_t: 34.0,
                                            pad_b: 22.0,
                                            pad_l: 14.0,
                                            pad_r: 14.0,
                                            days: f64::from(c.days.max(1)),
                                            budget: totals_v.as_ref().map_or(0.0, |t| t.budget.as_chf_f64()),
                                            daily: money_chf_series(&s.daily),
                                            cumulative: money_chf_series(&s.cumulative),
                                            pace: money_chf_series(&s.pace),
                                            last_cumulative: s.last_cycle_cumulative.as_ref().map(|v| money_chf_series(v)).unwrap_or_default(),
                                        }
                                    } else {
                                        div { style: "padding:40px 14px",
                                            Awaiting { label: "SPEND TRACE".to_string(), loading: series_loading, tone: "coral".to_string() }
                                        }
                                    }
                                    div { style: "position:absolute;bottom:8px;left:16px;display:flex;gap:16px",
                                        span { class: "hud sm", style: "color:var(--neon)", "━ THIS CYCLE" }
                                        span { class: "hud sm", style: "color:var(--indigo-neon)", "┄ BUDGET PACE" }
                                        span { class: "hud sm", style: "color:rgba(143,125,255,.7)", "┄ LAST CYCLE" }
                                    }
                                }

                                // channels strip (top 5 categories)
                                div { class: "c-channels",
                                    if cats_v.is_none() {
                                        Awaiting { label: "CHANNELS".to_string(), loading: cats_loading }
                                    } else {
                                        for cc in channels.iter() {
                                            {
                                                let p = if cc.budget.centimes() > 0 {
                                                    cc.spent.as_chf_f64() / cc.budget.as_chf_f64()
                                                } else {
                                                    0.0
                                                };
                                                let tone = pct_tone(p);
                                                let spark_tone = if tone == "alert" { "neon" } else { "indigo" };
                                                let has_spark = cc.spark.len() > 1;
                                                let pct_cls = format!("p pct {tone}");
                                                rsx! {
                                                    div { key: "{cc.name}", class: "chan",
                                                        span { class: "cn", "{cc.name}" }
                                                        if has_spark {
                                                            Spark { data: cc.spark.clone(), w: 150.0, h: 28.0, tone: spark_tone.to_string() }
                                                        } else {
                                                            div { style: "height:28px" }
                                                        }
                                                        div { class: "cv",
                                                            span { class: "{pct_cls}", "{cc.used_pct}%" }
                                                            span { class: "s", "CHF {chf0(cc.spent)} / {chf0(cc.budget)}" }
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }

                                // watch line
                                div { class: "c-watch",
                                    span { class: "hud sm", style: "flex:0 0 auto", "WATCH" }
                                    if alerts_v.is_none() {
                                        span { class: "dim", if alerts_loading { "Loading…" } else { "awaiting backend (/alerts)" } }
                                    } else if watch_items.is_empty() {
                                        span { class: "dim", "No active alerts." }
                                    } else {
                                        for (i , a) in watch_items.iter().enumerate() {
                                            {
                                                let col = match a.tone.as_str() {
                                                    "alert" => "var(--neon)",
                                                    "warn" => "var(--warn)",
                                                    _ => "var(--indigo-neon)",
                                                };
                                                let glyph = match a.tone.as_str() {
                                                    "alert" => "⚠",
                                                    "warn" => "◷",
                                                    _ => "⌁",
                                                };
                                                let tag = if a.tag.is_empty() { String::new() } else { format!("{} ", a.tag) };
                                                rsx! {
                                                    if i > 0 {
                                                        span { class: "dim", "·" }
                                                    }
                                                    span { style: "color:{col}", "{glyph} {tag}{a.head}" }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }

                        // ===================== ITEM-SIGNALS =====================
                        SignalStrip {
                            signals: signal_list,
                            sel: sel_id.clone(),
                            on_select: move |id: String| {
                                sel.set(Some(id));
                                if !dockable {
                                    drawer_sig.set(true);
                                }
                            },
                        }

                        // scroll seam
                        div { class: "pk-seam",
                            span { class: "rule" }
                            span { class: "lbl", "DETAIL · TERMINAL ▾" }
                            span { class: "rule" }
                        }

                        // ===================== TERMINAL DETAIL =====================
                        section { class: "pk-terminal dash-b", "data-screen-label": "TERMINAL",
                            div { class: "pk-term-grid",
                                // LEFT RAIL
                                div { class: "col left",
                                    div { class: "b-kpi osc-bkt blue",
                                        span { class: "osc-leg", "BUDGET" }
                                        div { class: "lbl", style: "justify-content:flex-end", span { "{cycle_label}" } }
                                        div { class: "big", "CHF {kpi_budget}" }
                                        div { class: "sub", "{kpi_budget_sub}" }
                                    }
                                    div { class: "b-kpi accent osc-bkt coral",
                                        span { class: "osc-leg", "SPENT" }
                                        div { class: "lbl", style: "justify-content:flex-end",
                                            span { "{kpi_spent_pct}" }
                                        }
                                        div { class: "big", "CHF {kpi_spent}" }
                                        div { class: "sub", "{kpi_spent_sub} · {txn_count}+ entries" }
                                    }
                                    div { class: "b-kpi osc-bkt blue",
                                        span { class: "osc-leg", "REMAINING" }
                                        div { class: "lbl", style: "justify-content:flex-end", span { "{days_left} D LEFT" } }
                                        div { class: "big", "CHF {kpi_remaining}" }
                                        div { class: "sub", "{kpi_remaining_sub}" }
                                    }
                                    div { class: "b-mini osc-bkt blue", style: "padding-top:14px",
                                        span { class: "osc-leg", "RATES" }
                                        if let Some(t) = &totals_v {
                                            div { class: "ministat",
                                                span { class: "k", "Savings rate" }
                                                span { class: "v", "{pct_str(Some(t.savings_rate))}" }
                                            }
                                            div { class: "ministat",
                                                span { class: "k", "vs last cycle" }
                                                span { class: "v", style: "color:var(--ok)",
                                                    "{vs_last_str.clone().unwrap_or_default()}"
                                                }
                                            }
                                            div { class: "ministat",
                                                span { class: "k", "Last cycle spent" }
                                                span { class: "v", style: "font-size:13px", "CHF {chf0(t.last_cycle_spent)}" }
                                            }
                                        } else {
                                            Awaiting { label: "RATES".to_string(), loading: totals_loading }
                                        }
                                    }
                                    div { class: "b-shops osc-bkt blue", style: "padding-top:16px",
                                        span { class: "osc-leg", "TOP SHOPS" }
                                        div { class: "hud sm", style: "margin-bottom:11px", "THIS CYCLE" }
                                        if shops_v.is_none() {
                                            Awaiting { label: "TOP SHOPS".to_string(), loading: shops_loading }
                                        } else if shops_v.as_ref().is_some_and(|s| s.shops.is_empty()) {
                                            div { class: "dim", style: "font-size:11px", "No shops this cycle." }
                                        } else {
                                            div { style: "display:flex;flex-direction:column;gap:11px",
                                                {
                                                    let sd = shops_v.as_ref().unwrap();
                                                    let denom = if sd.max_total.centimes() > 0 {
                                                        sd.max_total.as_chf_f64()
                                                    } else {
                                                        sd.shops.first().map_or(1.0, |s| s.total.as_chf_f64()).max(1.0)
                                                    };
                                                    rsx! {
                                                        for s in sd.shops.iter().take(4) {
                                                            {
                                                                let w = if denom > 0.0 { s.total.as_chf_f64() / denom * 100.0 } else { 0.0 };
                                                                rsx! {
                                                                    div { key: "{s.shop}",
                                                                        div { style: "display:flex;justify-content:space-between;align-items:baseline;margin-bottom:4px",
                                                                            span { class: "num", style: "font-size:12.5px;color:var(--ink);text-transform:uppercase;letter-spacing:.03em", "{s.shop}" }
                                                                            span { class: "mono", style: "font-size:11px;color:var(--ink-2)", "CHF {chf2(s.total)}" }
                                                                        }
                                                                        div { class: "phosk-bar", style: "height:5px",
                                                                            div { class: "phosk-bar-fill", style: "width:{w}%;background:var(--indigo);box-shadow:0 0 6px var(--indigo)" }
                                                                        }
                                                                    }
                                                                }
                                                            }
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }

                                // MID
                                div { class: "col mid",
                                    div { class: "matrix",
                                        div { class: "panel-h", style: "padding:10px 0;border-color:var(--hairline)",
                                            span { class: "ttl", "Category budgets" }
                                            span { class: "ct", "{cats_count_str}" }
                                            span { class: "rule" }
                                            span { class: "meta", "SPENT / CAP / USED" }
                                            Link { class: "gbtn p", to: Route::BudgetsPage {}, style: "margin-left:11px;white-space:nowrap", "BUDGETS ↗" }
                                        }
                                        if cats_v.is_none() {
                                            Awaiting { label: "CATEGORY BUDGETS".to_string(), loading: cats_loading }
                                        } else {
                                            CatRows { cats: cat_rows }
                                        }
                                    }
                                    div { class: "b-recent",
                                        div { class: "panel-h", style: "padding:10px 0;border-color:var(--hairline)",
                                            span { class: "ttl blue", "Recent" }
                                            span { class: "rule" }
                                            span { class: "meta", "SHOP / CATEGORY / AMOUNT" }
                                            Link { class: "gbtn p", to: Route::TransactionsPage {}, style: "margin-left:11px;white-space:nowrap", "ALL TXNS ↗" }
                                        }
                                        if txns_v.is_none() {
                                            Awaiting { label: "RECENT TXNS".to_string(), loading: txns_loading }
                                        } else {
                                            TxnTape { rows: txn_rows }
                                        }
                                    }
                                }

                                // RIGHT RAIL
                                div { class: "col right",
                                    div { class: "hud", style: "display:flex;justify-content:space-between;align-items:center",
                                        span { "⌁ NEEDS ATTENTION" }
                                        span { class: "coral num", style: "font-size:15px", "{alerts_count_str}" }
                                    }
                                    if alerts_v.is_none() {
                                        Awaiting { label: "NEEDS ATTENTION".to_string(), loading: alerts_loading, tone: "coral".to_string() }
                                    } else if alert_list.is_empty() {
                                        div { class: "dim", style: "font-size:11px;padding:6px 0", "Nothing needs attention." }
                                    } else {
                                        div { style: "display:flex;flex-direction:column;gap:9px",
                                            for a in alert_list.iter().take(3) {
                                                AlertItem {
                                                    key: "{a.id}",
                                                    a: alert_of(a),
                                                    on_action: move |(id, kind): (String, String)| {
                                                        // VIEW navigates to the alert's deep-link target
                                                        // (React resolved /alerts/{id}/target → /transactions);
                                                        // every other kind POSTs then re-fetches the list.
                                                        if kind == "navigate" {
                                                            let _ = nav.push(Route::TransactionsPage {});
                                                        } else {
                                                            let mut alerts = alerts;
                                                            let mut alert_error = alert_error;
                                                            spawn(async move {
                                                                match act_on_alert(id, kind).await {
                                                                    Ok(()) => {
                                                                        alert_error.set(None);
                                                                        alerts.restart();
                                                                    }
                                                                    Err(_) => {
                                                                        alert_error.set(Some(ALERT_ACTION_FAILED.to_string()));
                                                                    }
                                                                }
                                                            });
                                                        }
                                                    },
                                                }
                                            }
                                        }
                                        InlineStatus { error: alert_error() }
                                    }
                                    div { class: "hud", style: "display:flex;justify-content:space-between;align-items:center;margin-top:4px",
                                        span { "RECURRING · CLEAN" }
                                        span { class: "dim", "{monthly_total_str}" }
                                    }
                                    if !recurring_ok {
                                        Awaiting { label: "RECURRING".to_string(), loading: recurring_loading }
                                    } else if recurring_list.is_empty() {
                                        div { class: "dim", style: "font-size:11px;padding:6px 0", "No recurring charges." }
                                    } else {
                                        div { style: "display:flex;flex-direction:column;gap:6px",
                                            for r in recurring_list.iter().take(4) {
                                                RecRow { key: "{r.id}", r: rec_of(r) }
                                            }
                                        }
                                    }
                                    div { class: "b-insight osc-bkt blue",
                                        div { class: "hud sm", style: "display:flex;align-items:center;gap:7px",
                                            Dot { tone: "blue".to_string(), size: 6 }
                                            "{insight_model} · INSIGHT"
                                        }
                                        if insight_v.is_none() {
                                            Awaiting { label: "GEMMA4 INSIGHT".to_string(), loading: insight_loading }
                                        } else {
                                            div { class: "q",
                                                {
                                                    let iv = insight_v.as_ref().unwrap();
                                                    let est = if iv.estimated_savings.centimes() != 0 {
                                                        Some(chf2(iv.estimated_savings))
                                                    } else {
                                                        None
                                                    };
                                                    rsx! {
                                                        "{iv.text}"
                                                        if let Some(e) = est {
                                                            " · est. "
                                                            b { "CHF {e}" }
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }

                // docked signal panel (wide layouts)
                if dockable && sel_id.is_some() {
                    SignalPanel {
                        sig: sig_obj.clone(),
                        cycle_label: cycle_label.clone(),
                        on_close: move |()| sel.set(None),
                        on_track: move |id: String| {
                            // React onChanged: track the candidate, then re-fetch the
                            // detail + tracked list + candidate list, and close (the
                            // candidate is now tracked, the panel's candidate view stale).
                            let mut signals = signals;
                            let mut candidates = candidates;
                            let mut sig_detail = sig_detail;
                            let mut signal_error = signal_error;
                            spawn(async move {
                                match track_signal(id).await {
                                    Ok(()) => {
                                        signal_error.set(None);
                                        signals.restart();
                                        candidates.restart();
                                        sig_detail.restart();
                                        sel.set(None);
                                    }
                                    Err(e) => signal_error.set(Some(signal_action_error_text(&e))),
                                }
                            });
                        },
                        on_dismiss: move |id: String| {
                            let mut signals = signals;
                            let mut candidates = candidates;
                            let mut sig_detail = sig_detail;
                            let mut signal_error = signal_error;
                            spawn(async move {
                                match dismiss_signal(id).await {
                                    Ok(()) => {
                                        signal_error.set(None);
                                        signals.restart();
                                        candidates.restart();
                                        sig_detail.restart();
                                        sel.set(None);
                                    }
                                    Err(e) => signal_error.set(Some(signal_action_error_text(&e))),
                                }
                            });
                        },
                    }
                    InlineStatus { error: signal_error() }
                }
            }

            // drawer signal panel (narrow layouts)
            if !dockable && drawer_sig() && sel_id.is_some() {
                div { class: "sig-drawer-back", onclick: move |_| drawer_sig.set(false),
                    div { class: "sig-drawer", onclick: move |e: Event<MouseData>| e.stop_propagation(),
                        SignalPanel {
                            sig: sig_obj.clone(),
                            variant: "drawer".to_string(),
                            cycle_label: cycle_label.clone(),
                            on_close: move |()| drawer_sig.set(false),
                            on_track: move |id: String| {
                                let mut signals = signals;
                                let mut candidates = candidates;
                                let mut sig_detail = sig_detail;
                                let mut signal_error = signal_error;
                                spawn(async move {
                                    match track_signal(id).await {
                                        Ok(()) => {
                                            signal_error.set(None);
                                            signals.restart();
                                            candidates.restart();
                                            sig_detail.restart();
                                            drawer_sig.set(false);
                                            sel.set(None);
                                        }
                                        Err(e) => {
                                            signal_error.set(Some(signal_action_error_text(&e)));
                                        }
                                    }
                                });
                            },
                            on_dismiss: move |id: String| {
                                let mut signals = signals;
                                let mut candidates = candidates;
                                let mut sig_detail = sig_detail;
                                let mut signal_error = signal_error;
                                spawn(async move {
                                    match dismiss_signal(id).await {
                                        Ok(()) => {
                                            signal_error.set(None);
                                            signals.restart();
                                            candidates.restart();
                                            sig_detail.restart();
                                            drawer_sig.set(false);
                                            sel.set(None);
                                        }
                                        Err(e) => {
                                            signal_error.set(Some(signal_action_error_text(&e)));
                                        }
                                    }
                                });
                            },
                        }
                        InlineStatus { error: signal_error() }
                    }
                }
            }
        }
    }
}

/// Thin dashboard wrapper around the shared `AiPanel` (left assistant).
///
/// The React `DashFull` passed only `collapsed` / `onToggle` / `onTrack` to
/// `AiPanel` (the feed/chat/status load lazily). Here `on_toggle` flips the
/// rail and `on_track` selects the signal in the dashboard's `sel` signal (the
/// React `selectSig`, opening the dock or drawer) when a feed item's "track"
/// action fires — but `AiPanel` lives outside the dashboard's state closure, so
/// this tiny component owns that wiring and re-exposes both. The feed is left
/// to its (empty) default until an AI feed server fn lands, so the panel renders
/// its "awaiting backend (/ai/feed)" body, faithful to React. The chat needs no
/// wiring here: the panel loads and sends it itself.
#[component]
fn AiPanelDash(
    collapsed: bool,
    on_toggle: EventHandler<()>,
    on_track: EventHandler<String>,
) -> Element {
    use crate::components::shell::AiPanel;
    rsx! {
        AiPanel {
            collapsed,
            on_toggle: move |()| on_toggle.call(()),
            on_track: move |id: String| on_track.call(id),
        }
    }
}
