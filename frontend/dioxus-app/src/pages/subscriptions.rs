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
//!   * mutations (DETECT / MARK PAID / PAUSE / CANCEL / CONFIRM / DISMISS /
//!     SNOOZE) have no F3 server fn yet, so the buttons render faithfully but are
//!     inert (mirroring how the dashboard's `AlertItem` actions are empty).

use dioxus::prelude::*;
use phosk_core::money::Money;

use crate::components::prims::{Dot, ScannerBg, Spark};
use crate::components::shell::{AiPanel, ChatMsg, FeedItem, TopBar};
use crate::components::states::Awaiting;
use crate::data::ai::get_ai_panel;
use crate::data::subscriptions::{
    get_billing_sweep, get_subscription, get_subscription_stats, list_subscriptions,
    BillingSweepDto, ImpulseDto, SubFilter, SubscriptionDetailDto, SubscriptionDto,
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

/// Faithful port of React `SubInspector`. Lifecycle actions (MARK PAID / PAUSE /
/// CANCEL / RESUME / CONFIRM / DISMISS / SNOOZE) have no F3 server fn yet, so the
/// buttons render exactly but are inert (mirroring the dashboard's empty actions).
#[component]
fn SubInspector(
    detail: Option<SubscriptionDetailDto>,
    loading: bool,
    sel: Option<String>,
    on_close: EventHandler<()>,
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
    let candidate = d.candidate;

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
    let primary_label = if s.status == "due" {
        "MARK PAID"
    } else {
        "PAUSE"
    };
    let paused = s.status == "paused";
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
                    button { class: "gbtn p", "CONFIRM" }
                    button { class: "gbtn", "DISMISS" }
                } else {
                    button { class: "gbtn p", "{primary_label}" }
                    if paused {
                        button { class: "gbtn", "RESUME" }
                    } else {
                        button { class: "gbtn coral", "CANCEL" }
                    }
                }
            }

            div { class: "insp-acts", style: "margin-top:8px",
                button { class: "gbtn", "SNOOZE" }
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
