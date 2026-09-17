//! Shared primitives (OWNED BY AGENT F2 — except [`ScannerBg`], pre-built by F1).
//!
//! Port of React `src/components/prims.jsx`. F2 fills `Dot`, `HudCell`, `Spark`,
//! `CatBar`, `PhoskChart`, `SavingsDial`, `pctTone`, `barColor` here, faithful to
//! the JSX (every SVG path / attribute / token var preserved).
//!
//! [`ScannerBg`] is the JS-interop bridge to the verbatim canvas engine
//! `assets/osc-scanner.js` (which sets `window.OscScanner`). It is provided here
//! complete so page backgrounds work from day one. DO NOT rewrite the engine.

use dioxus::prelude::*;

/// The canvas differential-growth scanner engine, content-hashed.
#[allow(dead_code)]
const OSC_SCANNER_JS: Asset = asset!("/assets/osc-scanner.js");

/// Live differential-growth scanner background, sized to its artboard.
///
/// Faithful port of React `prims.jsx` `ScannerBg`: it renders a `<canvas>`, waits
/// for `window.OscScanner` (loaded from [`OSC_SCANNER_JS`]) to exist, then calls
/// `OscScanner.mount(canvas, { grid, dish, bg, parallax, seed, shapes })`. The
/// instance is destroyed when the component unmounts.
///
/// `shapes` is a raw JS array literal string (e.g.
/// `r#"[{ char:'8', cx:.8, cy:.4, scale:.3, style:'faint', live:false, fill:.4 }]"#`),
/// exactly the shape config the engine expects — each page passes its own to get
/// a unique background (see the engine's USAGE header). When empty, the React
/// default single faint "8" is used.
#[component]
pub fn ScannerBg(
    #[props(default = 1)] seed: i32,
    #[props(default = String::new())] shapes: String,
    #[props(default = String::from("phosk-bg"))] class: String,
    #[props(default = true)] bg: bool,
    #[props(default = true)] grid: bool,
    #[props(default = true)] dish: bool,
    #[props(default = false)] parallax: bool,
) -> Element {
    // Stable per-instance canvas id so the JS bridge can find this exact canvas.
    let canvas_id = use_hook(|| {
        use std::sync::atomic::{AtomicU64, Ordering};
        static N: AtomicU64 = AtomicU64::new(0);
        format!("osc-scanner-{}", N.fetch_add(1, Ordering::Relaxed))
    });

    // Mount on first render; poll for window.OscScanner exactly like React's
    // setTimeout(tryMount, 60) retry. Store the instance on the canvas element so
    // the cleanup `use_drop` can destroy it.
    {
        let canvas_id = canvas_id.clone();
        let shapes_js = if shapes.trim().is_empty() {
            String::from(
                "[{ char:'8', cx:.8, cy:.4, scale:.3, style:'faint', live:false, fill:.4 }]",
            )
        } else {
            shapes.clone()
        };
        use_effect(move || {
            let js = format!(
                r#"
                (function() {{
                  var id = "{id}";
                  function tryMount() {{
                    var el = document.getElementById(id);
                    if (window.OscScanner && el) {{
                      if (el.__oscInst && el.__oscInst.destroy) el.__oscInst.destroy();
                      el.__oscInst = window.OscScanner.mount(el, {{
                        grid: {grid}, dish: {dish}, bg: {bg}, parallax: {parallax},
                        seed: {seed}, shapes: {shapes}
                      }});
                    }} else {{
                      setTimeout(tryMount, 60);
                    }}
                  }}
                  tryMount();
                }})();
                "#,
                id = canvas_id,
                grid = grid,
                dish = dish,
                bg = bg,
                parallax = parallax,
                seed = seed,
                shapes = shapes_js,
            );
            document::eval(&js);
        });
    }

    // Destroy the scanner instance on unmount (React effect cleanup).
    {
        let canvas_id = canvas_id.clone();
        use_drop(move || {
            let js = format!(
                r#"(function(){{var el=document.getElementById("{id}");if(el&&el.__oscInst&&el.__oscInst.destroy){{el.__oscInst.destroy();el.__oscInst=null;}}}})();"#,
                id = canvas_id
            );
            document::eval(&js);
        });
    }

    rsx! {
        // Load the verbatim engine once; it self-registers window.OscScanner.
        document::Script { src: OSC_SCANNER_JS }
        canvas { id: "{canvas_id}", class: "{class}" }
    }
}

// ===========================================================================
// F2 primitives — faithful port of the rest of React `prims.jsx`.
// Every SVG path / attribute / token var preserved. Numbers in Pilowlava are
// produced by the page/CSS; these emit the geometry + token-coloured strokes.
// ===========================================================================

use phosk_core::money::Money;

/// Status indicator dot — coloured by `tone` with a matching glow.
///
/// Faithful port of `prims.jsx` `Dot`: `alert`→`--neon`, `warn`→`--warn`,
/// `blue`→`--indigo-neon`, anything else→`--ok`. Renders an inline-block round
/// span sized `size`px with a `0 0 8px <col>` glow (depth via glow, not shadow).
#[component]
pub fn Dot(
    #[props(default = String::from("ok"))] tone: String,
    #[props(default = 8)] size: i32,
) -> Element {
    let c = match tone.as_str() {
        "alert" => "var(--neon)",
        "warn" => "var(--warn)",
        "blue" => "var(--indigo-neon)",
        _ => "var(--ok)",
    };
    rsx! {
        span {
            style: "width:{size}px;height:{size}px;border-radius:999px;background:{c};box-shadow:0 0 8px {c};flex:0 0 auto;display:inline-block",
        }
    }
}

/// Framed Pilowlava glyph cell (the HUD module chip).
///
/// Faithful port of `prims.jsx` `HudCell`: a `.phosk-cell` square of side
/// `size`px holding a bold glyph at `size*.64` (display font via CSS).
#[component]
pub fn HudCell(glyph: String, #[props(default = 46)] size: i32) -> Element {
    let fs = (f64::from(size) * 0.64).round() as i32;
    rsx! {
        div { class: "phosk-cell", style: "width:{size}px;height:{size}px",
            b { style: "font-size:{fs}px", "{glyph}" }
        }
    }
}

/// Tiny neon sparkline trace.
///
/// Faithful port of `prims.jsx` `Spark`: a `w`×`h` polyline normalised over the
/// data range, `neon` (coral) by default or `indigo` (`--indigo-neon`) tone,
/// 1.4 stroke with a `drop-shadow` glow. `data` is a presentation series.
#[component]
pub fn Spark(
    data: Vec<f64>,
    #[props(default = 92.0)] w: f64,
    #[props(default = 26.0)] h: f64,
    #[props(default = String::from("neon"))] tone: String,
) -> Element {
    let max = data.iter().copied().fold(f64::MIN, f64::max);
    let min = data.iter().copied().fold(f64::MAX, f64::min);
    let rng = if (max - min).abs() < f64::EPSILON {
        1.0
    } else {
        max - min
    };
    let n = data.len();
    let pts = data
        .iter()
        .enumerate()
        .map(|(i, v)| {
            let x = if n > 1 {
                i as f64 / (n as f64 - 1.0) * w
            } else {
                0.0
            };
            let y = h - 2.0 - (v - min) / rng * (h - 4.0);
            format!("{x:.1},{y:.1}")
        })
        .collect::<Vec<_>>()
        .join(" ");
    let stroke = if tone == "indigo" {
        "var(--indigo-neon)"
    } else {
        "var(--neon)"
    };
    rsx! {
        svg {
            width: "{w}",
            height: "{h}",
            view_box: "0 0 {w} {h}",
            style: "display:block",
            polyline {
                points: "{pts}",
                fill: "none",
                stroke: "{stroke}",
                stroke_width: "1.4",
                stroke_linejoin: "round",
                stroke_linecap: "round",
                style: "filter:drop-shadow(0 0 2px {stroke})",
            }
        }
    }
}

/// Budget-progress tone for a spent/budget ratio `p`.
///
/// Faithful port of `prims.jsx` `pctTone`: over cap (`>1`)→`alert`, near cap
/// (`>=0.85`)→`warn`, else `ok`.
#[must_use]
pub fn pct_tone(p: f64) -> &'static str {
    if p > 1.0 {
        "alert"
    } else if p >= 0.85 {
        "warn"
    } else {
        "ok"
    }
}

/// Token colour for a budget-bar `tone`.
///
/// Faithful port of `prims.jsx` `barColor`: `alert`→`--neon`, `warn`→`--warn`,
/// else `--indigo`.
#[must_use]
pub fn bar_color(tone: &str) -> &'static str {
    match tone {
        "alert" => "var(--neon)",
        "warn" => "var(--warn)",
        _ => "var(--indigo)",
    }
}

/// A category budget row's progress bar (indigo→amber→coral, with overflow band).
///
/// Faithful port of `prims.jsx` `CatBar`. Takes exact [`Money`] `spent`/`budget`
/// (the ratio is computed via `as_chf_f64`, never re-entering money math), an
/// optional `tone` override, and a bar height `h`px. When over cap it adds the
/// `over` class + a `.phosk-bar-over` band whose width encodes how far over.
#[component]
pub fn CatBar(
    spent: Money,
    budget: Money,
    #[props(default = 7)] h: i32,
    #[props(default)] tone: Option<String>,
) -> Element {
    let b = budget.as_chf_f64();
    let p = if budget.centimes() > 0 {
        spent.as_chf_f64() / b
    } else {
        0.0
    };
    let tone = tone.unwrap_or_else(|| pct_tone(p).to_string());
    let col = bar_color(&tone);
    let over = tone == "alert" && p > 1.0;
    let fill = p.min(1.0) * 100.0;
    let band = if over {
        ((p - 1.0) * 100.0).clamp(12.0, 46.0)
    } else {
        0.0
    };
    let cls = if over { "phosk-bar over" } else { "phosk-bar" };
    rsx! {
        div { class: "{cls}", style: "height:{h}px",
            div {
                class: "phosk-bar-fill",
                style: "width:{fill}%;background:{col};box-shadow:0 0 7px {col}",
            }
            if over {
                div { class: "phosk-bar-over", style: "width:{band}%" }
            }
        }
    }
}

/// Spending-over-time chart: cumulative spend vs budget pace + daily bars.
///
/// Faithful port of `prims.jsx` `PhoskChart` — fully prop-driven. `budget` is
/// the cap in whole CHF, `daily`/`cumulative`/`pace`/`lastCumulative` are
/// presentation series (CHF, not domain money). Renders only the grid until
/// `cumulative` has points. All gradients, gridlines, bars, the prior-cycle
/// dashed line, the budget-pace dashed line, the coral cumulative area+line and
/// the today marker are preserved with their exact token / rgba strokes.
#[component]
#[allow(clippy::too_many_arguments, clippy::similar_names)]
pub fn PhoskChart(
    width: f64,
    height: f64,
    #[props(default = 30.0)] days: f64,
    #[props(default = 0.0)] budget: f64,
    #[props(default)] daily: Vec<f64>,
    #[props(default)] cumulative: Vec<f64>,
    #[props(default)] pace: Vec<f64>,
    #[props(default)] last_cumulative: Vec<f64>,
    #[props(default = true)] show_bars: bool,
    #[props(default = true)] show_pace: bool,
    #[props(default = true)] show_area: bool,
    #[props(default = false)] show_last: bool,
    #[props(default = 8.0)] pad_l: f64,
    #[props(default = 8.0)] pad_r: f64,
    #[props(default = 14.0)] pad_t: f64,
    #[props(default = 18.0)] pad_b: f64,
) -> Element {
    let w = width;
    let h = height;
    let span = if days > 1.0 { days - 1.0 } else { 1.0 };
    let x = |d: f64| pad_l + d / span * (w - pad_l - pad_r);
    let cum_max = cumulative.iter().copied().fold(1.0_f64, f64::max);
    let max_y = (if budget > 0.0 { budget } else { cum_max }) * 1.04;
    let y = |v: f64| h - pad_b - v / max_y * (h - pad_t - pad_b);

    let has_cum = !cumulative.is_empty();
    let cum_pts = cumulative
        .iter()
        .enumerate()
        .map(|(i, v)| format!("{:.1},{:.1}", x(i as f64), y(*v)))
        .collect::<Vec<_>>()
        .join(" ");
    let last_x = if has_cum {
        x(cumulative.len() as f64 - 1.0)
    } else {
        pad_l
    };
    let last_y = if has_cum {
        y(*cumulative.last().unwrap())
    } else {
        y(0.0)
    };
    let pace_pts = pace
        .iter()
        .enumerate()
        .map(|(i, v)| format!("{:.1},{:.1}", x(i as f64), y(*v)))
        .collect::<Vec<_>>()
        .join(" ");
    let last_pts = last_cumulative
        .iter()
        .enumerate()
        .map(|(i, v)| format!("{:.1},{:.1}", x(i as f64), y(*v)))
        .collect::<Vec<_>>()
        .join(" ");
    let area_pts = format!("{},{} {} {},{}", pad_l, y(0.0), cum_pts, last_x, y(0.0));
    let bar_w = ((w - pad_l - pad_r) / days.max(1.0) * 0.42).max(2.0);

    let gridlines = [0.25_f64, 0.5, 0.75, 1.0];
    rsx! {
        svg {
            width: "{w}",
            height: "{h}",
            view_box: "0 0 {w} {h}",
            style: "display:block",
            defs {
                linearGradient { id: "cumfill", x1: "0", y1: "0", x2: "0", y2: "1",
                    stop { offset: "0%", stop_color: "rgba(255,94,77,.30)" }
                    stop { offset: "100%", stop_color: "rgba(255,94,77,0)" }
                }
            }
            // horizontal gridlines at 25/50/75/100% of budget
            for f in gridlines {
                g {
                    line {
                        x1: "{pad_l}",
                        y1: "{y(budget * f)}",
                        x2: "{w - pad_r}",
                        y2: "{y(budget * f)}",
                        stroke: "rgba(106,95,192,.18)",
                        stroke_width: "1",
                        stroke_dasharray: if (f - 1.0).abs() < f64::EPSILON { "0" } else { "2 4" },
                    }
                    text {
                        x: "{w - pad_r}",
                        y: "{y(budget * f) - 3.0}",
                        text_anchor: "end",
                        fill: "var(--ink-3)",
                        font_size: "8.5",
                        font_family: "var(--font-body)",
                        letter_spacing: ".06em",
                        if (f - 1.0).abs() < f64::EPSILON {
                            "BUDGET"
                        } else {
                            "{(budget * f / 1000.0):.1}k"
                        }
                    }
                }
            }
            // daily bars
            if show_bars {
                for (i , d) in daily.iter().enumerate() {
                    {
                        let bh = d.min(600.0) / max_y * (h - pad_t - pad_b);
                        let bx = x(i as f64) - bar_w / 2.0;
                        let by = h - pad_b - bh;
                        let bf = if *d > 300.0 {
                            "rgba(255,94,77,.30)"
                        } else {
                            "rgba(132,116,222,.32)"
                        };
                        rsx! {
                            rect {
                                x: "{bx}",
                                y: "{by}",
                                width: "{bar_w}",
                                height: "{bh.max(0.0)}",
                                fill: "{bf}",
                            }
                        }
                    }
                }
            }
            // prior cycle
            if show_last && !last_pts.is_empty() {
                polyline {
                    points: "{last_pts}",
                    fill: "none",
                    stroke: "rgba(143,125,255,.55)",
                    stroke_width: "1.4",
                    stroke_dasharray: "4 4",
                }
            }
            // budget pace
            if show_pace && !pace_pts.is_empty() {
                polyline {
                    points: "{pace_pts}",
                    fill: "none",
                    stroke: "var(--indigo-neon)",
                    stroke_width: "1.2",
                    stroke_dasharray: "5 5",
                    opacity: ".8",
                }
            }
            // cumulative area + line
            if has_cum && show_area {
                polygon { points: "{area_pts}", fill: "url(#cumfill)" }
            }
            if has_cum {
                polyline {
                    points: "{cum_pts}",
                    fill: "none",
                    stroke: "var(--neon)",
                    stroke_width: "2",
                    stroke_linejoin: "round",
                    style: "filter:drop-shadow(0 0 3px var(--neon))",
                }
            }
            // today marker
            if has_cum {
                line {
                    x1: "{last_x}",
                    y1: "{pad_t - 6.0}",
                    x2: "{last_x}",
                    y2: "{h - pad_b}",
                    stroke: "rgba(255,59,46,.4)",
                    stroke_width: "1",
                    stroke_dasharray: "2 3",
                }
                circle {
                    cx: "{last_x}",
                    cy: "{last_y}",
                    r: "3",
                    fill: "var(--neon-white)",
                    style: "filter:drop-shadow(0 0 4px var(--neon))",
                }
            }
        }
    }
}

/// Arc gauge for savings progress (pure: props only).
///
/// Faithful port of `prims.jsx` `SavingsDial`: a `-220°…40°` swept arc, a faint
/// indigo track, a dashed `--indigo-neon` projected arc, the coral `--neon`
/// saved arc with glow, the centred Pilowlava percent numeral and an `OF TARGET`
/// caption. `saved`/`target`/`projected` are presentation amounts in CHF.
#[component]
pub fn SavingsDial(
    #[props(default = 132.0)] size: f64,
    #[props(default = 0.0)] saved: f64,
    #[props(default = 1.0)] target: f64,
    #[props(default = 0.0)] projected: f64,
) -> Element {
    let r = size / 2.0 - 12.0;
    let cx = size / 2.0;
    let cy = size / 2.0;
    let start = -220.0_f64;
    let end = 40.0_f64;
    let sweep = end - start;
    let p_saved = (saved / target).min(1.0);
    let p_proj = (projected / target).min(1.0);

    let pol = |deg: f64, rad: f64| {
        let a = deg * std::f64::consts::PI / 180.0;
        (cx + rad * a.cos(), cy + rad * a.sin())
    };
    let arc = |p: f64, rad: f64| {
        let a0 = start;
        let a1 = start + sweep * p;
        let (x0, y0) = pol(a0, rad);
        let (x1, y1) = pol(a1, rad);
        let large = i32::from(a1 - a0 > 180.0);
        format!("M {x0:.1} {y0:.1} A {rad} {rad} 0 {large} 1 {x1:.1} {y1:.1}")
    };

    let track = arc(1.0, r);
    let proj = arc(p_proj, r);
    let saved_arc = arc(p_saved, r);
    let pct = (p_saved * 100.0).round() as i32;
    let pct_fs = size * 0.2;
    let cap_y = cy + size * 0.16;
    rsx! {
        svg {
            width: "{size}",
            height: "{size}",
            view_box: "0 0 {size} {size}",
            style: "display:block",
            path {
                d: "{track}",
                fill: "none",
                stroke: "rgba(106,95,192,.25)",
                stroke_width: "7",
                stroke_linecap: "round",
            }
            path {
                d: "{proj}",
                fill: "none",
                stroke: "var(--indigo-neon)",
                stroke_width: "3",
                stroke_linecap: "round",
                stroke_dasharray: "2 3",
                opacity: ".7",
            }
            path {
                d: "{saved_arc}",
                fill: "none",
                stroke: "var(--neon)",
                stroke_width: "7",
                stroke_linecap: "round",
                style: "filter:drop-shadow(0 0 5px var(--neon))",
            }
            text {
                x: "{cx}",
                y: "{cy - 2.0}",
                text_anchor: "middle",
                fill: "var(--ink)",
                font_size: "{pct_fs}",
                font_family: "var(--font-display)",
                "{pct}%"
            }
            text {
                x: "{cx}",
                y: "{cap_y}",
                text_anchor: "middle",
                fill: "var(--ink-3)",
                font_size: "9",
                font_family: "var(--font-body)",
                letter_spacing: ".14em",
                "OF TARGET"
            }
        }
    }
}
