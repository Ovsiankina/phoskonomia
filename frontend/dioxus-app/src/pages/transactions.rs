//! Transactions page (route `/transactions`). Faithful port of React
//! `pages/Transactions.jsx` (`TxnPage`).
//!
//! Shape preserved 1:1 with the JSX: the `.pk` root + `ScannerBg`, the
//! `.app-shell.swap` with the left `AiPanel`, the `.app-main` (TopBar + scroll),
//! the page header + summary, the `.txn-filter` bar (horizon chips, three
//! selects, search), the dated `.txn-list` of accordion `.trow`s (each opening a
//! `.trow-body` that fetches its lines), the right-side signal inspector sheet,
//! the floating "collapse all" button, and the full-screen `ReceiptScreen`
//! overlay with its OCR-annotated-scan pane + structured-items pane.
//!
//! Data: where React fanned out `useGet` over the dead REST layer, this fans out
//! `use_resource` over the F3 `#[server]` fns:
//!   * the list ← `list_transactions(TxnFilter)` (refetches when filters change),
//!   * each opened row's lines ← `get_transaction_lines(id)`,
//!   * the receipt detail ← `get_transaction(id)`,
//!   * the inspected pill ← `get_signal(id)`.
//!
//! Notable mapping notes (vs the JSX, which spoke to a now-dead REST layer):
//!   * The per-line CONFIRM / CORRECT controls and the AI RE-READ nudge button
//!     POSTed/PATCHed the dead REST layer. There is no F3 server fn for those
//!     mutations yet, so the buttons are preserved verbatim (same DOM/classes/
//!     affordance) but their handlers only `stop_propagation` — the mutation
//!     wiring lands when the corresponding server fns do. (Faithful: structure +
//!     style preserved; only the dead transport is dropped, per the brief.)
//!   * F3's `TxnLineDto` always carries a concrete `line_total`/`confidence`
//!     (backend-derived), so React's `chf(undefined) → "—"` / `conf == null`
//!     branches collapse to the present-value path; the confidence formatting
//!     (`.toFixed(2)`) is reproduced.
//!   * `TxnLinesDto.low_conf` (camel `lowConf`) and `TxnDetailDto.ocr_regions`
//!     (a count, not an array) replace React's `body.lowConf` / `ocr_regions[]`.

use dioxus::prelude::*;
use phosk_core::money::Money;

use crate::components::prims::{Dot, ScannerBg};
use crate::components::shell::{AiPanel, Sig, SigOcc, SignalPanel, TopBar};
use crate::components::states::Awaiting;
use crate::data::signals::{get_signal, SignalDetailDto};
use crate::data::transactions::{
    get_transaction, get_transaction_lines, list_transactions, TransactionDto, TxnFilter,
    TxnLineDto, TxnLinesDto,
};
use crate::data::{chf2, cycle::get_cycle};

// ── tiny presentation helpers (faithful to the JSX) ─────────────────────────

/// Short pill label for a tracked item-signal id (React `SIG_SHORT`).
fn sig_short(id: &str) -> Option<&'static str> {
    match id {
        "coffee" => Some("COFFEE"),
        "pain" => Some("PAIN AU CHOC."),
        "beer" => Some("BEER"),
        "gruyere" => Some("GRUYÈRE"),
        _ => None,
    }
}

/// Map a horizon chip to the backend `period` query param (React `HZ_PERIOD`).
/// `ALL` → `""` (dropped).
fn hz_period(h: &str) -> &'static str {
    match h {
        "TODAY" => "day",
        "7D" => "week",
        "MONTH" => "month",
        "QUARTER" => "quarter",
        "YEAR" => "year",
        _ => "", // "ALL"
    }
}

/// Confidence tone (React `ConfDot`): `>=0.85` ok, `>=0.7` blue, else alert.
fn conf_tone(c: f64) -> &'static str {
    if c >= 0.85 {
        "ok"
    } else if c >= 0.7 {
        "blue"
    } else {
        "alert"
    }
}

// ── DTO → F2 component-struct mapper (signal detail → Sig) ───────────────────

/// `SignalDetailDto` → the fully-populated `Sig` the inspector panel renders.
/// (Same derivation the dashboard uses for the on-demand inspected pill.)
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

/// Group fetched rows by their day label, preserving order (React `groupByDay`).
fn group_by_day(rows: &[TransactionDto]) -> Vec<(String, Vec<TransactionDto>)> {
    let mut g: Vec<(String, Vec<TransactionDto>)> = Vec::new();
    for t in rows {
        if g.last().map(|(d, _)| d.as_str()) != Some(t.date.as_str()) {
            g.push((t.date.clone(), Vec::new()));
        }
        g.last_mut().unwrap().1.push(t.clone());
    }
    g
}

// ════════════════════════════════════════════════════════════════════════════
// Sub-components (faithful to the JSX's component decomposition)
// ════════════════════════════════════════════════════════════════════════════

/// Confidence dot (React `ConfDot`).
#[component]
fn ConfDot(conf: f64) -> Element {
    rsx! {
        Dot { tone: conf_tone(conf).to_string(), size: 6 }
    }
}

/// Tracked item-signal pill (React `SignalPill`). Renders nothing for an empty
/// id (the line shows its category tag instead).
#[component]
fn SignalPill(
    sig_id: String,
    label: String,
    #[props(default = false)] active: bool,
    on_select: EventHandler<String>,
) -> Element {
    if sig_id.is_empty() {
        return rsx! {};
    }
    let cls = if active { "spill on" } else { "spill" };
    let short = sig_short(&sig_id).map(String::from);
    let text = short.unwrap_or_else(|| {
        if label.is_empty() {
            sig_id.clone()
        } else {
            label.clone()
        }
    });
    let id = sig_id.clone();
    rsx! {
        span {
            class: "{cls}",
            title: "Inspect item-signal",
            onclick: move |e: Event<MouseData>| {
                e.stop_propagation();
                on_select.call(id.clone());
            },
            span { class: "g", "⌁" }
            span { "{text}" }
        }
    }
}

/// Quiet indigo category tag (React `CatTag`).
#[component]
fn CatTag(cat: String) -> Element {
    rsx! {
        span { class: "ctag",
            span { class: "d" }
            "{cat}"
        }
    }
}

/// A single fetched receipt line in the accordion body (React `LineItem`).
///
/// The CONFIRM / CORRECT controls (shown only for low-confidence lines) PATCHed
/// the dead REST layer in React; here they are preserved structurally but their
/// click handlers only `stop_propagation` until an F3 mutation server fn lands.
#[component]
fn LineItem(
    l: TxnLineDto,
    #[props(default = false)] active: bool,
    on_select_sig: EventHandler<String>,
) -> Element {
    let conf = l.confidence;
    let low = conf < 0.7;
    let nm_cls = if low { "nm low" } else { "nm" };
    let nm_text = if low {
        format!("{} ⚠", l.name)
    } else {
        l.name.clone()
    };
    let qty_str = format!("{}×{}", l.qty, chf2(l.unit_price));
    rsx! {
        div { class: "line",
            div { class: "nmwrap",
                span { class: "cdot", ConfDot { conf } }
                span { class: "{nm_cls}", "{nm_text}" }
            }
            if l.signal_id.is_empty() {
                CatTag { cat: l.category.clone() }
            } else {
                SignalPill {
                    sig_id: l.signal_id.clone(),
                    label: l.name.clone(),
                    active,
                    on_select: on_select_sig,
                }
            }
            div { style: "text-align:right",
                div { class: "lp", "{chf2(l.line_total)}" }
                div { class: "qty", "{qty_str}" }
            }
            if low {
                div { style: "grid-column:1 / -1;display:flex;gap:7px;margin-top:6px",
                    button {
                        class: "gbtn p",
                        onclick: move |e: Event<MouseData>| e.stop_propagation(),
                        "CONFIRM"
                    }
                    button {
                        class: "gbtn",
                        onclick: move |e: Event<MouseData>| e.stop_propagation(),
                        "CORRECT"
                    }
                }
            }
        }
    }
}

/// Accordion body — fetches this receipt's lines (React `TxnRowBody`). Mounted
/// only while the row is open (the parent only renders it when `open`), so the
/// `get_transaction_lines` read only fires for opened receipts.
#[component]
fn TxnRowBody(
    t: TransactionDto,
    #[props(default)] sel: Option<String>,
    on_select_sig: EventHandler<String>,
    on_details: EventHandler<TransactionDto>,
) -> Element {
    let id = t.id.clone();
    let lines_res = use_resource(move || get_transaction_lines(id.clone()));

    let body: Option<TxnLinesDto> = lines_res
        .read()
        .as_ref()
        .and_then(|r| r.as_ref().ok())
        .cloned();
    let loading = lines_res.read().is_none();

    let lines: Vec<TxnLineDto> = body.as_ref().map(|b| b.lines.clone()).unwrap_or_default();
    let low_conf = body.as_ref().map_or(t.low_conf_count, |b| b.low_conf);
    let has_lines = !lines.is_empty();

    let sel_str = sel.clone();
    let t_for_review = t.clone();
    let t_for_open = t.clone();

    rsx! {
        div { class: "trow-body",
            if !has_lines {
                Awaiting { label: "RECEIPT LINES".to_string(), loading, tone: "blue".to_string() }
            } else {
                div { class: "lh",
                    span { "ITEM" }
                    span { "TRACK / CATEGORY" }
                    span { "CHF" }
                }
                for (i , l) in lines.iter().enumerate() {
                    LineItem {
                        key: "{i}",
                        l: l.clone(),
                        active: sel_str.as_deref() == Some(l.signal_id.as_str()) && !l.signal_id.is_empty(),
                        on_select_sig,
                    }
                }
                div { class: "trow-foot",
                    span { class: "src",
                        Dot { tone: "ok".to_string(), size: 5 }
                        " SOURCE · PHOTO · OCR + LLM"
                    }
                    if low_conf > 0 {
                        div { class: "ai-nudge",
                            span { class: "g", "⌁" }
                            span {
                                b { "{low_conf}" }
                                " low-confidence — AI re-reading now"
                            }
                            button {
                                class: "gbtn",
                                onclick: move |e: Event<MouseData>| e.stop_propagation(),
                                "RE-READ"
                            }
                            button {
                                class: "gbtn",
                                onclick: move |e: Event<MouseData>| {
                                    e.stop_propagation();
                                    on_details.call(t_for_review.clone());
                                },
                                "REVIEW"
                            }
                        }
                    }
                    span { class: "spacer" }
                    button {
                        class: "gbtn p",
                        onclick: move |e: Event<MouseData>| {
                            e.stop_propagation();
                            on_details.call(t_for_open.clone());
                        },
                        "OPEN RECEIPT ⤢"
                    }
                }
            }
        }
    }
}

/// One accordion row in the dated list (React `TxnRow`).
#[component]
fn TxnRow(
    t: TransactionDto,
    #[props(default = false)] open: bool,
    on_toggle: EventHandler<()>,
    on_details: EventHandler<TransactionDto>,
    #[props(default)] sel: Option<String>,
    on_select_sig: EventHandler<String>,
) -> Element {
    let date_str = t.date.clone();
    let mut parts = date_str.splitn(2, ' ');
    let d = parts.next().unwrap_or("").to_string();
    let mo = parts.next().unwrap_or("").to_string();
    let d_disp = if d.is_empty() { date_str.clone() } else { d };

    let item_count = t.item_count;
    let low = t.low_conf_count;
    let sig_ids = t.signal_ids.clone();
    let fixed = t.fixed;

    let row_cls = if open { "trow open" } else { "trow" };
    let items_str = format!("· {item_count} items");
    let amt_str = chf2(t.amount);

    let t_body = t.clone();

    rsx! {
        div { class: "{row_cls}",
            div { class: "trow-main", onclick: move |_| on_toggle.call(()),
                div { class: "dt",
                    "{d_disp}"
                    small { "{mo}" }
                }
                div { class: "who",
                    span { class: "sh", "{t.shop}" }
                    div { class: "meta",
                        span { class: "ct", "{t.category}" }
                        span { class: "ct", style: "color:var(--ink-3)", "{items_str}" }
                        div { class: "marks",
                            for (i , _s) in sig_ids.iter().enumerate() {
                                span { key: "{i}", class: "mk-sig", title: "tracked item-signal", "⌁" }
                            }
                            if low > 0 {
                                span { class: "mk-warn", title: "low-confidence items", "⚠" }
                            }
                            if fixed {
                                span { class: "ct", style: "color:var(--text-blue)", "· FIXED" }
                            }
                        }
                    }
                }
                div { class: "amt",
                    span { class: "c", "CHF" }
                    "{amt_str}"
                }
                div { class: "chev", "▸" }
            }
            if open {
                TxnRowBody { t: t_body, sel, on_select_sig, on_details }
            }
        }
    }
}

/// The filter bar (React `FilterBar`): horizon chips, three selects, search.
#[component]
fn FilterBar(
    horizon: String,
    shop: String,
    cat: String,
    sort: String,
    q: String,
    shops: Vec<String>,
    cats: Vec<String>,
    on_horizon: EventHandler<String>,
    on_shop: EventHandler<String>,
    on_cat: EventHandler<String>,
    on_sort: EventHandler<String>,
    on_q: EventHandler<String>,
) -> Element {
    const HZ: [&str; 6] = ["TODAY", "7D", "MONTH", "QUARTER", "YEAR", "ALL"];
    rsx! {
        div { class: "txn-filter",
            div { class: "txn-horizon",
                for h in HZ {
                    {
                        let on = h == horizon;
                        let cls = if on { "h on" } else { "h" };
                        rsx! {
                            span {
                                key: "{h}",
                                class: "{cls}",
                                onclick: move |_| on_horizon.call(h.to_string()),
                                "{h}"
                            }
                        }
                    }
                }
            }
            div { class: "txn-sel",
                select {
                    value: "{shop}",
                    onchange: move |e| on_shop.call(e.value()),
                    option { value: "", "ALL SHOPS" }
                    for s in shops.iter() {
                        option { key: "{s}", value: "{s}", "{s}" }
                    }
                }
            }
            div { class: "txn-sel",
                select {
                    value: "{cat}",
                    onchange: move |e| on_cat.call(e.value()),
                    option { value: "", "ALL CATEGORIES" }
                    for c in cats.iter() {
                        option { key: "{c}", value: "{c}", "{c}" }
                    }
                }
            }
            div { class: "txn-sel",
                select {
                    value: "{sort}",
                    onchange: move |e| on_sort.call(e.value()),
                    option { value: "date", "SORT · DATE" }
                    option { value: "amount", "SORT · AMOUNT" }
                    option { value: "shop", "SORT · SHOP" }
                }
            }
            div { class: "txn-search",
                span { class: "mk", "⊙" }
                input {
                    value: "{q}",
                    placeholder: "Search shop or item…",
                    oninput: move |e| on_q.call(e.value()),
                }
            }
        }
    }
}

/// One OCR-annotated receipt-paper line (React's inner `ocrline` map).
#[component]
fn OcrLine(l: TxnLineDto) -> Element {
    let conf = l.confidence;
    let low = conf < 0.7;
    let mut cls = String::from("ocrline");
    if low {
        cls.push_str(" low");
    }
    if !l.signal_id.is_empty() {
        cls.push_str(" sigl");
    }
    let qty_prefix = if l.qty > 1.0 {
        format!("{}× ", l.qty)
    } else {
        String::new()
    };
    let nm = format!("{qty_prefix}{}", l.name);
    let cf = format!("{conf:.2}");
    rsx! {
        div { class: "{cls}",
            span { class: "box" }
            span { class: "nm", "{nm}" }
            span { class: "pr", "{chf2(l.line_total)}" }
            span { class: "cf", "{cf}" }
        }
    }
}

/// One structured line in the receipt screen's items pane (React's inner `il` map).
#[component]
fn ReceiptItem(
    l: TxnLineDto,
    #[props(default = false)] active: bool,
    on_select_sig: EventHandler<String>,
) -> Element {
    let conf = l.confidence;
    let low = conf < 0.7;
    let nm_cls = if low { "nm low" } else { "nm" };
    let nm_text = if low {
        format!("{} ⚠", l.name)
    } else {
        l.name.clone()
    };
    let qty_str = format!("{}×{}", l.qty, chf2(l.unit_price));
    rsx! {
        div { class: "il",
            div { class: "nmwrap",
                span { class: "cdot", ConfDot { conf } }
                span { class: "{nm_cls}", style: "font-size:12.5px", "{nm_text}" }
            }
            if l.signal_id.is_empty() {
                CatTag { cat: l.category.clone() }
            } else {
                SignalPill {
                    sig_id: l.signal_id.clone(),
                    label: l.name.clone(),
                    active,
                    on_select: on_select_sig,
                }
            }
            div { style: "text-align:right",
                div { class: "lp", "{chf2(l.line_total)}" }
                div { class: "qty", "{qty_str}" }
            }
            if low {
                div { style: "grid-column:1 / -1;display:flex;gap:7px;margin-top:6px",
                    button {
                        class: "gbtn p",
                        onclick: move |e: Event<MouseData>| e.stop_propagation(),
                        "CONFIRM"
                    }
                    button {
                        class: "gbtn",
                        onclick: move |e: Event<MouseData>| e.stop_propagation(),
                        "CORRECT"
                    }
                }
            }
        }
    }
}

/// The full-screen receipt overlay (React `ReceiptScreen`/`ReceiptBody`).
///
/// Fetches `get_transaction_lines(id)` (the line table) + `get_transaction(id)`
/// (detail: avg_confidence, source, ocr_regions count). Mounted only when a
/// receipt is open, so neither read fires while closed.
#[component]
fn ReceiptScreen(
    t: TransactionDto,
    #[props(default)] sel: Option<String>,
    on_close: EventHandler<()>,
    on_select_sig: EventHandler<String>,
) -> Element {
    let id = t.id.clone();
    let id2 = t.id.clone();
    let lines_res = use_resource(move || get_transaction_lines(id.clone()));
    let detail_res = use_resource(move || get_transaction(id2.clone()));

    let lbody: Option<TxnLinesDto> = lines_res
        .read()
        .as_ref()
        .and_then(|r| r.as_ref().ok())
        .cloned();
    let lines_loading = lines_res.read().is_none();
    let detail = detail_res
        .read()
        .as_ref()
        .and_then(|r| r.as_ref().ok())
        .cloned();

    let lines: Vec<TxnLineDto> = lbody.as_ref().map(|b| b.lines.clone()).unwrap_or_default();
    let sigs: Vec<String> = lbody.as_ref().map(|b| b.sigs.clone()).unwrap_or_default();
    let low_conf = lbody.as_ref().map_or(t.low_conf_count, |b| b.low_conf);
    let has_lines = !lines.is_empty();

    // detail (avg confidence / source / region count)
    let avg_conf: Option<f64> = detail.as_ref().map(|d| d.avg_confidence);
    let ocr_engine = detail.as_ref().map_or("PADDLEOCR".to_string(), |d| {
        d.source.ocr_engine.to_uppercase()
    });
    let source_type = detail
        .as_ref()
        .map_or("PHOTO".to_string(), |d| d.source.kind.to_uppercase());
    let region_count = detail.as_ref().map_or(lines.len(), |d| {
        let r = d.ocr_regions as usize;
        if r > 0 {
            r
        } else {
            lines.len()
        }
    });

    // Pre-computed compound display strings (the rsx segment parser rejects
    // string literals / closures / format! inside `{...}`).
    let ocr_meta = format!("{ocr_engine} · {region_count} REGIONS");
    let total_str = chf2(t.amount);
    let items_val = if has_lines {
        lines.len().to_string()
    } else {
        t.item_count.to_string()
    };
    let conf_pct = avg_conf.map_or(0, |c| (c * 100.0).round() as i64);
    let conf_w = format!("{conf_pct}%");
    let conf_bar_bg = if avg_conf.is_some_and(|c| c >= 0.85) {
        "var(--ok)"
    } else {
        "var(--warn)"
    };
    let conf_label = avg_conf.map_or("—".to_string(), |_| format!("{conf_pct}%"));
    let badge_str = format!("SOURCE · {source_type}");
    let low_conf_word = if low_conf > 1 { "items" } else { "item" };
    let sigs_count = sigs.len();
    let sigs_word = if sigs_count > 1 { "signals" } else { "signal" };

    rsx! {
        div { class: "rscreen-back", onclick: move |_| on_close.call(()),
            div {
                class: "rscreen",
                onclick: move |e: Event<MouseData>| e.stop_propagation(),
                div { class: "rscreen-h",
                    span { class: "bc",
                        b { "TRANSACTIONS" }
                        " / {t.date} /"
                    }
                    span { class: "sh", "{t.shop}" }
                    span {
                        class: "x",
                        title: "Close",
                        onclick: move |_| on_close.call(()),
                        "✕"
                    }
                }
                div { class: "rscreen-b",
                    // OCR photo reference
                    div { class: "ocr-pane",
                        div { class: "ph-h",
                            span { class: "t", "⌁ OCR · ANNOTATED SCAN" }
                            span { class: "s", "{ocr_meta}" }
                        }
                        if !has_lines {
                            Awaiting { label: "ANNOTATED SCAN".to_string(), loading: lines_loading, tone: "blue".to_string() }
                        } else {
                            div { class: "receipt-paper",
                                div { class: "rp-shop", "{t.shop}" }
                                div { class: "rp-meta", "{t.date} · CHF · TICKET" }
                                div { class: "rp-rule" }
                                for (i , l) in lines.iter().enumerate() {
                                    OcrLine { key: "{i}", l: l.clone() }
                                }
                                div { class: "rp-total",
                                    span { "TOTAL" }
                                    span { "CHF {total_str}" }
                                }
                            }
                        }
                        div { style: "margin-top:12px;font-size:9.5px;color:var(--ink-3);line-height:1.6;letter-spacing:.04em",
                            "Boxes = detected regions. "
                            span { style: "color:var(--neon)", "Coral" }
                            " = low confidence, queued for AI re-read. "
                            span { style: "color:var(--neon-hot)", "⌁" }
                            " = rolled into a tracked item-signal."
                        }
                    }

                    // structured items + AI
                    div { class: "items-pane",
                        div { class: "meta-row",
                            div { class: "kv",
                                span { class: "k", "Total" }
                                span { class: "v", style: "color:var(--neon)", "CHF {total_str}" }
                            }
                            div { class: "kv",
                                span { class: "k", "Items" }
                                span { class: "v", "{items_val}" }
                            }
                            div { class: "kv",
                                span { class: "k", "Category" }
                                span { class: "v", style: "font-size:13px", "{t.category}" }
                            }
                            span { class: "badge", "{badge_str}" }
                        }
                        div { class: "conf-sum",
                            span { "READING CONFIDENCE" }
                            div { class: "bar",
                                i { style: "width:{conf_w};background:{conf_bar_bg}" }
                            }
                            span { style: "font-family:var(--font-display);color:var(--ink)", "{conf_label}" }
                        }

                        if low_conf > 0 {
                            div { class: "ai-nudge", style: "margin:10px 0 4px",
                                span { class: "g", "⌁" }
                                span {
                                    "AI is re-reading "
                                    b { "{low_conf}" }
                                    " blurred {low_conf_word}. Confirm or correct below."
                                }
                                button {
                                    class: "gbtn",
                                    onclick: move |e: Event<MouseData>| e.stop_propagation(),
                                    "RE-READ"
                                }
                            }
                        }

                        div {
                            class: "lh",
                            style: "display:grid;grid-template-columns:1fr auto auto;gap:14px;padding:12px 0 7px;font-size:8.5px;letter-spacing:.18em;text-transform:uppercase;color:var(--ink-3);border-bottom:1px solid var(--hairline)",
                            span { "ITEM" }
                            span { "TRACK / CATEGORY" }
                            span { "CHF" }
                        }
                        if !has_lines {
                            Awaiting { label: "LINE ITEMS".to_string(), loading: lines_loading, tone: "blue".to_string() }
                        } else {
                            for (i , l) in lines.iter().enumerate() {
                                ReceiptItem {
                                    key: "{i}",
                                    l: l.clone(),
                                    active: sel.as_deref() == Some(l.signal_id.as_str()) && !l.signal_id.is_empty(),
                                    on_select_sig,
                                }
                            }
                        }

                        if sigs_count > 0 {
                            div {
                                class: "ai-nudge",
                                style: "margin-top:16px;border-color:var(--hairline);background:rgba(20,14,44,.34)",
                                span { class: "g", style: "color:var(--indigo-neon)", "⌁" }
                                span {
                                    "This receipt feeds "
                                    b { "{sigs_count}" }
                                    " tracked {sigs_word}. Click any ⌁ pill to inspect its trend across all time."
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

/// Right-side signal inspector sheet (React `SignalSheet`). Mounted only while a
/// signal id is selected, so `get_signal` fires for the inspected pill only.
#[component]
fn SignalSheet(sig_id: String, cycle_label: String, on_close: EventHandler<()>) -> Element {
    let id = sig_id.clone();
    let detail_res = use_resource(move || get_signal(id.clone()));
    let sig_obj: Option<Sig> = detail_res
        .read()
        .as_ref()
        .and_then(|r| r.as_ref().ok())
        .map(sig_of_detail);

    rsx! {
        div { class: "sig-sheet-back", onclick: move |_| on_close.call(()),
            div {
                class: "sig-sheet",
                onclick: move |e: Event<MouseData>| e.stop_propagation(),
                SignalPanel {
                    sig: sig_obj,
                    variant: "sheet".to_string(),
                    cycle_label,
                    on_close: move |()| on_close.call(()),
                }
            }
        }
    }
}

// ════════════════════════════════════════════════════════════════════════════

/// The composed Transactions page.
#[component]
pub fn TransactionsPage() -> Element {
    // ---- cycle (top-bar date) ----
    let cycle = use_resource(get_cycle);

    // ---- UI state (was: useState) ----
    let mut ai_collapsed = use_signal(|| false);
    let mut open_ids = use_signal(Vec::<String>::new);
    let mut detail = use_signal(|| Option::<TransactionDto>::None);
    let mut sel = use_signal(|| Option::<String>::None);
    let mut drawer_sig = use_signal(|| false);

    // ---- filters → list params (changing any re-fetches the list) ----
    let mut horizon = use_signal(|| "MONTH".to_string());
    let mut shop = use_signal(String::new);
    let mut cat = use_signal(String::new);
    let mut sort = use_signal(|| "date".to_string());
    let mut q = use_signal(String::new);

    // The list resource — refetches whenever any filter signal changes (Dioxus
    // tracks the reads inside the closure, exactly like React's `useGet` deps).
    //
    // React debounced the free-text query (a 300ms `setTimeout`) purely to spare
    // the dead REST layer per-keystroke requests. With in-process `#[server]`
    // calls there is no network round-trip to coalesce, so the query feeds the
    // filter directly and the list filters live — behaviourally identical to what
    // the debounced version converged to.
    let txns = use_resource(move || {
        let filter = TxnFilter {
            period: hz_period(&horizon()).to_string(),
            shop: shop(),
            category: cat(),
            sort: sort(),
            q: q(),
        };
        list_transactions(filter)
    });

    // ---- read resources into local snapshots ----
    let list = txns.read().as_ref().and_then(|r| r.as_ref().ok()).cloned();
    let list_loading = txns.read().is_none();

    let rows: Vec<TransactionDto> = list
        .as_ref()
        .map(|b| b.transactions.clone())
        .unwrap_or_default();
    let shops: Vec<String> = list
        .as_ref()
        .map(|b| b.available_shops.clone())
        .unwrap_or_default();
    let cats: Vec<String> = list
        .as_ref()
        .map(|b| b.available_categories.clone())
        .unwrap_or_default();
    let entry_count = list
        .as_ref()
        .map_or(rows.len() as u32, |b| b.summary.entry_count);
    let total_amount = list.as_ref().map(|b| b.summary.total_amount);
    let period_label = list.as_ref().map_or_else(
        // `horizon` is a Dioxus `Signal`, not a plain fn; the closure reads it.
        #[allow(clippy::redundant_closure)]
        || horizon(),
        |b| b.summary.period_label.clone(),
    );
    let has_rows = !rows.is_empty();

    let grouped = group_by_day(&rows);

    // cycle convenience for the top bar.
    let c = cycle.read().as_ref().and_then(|r| r.as_ref().ok()).cloned();
    let topbar_date = c
        .as_ref()
        .filter(|c| c.days > 0)
        .map_or(String::new(), |c| {
            format!("{} · DAY {}/{}", c.label, c.day, c.days)
        });

    // open-set conveniences.
    let open_now = open_ids();
    let open_count = open_now.len();
    let sel_id = sel();
    let show_sheet = drawer_sig() && sel_id.is_some();
    let detail_open = detail();

    // pre-computed header strings.
    let entry_str = entry_count.to_string();
    let total_str = total_amount.map_or("—".to_string(), chf2);

    // floating collapse-all button class.
    let collapse_cls = if ai_collapsed() {
        "collapse-all ai-min"
    } else {
        "collapse-all"
    };
    let collapse_n = open_count.to_string();

    rsx! {
        div { class: "pk", style: "height:100vh;min-height:0",
            // Transactions field — molten signal rides HIGH-LEFT (a stream
            // entering the frame), trailing wire + faint blobs toward the bottom.
            ScannerBg {
                class: "pk-bg".to_string(),
                seed: 47,
                shapes: r#"[
                    { char: "8", cx: .24, cy: .21, scale: .4, style: "red", morph: "vein", live: true, fill: .52 },
                    { char: "e", cx: .87, cy: .6, scale: .28, style: "faint", morph: "blob", live: false, fill: .48 },
                    { char: "1", cx: .62, cy: .9, scale: .2, style: "wire", morph: "vein", live: false, fill: .36 },
                    { char: "5", cx: .11, cy: .84, scale: .18, style: "faint", morph: "blob", live: false, fill: .36 }
                ]"#.to_string(),
            }

            div { class: "app-shell swap",
                AiPanelTxn {
                    collapsed: ai_collapsed(),
                    on_toggle: move |()| ai_collapsed.toggle(),
                    on_track: move |id: String| {
                        sel.set(Some(id));
                        drawer_sig.set(true);
                    },
                }

                div { class: "app-main",
                    TopBar { active: "TRANSACTIONS".to_string(), date_text: topbar_date }
                    div { class: "app-scroll", "data-screen-label": "TRANSACTIONS",
                        div { class: "txn-wrap",
                            div { class: "txn-top",
                                div {
                                    div { class: "ttl", "Transactions" }
                                    div { class: "sum",
                                        b { "{entry_str}" }
                                        " entries · "
                                        span { class: "coral", "CHF {total_str}" }
                                        " · {period_label}"
                                    }
                                }
                                crate::components::csv_export::CsvExport { kind: crate::data::csv_export::CsvExportKind::Transactions }
                            }

                            FilterBar {
                                horizon: horizon(),
                                shop: shop(),
                                cat: cat(),
                                sort: sort(),
                                q: q(),
                                shops,
                                cats,
                                on_horizon: move |h: String| horizon.set(h),
                                on_shop: move |s: String| shop.set(s),
                                on_cat: move |c: String| cat.set(c),
                                on_sort: move |s: String| sort.set(s),
                                on_q: move |v: String| q.set(v),
                            }

                            div { class: "txn-list",
                                if !has_rows {
                                    Awaiting { label: "TX · LIST".to_string(), loading: list_loading, tone: "blue".to_string() }
                                } else {
                                    for (day , items) in grouped.iter() {
                                        {
                                            let day = day.clone();
                                            let items = items.clone();
                                            rsx! {
                                                div { key: "{day}", class: "day",
                                                    "{day} "
                                                    span { class: "ru" }
                                                }
                                                for t in items.into_iter() {
                                                    {
                                                        let id = t.id.clone();
                                                        let id_toggle = t.id.clone();
                                                        let is_open = open_now.iter().any(|x| x == &id);
                                                        rsx! {
                                                            TxnRow {
                                                                key: "{id}",
                                                                t,
                                                                open: is_open,
                                                                on_toggle: move |()| {
                                                                    let mut next = open_ids();
                                                                    if let Some(pos) = next.iter().position(|x| x == &id_toggle) {
                                                                        next.remove(pos);
                                                                    } else {
                                                                        next.push(id_toggle.clone());
                                                                    }
                                                                    open_ids.set(next);
                                                                },
                                                                on_details: move |t: TransactionDto| detail.set(Some(t)),
                                                                sel: sel_id.clone(),
                                                                on_select_sig: move |id: String| {
                                                                    sel.set(Some(id));
                                                                    drawer_sig.set(true);
                                                                },
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

            if show_sheet {
                SignalSheet {
                    sig_id: sel_id.clone().unwrap_or_default(),
                    cycle_label: period_label.clone(),
                    on_close: move |()| drawer_sig.set(false),
                }
            }

            if open_count > 1 {
                button {
                    class: "{collapse_cls}",
                    title: "Collapse all open transactions",
                    onclick: move |_| open_ids.set(Vec::new()),
                    span { class: "ca-n", "{collapse_n}" }
                    span { class: "ca-l", "COLLAPSE ALL" }
                    span { class: "ca-i", "▴" }
                }
            }

            if let Some(t) = detail_open {
                ReceiptScreen {
                    t,
                    sel: sel_id.clone(),
                    on_close: move |()| detail.set(None),
                    on_select_sig: move |id: String| {
                        sel.set(Some(id));
                        drawer_sig.set(true);
                    },
                }
            }
        }
    }
}

/// Tiny wrapper around the shared `AiPanel`, carrying the page's collapse + track
/// wiring (the panel lives outside the page-state closure in the `.app-shell`,
/// faithful to the JSX). `on_track` selects + drawers the inspected signal.
#[component]
fn AiPanelTxn(
    collapsed: bool,
    on_toggle: EventHandler<()>,
    on_track: EventHandler<String>,
) -> Element {
    rsx! {
        AiPanel {
            collapsed,
            on_toggle: move |()| on_toggle.call(()),
            on_track: move |id: String| on_track.call(id),
        }
    }
}
