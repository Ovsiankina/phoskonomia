//! Subscriptions page (route `/subscriptions`). Faithful port of React
//! `pages/Subscriptions.jsx` (`SubsPage`).
//!
//! Shape preserved 1:1 with the JSX: the `.pk` root + `ScannerBg`, the
//! `.app-shell.swap` with the left `AiPanel`, the `.app-main` (TopBar + scroll),
//! the `.subs-wrap` (header + controls, the KPI band, the BILLING SWEEP impulse
//! train, the STANDING CHARGES section + the tunable card/row grid) and the
//! right-dock `.sub-insp` inspector.
//!
//! Data: where React fanned out `useGet` over the dead REST layer, this fans out
//! `use_resource` over the F3 `#[server]` fns (`list_subscriptions` w/ `SubFilter`,
//! `get_subscription_stats`, `get_billing_sweep`, `get_subscription`). Each is
//! read as `Some(Ok(_))` (green) / `Some(Err(_))` (awaiting) / `None` (loading) —
//! the exact tri-state React's `status === 200` / `loading` distinguished. Money
//! crosses as exact `Money`; raw chart numbers (`hist`) are presentation `f64`.
//!
//! JSX idioms → RSX (per the playbook):
//!   * `useState` → `use_signal`; the `useTweaks` store (authoring tooling, not
//!     ported) becomes plain `use_signal`s driven by the in-page `.subs-controls`.
//!   * the floating `SubTweaks`/`TweaksPanel` panel is `@ds-adherence-ignore`
//!     authoring tooling — NOT product UI — so it is intentionally omitted.
//!   * `useGet(path, params, [sel])` → `use_resource` (filter built from signals;
//!     refetches when they change). On-demand detail → `use_resource` over `sel`.
//!   * the hand-written `BillingSweep` / `SubHistBars` / `CycleMeter` SVG/markup
//!     are PAGE-SPECIFIC (not F2 primitives), so they are ported verbatim as RSX.
//!   * CONFIRM / DISMISS of an AI-detected recurring candidate are wired to the
//!     `confirm_recurring_candidate` / `dismiss_recurring_candidate` server fns
//!     (the AI CANDIDATES section + the inspector). The open-candidate list
//!     (`list_recurring_candidates`) is the authority for which records show
//!     them; only a click confirms.
//!   * NEW / EDIT / DELETE (with a confirm step) and the lifecycle actions
//!     (MARK PAID / PAUSE / RESUME / CANCEL / RECORD CHARGE) go through the
//!     write-path server fns; the inspector offers only the actions the
//!     backend reports it accepts (`SubscriptionDetailDto::actions`). One write
//!     at a time; every write re-reads the list, roll-ups, sweep and inspector.
//!   * DETECT / SNOOZE have no server fn yet, so those buttons render
//!     faithfully but are inert.

use dioxus::prelude::*;
use phosk_core::money::Money;

use crate::components::prims::{Dot, ScannerBg, Spark};
use crate::components::shell::{AiPanel, ChatMsg, FeedItem, TopBar};
use crate::components::states::Awaiting;
use crate::data::ai::get_ai_panel;
use crate::data::subscriptions::{
    confirm_recurring_candidate, dismiss_recurring_candidate, get_billing_sweep, get_subscription,
    get_subscription_stats, list_recurring_candidates, list_subscriptions, BillingSweepDto,
    ImpulseDto, RecurringCandidateDto, SubFilter, SubscriptionDetailDto, SubscriptionDto,
};
use crate::data::subscriptions::{
    create_subscription, delete_subscription, edit_subscription, record_subscription_charge,
    run_subscription_action, SubAction, SubscriptionForm,
};
use crate::data::{chf, chf2, cycle::get_cycle, cycle::CycleDto};

// ── presentation helpers (faithful to the JSX's subStatus/cadUnit/chf) ────────

/// status → (label, tone) used across the page. Faithful to React `subStatus`:
/// reads the FETCHED record's `status` and falls back to its `status_label`.
fn sub_status(status: &str, status_label: &str) -> (String, &'static str) {
    match status {
        "due" => (
            if status_label.is_empty() {
                "NOT SEEN".to_string()
            } else {
                status_label.to_string()
            },
            "coral",
        ),
        "soon" => (
            if status_label.is_empty() {
                "DUE SOON".to_string()
            } else {
                status_label.to_string()
            },
            "warn",
        ),
        "watch" => (
            if status_label.is_empty() {
                "REVIEW".to_string()
            } else {
                status_label.to_string()
            },
            "warn",
        ),
        _ => (
            if status_label.is_empty() {
                "ACTIVE".to_string()
            } else {
                status_label.to_string()
            },
            "blue",
        ),
    }
}

/// `"/ YR"` for yearly cadence, `"/ MO"` otherwise — display only (React `cadUnit`).
fn cad_unit(cadence: &str) -> &'static str {
    if cadence == "yearly" {
        "/ YR"
    } else {
        "/ MO"
    }
}

/// `chf` choosing 0 decimals for whole amounts, else `dp` (React's
/// `chf(big, (big%1) ? 2 : 0)` idiom — here `Money` has cents iff `centimes%100`).
fn chf_smart(m: Money, dp: usize) -> String {
    if m.centimes() % 100 == 0 {
        chf(m, 0)
    } else {
        chf(m, dp)
    }
}

// ── small DTO fallback ───────────────────────────────────────────────────────

/// `CycleDto` empty default — labels render empty until the cycle lands.
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
//  BILLING SWEEP (hero) — page-specific SVG impulse train, ported verbatim.
// ════════════════════════════════════════════════════════════════════════════

/// Faithful port of React `BillingSweep`. Renders the recurring-impulse train SVG
/// from the fetched `/subscriptions/billing-sweep` payload. `sweep_v` is the
/// green payload (or `None` → `<Awaiting/>`).
#[component]
fn BillingSweep(
    sweep_v: Option<BillingSweepDto>,
    loading: bool,
    sel: Option<String>,
    on_select: EventHandler<String>,
    cycle_label: String,
) -> Element {
    let cycle = sweep_v.as_ref().map(|s| s.cycle.clone());
    let impulses: Vec<ImpulseDto> = sweep_v
        .as_ref()
        .map(|s| s.impulses.clone())
        .unwrap_or_default();
    let footer = sweep_v.as_ref().map(|s| s.footer.clone());
    let ok = sweep_v.is_some() && !impulses.is_empty();

    // SVG geometry (verbatim from the JSX).
    let days = cycle.as_ref().map_or(30u32, |c| c.days).max(1);
    let today = cycle.as_ref().map_or(0u32, |c| c.day);
    let as_of = cycle.as_ref().map_or(String::new(), |c| c.as_of.clone());
    const W: f64 = 1000.0;
    const H: f64 = 178.0;
    const PAD_L: f64 = 50.0;
    const PAD_R: f64 = 50.0;
    const PAD_T: f64 = 30.0;
    const PAD_B: f64 = 38.0;
    let base = H - PAD_B;
    let usable = H - PAD_T - PAD_B;
    let denom = (days.max(2) as f64) - 1.0;
    let x_of = |d: f64| PAD_L + (d - 1.0) / denom * (W - PAD_L - PAD_R);
    let max_amt = impulses
        .iter()
        .map(|s| s.amount.as_chf_f64())
        .fold(1.0_f64, f64::max);
    let h_of = |a: f64| 18.0 + (a.max(0.0)).sqrt() / max_amt.sqrt() * (usable - 18.0);

    // group impulses by day, fan within a day (verbatim `items` useMemo).
    struct Plotted {
        s: ImpulseDto,
        cx: f64,
        top: f64,
    }
    let plotted: Vec<Plotted> = {
        use std::collections::BTreeMap;
        let mut by_day: BTreeMap<u32, Vec<ImpulseDto>> = BTreeMap::new();
        for s in &impulses {
            by_day.entry(s.day).or_default().push(s.clone());
        }
        let mut out: Vec<Plotted> = Vec::new();
        for (day, mut grp) in by_day {
            grp.sort_by(|a, b| {
                b.amount
                    .as_chf_f64()
                    .partial_cmp(&a.amount.as_chf_f64())
                    .unwrap_or(std::cmp::Ordering::Equal)
            });
            let n = grp.len();
            for (i, s) in grp.into_iter().enumerate() {
                let dx = (i as f64 - (n as f64 - 1.0) / 2.0) * 10.0;
                let amt = s.amount.as_chf_f64();
                out.push(Plotted {
                    cx: x_of(day as f64) + dx,
                    top: base - h_of(amt),
                    s,
                });
            }
        }
        out
    };

    let tick_days = [1.0_f64, 8.0, 15.0, 22.0, 29.0];

    // footer pre-computed display strings (format-segment parser is strict).
    let paid_str = footer
        .as_ref()
        .map(|f| format!("CHF {}", chf(f.paid_this_cycle, 0)));
    let due_str = footer
        .as_ref()
        .map(|f| format!("CHF {}", chf(f.still_due, 0)));
    let next_str = footer.as_ref().map(|f| {
        format!(
            "{} · {} · CHF {}",
            f.next.name,
            f.next.next_label,
            chf(f.next.amount, 0)
        )
    });
    let note = footer
        .as_ref()
        .map(|f| f.note.clone())
        .filter(|n| !n.is_empty());
    let cycle_tail = if cycle_label.is_empty() {
        String::new()
    } else {
        format!(" · {cycle_label}")
    };
    let today_label = if as_of.is_empty() {
        "TODAY".to_string()
    } else {
        format!("TODAY · {as_of}")
    };
    let awaiting_margin = "margin:6px 0 2px".to_string();

    rsx! {
        div { class: "sweep osc-bkt blue",
            span { class: "osc-leg", "BILLING SWEEP" }
            div { class: "sweep-h",
                span { class: "hud", "⊟ RECURRING IMPULSE TRAIN{cycle_tail}" }
                div { class: "sweep-key",
                    span { i { class: "k paid" } " PAID" }
                    span { i { class: "k up" } " UPCOMING" }
                    span { i { class: "k miss" } " NOT SEEN" }
                }
            }

            if !ok {
                Awaiting { label: "BILLING SWEEP".to_string(), loading, tone: "blue".to_string(), style: awaiting_margin }
            } else {
                svg {
                    width: "100%",
                    height: "{H}",
                    view_box: "0 0 {W} {H}",
                    preserve_aspect_ratio: "none",
                    class: "sweep-svg",
                    // ground line
                    line {
                        x1: "{PAD_L}", y1: "{base}", x2: "{W - PAD_R}", y2: "{base}",
                        stroke: "rgba(106,95,192,.4)", stroke_width: "1",
                    }
                    // weekly ticks
                    for d in tick_days {
                        g { key: "{d}",
                            line {
                                x1: "{x_of(d)}", y1: "{PAD_T - 6.0}", x2: "{x_of(d)}", y2: "{base}",
                                stroke: "rgba(106,95,192,.14)", stroke_width: "1", stroke_dasharray: "2 5",
                            }
                            text {
                                x: "{x_of(d)}", y: "{base + 18.0}", text_anchor: "middle",
                                fill: "var(--ink-3)", font_size: "9", font_family: "var(--font-body)", letter_spacing: ".1em",
                                "{d}"
                            }
                        }
                    }
                    text {
                        x: "{PAD_L}", y: "{base + 18.0}", text_anchor: "start",
                        fill: "var(--ink-3)", font_size: "8", font_family: "var(--font-body)", letter_spacing: ".18em",
                        "DAY"
                    }
                    // today marker
                    if today > 0 {
                        line {
                            x1: "{x_of(today as f64)}", y1: "{PAD_T - 8.0}", x2: "{x_of(today as f64)}", y2: "{base + 6.0}",
                            stroke: "rgba(255,59,46,.5)", stroke_width: "1.2", stroke_dasharray: "3 3",
                        }
                        text {
                            x: "{x_of(today as f64)}", y: "{PAD_T - 12.0}", text_anchor: "middle",
                            fill: "var(--neon-dim)", font_size: "8.5", font_family: "var(--font-body)", letter_spacing: ".12em",
                            "{today_label}"
                        }
                    }
                    // impulses
                    for p in plotted.iter() {
                        {
                            let s = &p.s;
                            let cx = p.cx;
                            let top = p.top;
                            let active = sel.as_deref() == Some(s.id.as_str());
                            // toneOf: col/glow/dash/hollow per status.
                            let (col, glow, dash, hollow) = match s.status.as_str() {
                                "due" => ("var(--neon)", "drop-shadow(0 0 5px var(--neon))", true, true),
                                "soon" => ("var(--warn)", "drop-shadow(0 0 5px var(--warn))", false, false),
                                "watch" => ("var(--warn)", "drop-shadow(0 0 4px var(--warn))", false, false),
                                "paid" => ("rgba(143,125,255,.55)", "none", false, false),
                                _ => ("var(--indigo-neon)", "drop-shadow(0 0 4px var(--indigo-neon))", false, false),
                            };
                            let r = if active { 5.0 } else { 3.5 };
                            let line_w = if active { 2.4 } else { 1.6 };
                            let dash_attr = if dash { "3 3" } else { "0" };
                            let fill_circ = if hollow { "var(--bg)" } else { col };
                            let stroke_w_circ = if hollow { 1.8 } else { 0.0 };
                            let amt = s.amount;
                            let amt_str = chf_smart(amt, 2);
                            let title = format!("{} · CHF {} · day {}", s.name, chf(amt, 2), s.day);
                            let text_fill = if active { "var(--ink)" } else { "var(--ink-2)" };
                            let text_style = if active { format!("text-shadow:0 0 6px {col}") } else { String::new() };
                            let id = s.id.clone();
                            rsx! {
                                g {
                                    key: "{s.id}",
                                    style: "cursor:pointer",
                                    onclick: move |_| on_select.call(id.clone()),
                                    title { "{title}" }
                                    // hit area
                                    rect {
                                        x: "{cx - 9.0}", y: "{PAD_T - 10.0}", width: "18",
                                        height: "{base - PAD_T + 22.0}", fill: "transparent",
                                    }
                                    line {
                                        x1: "{cx}", y1: "{base}", x2: "{cx}", y2: "{top}",
                                        stroke: "{col}", stroke_width: "{line_w}",
                                        stroke_dasharray: "{dash_attr}", style: "filter:{glow}",
                                    }
                                    circle {
                                        cx: "{cx}", cy: "{top}", r: "{r}",
                                        fill: "{fill_circ}", stroke: "{col}", stroke_width: "{stroke_w_circ}",
                                        style: "filter:{glow}",
                                    }
                                    if active {
                                        circle {
                                            cx: "{cx}", cy: "{top}", r: "{r + 4.0}",
                                            fill: "none", stroke: "{col}", stroke_width: "1", opacity: ".6",
                                        }
                                    }
                                    text {
                                        x: "{cx}", y: "{top - 9.0}", text_anchor: "middle",
                                        fill: "{text_fill}", font_size: "8.5", font_family: "var(--font-display)",
                                        style: "{text_style}",
                                        "{amt_str}"
                                    }
                                }
                            }
                        }
                    }
                }
            }

            // footer
            div { class: "sweep-foot",
                span { class: "sf-stat",
                    i { "PAID THIS CYCLE" }
                    " "
                    b { "{paid_str.clone().unwrap_or_else(|| \"—\".to_string())}" }
                }
                span { class: "sf-stat",
                    i { "STILL DUE" }
                    " "
                    b { class: "warn", "{due_str.clone().unwrap_or_else(|| \"—\".to_string())}" }
                }
                span { class: "sf-stat",
                    i { "NEXT" }
                    " "
                    b { "{next_str.clone().unwrap_or_else(|| \"—\".to_string())}" }
                }
                if let Some(n) = note {
                    span { class: "sf-note",
                        Dot { tone: "blue".to_string(), size: 6 }
                        " {n}"
                    }
                }
            }
        }
    }
}

// ════════════════════════════════════════════════════════════════════════════
//  price-history bars — page-specific SVG (React `SubHistBars`).
// ════════════════════════════════════════════════════════════════════════════

/// Faithful port of React `SubHistBars`: per-charge price history bars with the
/// last bar coloured by `price_rose`.
#[component]
fn SubHistBars(
    hist: Vec<f64>,
    cadence: String,
    price_rose: bool,
    #[props(default = 300.0)] w: f64,
    #[props(default = 96.0)] h: f64,
) -> Element {
    let yearly = cadence == "yearly";
    let labels: Vec<&str> = if yearly {
        vec!["'23", "'24", "'25"]
    } else {
        vec!["JAN", "FEB", "MAR", "APR", "MAY", "JUN"]
    };
    if hist.is_empty() {
        return rsx! {
            svg {
                width: "100%", height: "{h}", view_box: "0 0 {w} {h}",
                preserve_aspect_ratio: "none", style: "display:block",
            }
        };
    }
    let max = hist.iter().copied().fold(f64::MIN, f64::max) * 1.16;
    let max = if max <= 0.0 { 1.0 } else { max };
    const PAD_B: f64 = 15.0;
    const PAD_T: f64 = 8.0;
    let n = hist.len();
    let bw = (w / n as f64) * 0.5;
    let y_of = |v: f64| h - PAD_B - (v / max) * (h - PAD_T - PAD_B);
    let rose = price_rose;

    rsx! {
        svg {
            width: "100%", height: "{h}", view_box: "0 0 {w} {h}",
            preserve_aspect_ratio: "none", style: "display:block",
            for (i , v) in hist.iter().enumerate() {
                {
                    let cx = (i as f64 + 0.5) * (w / n as f64);
                    let last = i == n - 1;
                    let bumped = i > 0 && *v > hist[i - 1];
                    let col = if last {
                        if rose { "var(--neon)" } else { "var(--indigo-neon)" }
                    } else if bumped {
                        "rgba(255,94,77,.5)"
                    } else {
                        "rgba(132,116,222,.5)"
                    };
                    let by = y_of(*v);
                    let bh = h - PAD_B - by;
                    let bar_style = if last && rose { "filter:drop-shadow(0 0 4px var(--neon))" } else { "" };
                    let txt_fill = if last { "var(--ink-2)" } else { "var(--ink-3)" };
                    let lbl = labels.get(i).copied().unwrap_or("");
                    rsx! {
                        g { key: "{i}",
                            rect {
                                x: "{cx - bw / 2.0}", y: "{by}", width: "{bw}", height: "{bh}",
                                fill: "{col}", style: "{bar_style}",
                            }
                            text {
                                x: "{cx}", y: "{h - 3.0}", text_anchor: "middle",
                                fill: "{txt_fill}", font_size: "7.5", font_family: "var(--font-body)", letter_spacing: ".06em",
                                "{lbl}"
                            }
                        }
                    }
                }
            }
        }
    }
}

// ════════════════════════════════════════════════════════════════════════════
//  billing-cycle meter — page-specific markup (React `CycleMeter`).
// ════════════════════════════════════════════════════════════════════════════

/// Faithful port of React `CycleMeter`: a cosmetic countdown bar derived from the
/// fetched `days_until` (not recomputed).
#[component]
fn CycleMeter(
    cadence: String,
    status: String,
    days_until: i32,
    has_days_until: bool,
    #[props(default = 30)] cycle_days: u32,
) -> Element {
    if cadence != "monthly" {
        return rsx! {
            div { class: "cyc-meter yearly",
                div { class: "cyc-fill", style: "width:8%" }
                span { class: "cyc-mk", style: "left:8%" }
            }
        };
    }
    let overdue = status == "due";
    let cd = cycle_days.max(1) as f64;
    let frac = if overdue {
        1.0
    } else if !has_days_until {
        0.02
    } else {
        (1.0 - days_until as f64 / cd).clamp(0.02, 1.0)
    };
    let col = if overdue {
        "var(--neon)"
    } else if has_days_until && days_until <= 4 {
        "var(--warn)"
    } else {
        "var(--indigo)"
    };
    let pct = frac * 100.0;
    let cls = if overdue {
        "cyc-meter over"
    } else {
        "cyc-meter"
    };
    rsx! {
        div { class: "{cls}",
            div { class: "cyc-fill", style: "width:{pct}%;background:{col};box-shadow:0 0 6px {col}" }
            span { class: "cyc-mk", style: "left:{pct}%" }
        }
    }
}

// ════════════════════════════════════════════════════════════════════════════
//  subscription CARD (React `SubCard`).
// ════════════════════════════════════════════════════════════════════════════

/// Faithful port of React `SubCard`.
#[component]
fn SubCard(
    s: SubscriptionDto,
    active: bool,
    on_select: EventHandler<String>,
    amount_mode: String,
    #[props(default = 30)] cycle_days: u32,
) -> Element {
    let (label, tone) = sub_status(&s.status, &s.status_label);
    let mo = s.monthly_equiv;
    let yr = s.annual;
    let show_annual = amount_mode == "annual";
    let yearly = s.cadence == "yearly";
    let big = if show_annual { yr } else { s.amount };
    let unit = if show_annual {
        "/ YR"
    } else {
        cad_unit(&s.cadence)
    };
    let second = if show_annual {
        format!("CHF {} / mo", chf2(mo))
    } else {
        format!("CHF {} / yr", chf(yr, 0))
    };
    let du = if s.cadence == "monthly" {
        Some(s.days_until)
    } else {
        None
    };
    let auto = s.source == "llm";
    // big numeral: 2dp if has cents, else 0 for annual/yearly, else 2.
    let big_str = if big.centimes() % 100 != 0 {
        chf(big, 2)
    } else if show_annual || yearly {
        chf(big, 0)
    } else {
        chf(big, 2)
    };
    let cls = format!("sub osc-bkt {tone}{}", if active { " on" } else { "" });
    let dot_tone = if auto { "blue" } else { "ok" };
    let src_cls = if auto { "sub-src auto" } else { "sub-src" };
    let src_txt = if auto { "AUTO" } else { "USER" };
    let id_click = s.id.clone();
    let id_key = s.id.clone();

    // NEXT line tail.
    let next_warn = du.is_some_and(|d| d <= 4);
    let next_cls = if next_warn { "nx warn" } else { "nx" };
    let next_in = du.map_or(String::new(), |d| format!(" · IN {d}D"));
    let next_lbl = if s.next_label.is_empty() {
        "—".to_string()
    } else {
        s.next_label.clone()
    };
    let cad_txt = if yearly {
        "YEARLY".to_string()
    } else if s.day != 0 {
        format!("MONTHLY · {}", s.day)
    } else {
        "MONTHLY".to_string()
    };
    let since_txt = if s.since.is_empty() {
        "—".to_string()
    } else {
        s.since.clone()
    };
    let has_spark = s.hist.len() > 1;
    let spark_data = s.hist.clone();

    rsx! {
        div {
            class: "{cls}",
            role: "button",
            tabindex: 0,
            onclick: move |_| on_select.call(id_click.clone()),
            key: "{id_key}",
            span { class: "osc-leg", "{label}" }
            div { class: "sub-h",
                div { class: "sub-gl", b { "{s.glyph}" } }
                div { class: "sub-id",
                    span { class: "sub-nm", "{s.name}" }
                    span { class: "sub-cat",
                        Dot { tone: dot_tone.to_string(), size: 5 }
                        "{s.category}"
                    }
                }
                span { class: "{src_cls}", "{src_txt}" }
            }

            div { class: "sub-amt",
                span { class: "sp",
                    span { class: "cur", "CHF" }
                    "{big_str}"
                }
                span { class: "un", "{unit}" }
                span { class: "eq", "{second}" }
            }

            CycleMeter {
                cadence: s.cadence.clone(),
                status: s.status.clone(),
                days_until: s.days_until,
                has_days_until: true,
                cycle_days,
            }
            div { class: "sub-next",
                if s.status == "due" {
                    span { class: "nx alert", "⚠ NOT SEEN THIS CYCLE" }
                } else if yearly {
                    span { class: "nx", "NEXT · {next_lbl}" }
                } else {
                    span { class: "{next_cls}", "NEXT · {next_lbl}{next_in}" }
                }
                span { class: "cad", "{cad_txt}" }
            }

            div { class: "sub-foot",
                span { class: "since", "SINCE {since_txt}" }
                div { class: "pricetrack",
                    span { class: "pl", "PRICE" }
                    if has_spark {
                        Spark { data: spark_data, w: 70.0, h: 20.0, tone: "indigo".to_string() }
                    } else {
                        span { class: "dim", style: "font-size:9px", "—" }
                    }
                }
            }
        }
    }
}

// ════════════════════════════════════════════════════════════════════════════
//  compact ROW variant (React `SubRow`).
// ════════════════════════════════════════════════════════════════════════════

/// Faithful port of React `SubRow`.
#[component]
fn SubRow(
    s: SubscriptionDto,
    active: bool,
    on_select: EventHandler<String>,
    amount_mode: String,
    #[props(default = 30)] cycle_days: u32,
) -> Element {
    let (label, tone) = sub_status(&s.status, &s.status_label);
    let yr = s.annual;
    let yearly = s.cadence == "yearly";
    let du = if s.cadence == "monthly" {
        Some(s.days_until)
    } else {
        None
    };
    let show_annual = amount_mode == "annual";
    let big = if show_annual { yr } else { s.amount };
    let auto = s.source == "llm";
    let cls = format!("subrow{}", if active { " on" } else { "" });
    let stat_cls = format!("sr-stat {tone}");
    let big_str = if show_annual || yearly {
        chf(big, 0)
    } else {
        chf(big, 2)
    };
    let amt_unit = if show_annual || yearly { "/yr" } else { "/mo" };
    let next_txt = if s.status == "due" {
        "⚠ —".to_string()
    } else if s.next_label.is_empty() {
        "—".to_string()
    } else {
        s.next_label.clone()
    };
    let next_tail = match du {
        Some(d) if s.status != "due" => format!(" · {d}D"),
        _ => String::new(),
    };
    let src_cls = if auto { "sr-src auto" } else { "sr-src" };
    let src_txt = if auto { "AUTO" } else { "USER" };
    let id_click = s.id.clone();
    let id_key = s.id.clone();

    rsx! {
        div {
            class: "{cls}",
            key: "{id_key}",
            onclick: move |_| on_select.call(id_click.clone()),
            div { class: "sr-gl", b { "{s.glyph}" } }
            span { class: "sr-nm", "{s.name}" }
            span { class: "{stat_cls}", "{label}" }
            div { class: "sr-meter",
                CycleMeter {
                    cadence: s.cadence.clone(),
                    status: s.status.clone(),
                    days_until: s.days_until,
                    has_days_until: true,
                    cycle_days,
                }
            }
            span { class: "sr-next", "{next_txt}{next_tail}" }
            span { class: "sr-amt",
                "CHF "
                b { "{big_str}" }
                " "
                i { "{amt_unit}" }
            }
            span { class: "{src_cls}", "{src_txt}" }
        }
    }
}

// ════════════════════════════════════════════════════════════════════════════
//  INSPECTOR (right dock) — React `SubInspector`.
// ════════════════════════════════════════════════════════════════════════════

/// Faithful port of React `SubInspector`. CONFIRM / DISMISS show only for an
/// OPEN candidate (`candidate_open`, from the candidate list) and route to
/// `on_candidate`. Otherwise the lifecycle actions valid for the status, plus
/// RECORD CHARGE / EDIT / DELETE (two-step), route to `on_write`. SNOOZE has
/// no server fn yet and stays inert.
#[component]
fn SubInspector(
    detail: Option<SubscriptionDetailDto>,
    loading: bool,
    sel: Option<String>,
    on_close: EventHandler<()>,
    candidate_open: bool,
    candidate_busy: bool,
    candidate_pending: Option<CandidateAction>,
    candidate_error: Option<String>,
    on_candidate: EventHandler<(String, CandidateAction)>,
    write_busy: bool,
    write_error: Option<String>,
    on_write: EventHandler<SubWrite>,
    on_edit: EventHandler<(String, SubscriptionForm)>,
    confirm_del: Signal<Option<String>>,
    charge: Signal<Option<ChargeDraft>>,
    #[props(default)] variant: Option<String>,
) -> Element {
    let panel_cls = match &variant {
        Some(v) => format!("sig-panel sub-insp {v}"),
        None => "sig-panel sub-insp".to_string(),
    };

    // not selected → empty body.
    let Some(sel_id) = sel.clone() else {
        return rsx! {
            aside { class: "{panel_cls}",
                div { class: "sig-empty",
                    span { class: "mk", "⊟" }
                    div { class: "tx",
                        "No subscription selected."
                        br {}
                        "Click any "
                        b { "impulse" }
                        " on the sweep or a card to inspect its price history, cadence and AI guidance."
                    }
                }
            }
        };
    };

    // selected but detail not green / not the selected sub → awaiting body.
    let ok = detail.as_ref().is_some_and(|d| d.subscription.id == sel_id);
    if !ok {
        let awaiting_margin = "margin:14px".to_string();
        return rsx! {
            aside { class: "{panel_cls}",
                div { class: "sig-head",
                    div { class: "kls", "⊟ SUBSCRIPTION" }
                    div { class: "nm", "{sel_id}" }
                    span { class: "x", onclick: move |_| on_close.call(()), title: "Close", "✕" }
                }
                Awaiting { label: "SUBSCRIPTION DETAIL".to_string(), loading, tone: "blue".to_string(), style: awaiting_margin }
            }
        };
    }

    let d = detail.unwrap();
    let s = d.subscription;
    let mo = s.monthly_equiv;
    let yr = s.annual;
    let yearly = s.cadence == "yearly";
    let recent = d.recent;
    let guidance = if !d.guidance.text.is_empty() {
        d.guidance.text.clone()
    } else {
        s.note.clone()
    };
    let sev = if !d.guidance.severity.is_empty() {
        d.guidance.severity.clone()
    } else if s.status == "due" || s.status == "watch" {
        "coral".to_string()
    } else {
        String::new()
    };
    let axis_label = if yearly { "3 YEARS" } else { "6 CHARGES" };
    // CONFIRM/DISMISS only while the record is still an OPEN proposal (a
    // dismissed one is LLM-sourced too, but can no longer be confirmed).
    let candidate = d.candidate && candidate_open;
    let (confirm_lbl, dismiss_lbl) = candidate_button_labels(candidate_pending);
    let cand_confirm_id = s.id.clone();
    let cand_dismiss_id = s.id.clone();

    // Pre-computed display strings.
    let kls = format!("⊟ SUBSCRIPTION · {}", s.category);
    let amount_big = chf_smart(s.amount, 2);
    let vs_tail = format!("per charge · {} · annualized CHF {}", s.cadence, chf(yr, 0));
    let axis_rose = if s.price_rose { "PRICE ROSE" } else { "FLAT" };
    let per_charge = format!("CHF {}", chf_smart(s.amount, 2));
    let monthly_v = format!("CHF {}", chf2(mo));
    let annual_v = format!("CHF {}", chf(yr, 0));
    let next_v = if s.next_label.is_empty() {
        "—".to_string()
    } else {
        s.next_label.clone()
    };
    let cadence_v = if yearly {
        if s.month.is_empty() {
            "YEARLY".to_string()
        } else {
            format!("YEARLY · {}", s.month)
        }
    } else if s.day != 0 {
        format!("MONTHLY · {}", s.day)
    } else {
        "MONTHLY".to_string()
    };
    let since_v = if s.since.is_empty() {
        "—".to_string()
    } else {
        s.since.clone()
    };
    let actions = d.actions;
    let can_charge = d.can_record_charge;
    let deleting = confirm_del().as_deref() == Some(s.id.as_str());
    let charge_v = charge().filter(|c| c.id == s.id);
    let edit_form = form_from_sub(&s);
    let hist = s.hist.clone();
    let cadence_str = s.cadence.clone();
    let price_rose = s.price_rose;
    let auto = s.source == "llm";
    let guidance_coral = sev == "coral";

    rsx! {
        aside { class: "{panel_cls}",
            div { class: "sig-head",
                div { class: "kls", "{kls}" }
                div { class: "nm", "{s.name}" }
                div { class: "ds", "{s.note}" }
                span { class: "x", onclick: move |_| on_close.call(()), title: "Close", "✕" }
            }

            div { class: "sig-delta",
                span { class: "big up",
                    span { style: "font-size:16px;color:var(--ink-3);margin-right:5px;vertical-align:4px", "CHF" }
                    "{amount_big}"
                }
                span { class: "vs", "{vs_tail}" }
            }

            div { class: "sig-chart",
                SubHistBars { hist, cadence: cadence_str, price_rose }
                div { class: "axis",
                    span { "{axis_label}" }
                    span { "{axis_rose}" }
                }
            }

            div { class: "sig-stats",
                div { class: "st",
                    div { class: "k", "Per charge" }
                    div { class: "v coral", "{per_charge}" }
                }
                div { class: "st",
                    div { class: "k", "Monthly" }
                    div { class: "v", "{monthly_v}" }
                }
                div { class: "st",
                    div { class: "k", "Annualized" }
                    div { class: "v", "{annual_v}" }
                }
                div { class: "st",
                    div { class: "k", "Next charge" }
                    div { class: "v", style: "font-size:16px", "{next_v}" }
                }
                div { class: "st",
                    div { class: "k", "Cadence" }
                    div { class: "v", style: "font-size:15px", "{cadence_v}" }
                }
                div { class: "st",
                    div { class: "k", "Tracked since" }
                    div { class: "v", style: "font-size:16px", "{since_v}" }
                }
            }

            div { class: "sig-recent",
                div { class: "h", "Recent charges" }
                if recent.is_empty() {
                    div { class: "dim", style: "font-size:11px;padding:8px 2px;letter-spacing:.04em", "No charges recorded yet." }
                }
                for o in recent.iter() {
                    {
                        let note_txt = if !o.note.is_empty() {
                            o.note.clone()
                        } else if auto {
                            "auto-detected".to_string()
                        } else {
                            "confirmed".to_string()
                        };
                        let pr = format!("CHF {}", chf2(o.amount));
                        rsx! {
                            div { class: "sig-occ", key: "{o.id}",
                                span { class: "dt", "{o.date}" }
                                span { class: "no", style: "flex:1", "{note_txt}" }
                                span { class: "pr", "{pr}" }
                            }
                        }
                    }
                }
            }

            div { class: "insp-acts",
                if candidate {
                    button {
                        class: "gbtn p",
                        disabled: candidate_busy,
                        onclick: move |_| on_candidate.call((cand_confirm_id.clone(), CandidateAction::Confirm)),
                        "{confirm_lbl}"
                    }
                    button {
                        class: "gbtn",
                        disabled: candidate_busy,
                        onclick: move |_| on_candidate.call((cand_dismiss_id.clone(), CandidateAction::Dismiss)),
                        "{dismiss_lbl}"
                    }
                } else {
                    for (i , a) in actions.into_iter().enumerate() {
                        {
                            let cls = match (a, i) {
                                (SubAction::Cancel, _) => "gbtn coral",
                                (_, 0) => "gbtn p",
                                _ => "gbtn",
                            };
                            let id = s.id.clone();
                            rsx! {
                                button {
                                    key: "{a.label()}",
                                    class: "{cls}",
                                    disabled: write_busy,
                                    onclick: move |_| on_write.call(SubWrite::Action(id.clone(), a)),
                                    "{a.label()}"
                                }
                            }
                        }
                    }
                }
            }

            if let Some(msg) = candidate_error {
                Awaiting {
                    label: "CANDIDATE REVIEW".to_string(),
                    message: Some(msg),
                    style: "margin:var(--s-3) var(--s-4)".to_string(),
                }
            }

            div { class: "insp-acts", style: "margin-top:var(--s-2)",
                button { class: "gbtn", "SNOOZE" }
                if can_charge && !candidate {
                    button {
                        class: "gbtn",
                        disabled: write_busy,
                        onclick: {
                            let id = s.id.clone();
                            let amount = edit_form.amount.clone();
                            move |_| charge.set(Some(ChargeDraft { id: id.clone(), amount: amount.clone(), date: String::new() }))
                        },
                        "RECORD CHARGE"
                    }
                }
                if !candidate {
                    button {
                        class: "gbtn",
                        disabled: write_busy,
                        onclick: {
                            let id = s.id.clone();
                            move |_| on_edit.call((id.clone(), edit_form.clone()))
                        },
                        "EDIT"
                    }
                    if deleting {
                        button {
                            class: "gbtn p",
                            disabled: write_busy,
                            onclick: {
                                let id = s.id.clone();
                                move |_| on_write.call(SubWrite::Delete(id.clone()))
                            },
                            if write_busy { "DELETING…" } else { "CONFIRM DELETE" }
                        }
                        button { class: "gbtn", disabled: write_busy, onclick: move |_| confirm_del.set(None), "KEEP" }
                    } else {
                        button {
                            class: "gbtn",
                            disabled: write_busy,
                            onclick: {
                                let id = s.id.clone();
                                move |_| confirm_del.set(Some(id.clone()))
                            },
                            "DELETE"
                        }
                    }
                }
            }

            if let Some(c) = charge_v {
                ChargeForm { draft: c, busy: write_busy, charge, on_write }
            }

            if let Some(msg) = write_error {
                Awaiting {
                    label: "MOD·SUB · WRITE".to_string(),
                    legend: Some("NOT SAVED".to_string()),
                    message: Some(msg),
                    style: "margin:var(--s-3) var(--s-4)".to_string(),
                }
            }

            if !guidance.is_empty() {
                div { class: "sig-foot",
                    div { class: "tx",
                        if guidance_coral {
                            b { class: "coral", "{guidance}" }
                        } else {
                            "{guidance}"
                        }
                    }
                }
            }
        }
    }
}

// ════════════════════════════════════════════════════════════════════════════
//  WRITE PATH — create / edit / delete / lifecycle (T38).
// ════════════════════════════════════════════════════════════════════════════

/// One write the page can run. All of them go through `on_write`, which allows
/// a single write in flight and refreshes every read afterwards.
#[derive(Debug, Clone, PartialEq)]
enum SubWrite {
    Create(SubscriptionForm),
    Edit(String, SubscriptionForm),
    Delete(String),
    Action(String, SubAction),
    Charge(ChargeDraft),
}

impl SubWrite {
    /// `true` for a write from the create / edit form (its errors show there);
    /// every other write comes from the inspector.
    const fn is_form_write(&self) -> bool {
        matches!(self, Self::Create(_) | Self::Edit(..))
    }
}

/// The RECORD CHARGE draft: CHF text + `YYYY-MM-DD` date (empty = today).
#[derive(Debug, Clone, PartialEq)]
struct ChargeDraft {
    id: String,
    amount: String,
    date: String,
}

const MONTH_LABELS: [&str; 12] = [
    "JAN", "FEB", "MAR", "APR", "MAY", "JUN", "JUL", "AUG", "SEP", "OCT", "NOV", "DEC",
];

/// The edit form pre-filled from a listed subscription.
fn form_from_sub(s: &SubscriptionDto) -> SubscriptionForm {
    SubscriptionForm {
        name: s.name.clone(),
        amount: crate::data::budgets::cap_input_text(s.amount),
        cadence: s.cadence.clone(),
        day: if s.day == 0 {
            String::new()
        } else {
            s.day.to_string()
        },
        // A monthly charge has no month; YEARLY must start on a real one.
        month: if s.month.is_empty() {
            "JAN".to_string()
        } else {
            s.month.clone()
        },
        category: s.category.clone(),
        glyph: s.glyph.clone(),
        note: s.note.clone(),
    }
}

/// A blank create form (monthly by default).
fn blank_form() -> SubscriptionForm {
    SubscriptionForm {
        cadence: "monthly".to_string(),
        month: "JAN".to_string(),
        ..SubscriptionForm::default()
    }
}

/// The create / edit form. `editing` is the target id; `None` creates.
#[component]
fn SubForm(
    initial: SubscriptionForm,
    editing: Option<String>,
    busy: bool,
    error: Option<String>,
    on_write: EventHandler<SubWrite>,
    on_close: EventHandler<()>,
) -> Element {
    let mut draft = use_signal(|| initial.clone());
    let d = draft.read().clone();
    let yearly = d.cadence == "yearly";
    let (legend, submit) = match (&editing, busy) {
        (Some(_), false) => ("MOD·SUB · EDIT", "SAVE"),
        (Some(_), true) => ("MOD·SUB · EDIT", "SAVING…"),
        (None, false) => ("MOD·SUB · NEW", "CREATE"),
        (None, true) => ("MOD·SUB · NEW", "CREATING…"),
    };
    let submit_write = move |_| {
        let f = draft.read().clone();
        on_write.call(match &editing {
            Some(id) => SubWrite::Edit(id.clone(), f),
            None => SubWrite::Create(f),
        });
    };

    rsx! {
        div { class: "sub-form osc-bkt blue",
            span { class: "osc-leg", "{legend}" }
            div { class: "sub-form-grid",
                label { class: "sub-f",
                    span { class: "k", "Name" }
                    input { value: "{d.name}", disabled: busy, oninput: move |e| draft.write().name = e.value() }
                }
                label { class: "sub-f",
                    span { class: "k", "Amount · CHF" }
                    input { class: "num", inputmode: "decimal", value: "{d.amount}", disabled: busy, oninput: move |e| draft.write().amount = e.value() }
                }
                label { class: "sub-f",
                    span { class: "k", "Cadence" }
                    select { value: "{d.cadence}", disabled: busy, onchange: move |e| draft.write().cadence = e.value(),
                        option { value: "monthly", "MONTHLY" }
                        option { value: "yearly", "YEARLY" }
                    }
                }
                if yearly {
                    label { class: "sub-f",
                        span { class: "k", "Month" }
                        select { value: "{d.month}", disabled: busy, onchange: move |e| draft.write().month = e.value(),
                            for m in MONTH_LABELS {
                                option { key: "{m}", value: "{m}", "{m}" }
                            }
                        }
                    }
                } else {
                    label { class: "sub-f",
                        span { class: "k", "Day of month" }
                        input { class: "num", inputmode: "numeric", value: "{d.day}", disabled: busy, oninput: move |e| draft.write().day = e.value() }
                    }
                }
                label { class: "sub-f",
                    span { class: "k", "Category" }
                    input { value: "{d.category}", disabled: busy, oninput: move |e| draft.write().category = e.value() }
                }
                label { class: "sub-f",
                    span { class: "k", "Glyph" }
                    input { value: "{d.glyph}", disabled: busy, oninput: move |e| draft.write().glyph = e.value() }
                }
                label { class: "sub-f wide",
                    span { class: "k", "Note" }
                    input { value: "{d.note}", disabled: busy, oninput: move |e| draft.write().note = e.value() }
                }
            }
            if let Some(msg) = error {
                Awaiting {
                    label: "MOD·SUB · SAVE".to_string(),
                    legend: Some("NOT SAVED".to_string()),
                    message: Some(msg),
                    style: "margin-top:var(--s-3)".to_string(),
                }
            }
            div { class: "sub-form-acts",
                button { class: "gbtn", disabled: busy, onclick: move |_| on_close.call(()), "CLOSE" }
                button { class: "gbtn p", disabled: busy, onclick: submit_write, "{submit}" }
            }
        }
    }
}

/// The inspector's RECORD CHARGE form (amount + date).
#[component]
fn ChargeForm(
    draft: ChargeDraft,
    busy: bool,
    charge: Signal<Option<ChargeDraft>>,
    on_write: EventHandler<SubWrite>,
) -> Element {
    let submit = if busy { "RECORDING…" } else { "RECORD" };
    let send = draft.clone();
    rsx! {
        div { class: "sub-form osc-bkt blue", style: "margin:var(--s-3) var(--s-4)",
            span { class: "osc-leg", "MOD·SUB · CHARGE" }
            div { class: "sub-form-grid",
                label { class: "sub-f",
                    span { class: "k", "Amount · CHF" }
                    input {
                        class: "num",
                        inputmode: "decimal",
                        value: "{draft.amount}",
                        disabled: busy,
                        oninput: move |e| {
                            if let Some(c) = charge.write().as_mut() {
                                c.amount = e.value();
                            }
                        },
                    }
                }
                label { class: "sub-f",
                    span { class: "k", "Date · empty = today" }
                    input {
                        class: "num",
                        placeholder: "YYYY-MM-DD",
                        value: "{draft.date}",
                        disabled: busy,
                        oninput: move |e| {
                            if let Some(c) = charge.write().as_mut() {
                                c.date = e.value();
                            }
                        },
                    }
                }
            }
            div { class: "sub-form-acts",
                button { class: "gbtn", disabled: busy, onclick: move |_| charge.set(None), "CLOSE" }
                button { class: "gbtn p", disabled: busy, onclick: move |_| on_write.call(SubWrite::Charge(send.clone())), "{submit}" }
            }
        }
    }
}

// ════════════════════════════════════════════════════════════════════════════
//  AI RECURRING CANDIDATES — human review (confirm / dismiss).
// ════════════════════════════════════════════════════════════════════════════

/// The human decision a candidate button carries. CONFIRM is the approval
/// that turns an AI proposal into a tracked subscription; nothing else does.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CandidateAction {
    Confirm,
    Dismiss,
}

/// `(confirm, dismiss)` button labels, showing which decision is in flight.
fn candidate_button_labels(pending: Option<CandidateAction>) -> (&'static str, &'static str) {
    match pending {
        Some(CandidateAction::Confirm) => ("CONFIRMING…", "DISMISS"),
        Some(CandidateAction::Dismiss) => ("CONFIRM", "DISMISSING…"),
        None => ("CONFIRM", "DISMISS"),
    }
}

/// The user-facing text of a failed candidate call. The server already sends a
/// fixed, sanitized message; a transport failure gets a generic line.
fn candidate_error_text(e: &ServerFnError) -> String {
    match e {
        ServerFnError::ServerError { message, .. } => message.clone(),
        _ => "Could not reach the server. Refresh to see the current state.".to_string(),
    }
}

/// One open candidate: evidence, amount, confidence and the two decisions.
#[component]
fn CandidateRow(
    c: RecurringCandidateDto,
    busy: bool,
    pending: Option<CandidateAction>,
    on_action: EventHandler<(String, CandidateAction)>,
    on_select: EventHandler<String>,
) -> Element {
    let amount = chf_smart(c.amount, 2);
    let unit = cad_unit(&c.cadence);
    let yearly = c.cadence == "yearly";
    let conf_pct = format!("{:.0}", c.confidence * 100.0);
    let (conf_cls, conf_tag) = if c.low_confidence {
        ("cand-conf low", "LOW CONF · REVIEW")
    } else {
        ("cand-conf", "CONF")
    };
    let (confirm_lbl, dismiss_lbl) = candidate_button_labels(pending);
    let id_select = c.id.clone();
    let id_confirm = c.id.clone();
    let id_dismiss = c.id.clone();

    rsx! {
        div { class: "cand-row",
            div {
                class: "cand-id",
                role: "button",
                tabindex: 0,
                title: "{c.rationale}",
                onclick: move |_| on_select.call(id_select.clone()),
                span { class: "cand-nm", "{c.name}" }
                span { class: "cand-ev",
                    b { "{c.occurrences}" }
                    " matching charges · "
                    if yearly {
                        "yearly"
                    } else {
                        "monthly · day "
                        b { "{c.day}" }
                    }
                }
            }
            span { class: "cand-amt",
                "CHF "
                b { "{amount}" }
                " {unit}"
            }
            span { class: "{conf_cls}",
                "{conf_tag} "
                b { "{conf_pct}%" }
            }
            div { class: "cand-acts",
                button {
                    class: "gbtn p",
                    disabled: busy,
                    onclick: move |_| on_action.call((id_confirm.clone(), CandidateAction::Confirm)),
                    "{confirm_lbl}"
                }
                button {
                    class: "gbtn",
                    disabled: busy,
                    onclick: move |_| on_action.call((id_dismiss.clone(), CandidateAction::Dismiss)),
                    "{dismiss_lbl}"
                }
            }
        }
    }
}

/// The AI CANDIDATES section: header, action error, then the loading / error /
/// empty state or the open candidates. `busy` disables every decision while
/// one is in flight or the list is refreshing (double clicks are no-ops).
#[component]
fn CandidateSection(
    candidates: Option<Vec<RecurringCandidateDto>>,
    loading: bool,
    load_error: Option<String>,
    busy: bool,
    pending: Option<(String, CandidateAction)>,
    action_error: Option<String>,
    on_action: EventHandler<(String, CandidateAction)>,
    on_select: EventHandler<String>,
) -> Element {
    let count = candidates
        .as_ref()
        .map_or_else(|| "—".to_string(), |l| l.len().to_string());
    let state_style = "margin-bottom:var(--s-5)".to_string();

    rsx! {
        div { class: "subs-sec",
            span { class: "lbl", "⌁ AI CANDIDATES" }
            span { class: "ct ind", "{count}" }
            span { class: "rule" }
            span { class: "meta", "PROPOSED BY THE MODEL · TRACKED ONLY ONCE YOU CONFIRM" }
        }
        if let Some(msg) = action_error {
            Awaiting {
                label: "CANDIDATE REVIEW".to_string(),
                message: Some(msg),
                style: "margin-bottom:var(--s-4)".to_string(),
            }
        }
        match candidates {
            None => rsx! {
                Awaiting {
                    label: "AI CANDIDATES".to_string(),
                    loading,
                    message: load_error,
                    style: state_style,
                }
            },
            Some(list) if list.is_empty() => rsx! {
                Awaiting {
                    label: "AI CANDIDATES".to_string(),
                    message: Some("No open candidates. Nothing is waiting for your approval.".to_string()),
                    style: state_style,
                }
            },
            Some(list) => rsx! {
                div { class: "cand-list osc-glass hair osc-bkt blue",
                    span { class: "osc-leg", "MOD·SUB · CANDIDATES" }
                    for c in list {
                        {
                            let row_pending = pending
                                .as_ref()
                                .filter(|(id, _)| *id == c.id)
                                .map(|(_, action)| *action);
                            let key = c.id.clone();
                            rsx! {
                                CandidateRow {
                                    key: "{key}",
                                    c,
                                    busy,
                                    pending: row_pending,
                                    on_action,
                                    on_select,
                                }
                            }
                        }
                    }
                }
            },
        }
    }
}

// ════════════════════════════════════════════════════════════════════════════
//  PAGE COMPONENT
// ════════════════════════════════════════════════════════════════════════════

/// The composed Phoskonomia subscriptions page.
#[component]
pub fn SubscriptionsPage() -> Element {
    // ---- UI state (was: useTweaks store + useState) ----
    // The floating tweaks panel (authoring tooling) is intentionally not ported;
    // its defaults become plain signals driven by the in-page `.subs-controls`.
    let mut ai_collapsed = use_signal(|| false);
    let mut sel = use_signal(|| Option::<String>::None);
    let mut sub_view = use_signal(|| "cards".to_string()); // cards | rows
    let mut sub_sort = use_signal(|| "due".to_string()); // due | amount | name
    let sub_amounts = use_signal(|| "monthly".to_string()); // monthly | annual
    let mut sub_group = use_signal(|| false);

    // ---- backend data loads (was: useGet) ----
    let cycle = use_resource(get_cycle);
    // list re-fetches when the sort/group/amounts signals change.
    let list = use_resource(move || {
        let filter = SubFilter {
            sort: sub_sort(),
            group: if sub_group() {
                "cadence".to_string()
            } else {
                String::new()
            },
            amounts: sub_amounts(),
        };
        list_subscriptions(filter)
    });
    let stats = use_resource(get_subscription_stats);
    let sweep = use_resource(get_billing_sweep);
    // selected-sub detail — fetched on demand when something is selected.
    let detail = use_resource(move || async move {
        match sel() {
            Some(id) => Some(get_subscription(id).await),
            None => None,
        }
    });

    // ---- read resources into local snapshots ----
    let c = match &*cycle.read() {
        Some(Ok(v)) => v.clone(),
        _ => empty_cycle(),
    };
    let cycle_days = if c.days == 0 { 30 } else { c.days };
    let subs: Vec<SubscriptionDto> = list
        .read()
        .as_ref()
        .and_then(|r| r.as_ref().ok())
        .cloned()
        .unwrap_or_default();
    let list_loading = list.read().is_none();
    let list_ok = list.read().as_ref().is_some_and(|r| r.is_ok()) && !subs.is_empty();

    let stats_v = stats.read().as_ref().and_then(|r| r.as_ref().ok()).cloned();

    let sweep_v = sweep.read().as_ref().and_then(|r| r.as_ref().ok()).cloned();
    let sweep_loading = sweep.read().is_none();

    let detail_v: Option<SubscriptionDetailDto> = match &*detail.read() {
        Some(Some(Ok(d))) => Some(d.clone()),
        _ => None,
    };
    let detail_loading = matches!(&*detail.read(), None | Some(None));

    // ---- AI recurring candidates: open list + human decisions ----
    let candidates = use_resource(list_recurring_candidates);
    let mut cand_pending = use_signal(|| Option::<(String, CandidateAction)>::None);
    // `(candidate id, message)` of the last failed decision.
    let mut cand_error = use_signal(|| Option::<(String, String)>::None);
    let on_candidate = use_callback(move |(id, action): (String, CandidateAction)| {
        // One decision at a time and never against a stale list: a second
        // click while a call is in flight or the list refreshes is a no-op.
        if cand_pending.peek().is_some() || candidates.pending() {
            return;
        }
        cand_pending.set(Some((id.clone(), action)));
        cand_error.set(None);
        let mut candidates = candidates;
        let mut list = list;
        let mut stats = stats;
        let mut sweep = sweep;
        let mut detail = detail;
        spawn(async move {
            let result = match action {
                CandidateAction::Confirm => confirm_recurring_candidate(id.clone()).await,
                CandidateAction::Dismiss => dismiss_recurring_candidate(id.clone()).await,
            };
            if let Err(e) = result {
                cand_error.set(Some((id, candidate_error_text(&e))));
            }
            // Whatever the outcome, re-read the candidates AND the standing
            // charges (plus the roll-ups and inspector derived from them).
            // `restart` flips the candidate list to pending synchronously, so
            // the buttons stay disabled until the fresh list lands.
            candidates.restart();
            list.restart();
            stats.restart();
            sweep.restart();
            detail.restart();
            cand_pending.set(None);
        });
    });

    // ---- write path: create / edit / delete / lifecycle ----
    let mut write_pending = use_signal(|| false);
    // The last failed write's message, and whether it came from the form.
    let mut write_error = use_signal(|| Option::<(bool, String)>::None);
    // `(edit target, initial form)` of the open form; target `None` creates.
    let mut form = use_signal(|| Option::<(Option<String>, SubscriptionForm)>::None);
    let mut confirm_del = use_signal(|| Option::<String>::None);
    let mut charge = use_signal(|| Option::<ChargeDraft>::None);
    let on_write = use_callback(move |w: SubWrite| {
        // One write at a time, never against a list that is still refreshing.
        if *write_pending.peek() || list.pending() {
            return;
        }
        write_pending.set(true);
        write_error.set(None);
        let mut candidates = candidates;
        let mut list = list;
        let mut stats = stats;
        let mut sweep = sweep;
        let mut detail = detail;
        spawn(async move {
            let from_form = w.is_form_write();
            // Only the UI state belonging to the write that succeeded closes:
            // a PAUSE never discards a half-typed create/edit form.
            let done = w.clone();
            let result = match w {
                SubWrite::Create(f) => create_subscription(f).await.map(Some),
                SubWrite::Edit(id, f) => edit_subscription(id, f).await.map(|()| None),
                SubWrite::Delete(id) => delete_subscription(id).await.map(|()| None),
                SubWrite::Action(id, a) => run_subscription_action(id, a).await.map(|()| None),
                SubWrite::Charge(c) => record_subscription_charge(c.id, c.amount, c.date)
                    .await
                    .map(|()| None),
            };
            match (result, done) {
                (Ok(created), SubWrite::Create(_)) => {
                    form.set(None);
                    sel.set(created);
                }
                (Ok(_), SubWrite::Edit(..)) => form.set(None),
                (Ok(_), SubWrite::Charge(_)) => charge.set(None),
                (Ok(_), SubWrite::Delete(id)) => {
                    confirm_del.set(None);
                    if form.peek().as_ref().and_then(|(t, _)| t.as_deref()) == Some(id.as_str()) {
                        form.set(None);
                    }
                    if charge.peek().as_ref().map(|c| c.id.as_str()) == Some(id.as_str()) {
                        charge.set(None);
                    }
                    sel.set(None);
                }
                (Ok(_), SubWrite::Action(..)) => {}
                (Err(e), _) => write_error.set(Some((from_form, candidate_error_text(&e)))),
            }
            // Whatever the outcome, re-read everything derived from the store.
            list.restart();
            stats.restart();
            sweep.restart();
            detail.restart();
            candidates.restart();
            write_pending.set(false);
        });
    });
    let list_refreshing = *list.state().read() == UseResourceState::Pending;
    let write_busy = write_pending() || list_refreshing;
    let form_v = form();
    let form_error = write_error()
        .filter(|(from_form, _)| *from_form && form_v.is_some())
        .map(|(_, msg)| msg);
    let insp_error = write_error()
        .filter(|(from_form, _)| !*from_form)
        .map(|(_, msg)| msg);

    let cand_refreshing = *candidates.state().read() == UseResourceState::Pending;
    let cand_list: Option<Vec<RecurringCandidateDto>> = candidates
        .read()
        .as_ref()
        .and_then(|r| r.as_ref().ok())
        .cloned();
    let cand_load_error: Option<String> = candidates
        .read()
        .as_ref()
        .and_then(|r| r.as_ref().err())
        .map(candidate_error_text);
    let cand_loading = candidates.read().is_none();
    let cand_pending_v = cand_pending();
    let cand_busy = cand_pending_v.is_some() || cand_refreshing;
    let cand_error_v = cand_error();
    let sel_candidate_open = sel().is_some_and(|id| {
        cand_list
            .as_ref()
            .is_some_and(|l| l.iter().any(|c| c.id == id))
    });
    let sel_candidate_pending = cand_pending_v
        .as_ref()
        .filter(|(id, _)| sel().as_deref() == Some(id.as_str()))
        .map(|(_, action)| *action);
    let sel_candidate_error = cand_error_v
        .as_ref()
        .filter(|(id, _)| sel().as_deref() == Some(id.as_str()))
        .map(|(_, msg)| msg.clone());
    let section_error = cand_error_v.map(|(_, msg)| msg);

    let sel_id = sel();
    let amount_mode = sub_amounts();
    let view = sub_view();
    let grouped = sub_group();

    // ---- pre-computed header / KPI strings ----
    const DASH: &str = "—";
    let count_str = stats_v
        .as_ref()
        .map_or(DASH.to_string(), |s| s.count.to_string());
    let monthly_str = stats_v
        .as_ref()
        .map_or(DASH.to_string(), |s| chf(s.monthly, 0));
    let annual_str = stats_v
        .as_ref()
        .map_or(DASH.to_string(), |s| chf(s.annual, 0));
    let cycle_tail = if c.label.is_empty() {
        String::new()
    } else {
        format!(" · {}", c.label)
    };

    // KPI: monthly run-rate sub-line.
    let kpi_mo_sub = stats_v.as_ref().map_or(DASH.to_string(), |s| {
        let mut t = format!("{} active", s.count);
        if s.auto_count > 0 {
            t.push_str(&format!(" · {} auto-detected by GEMMA4", s.auto_count));
        }
        t
    });
    // KPI: next 30 days.
    let next30_count_lbl = stats_v
        .as_ref()
        .map_or(DASH.to_string(), |s| format!("{} CHARGES", s.next30.count));
    let next30_total = stats_v
        .as_ref()
        .map_or(DASH.to_string(), |s| chf(s.next30.total, 0));
    let next30_sub = stats_v
        .as_ref()
        .map_or("Nothing scheduled".to_string(), |s| {
            match s.next30.items.first() {
                Some(it) => format!("Next · {} in {}d", it.name, it.days_until),
                None => "Nothing scheduled".to_string(),
            }
        });
    // KPI: needs attention.
    let flagged_count = stats_v
        .as_ref()
        .map_or(DASH.to_string(), |s| s.flagged.count.to_string());
    let flagged_note = stats_v.as_ref().map_or_else(
        || "Awaiting backend".to_string(),
        |s| {
            if s.flagged.note.is_empty() {
                "Flagged by GEMMA4 for review".to_string()
            } else {
                s.flagged.note.clone()
            }
        },
    );

    // ---- grouping (display split only; backend owns ordering) ----
    struct Group {
        label: Option<String>,
        items: Vec<SubscriptionDto>,
    }
    let groups: Vec<Group> = if !grouped {
        vec![Group {
            label: None,
            items: subs.clone(),
        }]
    } else {
        let m: Vec<SubscriptionDto> = subs
            .iter()
            .filter(|s| s.cadence == "monthly")
            .cloned()
            .collect();
        let y: Vec<SubscriptionDto> = subs
            .iter()
            .filter(|s| s.cadence == "yearly")
            .cloned()
            .collect();
        let mut out = Vec::new();
        if !m.is_empty() {
            out.push(Group {
                label: Some(format!("MONTHLY · {}", m.len())),
                items: m,
            });
        }
        if !y.is_empty() {
            out.push(Group {
                label: Some(format!("YEARLY · {}", y.len())),
                items: y,
            });
        }
        out
    };

    // control button classes (pre-computed).
    let sort_due_cls = if sub_sort() == "due" { "m on" } else { "m" };
    let sort_amount_cls = if sub_sort() == "amount" { "m on" } else { "m" };
    let sort_name_cls = if sub_sort() == "name" { "m on" } else { "m" };
    let group_cls = if grouped { "m on" } else { "m" };
    let view_cards_cls = if view == "cards" { "m on" } else { "m" };
    let view_rows_cls = if view == "rows" { "m on" } else { "m" };

    let cycle_label = c.label.clone();
    let topbar_date = if c.days == 0 {
        String::new()
    } else {
        format!("{} · DAY {}/{}", c.label, c.day, c.days)
    };
    // narrow var hl-auto effect: React toggled body class on subHlAuto — that
    // tweak control lived only in the (un-ported) tweaks panel, so it stays off.

    rsx! {
        div { class: "pk", style: "height:100vh;min-height:0",
            ScannerBg {
                class: "pk-bg".to_string(),
                seed: 71,
                shapes: r#"[
                    { char: "6", cx: .88, cy: .5, scale: .42, style: "red", morph: "vein", live: true, fill: .52 },
                    { char: "2", cx: .16, cy: .26, scale: .3, style: "faint", morph: "blob", live: false, fill: .5 },
                    { char: "9", cx: .4, cy: .14, scale: .16, style: "wire", morph: "vein", live: false, fill: .34 },
                    { char: "0", cx: .27, cy: .86, scale: .26, style: "faint", morph: "blob", live: false, fill: .42 }
                ]"#.to_string(),
            }

            div { class: "app-shell swap",
                AiPanelSubs { collapsed: ai_collapsed(), on_toggle: move |()| ai_collapsed.toggle() }

                div { class: "app-main",
                    TopBar { active: "SUBSCRIPTIONS".to_string(), date_text: topbar_date }
                    div { class: "app-scroll", "data-screen-label": "SUBSCRIPTIONS",
                        div { class: "subs-wrap",

                            // ===================== HEADER + CONTROLS =====================
                            div { class: "subs-top",
                                div {
                                    div { class: "ttl", "Subscriptions" }
                                    div { class: "sum",
                                        b { "{count_str}" }
                                        " standing charges · "
                                        span { class: "coral", "CHF {monthly_str}" }
                                        "/mo · "
                                        b { " CHF {annual_str}" }
                                        "/yr{cycle_tail}"
                                    }
                                }
                                div { class: "subs-controls",
                                    crate::components::csv_export::CsvExport { kind: crate::data::csv_export::CsvExportKind::Subscriptions }
                                    div { class: "modes",
                                        span { class: "mlbl", "SORT" }
                                        button { class: "{sort_due_cls}", onclick: move |_| sub_sort.set("due".to_string()), "DUE" }
                                        button { class: "{sort_amount_cls}", onclick: move |_| sub_sort.set("amount".to_string()), "COST" }
                                        button { class: "{sort_name_cls}", onclick: move |_| sub_sort.set("name".to_string()), "A–Z" }
                                    }
                                    div { class: "modes",
                                        button { class: "{group_cls}", onclick: move |_| sub_group.toggle(), "⊞ GROUP BY CADENCE" }
                                    }
                                    div { class: "modes",
                                        button { class: "{view_cards_cls}", onclick: move |_| sub_view.set("cards".to_string()), "▦ CARDS" }
                                        button { class: "{view_rows_cls}", onclick: move |_| sub_view.set("rows".to_string()), "≡ ROWS" }
                                    }
                                    div { class: "modes",
                                        button { class: "m", title: "AI: scan transactions for recurring charges", "⌁ DETECT" }
                                        button {
                                            class: "m",
                                            disabled: write_busy,
                                            onclick: move |_| {
                                                write_error.set(None);
                                                form.set(Some((None, blank_form())));
                                            },
                                            "+ NEW"
                                        }
                                    }
                                }
                            }

                            // ===================== KPI BAND =====================
                            div { class: "subs-kpis",
                                div { class: "subs-kpi accent",
                                    div { class: "lbl",
                                        span { "MONTHLY RECURRING" }
                                        span { "RUN-RATE" }
                                    }
                                    div { class: "big",
                                        span { class: "cur", "CHF" }
                                        "{monthly_str}"
                                    }
                                    div { class: "ksub", "{kpi_mo_sub}" }
                                }
                                div { class: "subs-kpi blue",
                                    div { class: "lbl",
                                        span { "ANNUALIZED" }
                                        span { "12 MO" }
                                    }
                                    div { class: "big",
                                        span { class: "cur", "CHF" }
                                        "{annual_str}"
                                    }
                                    div { class: "ksub", "Committed across every standing charge" }
                                }
                                div { class: "subs-kpi",
                                    div { class: "lbl",
                                        span { "NEXT 30 DAYS" }
                                        span { "{next30_count_lbl}" }
                                    }
                                    div { class: "big",
                                        span { class: "cur", "CHF" }
                                        "{next30_total}"
                                    }
                                    div { class: "ksub", "{next30_sub}" }
                                }
                                div { class: "subs-kpi",
                                    div { class: "lbl",
                                        span { "NEEDS ATTENTION" }
                                        span { "AI" }
                                    }
                                    div { class: "big", style: "color:var(--neon);text-shadow:var(--glow-text)", "{flagged_count}" }
                                    div { class: "ksub", "{flagged_note}" }
                                }
                            }

                            // ===================== BILLING SWEEP =====================
                            BillingSweep {
                                sweep_v: sweep_v.clone(),
                                loading: sweep_loading,
                                sel: sel_id.clone(),
                                on_select: move |id: String| {
                                    let cur = sel();
                                    sel.set(if cur.as_deref() == Some(id.as_str()) { None } else { Some(id) });
                                },
                                cycle_label: cycle_label.clone(),
                            }

                            if let Some((target, initial)) = form_v {
                                SubForm {
                                    key: "{target.clone().unwrap_or_default()}",
                                    initial,
                                    editing: target,
                                    busy: write_busy,
                                    error: form_error,
                                    on_write,
                                    on_close: move |()| {
                                        write_error.set(None);
                                        form.set(None);
                                    },
                                }
                            }

                            // ===================== AI CANDIDATES =====================
                            CandidateSection {
                                candidates: cand_list,
                                loading: cand_loading,
                                load_error: cand_load_error,
                                busy: cand_busy,
                                pending: cand_pending_v,
                                action_error: section_error,
                                on_action: on_candidate,
                                on_select: move |id: String| sel.set(Some(id)),
                            }

                            // ===================== STANDING CHARGES =====================
                            div { class: "subs-sec",
                                span { class: "lbl", "⊟ STANDING CHARGES" }
                                span { class: "ct", "{count_str}" }
                                span { class: "rule" }
                                span { class: "meta", "▌ IMPULSE = CHARGE · ━ CYCLE COUNTDOWN · CLICK TO INSPECT" }
                            }

                            if !list_ok {
                                Awaiting { label: "STANDING CHARGES".to_string(), loading: list_loading, tone: "blue".to_string() }
                            } else {
                                for (gi , g) in groups.iter().enumerate() {
                                    {
                                        let items = g.items.clone();
                                        let group_label = g.label.clone();
                                        let key = group_label.clone().unwrap_or_else(|| gi.to_string());
                                        let view = view.clone();
                                        let amount_mode = amount_mode.clone();
                                        let sel_id = sel_id.clone();
                                        rsx! {
                                            div { key: "{key}",
                                                if let Some(lbl) = group_label {
                                                    div { class: "subs-group",
                                                        "{lbl}"
                                                        span { class: "gr" }
                                                    }
                                                }
                                                if view == "rows" {
                                                    div { class: "sub-rows",
                                                        for s in items.iter() {
                                                            div { key: "{s.id}", "data-src": "{s.source}",
                                                                SubRow {
                                                                    s: s.clone(),
                                                                    active: sel_id.as_deref() == Some(s.id.as_str()),
                                                                    on_select: move |id: String| {
                                                                        let cur = sel();
                                                                        sel.set(if cur.as_deref() == Some(id.as_str()) { None } else { Some(id) });
                                                                    },
                                                                    amount_mode: amount_mode.clone(),
                                                                    cycle_days,
                                                                }
                                                            }
                                                        }
                                                    }
                                                } else {
                                                    div { class: "sub-grid",
                                                        for s in items.iter() {
                                                            div { key: "{s.id}", "data-src": "{s.source}", style: "display:contents",
                                                                SubCard {
                                                                    s: s.clone(),
                                                                    active: sel_id.as_deref() == Some(s.id.as_str()),
                                                                    on_select: move |id: String| {
                                                                        let cur = sel();
                                                                        sel.set(if cur.as_deref() == Some(id.as_str()) { None } else { Some(id) });
                                                                    },
                                                                    amount_mode: amount_mode.clone(),
                                                                    cycle_days,
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
                    }
                }

                // right-dock inspector
                SubInspector {
                    detail: detail_v.clone(),
                    loading: detail_loading,
                    sel: sel_id.clone(),
                    on_close: move |()| sel.set(None),
                    candidate_open: sel_candidate_open,
                    candidate_busy: cand_busy,
                    candidate_pending: sel_candidate_pending,
                    candidate_error: sel_candidate_error,
                    on_candidate,
                    write_busy,
                    write_error: insp_error,
                    on_write,
                    on_edit: move |(id, f): (String, SubscriptionForm)| {
                        write_error.set(None);
                        form.set(Some((Some(id), f)));
                    },
                    confirm_del,
                    charge,
                }
            }
        }
    }
}

/// Thin subscriptions wrapper around the shared `AiPanel` (left assistant) — same
/// pattern the dashboard uses to carry `collapsed`/`on_toggle` cleanly past the
/// page-state closure. Owns its own `use_resource` over the seeded `get_ai_panel`
/// `#[server]` fn (React's `/ai/feed` + `/ai/chat` + `/ai/status`) and maps the
/// payload onto the panel's `feed`/`msgs` and `online`/`model`/`engine`/`location`
/// props, so the assistant renders a populated live feed, chat transcript and
/// online pulse — faithful to the React page (vs the empty awaiting states).
#[component]
fn AiPanelSubs(collapsed: bool, on_toggle: EventHandler<()>) -> Element {
    let ai = use_resource(get_ai_panel);
    let ai_v = ai.read().as_ref().and_then(|r| r.as_ref().ok()).cloned();

    // Map the serde DTOs onto the panel's component props (empty until green —
    // the panel's own awaiting bodies cover that tri-state).
    let feed: Vec<FeedItem> = ai_v
        .as_ref()
        .map(|p| {
            p.feed
                .iter()
                .map(|f| FeedItem {
                    id: f.id.clone(),
                    kind: f.kind.clone(),
                    text: f.text.clone(),
                    conf: f.conf,
                    state: f.state.clone(),
                    time: f.time.clone(),
                    actions: f.actions.clone(),
                    cand: f.cand,
                })
                .collect()
        })
        .unwrap_or_default();
    let msgs: Vec<ChatMsg> = ai_v
        .as_ref()
        .map(|p| {
            p.msgs
                .iter()
                .map(|m| ChatMsg {
                    who: m.who.clone(),
                    text: m.text.clone(),
                })
                .collect()
        })
        .unwrap_or_default();
    let online = ai_v.as_ref().is_some_and(|p| p.status.online);
    let model = ai_v
        .as_ref()
        .map_or_else(|| "GEMMA4".to_string(), |p| p.status.model.clone());
    let engine = ai_v
        .as_ref()
        .map_or_else(|| "OLLAMA".to_string(), |p| p.status.engine.clone());
    let location = ai_v
        .as_ref()
        .map_or_else(|| "LOCAL".to_string(), |p| p.status.location.clone());

    rsx! {
        AiPanel {
            collapsed,
            on_toggle: move |()| on_toggle.call(()),
            feed,
            msgs,
            online,
            model,
            engine,
            location,
        }
    }
}
