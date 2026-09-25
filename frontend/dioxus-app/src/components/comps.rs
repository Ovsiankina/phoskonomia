//! Composite UI pieces (OWNED BY AGENT F2).
//!
//! Faithful port of React `src/components/comps.jsx`: the Phoskonomia chrome
//! ([`TopBar`] with its responsive nav tiers, hover-card and mega-menu), the KPI
//! readout ([`Kpi`]), and the four tape/list rows ([`CatRows`], [`TxnTape`],
//! [`RecRow`], [`AlertItem`]). Every class / inline style / token var preserved;
//! numerals render in Pilowlava via the design-system CSS.
//!
//! The React originals were wired to the dead REST layer (`api`/`useGet`) and to
//! `react-router`'s `<Link>`. Here the data those calls produced arrives as
//! **props** (the `#[server]` data layer F3 owns feeds them) and navigation uses
//! the Dioxus router's [`Link`] over [`crate::Route`]. The `react-dom` portals
//! (hover-card, mega-menu) are rendered inline — the design CSS positions them
//! `fixed`, so the visual result is identical without a portal.

use dioxus::prelude::*;
use phosk_core::money::Money;

use crate::components::prims::{pct_tone, CatBar, Dot, HudCell};
use crate::data::{chf, chf2};
use crate::Route;

/// One entry in the Phoskonomia page table (nav / hover-card / mega-menu).
///
/// Faithful port of the `PHOSK_PAGES` array in `comps.jsx`. `route` is the
/// router target (every page exists, so there is no "SOON" disabled case).
#[derive(Clone, PartialEq)]
pub struct PhoskPage {
    /// Full uppercase label (e.g. `DASHBOARD`).
    pub key: &'static str,
    /// Compact label used by the `abbr` nav tier (e.g. `DASH`).
    pub abbr: &'static str,
    /// The page glyph shown in hover-card / mega-menu.
    pub glyph: &'static str,
    /// What the page is for (hover-card / mega-menu description).
    pub desc: &'static str,
    /// Router target.
    pub route: Route,
}

/// The Phoskonomia pages, in nav order.
///
/// Faithful port of `PHOSK_PAGES` (`comps.jsx`). Order is cascade-significant for
/// the probe-width measurement that picks the nav tier.
#[must_use]
pub fn phosk_pages() -> Vec<PhoskPage> {
    vec![
        PhoskPage {
            key: "DASHBOARD",
            abbr: "DASH",
            glyph: "◳",
            desc: "Spend trace, savings & alerts",
            route: Route::DashboardPage {},
        },
        PhoskPage {
            key: "TRANSACTIONS",
            abbr: "TXN",
            glyph: "⊟",
            desc: "Every receipt, itemized",
            route: Route::TransactionsPage {},
        },
        PhoskPage {
            key: "BUDGETS",
            abbr: "BUDG",
            glyph: "▦",
            desc: "Envelopes & monthly caps",
            route: Route::BudgetsPage {},
        },
        PhoskPage {
            key: "CATEGORIES",
            abbr: "CATS",
            glyph: "◆",
            desc: "Names, colours & merges",
            route: Route::CategoriesPage {},
        },
        PhoskPage {
            key: "SUBSCRIPTIONS",
            abbr: "SUBS",
            glyph: "⊠",
            desc: "Standing recurring charges",
            route: Route::SubscriptionsPage {},
        },
        PhoskPage {
            key: "DEBTS",
            abbr: "DEBT",
            glyph: "∿",
            desc: "Balances, payoff & IOUs",
            route: Route::DebtsPage {},
        },
        PhoskPage {
            key: "ANALYTICS",
            abbr: "ANLY",
            glyph: "⌁",
            desc: "Trends & item-signals",
            route: Route::AnalyticsPage {},
        },
        PhoskPage {
            key: "CONFIG",
            abbr: "CFG",
            glyph: "⊙",
            desc: "Preferences for every surface",
            route: Route::ConfigPage {},
        },
    ]
}

/// The default top-bar date format (React `TOPDATE_DEFAULT` in `comps.jsx`).
pub const TOPDATE_DEFAULT: &str = "{label} · DAY {day}/{days}";

/// Substitute the `{label}/{day}/{days}/{asOf}` tokens in a top-bar date format
/// against cycle values, then trim — faithful to the React `dateText` chain
/// (`comps.jsx` L50-56), including its `|| " "` empty fallback.
#[must_use]
pub fn fmt_top_date(fmt: &str, label: &str, day: &str, days: &str, as_of: &str) -> String {
    let out = fmt
        .replace("{label}", label)
        .replace("{day}", day)
        .replace("{days}", days)
        .replace("{asOf}", as_of);
    let trimmed = out.trim();
    if trimmed.is_empty() {
        " ".to_string()
    } else {
        trimmed.to_string()
    }
}

/// Phoskonomia top chrome: brand, responsive nav, date readout, HUD cell.
///
/// Faithful port of `comps.jsx` `TopBar`. `active` is the current page key
/// (e.g. `"DASHBOARD"`).
///
/// The date readout follows React exactly: the configurable `topDateFmt` from
/// the shared `phosk.cfg` store is token-substituted against the current cycle.
/// When `label`/`day`/`days`/`as_of` cycle tokens are supplied, the bar reads
/// `localStorage["phosk.cfg"].topDateFmt` on mount and listens for the
/// `"phoskcfg"` CustomEvent (over a `document::eval` bridge — the only way to
/// touch `window`/`localStorage` from Rust), so editing the format on the Config
/// page updates the bar live, on this page and every other page — faithful to
/// `comps.jsx` L37-56. Pages that haven't been migrated to pass cycle tokens
/// fall back to the pre-baked `date_text` (the legacy path).
///
/// The responsive behaviour is preserved in spirit: hidden full/abbr probes are
/// measured against the free middle space to choose the `full`/`abbr`/`compact`
/// nav tier, and the date readout scrolls (ticker) only when its slot is too
/// small. That layout pass — `ResizeObserver` + width comparison — runs as a
/// `document::eval` bridge (it needs real DOM metrics, exactly like React's
/// `useLayoutEffect`) which posts its decisions back into the signals. The
/// hover-card and mega-menu are rendered inline (the CSS positions them `fixed`).
#[component]
pub fn TopBar(
    #[props(default = String::from("DASHBOARD"))] active: String,
    /// Legacy pre-baked readout (used only when no cycle tokens are supplied).
    #[props(default = String::from(" "))]
    date_text: String,
    /// Cycle token `{label}` (e.g. `"JUN 2026"`). When `Some`, the bar formats
    /// the readout from the live `topDateFmt` cfg value instead of `date_text`.
    #[props(default)]
    label: Option<String>,
    /// Cycle token `{day}` (1-based day index, e.g. `"18"`).
    #[props(default)]
    day: Option<String>,
    /// Cycle token `{days}` (total days in the cycle, e.g. `"30"`).
    #[props(default)]
    days: Option<String>,
    /// Cycle token `{asOf}` (short "today" label, e.g. `"18 JUN"`).
    #[props(default)]
    as_of: Option<String>,
) -> Element {
    let pages = phosk_pages();
    let nav = use_navigator();

    // ---- configurable date format (shared phosk.cfg store) ----
    // Faithful to React's `dateFmt` state + `phoskcfg` listener (comps.jsx
    // L37-49): seed from `localStorage["phosk.cfg"].topDateFmt`, then live-update
    // on every `"phoskcfg"` broadcast. The bridge posts the raw format string
    // back over the eval channel (no JSON dep — the value is a plain string).
    let mut date_fmt = use_signal(|| TOPDATE_DEFAULT.to_string());
    // Only the token-driven path needs the cfg bridge; legacy pages keep `date_text`.
    let token_mode = label.is_some() || day.is_some() || days.is_some() || as_of.is_some();
    // Registered unconditionally (hook order must be stable); the bridge only
    // runs in token mode. Faithful to React's `dateFmt` state + `phoskcfg`
    // listener (comps.jsx L37-49).
    use_effect(move || {
        if !token_mode {
            return;
        }
        let mut eval = document::eval(
            r#"
            try {
              var cur = {};
              try { cur = JSON.parse(localStorage.getItem("phosk.cfg") || "{}") || {}; } catch (e) {}
              if (cur.topDateFmt != null) dioxus.send(String(cur.topDateFmt));
              window.addEventListener("phoskcfg", function (e) {
                var d = (e && e.detail) || {};
                if (Object.prototype.hasOwnProperty.call(d, "topDateFmt")) {
                  dioxus.send(String(d.topDateFmt == null ? "" : d.topDateFmt));
                }
              });
            } catch (e) {}
            "#,
        );
        spawn(async move {
            while let Ok(fmt) = eval.recv::<String>().await {
                date_fmt.set(fmt);
            }
        });
    });

    // The effective readout: token-substitute the live format when cycle tokens
    // are supplied, else the legacy pre-baked string. (React `dateText`.)
    let date_text = if token_mode {
        fmt_top_date(
            &date_fmt(),
            label.as_deref().unwrap_or(""),
            day.as_deref().unwrap_or(""),
            days.as_deref().unwrap_or(""),
            as_of.as_deref().unwrap_or(""),
        )
    } else {
        date_text
    };

    let mut menu = use_signal(|| false); // full grid (hamburger)
    let nav_mode = use_signal(|| String::from("full")); // "full" | "abbr" | "compact"
    let mut hover = use_signal(|| Option::<(String, f64)>::None); // (key, left)
    let mut menu_left = use_signal(|| Option::<f64>::None);
    let tick = use_signal(|| false);
    let tick_dur = use_signal(|| 8_i32);

    // Stable element ids so the JS layout bridge can find this bar's nodes.
    let ids = use_hook(|| {
        use std::sync::atomic::{AtomicU64, Ordering};
        static N: AtomicU64 = AtomicU64::new(0);
        N.fetch_add(1, Ordering::Relaxed)
    });
    let navwrap_id = format!("pk-navwrap-{ids}");
    let probe_full_id = format!("pk-probef-{ids}");
    let probe_abbr_id = format!("pk-probea-{ids}");
    let month_vp_id = format!("pk-monthvp-{ids}");
    let month_tx_id = format!("pk-monthtx-{ids}");

    let compact = nav_mode() == "compact";

    // One layout pass picks the nav tier AND the date ticker from real widths,
    // re-run on resize via ResizeObserver — faithful to React's useLayoutEffect.
    // It posts its decisions back to the Dioxus signals over the eval channel.
    {
        let navwrap_id = navwrap_id.clone();
        let probe_full_id = probe_full_id.clone();
        let probe_abbr_id = probe_abbr_id.clone();
        let month_vp_id = month_vp_id.clone();
        let month_tx_id = month_tx_id.clone();
        let date_dep = date_text.clone();
        use_effect(use_reactive(&date_dep, move |_| {
            let mut set_mode = nav_mode;
            let mut set_tick = tick;
            let mut set_dur = tick_dur;
            // The bridge posts each decision as a single pipe-delimited string
            // (`mode|full`, `tick|1|8`) so no JSON dependency is needed — Dioxus
            // deserializes a JS string straight into a Rust `String`.
            let js = format!(
                r#"
                (function() {{
                  function measure() {{
                    var wrap = document.getElementById("{navwrap}");
                    var pf = document.getElementById("{probef}");
                    var pa = document.getElementById("{probea}");
                    if (wrap && pf && pa) {{
                      var avail = wrap.clientWidth;
                      var mode = avail >= pf.offsetWidth + 4 ? "full"
                               : avail >= pa.offsetWidth + 4 ? "abbr" : "compact";
                      dioxus.send("mode|" + mode);
                    }}
                    var tx = document.getElementById("{monthtx}");
                    var vp = document.getElementById("{monthvp}");
                    if (tx && vp) {{
                      var w = tx.scrollWidth;
                      var over = w > vp.clientWidth + 1;
                      var dur = over ? Math.max(6, Math.round((w + 40) / 32)) : 8;
                      dioxus.send("tick|" + (over ? 1 : 0) + "|" + dur);
                    }}
                  }}
                  measure();
                  if (typeof ResizeObserver !== "undefined") {{
                    var ro = new ResizeObserver(measure);
                    var wrap = document.getElementById("{navwrap}");
                    var vp2 = document.getElementById("{monthvp}");
                    if (wrap) ro.observe(wrap);
                    if (vp2) ro.observe(vp2);
                    ro.observe(document.documentElement);
                  }}
                  window.addEventListener("resize", measure);
                }})();
                "#,
                navwrap = navwrap_id,
                probef = probe_full_id,
                probea = probe_abbr_id,
                monthtx = month_tx_id,
                monthvp = month_vp_id,
            );
            let mut eval = document::eval(&js);
            spawn(async move {
                while let Ok(msg) = eval.recv::<String>().await {
                    let mut parts = msg.split('|');
                    match parts.next() {
                        Some("mode") => {
                            if let Some(m) = parts.next() {
                                set_mode.set(m.to_string());
                            }
                        }
                        Some("tick") => {
                            let over = parts.next() == Some("1");
                            let dur = parts
                                .next()
                                .and_then(|s| s.parse::<i32>().ok())
                                .unwrap_or(8);
                            set_tick.set(over);
                            set_dur.set(dur);
                        }
                        _ => {}
                    }
                }
            });
        }));
    }

    let hover_page = hover().and_then(|(k, left)| {
        pages
            .iter()
            .find(|&p| p.key == k)
            .cloned()
            .map(|p| (p, left))
    });
    let navwrap_cls = if compact {
        "phosk-navwrap compact"
    } else {
        "phosk-navwrap"
    };
    let menu_btn_cls = if menu() {
        "phosk-menu-btn on"
    } else {
        "phosk-menu-btn"
    };
    let month_track_style = if tick() {
        format!("animation-duration:{}s", tick_dur())
    } else {
        String::new()
    };

    rsx! {
        header { class: "phosk-top",
            Link {
                class: "phosk-brand",
                to: Route::DashboardPage {},
                aria_label: "Phoskonomia — home",
                span { class: "mark", "P", b { "O" } }
            }

            div { id: "{navwrap_id}", class: "{navwrap_cls}",
                // hidden probes — natural widths of full + abbreviated nav drive the tier
                nav { id: "{probe_full_id}", class: "phosk-nav-probe", aria_hidden: "true",
                    for p in pages.iter() {
                        span { key: "{p.key}", class: "t", "{p.key}" }
                    }
                }
                nav { id: "{probe_abbr_id}", class: "phosk-nav-probe", aria_hidden: "true",
                    for p in pages.iter() {
                        span { key: "{p.key}", class: "t", "{p.abbr}" }
                    }
                }

                if !compact {
                    // Faithful to React's `<Link>` (which renders an `<a>`): a real
                    // anchor carrying the hover/focus listeners (the design system
                    // styles `.phosk-nav .t`), navigating via the router on click.
                    nav { class: "phosk-nav", onmouseleave: move |_| hover.set(None),
                        for p in pages.iter().cloned() {
                            a {
                                key: "{p.key}",
                                class: if p.key == active { "t on" } else { "t" },
                                href: "#",
                                onclick: {
                                    let route = p.route.clone();
                                    move |e: Event<MouseData>| {
                                        e.prevent_default();
                                        nav.push(route.clone());
                                    }
                                },
                                onmouseenter: {
                                    let key = p.key.to_string();
                                    move |e: Event<MouseData>| {
                                        let left = e.client_coordinates().x;
                                        hover.set(Some((key.clone(), left)));
                                    }
                                },
                                onfocus: {
                                    let key = p.key.to_string();
                                    move |_| hover.set(Some((key.clone(), 0.0)))
                                },
                                if nav_mode() == "abbr" {
                                    "{p.abbr}"
                                } else {
                                    "{p.key}"
                                }
                            }
                        }
                    }
                }

                if compact {
                    button {
                        class: "{menu_btn_cls}",
                        aria_label: "Pages",
                        aria_expanded: "{menu()}",
                        onclick: move |e: Event<MouseData>| {
                            if !menu() {
                                menu_left.set(Some(e.client_coordinates().x));
                            }
                            menu.toggle();
                        },
                        span { class: "mi", if menu() { "✕" } else { "▤" } }
                        span { class: "ml", "MENU" }
                    }
                }
            }

            div { class: "phosk-month", "data-tick": if tick() { "1" } else { "0" },
                span { id: "{month_vp_id}", class: "month-vp",
                    span { class: "month-track", style: "{month_track_style}",
                        span { id: "{month_tx_id}", class: "month-tx", "{date_text}" }
                        if tick() {
                            span { class: "month-tx", aria_hidden: "true", "{date_text}" }
                        }
                    }
                }
            }
            HudCell { glyph: "P".to_string(), size: 38 }

            // inline hover-card (expanded nav only) — full name + what the page is for
            if !compact {
                if let Some((hp, left)) = hover_page {
                    div {
                        class: "phosk-hovercard osc-glass",
                        style: "left:{left}px",
                        onmouseleave: move |_| hover.set(None),
                        div { class: "hc-gl", b { "{hp.glyph}" } }
                        div { class: "hc-tx",
                            span { class: "nm", "{hp.key}" }
                            span { class: "ds", "{hp.desc}" }
                        }
                        if hp.key == active {
                            span { class: "hc-cur", "● ACTIVE" }
                        }
                    }
                }
            }

            // full grid menu (hamburger, compact width only)
            if menu() {
                div { class: "phosk-mega-back", onclick: move |_| menu.set(false) }
                div {
                    class: "phosk-mega osc-glass",
                    role: "menu",
                    style: if let Some(l) = menu_left() { "left:{l}px" } else { String::new() },
                    div { class: "mega-h", "Jump to" }
                    div { class: "mega-grid",
                        for p in pages.iter().cloned() {
                            Link {
                                key: "{p.key}",
                                class: if p.key == active { "mega-card on" } else { "mega-card" },
                                to: p.route.clone(),
                                role: "menuitem",
                                onclick: move |_| menu.set(false),
                                div { class: "mega-gl", b { "{p.glyph}" } }
                                div { class: "mega-tx",
                                    span { class: "nm", "{p.key}" }
                                    span { class: "ds", "{p.desc}" }
                                }
                                if p.key == active {
                                    span { class: "mega-cur", "● ACTIVE" }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

/// KPI readout (framed, corner brackets + Pilowlava numeral).
///
/// Faithful port of `comps.jsx` `Kpi`: a `.kpi` block with optional `accent`
/// (coral) / `blue` modifiers, a label, a big numeral with an optional currency
/// chip, and an optional sub-line. `value` is the page-formatted numeral string
/// (rendered in the Pilowlava display font by CSS).
#[component]
pub fn Kpi(
    label: String,
    value: String,
    #[props(default = String::from("CHF"))] cur: String,
    #[props(default)] sub: Option<String>,
    #[props(default = false)] accent: bool,
    #[props(default = false)] blue: bool,
) -> Element {
    let mut cls = String::from("kpi");
    if accent {
        cls.push_str(" accent");
    }
    if blue {
        cls.push_str(" blue");
    }
    rsx! {
        div { class: "{cls}",
            div { class: "lbl", "{label}" }
            div { class: "big",
                if !cur.is_empty() {
                    span { class: "cur", "{cur}" }
                }
                "{value}"
            }
            if let Some(s) = sub {
                div { class: "sub", "{s}" }
            }
        }
    }
}

/// A category budget row.
///
/// Faithful port of the `c` object consumed by `comps.jsx` `CatRows`. `budget`
/// of [`Money::ZERO`] renders as NO BUDGET.
#[derive(Clone, PartialEq)]
pub struct Cat {
    /// Category name.
    pub name: String,
    /// Spent so far this cycle.
    pub spent: Money,
    /// Cap (`Money::ZERO` = NO BUDGET).
    pub budget: Money,
    /// Number of line items rolled into this category.
    pub items: i64,
    /// Fixed (non-variable) categories show as `ok`/`FIXED`.
    pub fixed: bool,
}

/// Render a list of category budget rows (name, bar, items/cap meta, amt, pct).
///
/// Faithful port of `comps.jsx` `CatRows`. `show_items` toggles the meta between
/// "N items" and FIXED/VARIABLE.
#[component]
pub fn CatRows(cats: Vec<Cat>, #[props(default = true)] show_items: bool) -> Element {
    rsx! {
        for c in cats {
            {
                let p = if c.budget.centimes() > 0 {
                    c.spent.as_chf_f64() / c.budget.as_chf_f64()
                } else {
                    0.0
                };
                let tone = if c.fixed { "ok".to_string() } else { pct_tone(p).to_string() };
                let cn_cls = if c.fixed { "cn fixed" } else { "cn" };
                let pct_cls = format!("pct {tone}");
                let pct_txt = if c.budget.centimes() > 0 {
                    format!("{}%", (p * 100.0).round() as i64)
                } else {
                    "—".to_string()
                };
                let cap_txt = if c.budget.centimes() == 0 {
                    "NO BUDGET".to_string()
                } else {
                    format!("CAP CHF {}", chf(c.budget, 0))
                };
                let meta_left = if show_items {
                    format!("{} items", c.items)
                } else if c.fixed {
                    "FIXED".to_string()
                } else {
                    "VARIABLE".to_string()
                };
                rsx! {
                    div { key: "{c.name}", class: "catrow",
                        span { class: "{cn_cls}", "{c.name}" }
                        div { class: "barwrap",
                            CatBar { spent: c.spent, budget: c.budget, tone: tone.clone() }
                            div { class: "barmeta",
                                span { "{meta_left}" }
                                span { "{cap_txt}" }
                            }
                        }
                        span { class: "amt", "CHF ", b { "{chf2(c.spent)}" } }
                        span { class: "{pct_cls}", "{pct_txt}" }
                    }
                }
            }
        }
    }
}

/// One row of the transaction tape.
///
/// Faithful port of the `t` object consumed by `comps.jsx` `TxnTape`.
#[derive(Clone, PartialEq)]
pub struct Txn {
    /// Stable id (key).
    pub id: String,
    /// Display date.
    pub date: String,
    /// Shop / merchant.
    pub shop: String,
    /// Category label.
    pub category: String,
    /// Signed amount.
    pub amount: Money,
    /// Needs-review flag (shows a ⚠ before the shop).
    pub flag: bool,
    /// Fixed-category dot tone (`blue`) vs variable (`ok`).
    pub fixed: bool,
}

/// Transaction tape — date · shop · category · amount rows.
///
/// Faithful port of `comps.jsx` `TxnTape`.
#[component]
pub fn TxnTape(rows: Vec<Txn>) -> Element {
    rsx! {
        for t in rows {
            div { key: "{t.id}", class: "txn",
                span { class: "dt", "{t.date}" }
                span { class: "sh",
                    if t.flag {
                        span { class: "flag", title: "needs review", "⚠" }
                    }
                    "{t.shop}"
                }
                span { class: "tag",
                    Dot { tone: if t.fixed { "blue".to_string() } else { "ok".to_string() }, size: 5 }
                    "{t.category}"
                }
                span { class: "am",
                    span { class: "c", "CHF" }
                    "{chf2(t.amount)}"
                }
            }
        }
    }
}

/// A recurring/standing charge.
///
/// Faithful port of the `r` object consumed by `comps.jsx` `RecRow`.
#[derive(Clone, PartialEq)]
pub struct Rec {
    /// Charge name.
    pub name: String,
    /// Billing cycle label (e.g. `MONTHLY`).
    pub cycle: String,
    /// Amount per charge.
    pub amount: Money,
    /// `due` | `soon` | other — drives the dot tone.
    pub status: String,
    /// Next charge date (shown unless `due`).
    pub next: String,
    /// Source: `llm` shows AUTO, anything else shows USER.
    pub src: String,
}

/// Recurring list item — status dot, name/cycle, amount/next, source chip.
///
/// Faithful port of `comps.jsx` `RecRow`.
#[component]
pub fn RecRow(r: Rec) -> Element {
    let tone = match r.status.as_str() {
        "due" => "alert",
        "soon" => "warn",
        _ => "ok",
    };
    let nx = if r.status == "due" {
        "⚠ DUE".to_string()
    } else {
        format!("NEXT {}", r.next)
    };
    let is_llm = r.src == "llm";
    let src_cls = if is_llm { "src llm" } else { "src" };
    let src_txt = if is_llm { "AUTO" } else { "USER" };
    rsx! {
        div { class: "rec",
            Dot { tone: tone.to_string(), size: 8 }
            div { class: "body",
                span { class: "nm", "{r.name}" }
                span { class: "cy", "{r.cycle}" }
            }
            div { class: "right",
                span { class: "am", "CHF {chf2(r.amount)}" }
                span { class: "nx", "{nx}" }
            }
            span { class: "{src_cls}", "{src_txt}" }
        }
    }
}

/// An AI / budget alert with action buttons.
///
/// Faithful port of the `a` object consumed by `comps.jsx` `AlertItem`. The
/// React version POSTed each action to the dead REST layer; here `actions` are
/// labels and the page wires behaviour via `on_action` (the F3 server fns).
#[derive(Clone, PartialEq)]
pub struct Alert {
    /// Stable id.
    pub id: String,
    /// `llm` | `warn` | other — picks the icon and the row modifier class.
    pub tone: String,
    /// Small tag chip.
    pub tag: String,
    /// Headline.
    pub head: String,
    /// Body text.
    pub body: String,
    /// Action button labels (first is the primary `p`).
    pub actions: Vec<String>,
}

/// Alert item — icon, tag/head, body, action buttons.
///
/// Faithful port of `comps.jsx` `AlertItem`. `on_action` receives `(alert id,
/// action label)` when a button is pressed (the page routes it to an F3 server
/// fn — the React DISMISS / SNOOZE / APPLY / VIEW / MARK-PAID logic moves there).
#[component]
pub fn AlertItem(
    a: Alert,
    #[props(default)] on_action: Option<EventHandler<(String, String)>>,
) -> Element {
    let ic = match a.tone.as_str() {
        "llm" => "⌁",
        "warn" => "◷",
        _ => "⚠",
    };
    let row_cls = format!("alert-i {}", a.tone);
    let id = a.id.clone();
    rsx! {
        div { class: "{row_cls}",
            span { class: "ic", "{ic}" }
            div { class: "main",
                div { class: "ah",
                    span { class: "tg", "{a.tag}" }
                    span { class: "hd", "{a.head}" }
                }
                div { class: "bd", "{a.body}" }
                div { class: "acts",
                    for (i , act) in a.actions.iter().enumerate() {
                        button {
                            key: "{i}",
                            class: if i == 0 { "btn p" } else { "btn" },
                            onclick: {
                                let id = id.clone();
                                let act = act.clone();
                                move |_| {
                                    if let Some(h) = &on_action {
                                        h.call((id.clone(), act.clone()));
                                    }
                                }
                            },
                            "{act}"
                        }
                    }
                }
            }
        }
    }
}
