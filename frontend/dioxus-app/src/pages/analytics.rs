//! Analytics page (route `/analytics`). Faithful port of React `pages/Analytics.jsx`.
//!
//! The retrospective read: a 12-cycle SPEND TREND oscilloscope (hand-written
//! inline SVG, no F2 primitive — ported verbatim as a page-local component), the
//! ITEM-SIGNAL matrix, the fastest riser/faller mover cards + GEMMA4 read,
//! per-category MOMENTUM small-multiples, and a weekday SPENDING RHYTHM heatmap.
//!
//! Shape preserved 1:1 with the JSX: the `.pk` root + `ScannerBg`, the
//! `.app-shell.swap` with the left `AiPanel`, the `.app-main` (TopBar + scroll),
//! the `.an-wrap` (top header + mode toggles, KPI band, spend-trend hero,
//! item-signals + movers/insight, category momentum, spending rhythm), and the
//! docked-or-drawered `SignalPanel`.
//!
//! Data: where React fanned out `useGet` over the dead REST layer, this fans out
//! `use_resource` over the F3 `#[server]` fns. The on-demand selected-signal
//! detail is a `use_resource` over the `sel` signal (refetches when it changes).
//!
//! Tweaks: React's `useTweaks`/`TweaksPanel` is the `@ds-adherence-ignore`
//! authoring tooling — NOT ported. Its tweak-driven interactivity (trend mode,
//! window, signal/momentum sort, section toggles, inspector dock-vs-drawer) is
//! preserved as plain `use_signal` UI state with the same defaults
//! (`AN_TWEAK_DEFAULTS`). The `an-modes` toggle buttons drive `trend_mode`.
//!
//! JSX idioms → RSX (per the page-porting playbook):
//!   * `useState` → `use_signal`; resize `useEffect` → `use_effect` + `document::eval`.
//!   * `useGet(...)` → `use_resource(move || server_fn())`; on-demand → over `sel`.
//!   * `useMemo` sort → a plain sorted `Vec` recomputed each render.
//!   * `.map(...)` → `for x in iter` (with `{ ... rsx!{} }` blocks for per-item locals).
//!   * compound display strings are pre-computed into `let`s above the `rsx!`.

use dioxus::prelude::*;
use phosk_core::money::Money;

use crate::components::prims::{Dot, ScannerBg, Spark};
use crate::components::shell::{AiPanel, ChatMsg, Sig, SigOcc, SigSpark, SignalPanel, TopBar};
use crate::components::states::Awaiting;
use crate::data::analytics::{
    get_analytics_insight, get_category_momentum, get_rhythm, get_spend_history, get_spend_stats,
    AnalyticsInsightDto, MomentumDto, RhythmDto, SpendHistoryDto, SpendPointDto, SpendStatsDto,
};
use crate::data::cycle::{get_cycle, CycleDto};
use crate::data::signals::{
    get_movers, get_signal, get_signal_candidates, get_signals, MoversDto, SignalDetailDto,
    SignalDto,
};
use crate::data::{chf, chf2};

// ── tiny presentation helpers (faithful to the JSX `chf(v,0)` / `num`) ────────

/// Whole-CHF (0 decimals).
fn chf0(m: Money) -> String {
    chf(m, 0)
}

/// `Money` → presentation `f64` CHF, for the spark primitives.
fn money_chf(m: Money) -> f64 {
    m.as_chf_f64()
}

// ── DTO → F2 `Sig` mapper (faithful to dashboard.rs `sig_of_detail`) ──────────
// The matrix rows render the raw `SignalDto` (via `ItemSignalRow`), so only the
// inspector's fully-populated `Sig` needs mapping here.

/// `SignalDetailDto` → the fully-populated `Sig` the inspector panel renders.
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

// ════════════════════════════════════════════════════════════════════════════
// SPEND TREND (hero) — JSX `SpendTrend`. Hand-written inline SVG ported verbatim
// (gridlines, budget reference, molten bars w/ projected-hollow, connecting trace
// + area, point markers, x labels). `is_rate` swaps to the savings-rate curve.
// ════════════════════════════════════════════════════════════════════════════

#[component]
fn SpendTrend(points: Vec<SpendPointDto>, stats: Option<SpendStatsDto>, is_rate: bool) -> Element {
    let data = points;
    let w = 1000.0_f64;
    let h = 212.0_f64;
    let pad_l = 52.0_f64;
    let pad_r = 20.0_f64;
    let pad_t = 24.0_f64;
    let pad_b = 30.0_f64;
    let base = h - pad_b;
    let n = data.len();
    let slot = if n > 0 {
        (w - pad_l - pad_r) / n as f64
    } else {
        w - pad_l - pad_r
    };
    let cx = |i: usize| pad_l + slot * (i as f64 + 0.5);

    // Budget line: prefer first point's budget; fall back to max spend.
    let budget_line = data.first().map_or(0.0, |d| money_chf(d.budget));
    let max_y = if is_rate {
        let m = data.iter().map(|d| d.rate).fold(0.0001_f64, f64::max);
        m * 1.25
    } else {
        let m = data
            .iter()
            .map(|d| money_chf(d.spend))
            .fold(budget_line.max(1.0), f64::max);
        m * 1.08
    };
    let val = |d: &SpendPointDto| if is_rate { d.rate } else { money_chf(d.spend) };
    let y = |v: f64| base - (v / max_y) * (base - pad_t);

    // y gridlines
    struct GLine {
        v: f64,
        lab: String,
    }
    let lines: Vec<GLine> = if is_rate {
        [0.1, 0.2, 0.3]
            .iter()
            .map(|&v| GLine {
                v,
                lab: format!("{}%", (v * 100.0).round() as i64),
            })
            .collect()
    } else {
        [2000.0, 4000.0]
            .iter()
            .map(|&v: &f64| GLine {
                v,
                lab: format!("{}k", (v / 1000.0) as i64),
            })
            .collect()
    };

    let line_pts: String = data
        .iter()
        .enumerate()
        .map(|(i, d)| format!("{:.1},{:.1}", cx(i), y(val(d))))
        .collect::<Vec<_>>()
        .join(" ");
    let area_pts = if n > 0 {
        format!("{},{base} {line_pts} {},{base}", cx(0), cx(n - 1))
    } else {
        String::new()
    };
    let bar_w = (slot * 0.5).min(30.0);
    let budget_y = y(budget_line);

    let first = data.first().cloned();
    let s = stats;
    let cur = s
        .as_ref()
        .map(|st| (st.cur.m.clone(), st.cur.yr.clone()))
        .or_else(|| data.last().map(|d| (d.m.clone(), d.yr.clone())));
    let (cur_m, cur_yr) = cur.unwrap_or_default();
    let (first_m, first_yr) = first.map(|d| (d.m, d.yr)).unwrap_or_default();

    // Header label (legend + hud).
    let leg = if is_rate {
        "SAVINGS-RATE TREND"
    } else {
        "SPEND TREND"
    };
    let hud_word = if is_rate { "CASHFLOW SAVED" } else { "SPEND" };
    let hud_range = if n > 0 {
        format!(" · {first_m}{first_yr} → {cur_m}{cur_yr}")
    } else {
        String::new()
    };
    let hud_text = format!("⌁ {hud_word} · LAST {n} CYCLES{hud_range}");

    // Footer stats (em-dash defaults faithful to `num`).
    let num = |m: Option<Money>| m.map_or("—".to_string(), chf0);
    let avg_str = num(s.as_ref().map(|st| st.avg));
    let peak_spend = num(s.as_ref().map(|st| st.peak.spend));
    let peak_label = s
        .as_ref()
        .map(|st| format!("{} {}", st.peak.m, st.peak.yr))
        .unwrap_or_default();
    let low_spend = num(s.as_ref().map(|st| st.low.spend));
    let low_label = s
        .as_ref()
        .map(|st| format!("{} {}", st.low.m, st.low.yr))
        .unwrap_or_default();
    let cur_vs_avg = s.as_ref().map(|st| st.cur_vs_avg_pct);
    let cur_vs_prev = s.as_ref().map(|st| st.cur_vs_prev_pct);
    let prev_m = s.as_ref().map(|st| st.prev.m.clone()).unwrap_or_default();

    // Pre-computed foot-note string + tone.
    let foot_note = cur_vs_avg.map(|va| {
        let sign = if va > 0 { "+" } else { "−" };
        let tail = match cur_vs_prev {
            Some(vp) => {
                let arrow = if vp > 0 { "↑" } else { "↓" };
                format!(" · {arrow} {}% vs {prev_m}", vp.abs())
            }
            None => String::new(),
        };
        format!("THIS CYCLE TRACKING {sign}{}% vs 6-MO AVG{tail}", va.abs())
    });
    let foot_tone = match cur_vs_avg {
        Some(v) if v > 0 => "alert".to_string(),
        _ => "ok".to_string(),
    };

    // gradient stops by mode
    let grad_top = if is_rate {
        "rgba(143,125,255,.26)"
    } else {
        "rgba(255,94,77,.28)"
    };
    let grad_bot = if is_rate {
        "rgba(143,125,255,0)"
    } else {
        "rgba(255,94,77,0)"
    };
    let trace_col = if is_rate {
        "var(--indigo-neon)"
    } else {
        "var(--neon)"
    };

    rsx! {
        div { class: "atrend osc-bkt blue",
            span { class: "osc-leg", "{leg}" }
            div { class: "atrend-h",
                span { class: "hud", "{hud_text}" }
                div { class: "atrend-key",
                    if !is_rate {
                        span {
                            i { class: "k spend" }
                            " SPEND"
                        }
                        span {
                            i { class: "k bud" }
                            " BUDGET {num(Some(Money::from_centimes((budget_line * 100.0) as i64)))}"
                        }
                    }
                    if is_rate {
                        span {
                            i { class: "k rate" }
                            " SAVED / INCOME"
                        }
                    }
                    span {
                        i { class: "k proj" }
                        " CURRENT · PROJECTED"
                    }
                }
            }

            svg {
                width: "100%",
                height: "{h}",
                view_box: "0 0 {w} {h}",
                preserve_aspect_ratio: "none",
                class: "atrend-svg",
                defs {
                    linearGradient { id: "atrendfill", x1: "0", y1: "0", x2: "0", y2: "1",
                        stop { offset: "0%", stop_color: "{grad_top}" }
                        stop { offset: "100%", stop_color: "{grad_bot}" }
                    }
                }

                // gridlines
                for g in lines.iter() {
                    {
                        let gy = y(g.v);
                        rsx! {
                            g { key: "{g.lab}",
                                line {
                                    x1: "{pad_l}",
                                    y1: "{gy}",
                                    x2: "{w - pad_r}",
                                    y2: "{gy}",
                                    stroke: "rgba(106,95,192,.16)",
                                    stroke_width: "1",
                                    stroke_dasharray: "2 4",
                                }
                                text {
                                    x: "{pad_l - 8.0}",
                                    y: "{gy + 3.0}",
                                    text_anchor: "end",
                                    fill: "var(--ink-3)",
                                    font_size: "8.5",
                                    font_family: "var(--font-body)",
                                    letter_spacing: ".04em",
                                    "{g.lab}"
                                }
                            }
                        }
                    }
                }
                line {
                    x1: "{pad_l}",
                    y1: "{base}",
                    x2: "{w - pad_r}",
                    y2: "{base}",
                    stroke: "rgba(106,95,192,.4)",
                    stroke_width: "1",
                }

                // budget reference (spend mode only)
                if !is_rate && budget_line > 0.0 {
                    g {
                        line {
                            x1: "{pad_l}",
                            y1: "{budget_y}",
                            x2: "{w - pad_r}",
                            y2: "{budget_y}",
                            stroke: "var(--indigo-neon)",
                            stroke_width: "1.2",
                            stroke_dasharray: "5 5",
                            opacity: ".75",
                        }
                        text {
                            x: "{w - pad_r}",
                            y: "{budget_y - 4.0}",
                            text_anchor: "end",
                            fill: "var(--indigo-neon)",
                            font_size: "8.5",
                            font_family: "var(--font-body)",
                            letter_spacing: ".1em",
                            "BUDGET"
                        }
                    }
                }

                // bars
                for (i , d) in data.iter().enumerate() {
                    {
                        let bx = cx(i) - bar_w / 2.0;
                        let by = y(val(d));
                        let bh = (base - by).max(0.0);
                        let proj = d.projected;
                        let over = !is_rate && d.over;
                        let col = if is_rate {
                            "rgba(143,125,255,.42)"
                        } else if over {
                            "rgba(255,59,46,.5)"
                        } else {
                            "rgba(255,94,77,.34)"
                        };
                        let proj_stroke = if is_rate { "var(--indigo-neon)" } else { "var(--neon)" };
                        let proj_fill = if is_rate { "rgba(143,125,255,.1)" } else { "rgba(255,94,77,.1)" };
                        if proj {
                            rsx! {
                                g { key: "{i}",
                                    rect {
                                        x: "{bx}",
                                        y: "{by}",
                                        width: "{bar_w}",
                                        height: "{bh}",
                                        fill: "none",
                                        stroke: "{proj_stroke}",
                                        stroke_width: "1.3",
                                        stroke_dasharray: "3 2",
                                        opacity: ".9",
                                    }
                                    rect {
                                        x: "{bx}",
                                        y: "{by}",
                                        width: "{bar_w}",
                                        height: "{bh}",
                                        fill: "{proj_fill}",
                                    }
                                }
                            }
                        } else {
                            rsx! {
                                rect {
                                    key: "{i}",
                                    x: "{bx}",
                                    y: "{by}",
                                    width: "{bar_w}",
                                    height: "{bh}",
                                    fill: "{col}",
                                }
                            }
                        }
                    }
                }

                // connecting trace
                if n > 0 {
                    polygon { points: "{area_pts}", fill: "url(#atrendfill)" }
                    polyline {
                        points: "{line_pts}",
                        fill: "none",
                        stroke: "{trace_col}",
                        stroke_width: "2.2",
                        stroke_linejoin: "round",
                        stroke_linecap: "round",
                        style: "filter:drop-shadow(0 0 3px {trace_col})",
                    }
                }

                // point markers + current
                for (i , d) in data.iter().enumerate() {
                    {
                        let last = i == n - 1;
                        let r = if last { 4.0 } else { 2.4 };
                        let fill = if last {
                            "var(--neon-white)"
                        } else if is_rate {
                            "var(--indigo-neon)"
                        } else {
                            "var(--neon)"
                        };
                        let style = if last {
                            format!("filter:drop-shadow(0 0 4px {trace_col})")
                        } else {
                            String::new()
                        };
                        rsx! {
                            circle {
                                key: "{i}",
                                cx: "{cx(i)}",
                                cy: "{y(val(d))}",
                                r: "{r}",
                                fill: "{fill}",
                                style: "{style}",
                            }
                        }
                    }
                }

                // x labels
                for (i , d) in data.iter().enumerate() {
                    {
                        let lab_fill = if d.projected { "var(--neon-dim)" } else { "var(--ink-3)" };
                        rsx! {
                            text {
                                key: "{i}",
                                x: "{cx(i)}",
                                y: "{base + 16.0}",
                                text_anchor: "middle",
                                fill: "{lab_fill}",
                                font_size: "8.5",
                                font_family: "var(--font-body)",
                                letter_spacing: ".06em",
                                "{d.m}"
                            }
                        }
                    }
                }
            }

            div { class: "atrend-foot",
                span { class: "tf",
                    i { "6-MO AVG" }
                    " "
                    b { "CHF {avg_str}" }
                }
                span { class: "tf",
                    i { "PEAK" }
                    " "
                    b { class: "warn", "CHF {peak_spend}" }
                    " "
                    em { "{peak_label}" }
                }
                span { class: "tf",
                    i { "LEANEST" }
                    " "
                    b { style: "color:var(--ok)", "CHF {low_spend}" }
                    " "
                    em { "{low_label}" }
                }
                if let Some(note) = foot_note {
                    span { class: "tf-note",
                        Dot { tone: foot_tone, size: 6 }
                        "{note}"
                    }
                }
            }
        }
    }
}

// ════════════════════════════════════════════════════════════════════════════
// ITEM-SIGNAL MATRIX ROW — JSX `ItemSignalRow`. A `.isig` button over a signal.
// ════════════════════════════════════════════════════════════════════════════

#[component]
fn ItemSignalRow(
    sig: SignalDto,
    #[props(default = false)] active: bool,
    rank: usize,
    on_select: EventHandler<String>,
    on_track: EventHandler<String>,
) -> Element {
    let cand = sig.candidate;
    let up = sig.delta_pct >= 0;
    let delta = if cand {
        "NEW".to_string()
    } else {
        format!("{}{}%", if up { "↑" } else { "↓" }, sig.delta_pct.abs())
    };
    let mut cls = String::from("isig");
    if active {
        cls.push_str(" on");
    }
    if cand {
        cls.push_str(" cand");
    }
    let rk = if cand {
        "—".to_string()
    } else {
        rank.to_string()
    };
    let pa = if cand {
        sig.desc.clone()
    } else {
        format!("{} · since {}", sig.parent, sig.since)
    };
    let dl_cls = if cand {
        "isig-dl cand"
    } else if up {
        "isig-dl up"
    } else {
        "isig-dl down"
    };
    let sub = if cand {
        "this cycle".to_string()
    } else {
        format!("CHF {} · {} txns", chf2(sig.cycle_spend), sig.txns)
    };
    let go = if cand { "TRACK ▸" } else { "▸" };
    let id = sig.id.clone();
    let id2 = sig.id.clone();
    rsx! {
        button {
            class: "{cls}",
            onclick: move |_| {
                if cand {
                    on_track.call(id.clone());
                } else {
                    on_select.call(id2.clone());
                }
            },
            span { class: "isig-rk", "{rk}" }
            span { class: "isig-gl", "⌁" }
            div { class: "isig-id",
                span { class: "isig-nm", "{sig.label}" }
                span { class: "isig-pa", "{pa}" }
            }
            div { class: "isig-spk",
                SigSpark { data: sig.series.clone(), w: 150.0, h: 38.0 }
            }
            span { class: "{dl_cls}", "{delta}" }
            div { class: "isig-num",
                span { class: "v",
                    "{sig.cycle_qty} "
                    i { "{sig.unit}" }
                }
                span { class: "s", "{sub}" }
            }
            span { class: "isig-go", "{go}" }
        }
    }
}

// ════════════════════════════════════════════════════════════════════════════
// SIGNAL MOVER CARD — JSX `MoverCard`. A `.mover` card over a mover signal.
// ════════════════════════════════════════════════════════════════════════════

#[component]
fn MoverCard(sig: SignalDto, riser: bool) -> Element {
    let cls = if riser { "mover up" } else { "mover down" };
    let lbl = if riser {
        "↑ FASTEST RISER"
    } else {
        "↓ FASTEST FALLER"
    };
    let dl_cls = if riser { "dl up" } else { "dl down" };
    let dl_sign = if sig.delta_pct > 0 { "+" } else { "" };
    let dl = format!("{dl_sign}{}%", sig.delta_pct);
    let sub = format!(
        "{} {} · CHF {} this cycle · {}",
        sig.cycle_qty,
        sig.unit,
        chf2(sig.cycle_spend),
        sig.parent
    );
    rsx! {
        div { class: "{cls}",
            div { class: "mover-h",
                span { class: "lbl", "{lbl}" }
                span { class: "{dl_cls}", "{dl}" }
            }
            div { class: "mover-nm", "{sig.label}" }
            div { class: "mover-spk",
                SigSpark { data: sig.series.clone(), w: 232.0, h: 44.0 }
            }
            div { class: "mover-sub", "{sub}" }
        }
    }
}

// ════════════════════════════════════════════════════════════════════════════
// CATEGORY MOMENTUM CARD — JSX `MomentumCard`. A `.momo` card over a momentum DTO.
// ════════════════════════════════════════════════════════════════════════════

#[component]
fn MomentumCard(c: MomentumDto) -> Element {
    let up = c.delta_pct >= 0;
    let strong = c.delta_pct.abs() >= 15;
    let tone = if c.fixed {
        "flat"
    } else if up {
        if strong {
            "hot"
        } else {
            "up"
        }
    } else {
        "down"
    };
    let cls = format!("momo {tone}");
    let dl_cls = format!("dl {tone}");
    let dl = if c.fixed {
        "FIXED".to_string()
    } else {
        format!("{}{}%", if up { "↑" } else { "↓" }, c.delta_pct.abs())
    };
    let series = if c.series.is_empty() {
        vec![0.0, 0.0]
    } else {
        c.series.clone()
    };
    let spark_tone = if !c.fixed && up && strong {
        "neon"
    } else {
        "indigo"
    };
    rsx! {
        div { class: "{cls}",
            div { class: "momo-h",
                span { class: "cn", "{c.name}" }
                span { class: "{dl_cls}", "{dl}" }
            }
            div { class: "momo-spk",
                Spark { data: series, w: 150.0, h: 30.0, tone: spark_tone.to_string() }
            }
            div { class: "momo-f",
                span { class: "now", "CHF {chf0(c.now)}" }
                span { class: "vs", "vs 3-cyc avg" }
            }
        }
    }
}

// ════════════════════════════════════════════════════════════════════════════
// SPENDING RHYTHM — JSX `RhythmHeatmap`. A `.rhythm` weekday bar heatmap.
// ════════════════════════════════════════════════════════════════════════════

#[component]
fn RhythmHeatmap(rhythm: RhythmDto) -> Element {
    let wd = rhythm.weekday;
    let max = if rhythm.stats.max > 0.0 {
        rhythm.stats.max
    } else {
        wd.iter().map(|x| x.v).fold(1.0_f64, f64::max)
    };
    let peak_day = if !rhythm.stats.peak.d.is_empty() {
        rhythm.stats.peak.d.clone()
    } else {
        wd.iter()
            .fold(wd.first().cloned(), |a, x| match a {
                Some(ref cur) if x.v > cur.v => Some(x.clone()),
                other => other,
            })
            .map(|x| x.d)
            .unwrap_or_default()
    };
    let weekend_share = format!("{}%", rhythm.stats.weekend_share);
    rsx! {
        div { class: "rhythm osc-bkt coral",
            span { class: "osc-leg", "SPENDING RHYTHM" }
            div { class: "rhythm-h",
                span { class: "hud", "∿ DISCRETIONARY SPEND · AVG BY WEEKDAY" }
                span { class: "rhythm-meta",
                    "{weekend_share} LANDS FRI–SUN · RENT & INSURANCE EXCLUDED"
                }
            }
            div { class: "rhythm-grid",
                for x in wd.iter() {
                    {
                        let p = if max > 0.0 { x.v / max } else { 0.0 };
                        let is_peak = x.d == peak_day;
                        let col_cls = if is_peak { "rcol peak" } else { "rcol" };
                        let bar_h = (p * 100.0).max(6.0);
                        rsx! {
                            div { key: "{x.d}", class: "{col_cls}",
                                span { class: "rv", "CHF {x.v}" }
                                div { class: "rbar-wrap",
                                    div { class: "rbar", style: "height:{bar_h}%" }
                                }
                                span { class: "rd", "{x.d}" }
                            }
                        }
                    }
                }
            }
        }
    }
}

// ════════════════════════════════════════════════════════════════════════════
// PAGE
// ════════════════════════════════════════════════════════════════════════════

/// `CycleDto` empty default until the read lands.
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

/// The composed Phoskonomia analytics page.
#[component]
pub fn AnalyticsPage() -> Element {
    // ---- backend data loads (was: useGet over the dead REST layer) ----
    let cycle = use_resource(get_cycle);
    let spend_history = use_resource(get_spend_history);
    let hist_stats = use_resource(get_spend_stats);
    let momentum = use_resource(get_category_momentum);
    let rhythm = use_resource(get_rhythm);
    let insights = use_resource(get_analytics_insight);
    let movers = use_resource(get_movers);
    let signals = use_resource(get_signals);
    let candidates = use_resource(get_signal_candidates);

    // ---- UI state (was: useTweaks + useState; tweaks tooling NOT ported, its
    // state preserved as signals with the same AN_TWEAK_DEFAULTS) ----
    let mut ai_collapsed = use_signal(|| false); // aiOpen=true → collapsed=false
    let mut sel = use_signal(|| Option::<String>::None);
    let mut drawer = use_signal(|| false);

    // tweak-driven UI knobs (defaults from AN_TWEAK_DEFAULTS)
    let mut trend_mode = use_signal(|| "spend".to_string()); // "spend" | "rate"
    let trend_window = use_signal(|| 12_usize); // 12 | 6
    let sig_sort = use_signal(|| "momentum".to_string()); // momentum | spend | az
    let show_cand = use_signal(|| true);
    let show_momentum = use_signal(|| true);
    let momentum_sort = use_signal(|| "momentum".to_string());
    let show_rhythm = use_signal(|| true);
    let sig_insp = use_signal(|| "dock".to_string()); // dock | drawer

    // Responsive narrow (<1280px) → drawer the inspector. Faithful to React's
    // window.innerWidth resize listener.
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

    let dockable = !narrow() && sig_insp() == "dock";

    // Selected item-signal detail — fetched on demand (refetches when sel changes).
    let sig_detail = use_resource(move || async move {
        match sel() {
            Some(id) => Some(get_signal(id).await),
            None => None,
        }
    });

    // ---- read resources into local snapshots (clone out of the borrow) ----
    let c = match &*cycle.read() {
        Some(Ok(v)) => v.clone(),
        _ => empty_cycle(),
    };
    let history_v: Option<SpendHistoryDto> = spend_history
        .read()
        .as_ref()
        .and_then(|r| r.as_ref().ok())
        .cloned();
    let stats_v: Option<SpendStatsDto> = hist_stats
        .read()
        .as_ref()
        .and_then(|r| r.as_ref().ok())
        .cloned();
    let momentum_v: Option<Vec<MomentumDto>> = momentum
        .read()
        .as_ref()
        .and_then(|r| r.as_ref().ok())
        .cloned();
    let rhythm_v: Option<RhythmDto> = rhythm
        .read()
        .as_ref()
        .and_then(|r| r.as_ref().ok())
        .cloned();
    let insights_v: Option<AnalyticsInsightDto> = insights
        .read()
        .as_ref()
        .and_then(|r| r.as_ref().ok())
        .cloned();
    let movers_v: Option<MoversDto> = movers
        .read()
        .as_ref()
        .and_then(|r| r.as_ref().ok())
        .cloned();
    let signals_v: Option<Vec<SignalDto>> = signals
        .read()
        .as_ref()
        .and_then(|r| r.as_ref().ok())
        .cloned();
    let candidates_v: Option<Vec<SignalDto>> = candidates
        .read()
        .as_ref()
        .and_then(|r| r.as_ref().ok())
        .cloned();

    // loading flags
    let history_loading = spend_history.read().is_none();
    let momentum_loading = momentum.read().is_none();
    let rhythm_loading = rhythm.read().is_none();
    let insights_loading = insights.read().is_none();
    let movers_loading = movers.read().is_none();
    let signals_loading = signals.read().is_none();

    let history_ok = history_v.is_some();
    let momentum_ok = momentum_v.is_some();
    let rhythm_ok = rhythm_v.is_some();
    let signals_ok = signals_v.is_some();
    let movers_ok = movers_v.is_some();
    let insights_ok = insights_v.is_some();

    let is_rate = trend_mode() == "rate";

    // ---- derive collections ----
    let hist_points: Vec<SpendPointDto> = history_v
        .as_ref()
        .map(|h| h.points.clone())
        .unwrap_or_default();
    let window_n = trend_window();
    let trend_points: Vec<SpendPointDto> = {
        let start = hist_points.len().saturating_sub(window_n);
        hist_points[start..].to_vec()
    };

    // tracked signals sorted by sig_sort
    let tracked_raw: Vec<SignalDto> = signals_v.clone().unwrap_or_default();
    let candidates_raw: Vec<SignalDto> = candidates_v.clone().unwrap_or_default();
    let sorted_signals: Vec<SignalDto> = {
        let mut arr = tracked_raw.clone();
        match sig_sort().as_str() {
            "spend" => arr.sort_by(|a, b| b.cycle_spend.centimes().cmp(&a.cycle_spend.centimes())),
            "az" => arr.sort_by(|a, b| a.label.cmp(&b.label)),
            _ => arr.sort_by(|a, b| b.delta_pct.cmp(&a.delta_pct)),
        }
        arr
    };

    // category momentum sorted by momentum_sort
    let sorted_cats: Vec<MomentumDto> = {
        let mut arr = momentum_v.clone().unwrap_or_default();
        match momentum_sort().as_str() {
            "spend" => arr.sort_by(|a, b| b.now.centimes().cmp(&a.now.centimes())),
            "az" => arr.sort_by(|a, b| a.name.cmp(&b.name)),
            _ => arr.sort_by(|a, b| {
                let ka = if a.fixed { -999 } else { a.delta_pct.abs() };
                let kb = if b.fixed { -999 } else { b.delta_pct.abs() };
                kb.cmp(&ka)
            }),
        }
        arr
    };

    // selected signal detail → Sig for the panel.
    let sig_obj: Option<Sig> = match &*sig_detail.read() {
        Some(Some(Ok(d))) => Some(sig_of_detail(d)),
        _ => None,
    };
    let sel_id = sel();
    let show_drawer = !dockable && drawer() && sel_id.is_some();

    let sig_count = tracked_raw.len();
    let cand_count = candidates_raw.len();

    let movers_data = movers_v.clone();
    let riser = movers_data.as_ref().map(|m| m.riser.clone());
    let faller = movers_data.as_ref().map(|m| m.faller.clone());

    let ins = insights_v.clone();
    let suggested_cap = ins.as_ref().map(|i| i.suggested_cap.clone());
    let ins_model = ins
        .as_ref()
        .map_or("GEMMA4".to_string(), |i| i.model.clone());

    let cycle_label = if c.label.is_empty() {
        "THIS CYCLE".to_string()
    } else {
        c.label.clone()
    };
    let topbar_date = if c.days == 0 {
        String::new()
    } else {
        format!("{} · DAY {}/{}", c.label, c.day, c.days)
    };

    // ---- pre-computed display strings (the rsx format-segment parser rejects
    // string-literal / closure / format! inside `{...}`) ----
    let cur_spend = stats_v.as_ref().map(|s| s.cur.spend);
    let cur_spend_str = cur_spend.map_or("—".to_string(), chf0);
    let months_str = stats_v
        .as_ref()
        .map_or("—".to_string(), |s| s.months.to_string());
    let sig_count_str = sig_count.to_string();
    let cats_count_str = sorted_cats.len().to_string();

    // KPI band sub-lines + numerals
    let cur_vs_avg = stats_v.as_ref().map(|s| s.cur_vs_avg_pct);
    let cur_vs_avg_str = cur_vs_avg.map(|v| {
        let arrow = if v > 0 { "↑" } else { "↓" };
        format!("{arrow} {}%", v.abs())
    });
    let cur_vs_avg_cls = match cur_vs_avg {
        Some(v) if v > 0 => "up",
        _ => "dn",
    };

    let avg_str = stats_v.as_ref().map_or("—".to_string(), |s| chf0(s.avg));
    let avg_budget_str = stats_v
        .as_ref()
        .map_or("—".to_string(), |s| chf0(s.cur.budget));
    let avg_peak_str = stats_v
        .as_ref()
        .map(|s| format!(" · peak {} {}", s.peak.m, chf0(s.peak.spend)));

    let rate_str = stats_v.as_ref().map_or("—".to_string(), |s| {
        ((s.avg_rate * 100.0).round() as i64).to_string()
    });
    let rate_saved_str = stats_v
        .as_ref()
        .map(|s| format!(" · CHF {} saved over {} cyc", chf0(s.total_saved), s.months));

    let kpi_sig_str = if sig_count > 0 {
        sig_count.to_string()
    } else {
        "—".to_string()
    };
    let cand_prefix = if cand_count > 0 {
        format!("+{cand_count} candidate · ")
    } else {
        String::new()
    };
    let riser_lead = riser
        .as_ref()
        .map(|r| format!("{} ↑{}% leads", r.label, r.delta_pct));

    // SuggestedCap button label
    let cap_label = suggested_cap.as_ref().map(|cap| {
        let tail = if cap.projected_savings.centimes() != 0 {
            format!(" · +CHF {} SAVED", chf0(cap.projected_savings))
        } else {
            String::new()
        };
        format!("CAP {} AT CHF {}{}", cap.signal_id, chf0(cap.amount), tail)
    });

    rsx! {
        div { class: "pk", style: "height:100vh;min-height:0",
            // Analytics field — a wide READ-OUT sweep.
            ScannerBg {
                class: "pk-bg".to_string(),
                seed: 211,
                shapes: r#"[
                    { char: "2", cx: .2, cy: .78, scale: .5, style: "red", morph: "vein", live: true, fill: .5 },
                    { char: "8", cx: .88, cy: .26, scale: .44, style: "faint", morph: "blob", live: false, fill: .62 },
                    { char: "e", cx: .52, cy: .5, scale: .24, style: "wire", morph: "vein", live: false, fill: .4 },
                    { char: "5", cx: .07, cy: .2, scale: .26, style: "faint", morph: "blob", live: false, fill: .42 },
                    { char: "3", cx: .7, cy: .9, scale: .18, style: "wire", morph: "vein", live: false, fill: .35 }
                ]"#.to_string(),
            }

            div { class: "app-shell swap",
                AiPanelAnalytics {
                    collapsed: ai_collapsed(),
                    on_toggle: move |()| ai_collapsed.toggle(),
                    on_track: move |id: String| {
                        sel.set(Some(id));
                        if !dockable {
                            drawer.set(true);
                        }
                    },
                }

                div { class: "app-main",
                    TopBar { active: "ANALYTICS".to_string(), date_text: topbar_date }
                    div { class: "app-scroll", "data-screen-label": "ANALYTICS",
                        div { class: "an-wrap",

                            // ===================== TOP HEADER =====================
                            div { class: "an-top",
                                div {
                                    div { class: "ttl", "Analytics" }
                                    div { class: "sum",
                                        b { "{months_str}" }
                                        " cycles on record · trending "
                                        span { class: "coral", "CHF {cur_spend_str}" }
                                        " this cycle ·"
                                        b { " {sig_count_str}" }
                                        " item-signals tracked · AI-maintained"
                                    }
                                }
                                div { class: "an-modes",
                                    button {
                                        class: if !is_rate { "m on" } else { "m" },
                                        onclick: move |_| trend_mode.set("spend".to_string()),
                                        "∿ SPEND"
                                    }
                                    button {
                                        class: if is_rate { "m on" } else { "m" },
                                        onclick: move |_| trend_mode.set("rate".to_string()),
                                        "⌁ SAVINGS"
                                    }
                                }
                            }

                            // ===================== KPI BAND =====================
                            div { class: "an-kpis",
                                div { class: "akpi accent",
                                    div { class: "lbl",
                                        span { "THIS CYCLE" }
                                        span { "RUN-RATE" }
                                    }
                                    div { class: "big",
                                        span { class: "cur", "CHF" }
                                        "{cur_spend_str}"
                                    }
                                    div { class: "sub",
                                        if let Some(d) = cur_vs_avg_str.clone() {
                                            span { class: "{cur_vs_avg_cls}", "{d}" }
                                            " vs 6-mo avg"
                                        } else {
                                            span { class: "dim", "awaiting backend" }
                                        }
                                    }
                                }
                                div { class: "akpi blue",
                                    div { class: "lbl",
                                        span { "6-MONTH AVG" }
                                        span { "SPEND" }
                                    }
                                    div { class: "big",
                                        span { class: "cur", "CHF" }
                                        "{avg_str}"
                                    }
                                    div { class: "sub",
                                        "budget CHF {avg_budget_str}"
                                        if let Some(p) = avg_peak_str.clone() {
                                            "{p}"
                                        }
                                    }
                                }
                                div { class: "akpi blue bluebig",
                                    div { class: "lbl",
                                        span { "SAVINGS RATE" }
                                        span { "CASHFLOW" }
                                    }
                                    div { class: "big",
                                        "{rate_str}"
                                        span { class: "cur", style: "margin-left:2px", "%" }
                                    }
                                    div { class: "sub",
                                        "avg of income"
                                        if let Some(t) = rate_saved_str.clone() {
                                            "{t}"
                                        }
                                    }
                                }
                                div { class: "akpi",
                                    div { class: "lbl",
                                        span { "ITEM-SIGNALS" }
                                        span { "TRACKED" }
                                    }
                                    div { class: "big", "{kpi_sig_str}" }
                                    div { class: "sub",
                                        "{cand_prefix}"
                                        if let Some(lead) = riser_lead.clone() {
                                            "{lead}"
                                        } else {
                                            "awaiting movers"
                                        }
                                    }
                                }
                            }

                            // ===================== SPEND TREND HERO =====================
                            if history_ok && !hist_points.is_empty() {
                                SpendTrend {
                                    points: trend_points,
                                    stats: stats_v.clone(),
                                    is_rate,
                                }
                            } else {
                                Awaiting {
                                    label: "SPEND TREND".to_string(),
                                    loading: history_loading,
                                    tone: "blue".to_string(),
                                }
                            }

                            // ===================== ITEM-SIGNALS =====================
                            div { class: "an-sec",
                                span { class: "lbl", "⌁ ITEM-SIGNALS" }
                                span { class: "ct", "{sig_count_str}" }
                                span { class: "rule" }
                                span { class: "meta", "12-MO TREND · CLICK TO INSPECT" }
                            }

                            if signals_ok && (!sorted_signals.is_empty() || (show_cand() && !candidates_raw.is_empty())) {
                                div { class: "isig-list",
                                    for (i , s) in sorted_signals.iter().enumerate() {
                                        ItemSignalRow {
                                            key: "{s.id}",
                                            sig: s.clone(),
                                            rank: i + 1,
                                            active: sel_id.as_deref() == Some(s.id.as_str()),
                                            on_select: move |id: String| {
                                                let same = sel().as_deref() == Some(id.as_str());
                                                sel.set(if same { None } else { Some(id) });
                                                if !same && !dockable {
                                                    drawer.set(true);
                                                }
                                            },
                                            on_track: move |_id: String| {},
                                        }
                                    }
                                    if show_cand() {
                                        for s in candidates_raw.iter() {
                                            {
                                                let mut cs = s.clone();
                                                cs.candidate = true;
                                                rsx! {
                                                    ItemSignalRow {
                                                        key: "cand-{cs.id}",
                                                        sig: cs.clone(),
                                                        rank: 0,
                                                        active: sel_id.as_deref() == Some(cs.id.as_str()),
                                                        on_select: move |id: String| {
                                                            let same = sel().as_deref() == Some(id.as_str());
                                                            sel.set(if same { None } else { Some(id) });
                                                            if !same && !dockable {
                                                                drawer.set(true);
                                                            }
                                                        },
                                                        on_track: move |id: String| {
                                                            sel.set(Some(id));
                                                            if !dockable {
                                                                drawer.set(true);
                                                            }
                                                        },
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            } else {
                                Awaiting {
                                    label: "ITEM-SIGNALS".to_string(),
                                    loading: signals_loading,
                                    tone: "blue".to_string(),
                                }
                            }

                            // ===================== MOVERS + INSIGHT =====================
                            div { class: "an-movers",
                                if movers_ok && riser.is_some() {
                                    MoverCard { sig: riser.clone().unwrap(), riser: true }
                                } else {
                                    Awaiting {
                                        label: "FASTEST RISER".to_string(),
                                        loading: movers_loading,
                                        tone: "coral".to_string(),
                                    }
                                }
                                if movers_ok && faller.is_some() {
                                    MoverCard { sig: faller.clone().unwrap(), riser: false }
                                } else {
                                    Awaiting {
                                        label: "FASTEST FALLER".to_string(),
                                        loading: movers_loading,
                                        tone: "blue".to_string(),
                                    }
                                }
                                div { class: "an-insight",
                                    div { class: "ih",
                                        Dot { tone: "blue".to_string(), size: 6 }
                                        "{ins_model} · READ"
                                    }
                                    if insights_ok && ins.as_ref().is_some_and(|i| !i.text.is_empty()) {
                                        div { class: "q", "{ins.as_ref().unwrap().text}" }
                                        if let Some(label) = cap_label.clone() {
                                            div { style: "margin-top:10px",
                                                button {
                                                    class: "gbtn p",
                                                    onclick: move |_| {},
                                                    "{label}"
                                                }
                                            }
                                        }
                                    } else {
                                        Awaiting {
                                            label: "GEMMA4 READ".to_string(),
                                            loading: insights_loading,
                                            tone: "blue".to_string(),
                                        }
                                    }
                                }
                            }

                            // ===================== CATEGORY MOMENTUM =====================
                            if show_momentum() {
                                div { class: "an-sec",
                                    span { class: "lbl", "▦ CATEGORY MOMENTUM" }
                                    span { class: "ct", "{cats_count_str}" }
                                    span { class: "rule" }
                                    span { class: "meta", "NOW vs 3-CYCLE AVG · ↑ HEATING · ↓ COOLING" }
                                }
                                if momentum_ok && !sorted_cats.is_empty() {
                                    div { class: "momo-grid",
                                        for cm in sorted_cats.iter() {
                                            MomentumCard { key: "{cm.name}", c: cm.clone() }
                                        }
                                    }
                                } else {
                                    Awaiting {
                                        label: "CATEGORY MOMENTUM".to_string(),
                                        loading: momentum_loading,
                                        tone: "blue".to_string(),
                                    }
                                }
                            }

                            // ===================== SPENDING RHYTHM =====================
                            if show_rhythm() {
                                div { class: "an-sec",
                                    span { class: "lbl", "∿ SPENDING RHYTHM" }
                                    span { class: "rule" }
                                    span { class: "meta", "WHEN IN THE WEEK IT GOES" }
                                }
                                if rhythm_ok && rhythm_v.as_ref().is_some_and(|r| !r.weekday.is_empty()) {
                                    RhythmHeatmap { rhythm: rhythm_v.clone().unwrap() }
                                } else {
                                    Awaiting {
                                        label: "SPENDING RHYTHM".to_string(),
                                        loading: rhythm_loading,
                                        tone: "coral".to_string(),
                                    }
                                }
                            }
                        }
                    }
                }

                // docked signal inspector (wide layouts)
                if dockable && sel_id.is_some() {
                    SignalPanel {
                        sig: sig_obj.clone(),
                        cycle_label: cycle_label.clone(),
                        on_close: move |()| sel.set(None),
                    }
                }
            }

            // drawer signal inspector (narrow layouts)
            if show_drawer {
                div { class: "sig-drawer-back", onclick: move |_| drawer.set(false),
                    div { class: "sig-drawer", onclick: move |e: Event<MouseData>| e.stop_propagation(),
                        SignalPanel {
                            sig: sig_obj.clone(),
                            variant: "drawer".to_string(),
                            cycle_label: cycle_label.clone(),
                            on_close: move |()| drawer.set(false),
                        }
                    }
                }
            }
        }
    }
}

/// Thin analytics wrapper around the shared [`AiPanel`] (left assistant).
///
/// React's `AiPanel` self-fetched `/ai/feed`, `/ai/chat`, `/ai/status` and, on
/// send, appended the user line then a canned reply (the live backend currently
/// answers `POST /ai/chat` with `501`, so the rendered reply is the
/// "NOT IMPLEMENTED YET" stub). The Dioxus `AiPanel` is prop-driven and owns no
/// transcript, so this wrapper owns the chat `msgs` signal and wires `on_send`
/// to append the user's draft plus that same stub reply — preserving the
/// chat-input affordance even before an AI server fn lands. No AI server fns
/// exist yet, so feed/status keep their (empty / offline / GEMMA4·OLLAMA·LOCAL)
/// defaults, faithful to React rendering the empty-feed placeholder against a
/// dead backend. `on_toggle` flips the rail; `on_track` selects the candidate in
/// the page's `sel` (opening the dock/drawer) when a feed item's track fires.
#[component]
fn AiPanelAnalytics(
    collapsed: bool,
    on_toggle: EventHandler<()>,
    on_track: EventHandler<String>,
) -> Element {
    // The chat transcript the page owns (React's `msgs`). Seeded empty so the
    // panel shows the "Ask the assistant anything…" placeholder until a send.
    let mut msgs = use_signal(Vec::<ChatMsg>::new);

    rsx! {
        AiPanel {
            collapsed,
            msgs: msgs(),
            on_toggle: move |()| on_toggle.call(()),
            on_track: move |id: String| on_track.call(id),
            on_send: move |q: String| {
                // React `send`: append the user line, then the backend's reply
                // (currently the 501 stub). With no AI server fn here, append the
                // same stub so the input affordance round-trips.
                msgs.with_mut(|m| {
                    m.push(ChatMsg { who: "usr".to_string(), text: q });
                    m.push(ChatMsg {
                        who: "sys".to_string(),
                        text: "NOT IMPLEMENTED YET. The backend received this (POST /ai/chat → 501)."
                            .to_string(),
                    });
                });
            },
        }
    }
}
