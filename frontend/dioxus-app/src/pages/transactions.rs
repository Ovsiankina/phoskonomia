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
//!   * Line review is live: picking a line's name (or CORRECT on a flagged
//!     line) opens an inline editor that saves through
//!     `correct_transaction_line`; CONFIRM accepts the reading as-is. After a
//!     save the list and every open line list refetch (`LinesRev`), so the ⚠
//!     marks clear and no other view keeps editing an outdated line.
//!   * Low-confidence lines (< 0.7) wear the design system's CAUTION treatment
//!     (`--warn`): a flagged row rule, an amber dot and name. Coral stays the
//!     page's single signal moment.
//!   * NEW (page header) opens the manual-entry form (`NewTransactionForm`),
//!     which saves through `create_transaction`; on success the form closes
//!     and the list refetches. The design export has no NEW control; it sits
//!     next to the CSV export as an indigo `gbtn`.
//!   * The AI RE-READ nudge button still has no server fn (it POSTed the dead
//!     REST layer), so its handler only `stop_propagation`s.
//!   * F3's `TxnLineDto` always carries a concrete `line_total`/`confidence`
//!     (backend-derived), so React's `chf(undefined) → "—"` / `conf == null`
//!     branches collapse to the present-value path; the confidence formatting
//!     (`.toFixed(2)`) is reproduced.
//!   * `TxnLinesDto.low_conf` (camel `lowConf`) and `TxnDetailDto.ocr_regions`
//!     (a count, not an array) replace React's `body.lowConf` / `ocr_regions[]`.

use dioxus::prelude::*;
use phosk_core::money::Money;

use crate::components::edit_transaction::TxnActions;
use crate::components::new_transaction::NewTransactionForm;
use crate::components::prims::{Dot, ScannerBg};
use crate::components::shell::{AiPanel, Sig, SigOcc, SignalPanel, TopBar};
use crate::components::states::Awaiting;
use crate::data::signals::{get_signal, SignalDetailDto};
use crate::data::transactions::{
    correct_transaction_line, correction_error_text, correction_needs_reload, get_transaction,
    get_transaction_lines, list_transactions, TransactionDto, TxnFilter, TxnLineCorrection,
    TxnLineDraft, TxnLineDto, TxnLinesDto,
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

/// Whether a reading is below the review threshold — delegates to
/// `phosk_model::is_low_confidence`, the single source of truth this crate
/// shares with the backend (including how a NaN/out-of-range reading is
/// treated as low-confidence, never as confident).
fn is_low_conf(c: f64) -> bool {
    phosk_model::is_low_confidence(c)
}

/// Confidence tone (React `ConfDot`): `>=0.85` ok, `>=0.7` blue, else `warn`.
///
/// A low reading is a caution, so it takes the canonical `--warn` flag rather
/// than coral (`alert`): several flagged lines must not multiply the page's one
/// coral moment.
fn conf_tone(c: f64) -> &'static str {
    if is_low_conf(c) {
        "warn"
    } else if c >= 0.85 {
        "ok"
    } else {
        "blue"
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

/// Page-wide revision of the receipt lines, bumped after every saved line
/// correction and every refusal that says the page is out of date.
///
/// Every open line list (accordion rows and the receipt screen) reads it in its
/// fetch, so all of them refetch together and no view keeps showing, or
/// editing, an outdated line.
#[derive(Clone, Copy)]
struct LinesRev(Signal<u64>);

/// Subscribe the calling fetch to [`LinesRev`] (a no-op outside the page).
fn track_lines_rev(rev: Option<LinesRev>) {
    if let Some(LinesRev(rev)) = rev {
        let _: u64 = rev();
    }
}

/// A line's name. Clicking it, or Enter / Space while it has focus, picks the
/// line for correction; a flagged line carries the ⚠ mark.
#[component]
fn PickName(name: String, low: bool, index: usize, on_pick: EventHandler<usize>) -> Element {
    let cls = if low { "nm pick low" } else { "nm pick" };
    let text = if low { format!("{name} ⚠") } else { name };
    rsx! {
        span {
            class: "{cls}",
            role: "button",
            tabindex: "0",
            title: "Review / correct this line",
            onclick: move |e: Event<MouseData>| {
                e.stop_propagation();
                on_pick.call(index);
            },
            onkeydown: move |e: Event<KeyboardData>| {
                let key = e.key();
                if key == Key::Enter || key == Key::Character(" ".to_owned()) {
                    e.prevent_default();
                    e.stop_propagation();
                    on_pick.call(index);
                }
            },
            "{text}"
        }
    }
}

/// A single fetched receipt line in the accordion body (React `LineItem`).
///
/// Picking the name opens the line for correction; a flagged (low-confidence)
/// line also shows CONFIRM / CORRECT. See [`LineReview`].
#[component]
fn LineItem(
    l: TxnLineDto,
    #[props(default = false)] active: bool,
    on_select_sig: EventHandler<String>,
    receipt_id: String,
    index: usize,
    editing: bool,
    on_pick: EventHandler<usize>,
    on_saved: EventHandler<()>,
    on_refresh: EventHandler<()>,
) -> Element {
    let conf = l.confidence;
    let low = is_low_conf(conf);
    let row_cls = if low { "line flag" } else { "line" };
    let qty_str = format!("{}×{}", l.qty, chf2(l.unit_price));
    rsx! {
        div { class: "{row_cls}",
            div { class: "nmwrap",
                span { class: "cdot", ConfDot { conf } }
                PickName { name: l.name.clone(), low, index, on_pick }
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
            LineReview {
                receipt_id,
                index,
                l: l.clone(),
                editing,
                on_pick,
                on_saved,
                on_refresh,
            }
        }
    }
}

/// Review controls under one receipt line (T31).
///
/// * a flagged (low-confidence) line shows CONFIRM (accept the reading as-is)
///   and CORRECT (open the editor);
/// * while the line is picked, the inline [`LineEditor`] replaces them.
///
/// CONFIRM's pending / refused states use the shared `Awaiting` block and show
/// only while the line is still flagged: a later save clears them. Every grid
/// child here spans the full row (`txn.css`, T31 block).
///
/// `on_saved` closes the editor and refreshes; `on_refresh` only refreshes (a
/// CONFIRM, or a refusal that says the page is out of date).
#[component]
fn LineReview(
    receipt_id: String,
    index: usize,
    l: TxnLineDto,
    editing: bool,
    on_pick: EventHandler<usize>,
    on_saved: EventHandler<()>,
    on_refresh: EventHandler<()>,
) -> Element {
    let mut pending = use_signal(|| false);
    let mut error = use_signal(|| Option::<String>::None);
    let low = is_low_conf(l.confidence);

    let confirm_fix = TxnLineCorrection::confirm(&receipt_id, index, &l);
    let confirm = move |e: Event<MouseData>| {
        e.stop_propagation();
        if pending() {
            return;
        }
        let fix = confirm_fix.clone();
        pending.set(true);
        error.set(None);
        spawn(async move {
            let res = correct_transaction_line(fix).await;
            pending.set(false);
            match res {
                // Refresh only: another line's open editor must stay open.
                Ok(()) => on_refresh.call(()),
                Err(err) => {
                    error.set(Some(correction_error_text(&err)));
                    if correction_needs_reload(&err) {
                        on_refresh.call(());
                    }
                }
            }
        });
    };
    let busy = pending();

    rsx! {
        if editing {
            LineEditor {
                receipt_id,
                index,
                l,
                on_close: move |()| on_pick.call(index),
                on_saved: move |()| {
                    error.set(None);
                    on_saved.call(());
                },
                on_refresh,
            }
        } else if low {
            div { class: "line-acts",
                button { class: "gbtn p", disabled: busy, onclick: confirm, "CONFIRM" }
                button {
                    class: "gbtn",
                    disabled: busy,
                    onclick: move |e: Event<MouseData>| {
                        e.stop_propagation();
                        error.set(None);
                        on_pick.call(index);
                    },
                    "CORRECT"
                }
            }
            if busy {
                div { class: "line-state",
                    Awaiting {
                        label: "MOD·TX · CONFIRM".to_string(),
                        legend: Some("SAVING".to_string()),
                        message: Some("Confirming this reading…".to_string()),
                    }
                }
            } else if let Some(msg) = error() {
                div { class: "line-state",
                    Awaiting {
                        label: "MOD·TX · CONFIRM".to_string(),
                        legend: Some("NOT SAVED".to_string()),
                        message: Some(msg),
                    }
                }
            }
        }
    }
}

/// Inline correction form for one picked line (T31): item, category, quantity
/// and unit price, then SAVE or CANCEL.
///
/// The fields hold raw text and only the ones the user changed are sent (see
/// `TxnLineCorrection::from_draft`). The server checks the line still shows
/// what this form opened on, validates the changes and parses the price to
/// exact centimes. Saving without an edit confirms the reading.
///
/// If the stored line changes while the form is open (a save elsewhere, or the
/// refetch after a refusal), SAVE gives way to RELOAD, which loads the saved
/// values and drops the edits here. Quantity and price inputs are set in
/// Pilowlava (numbers are display type); labels in VG5000.
#[component]
fn LineEditor(
    receipt_id: String,
    index: usize,
    l: TxnLineDto,
    on_close: EventHandler<()>,
    on_saved: EventHandler<()>,
    on_refresh: EventHandler<()>,
) -> Element {
    // The line as this form opened it: the request's stale-read guard and the
    // baseline that tells edited fields from untouched ones.
    let mut seen = use_signal(|| l.clone());
    let mut draft = use_signal(|| TxnLineDraft::of(&l));
    let mut pending = use_signal(|| false);
    let mut error = use_signal(|| Option::<String>::None);

    let moved = TxnLineDraft::of(&l) != TxnLineDraft::of(&seen.read());
    let save = move |e: Event<MouseData>| {
        e.stop_propagation();
        if pending() || moved {
            return;
        }
        let fix = TxnLineCorrection::from_draft(&receipt_id, index, &seen.read(), &draft.read());
        pending.set(true);
        error.set(None);
        spawn(async move {
            let res = correct_transaction_line(fix).await;
            pending.set(false);
            match res {
                Ok(()) => on_saved.call(()),
                Err(err) => {
                    error.set(Some(correction_error_text(&err)));
                    if correction_needs_reload(&err) {
                        on_refresh.call(());
                    }
                }
            }
        });
    };
    let reload = move |e: Event<MouseData>| {
        e.stop_propagation();
        draft.set(TxnLineDraft::of(&l));
        seen.set(l.clone());
        error.set(None);
    };
    let busy = pending();
    let legend = format!("MOD·TX · LINE {}", index + 1);
    let d = draft.read().clone();

    rsx! {
        div {
            class: "lfix osc-bkt blue",
            onclick: move |e: Event<MouseData>| e.stop_propagation(),
            span { class: "osc-leg", "{legend}" }
            div { class: "lfix-grid",
                label { class: "lfix-f",
                    span { class: "k", "Item" }
                    input {
                        value: "{d.name}",
                        disabled: busy || moved,
                        oninput: move |e| draft.write().name = e.value(),
                    }
                }
                label { class: "lfix-f",
                    span { class: "k", "Category" }
                    input {
                        value: "{d.category}",
                        disabled: busy || moved,
                        oninput: move |e| draft.write().category = e.value(),
                    }
                }
                label { class: "lfix-f",
                    span { class: "k", "Qty" }
                    input {
                        class: "num",
                        inputmode: "decimal",
                        value: "{d.qty}",
                        disabled: busy || moved,
                        oninput: move |e| draft.write().qty = e.value(),
                    }
                }
                label { class: "lfix-f",
                    span { class: "k", "Unit · CHF" }
                    input {
                        class: "num",
                        inputmode: "decimal",
                        value: "{d.unit_price}",
                        disabled: busy || moved,
                        oninput: move |e| draft.write().unit_price = e.value(),
                    }
                }
            }
            if busy {
                div { class: "line-state",
                    Awaiting {
                        label: "MOD·TX · CORRECTION".to_string(),
                        legend: Some("SAVING".to_string()),
                        message: Some("Saving the correction…".to_string()),
                    }
                }
            } else if moved {
                div { class: "line-state",
                    Awaiting {
                        label: "MOD·TX · CORRECTION".to_string(),
                        legend: Some("LINE CHANGED".to_string()),
                        message: Some(
                            "This line was changed since the form opened. RELOAD shows the saved values and drops the edits here."
                                .to_string(),
                        ),
                    }
                }
            } else if let Some(msg) = error() {
                div { class: "line-state",
                    Awaiting {
                        label: "MOD·TX · CORRECTION".to_string(),
                        legend: Some("NOT SAVED".to_string()),
                        message: Some(msg),
                    }
                }
            }
            div { class: "lfix-acts",
                if moved {
                    button { class: "gbtn p", onclick: reload, "RELOAD" }
                } else {
                    button { class: "gbtn p", disabled: busy, onclick: save, "SAVE" }
                }
                button {
                    class: "gbtn",
                    disabled: busy,
                    onclick: move |e: Event<MouseData>| {
                        e.stop_propagation();
                        on_close.call(());
                    },
                    "CANCEL"
                }
            }
        }
    }
}

/// Accordion body — fetches this receipt's lines (React `TxnRowBody`). Mounted
/// only while the row is open (the parent only renders it when `open`), so the
/// `get_transaction_lines` read only fires for opened receipts.
///
/// One line at a time can be picked for correction (`editing`). A saved
/// correction tells the page (`on_changed`), which refetches the list and bumps
/// [`LinesRev`], so these lines refetch too.
#[component]
fn TxnRowBody(
    t: TransactionDto,
    #[props(default)] sel: Option<String>,
    on_select_sig: EventHandler<String>,
    on_details: EventHandler<TransactionDto>,
    on_changed: EventHandler<()>,
) -> Element {
    let id = t.id.clone();
    let rev = try_use_context::<LinesRev>();
    let lines_res = use_resource(move || {
        track_lines_rev(rev);
        get_transaction_lines(id.clone())
    });
    let mut editing = use_signal(|| Option::<usize>::None);

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
    let receipt_id = t.id.clone();

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
                        receipt_id: receipt_id.clone(),
                        index: i,
                        editing: editing() == Some(i),
                        on_pick: move |pick: usize| {
                            let next = if editing() == Some(pick) { None } else { Some(pick) };
                            editing.set(next);
                        },
                        on_saved: move |()| {
                            editing.set(None);
                            on_changed.call(());
                        },
                        on_refresh: move |()| on_changed.call(()),
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
    on_changed: EventHandler<()>,
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
                TxnRowBody { t: t_body, sel, on_select_sig, on_details, on_changed }
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
    let low = is_low_conf(conf);
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
///
/// Same review affordances as the accordion [`LineItem`] (see [`LineReview`]).
#[component]
fn ReceiptItem(
    l: TxnLineDto,
    #[props(default = false)] active: bool,
    on_select_sig: EventHandler<String>,
    receipt_id: String,
    index: usize,
    editing: bool,
    on_pick: EventHandler<usize>,
    on_saved: EventHandler<()>,
    on_refresh: EventHandler<()>,
) -> Element {
    let conf = l.confidence;
    let low = is_low_conf(conf);
    let row_cls = if low { "il flag" } else { "il" };
    let qty_str = format!("{}×{}", l.qty, chf2(l.unit_price));
    rsx! {
        div { class: "{row_cls}",
            div { class: "nmwrap",
                span { class: "cdot", ConfDot { conf } }
                PickName { name: l.name.clone(), low, index, on_pick }
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
            LineReview {
                receipt_id,
                index,
                l: l.clone(),
                editing,
                on_pick,
                on_saved,
                on_refresh,
            }
        }
    }
}

/// The full-screen receipt overlay (React `ReceiptScreen`/`ReceiptBody`).
///
/// Fetches `get_transaction_lines(id)` (the line table) + `get_transaction(id)`
/// (detail: avg_confidence, source, ocr_regions count). Mounted only when a
/// receipt is open, so neither read fires while closed.
///
/// Lines are reviewable here too. A saved correction tells the page
/// (`on_changed`), which bumps [`LinesRev`]: the lines and the detail (average
/// confidence) refetch here, and so do the accordion rows open underneath.
///
/// EDIT / DELETE of the whole transaction ([`TxnActions`]) sit under the
/// meta row: `on_edited` hands the page the patched row, `on_deleted` lets it
/// close this overlay.
#[component]
fn ReceiptScreen(
    t: TransactionDto,
    #[props(default)] sel: Option<String>,
    on_close: EventHandler<()>,
    on_select_sig: EventHandler<String>,
    on_changed: EventHandler<()>,
    on_edited: EventHandler<TransactionDto>,
    on_deleted: EventHandler<()>,
) -> Element {
    let id = t.id.clone();
    let id2 = t.id.clone();
    let receipt_id = t.id.clone();
    let mut editing = use_signal(|| Option::<usize>::None);
    // Guards against the T11 write race between an EDIT/DELETE save here and a
    // line correction below: `edit_transaction` re-inserts the line list it
    // read earlier, so a line fix landing mid-save would be silently reverted.
    // `txn_open` blocks a line pick while the EDIT/DELETE panel is open;
    // `txn_busy` blocks this screen's own close (backdrop and ✕) while a
    // save/delete is in flight, so it can't be dropped with no refresh to
    // show for it.
    let mut txn_open = use_signal(|| false);
    let mut txn_busy = use_signal(|| false);
    let rev = try_use_context::<LinesRev>();
    let lines_res = use_resource(move || {
        track_lines_rev(rev);
        get_transaction_lines(id.clone())
    });
    let detail_res = use_resource(move || {
        track_lines_rev(rev);
        get_transaction(id2.clone())
    });

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
    let conf_pct = avg_conf.map_or(0, |c| {
        if c.is_nan() {
            0
        } else {
            ((c * 100.0).round().clamp(0.0, 100.0)) as i64
        }
    });
    let conf_w = format!("{conf_pct}%");
    let conf_bar_bg = if avg_conf.is_some_and(|c| !is_low_conf(c) && c >= 0.85) {
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
        div {
            class: "rscreen-back",
            onclick: move |_| if !txn_busy() {
                on_close.call(());
            },
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
                        onclick: move |_| if !txn_busy() {
                            on_close.call(());
                        },
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
                            span { style: "color:var(--warn)", "Amber" }
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
                        TxnActions {
                            key: "{t.id}",
                            t: t.clone(),
                            itemised: has_lines || t.item_count > 0,
                            line_editing: editing().is_some(),
                            on_open_change: move |open| txn_open.set(open),
                            on_busy_change: move |busy| txn_busy.set(busy),
                            on_edited,
                            on_deleted,
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
                                    receipt_id: receipt_id.clone(),
                                    index: i,
                                    editing: editing() == Some(i),
                                    on_pick: move |pick: usize| {
                                        if txn_open() {
                                            return;
                                        }
                                        let next = if editing() == Some(pick) { None } else { Some(pick) };
                                        editing.set(next);
                                    },
                                    on_saved: move |()| {
                                        editing.set(None);
                                        on_changed.call(());
                                    },
                                    on_refresh: move |()| on_changed.call(()),
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
    let mut adding = use_signal(|| false);

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
    // Bumped with every list refetch after a line correction; see `LinesRev`.
    let mut lines_rev = use_signal(|| 0_u64);
    use_context_provider(|| LinesRev(lines_rev));

    let mut txns = use_resource(move || {
        let filter = TxnFilter {
            period: hz_period(&horizon()).to_string(),
            shop: shop(),
            category: cat(),
            sort: sort(),
            q: q(),
        };
        list_transactions(filter)
    });

    // An open detail follows the refetched list (after an EDIT the list has
    // the server's own row, e.g. the new date label). A row that left the
    // filtered list keeps the snapshot `on_edited` patched in.
    use_effect(move || {
        let fresh = txns
            .read()
            .as_ref()
            .and_then(|r| r.as_ref().ok())
            .and_then(|l| {
                let open = detail.peek().as_ref().map(|t| t.id.clone())?;
                l.transactions.iter().find(|t| t.id == open).cloned()
            });
        if let Some(row) = fresh {
            if detail.peek().as_ref() != Some(&row) {
                detail.set(Some(row));
            }
        }
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
                                div { class: "txn-acts",
                                    button {
                                        class: "gbtn p",
                                        disabled: adding(),
                                        onclick: move |_| adding.set(true),
                                        "+ NEW"
                                    }
                                    crate::components::csv_export::CsvExport { kind: crate::data::csv_export::CsvExportKind::Transactions }
                                }
                            }

                            if adding() {
                                NewTransactionForm {
                                    on_close: move |()| adding.set(false),
                                    on_saved: move |()| {
                                        adding.set(false);
                                        txns.restart();
                                        lines_rev += 1;
                                    },
                                }
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
                                                                on_changed: move |()| {
                                                                    txns.restart();
                                                                    lines_rev += 1;
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
                    on_changed: move |()| {
                        txns.restart();
                        lines_rev += 1;
                    },
                    on_edited: move |row: TransactionDto| {
                        detail.set(Some(row));
                        txns.restart();
                        lines_rev += 1;
                    },
                    on_deleted: move |()| {
                        if let Some(gone) = detail.take() {
                            open_ids.write().retain(|id| *id != gone.id);
                        }
                        txns.restart();
                        lines_rev += 1;
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

#[cfg(test)]
mod is_low_conf_tests {
    use super::is_low_conf;

    #[test]
    fn nan_confidence_is_low_confidence() {
        assert!(is_low_conf(f64::NAN));
    }

    #[test]
    fn a_normal_confident_value_is_not_low_confidence() {
        assert!(!is_low_conf(0.9));
    }
}

#[cfg(test)]
mod conf_tone_tests {
    use super::conf_tone;

    #[test]
    fn out_of_range_confidence_is_warn() {
        assert_eq!(conf_tone(1.5), "warn");
    }

    #[test]
    fn positive_infinity_confidence_is_warn() {
        assert_eq!(conf_tone(f64::INFINITY), "warn");
    }

    #[test]
    fn nan_confidence_is_warn() {
        assert_eq!(conf_tone(f64::NAN), "warn");
    }

    #[test]
    fn high_confidence_is_ok() {
        assert_eq!(conf_tone(0.9), "ok");
    }

    #[test]
    fn mid_confidence_is_blue() {
        assert_eq!(conf_tone(0.75), "blue");
    }
}
