//! Shell panels + chrome re-exports (OWNED BY AGENT F2).
//!
//! Faithful port of React `src/components/shell.jsx`: the left **AI panel**
//! ([`AiPanel`]), the right **Signal inspector** ([`SignalPanel`]), the larger
//! sparkline ([`SigSpark`]), and the dashboard item-signal strip ([`SignalCard`]
//! / [`SignalStrip`]). Every class / inline style / token var / SVG path is
//! preserved; numerals render in Pilowlava via the design-system CSS.
//!
//! The React originals were wired to the dead REST layer (`api`/`useGet`): the
//! feed, chat history and AI status loaded over HTTP and every action POSTed.
//! Here that data arrives as **props** (the `#[server]` data layer F3 owns) and
//! mutations are surfaced as callbacks (`on_feed_action`, `on_track`, …) the
//! page wires to its server fns. The one exception is the assistant **chat**:
//! [`AiPanel`] loads the persisted transcript, sends messages and runs `/clear`
//! itself through `crate::data::ai`, so every page gets the same live chat.
//!
//! Note: the mandatory chrome — [`TopBar`] and the page table ([`phosk_pages`])
//! — lives in [`crate::components::comps`] (faithful to the React source, where
//! `TopBar`/`PHOSK_PAGES` are in `comps.jsx`, not `shell.jsx`). It is re-exported
//! here so a page can pull the whole shell vocabulary from `shell::*`.

use dioxus::prelude::*;
use phosk_core::money::Money;

use crate::components::prims::Dot;
use crate::components::states::Awaiting;
use crate::data::ai::{
    bound_chat_text, chat_error_message, get_chat_history, send_chat_message, CHAT_INPUT_MAX_CHARS,
};
use crate::data::chf2;

// Re-export the mandatory chrome so pages can `use crate::components::shell::*`.
// (Page agents consume these; they are unused until the pages land.)
#[allow(unused_imports)]
pub use crate::components::comps::{phosk_pages, PhoskPage, TopBar};

/// Larger sparkline with up/down tone + area, for the signal inspector.
///
/// Faithful port of `shell.jsx` `SigSpark`: a full-width responsive trace
/// (`preserveAspectRatio="none"`), coral when the last point is up vs `--ok`
/// when down, a gradient area fill, a 50%% gridline and an end-point dot. Empty
/// `data` renders just the empty viewBox (same as React).
#[component]
pub fn SigSpark(
    data: Vec<f64>,
    #[props(default = 312.0)] w: f64,
    #[props(default = 88.0)] h: f64,
) -> Element {
    if data.is_empty() {
        return rsx! {
            svg {
                width: "100%",
                height: "{h}",
                view_box: "0 0 {w} {h}",
                preserve_aspect_ratio: "none",
                style: "display:block",
            }
        };
    }
    let max = data.iter().copied().fold(f64::MIN, f64::max);
    let min = data.iter().copied().fold(f64::MAX, f64::min);
    let rng = if (max - min).abs() < f64::EPSILON {
        1.0
    } else {
        max - min
    };
    let up = *data.last().unwrap() >= data[0];
    let col = if up { "var(--neon)" } else { "var(--ok)" };
    let fill_top = if up {
        "rgba(255,94,77,.26)"
    } else {
        "rgba(95,208,138,.22)"
    };
    let n = data.len();
    let xf = |i: usize| i as f64 / (n as f64 - 1.0) * (w - 2.0) + 1.0;
    let yf = |v: f64| h - 6.0 - (v - min) / rng * (h - 14.0);
    let pts = data
        .iter()
        .enumerate()
        .map(|(i, v)| format!("{:.1},{:.1}", xf(i), yf(*v)))
        .collect::<Vec<_>>()
        .join(" ");
    let area = format!("1,{h} {pts} {},{h}", w - 1.0);
    let mid_y = yf(min + rng * 0.5);
    let end_x = xf(n - 1);
    let end_y = yf(*data.last().unwrap());
    rsx! {
        svg {
            width: "100%",
            height: "{h}",
            view_box: "0 0 {w} {h}",
            preserve_aspect_ratio: "none",
            style: "display:block",
            defs {
                linearGradient { id: "sigfill", x1: "0", y1: "0", x2: "0", y2: "1",
                    stop { offset: "0%", stop_color: "{fill_top}" }
                    stop { offset: "100%", stop_color: "rgba(0,0,0,0)" }
                }
            }
            line {
                x1: "1",
                y1: "{mid_y}",
                x2: "{w - 1.0}",
                y2: "{mid_y}",
                stroke: "rgba(106,95,192,.18)",
                stroke_width: "1",
                stroke_dasharray: "2 4",
            }
            polygon { points: "{area}", fill: "url(#sigfill)" }
            polyline {
                points: "{pts}",
                fill: "none",
                stroke: "{col}",
                stroke_width: "2",
                stroke_linejoin: "round",
                stroke_linecap: "round",
                style: "filter:drop-shadow(0 0 3px {col})",
            }
            circle {
                cx: "{end_x}",
                cy: "{end_y}",
                r: "3",
                fill: "var(--neon-white)",
                style: "filter:drop-shadow(0 0 4px {col})",
            }
        }
    }
}

// ===================== LEFT — AI PANEL =====================

/// One AI-feed activity item.
///
/// Faithful port of the `f` object the AI feed yields in `shell.jsx`.
#[derive(Clone, PartialEq)]
pub struct FeedItem {
    /// Stable id (key + the target for track/dismiss).
    pub id: String,
    /// `categorize` | `reprocess` | `suggest` | other — picks the icon.
    pub kind: String,
    /// Activity text.
    pub text: String,
    /// Optional confidence 0..1 (shown as `CONF NN%`).
    pub conf: Option<f64>,
    /// Optional state (`running` shows the RUNNING… chip).
    pub state: Option<String>,
    /// Relative time label.
    pub time: String,
    /// Action button labels (first is primary; coral unless `cand`).
    pub actions: Vec<String>,
    /// Candidate flag — first action is the primary "track" affordance.
    pub cand: bool,
}

/// One chat transcript line.
///
/// Faithful port of the transcript entries in `shell.jsx` `AiPanel`.
#[derive(Clone, PartialEq)]
pub struct ChatMsg {
    /// `usr` (the user) or `sys` (the model).
    pub who: String,
    /// Message text.
    pub text: String,
}

/// Left AI assistant panel — rail, feed, chat, input.
///
/// Faithful port of `shell.jsx` `AiPanel`. `collapsed` shows the vertical rail;
/// `feed` is the (F3-provided) activity feed; `online`/`model`/`engine`/
/// `location` are the AI status. `on_toggle` flips the collapse,
/// `on_feed_action` carries `(item id, action label, is_candidate_primary)` for
/// the feed buttons, and `on_track` carries a candidate id when its primary
/// "track" action fires.
///
/// The chat is live and owned here: the transcript is the persisted history
/// ([`get_chat_history`]); Enter / → submits through [`send_chat_message`]
/// (`/clear` wipes the history). While a message is in flight the input is
/// read-only; on failure the error state shows and the text stays in the
/// input. `msgs` is only a placeholder transcript shown until the persisted
/// history has loaded; `on_send` is notified with each submitted line. Model
/// text is rendered as plain text, bounded by [`bound_chat_text`]. The head's
/// ⇔ toggle widens the panel, and the thread follows its newest line.
#[component]
pub fn AiPanel(
    #[props(default = false)] collapsed: bool,
    #[props(default)] feed: Vec<FeedItem>,
    #[props(default)] msgs: Vec<ChatMsg>,
    #[props(default = false)] online: bool,
    #[props(default = String::from("GEMMA4"))] model: String,
    #[props(default = String::from("OLLAMA"))] engine: String,
    #[props(default = String::from("LOCAL"))] location: String,
    #[props(default)] on_toggle: Option<EventHandler<()>>,
    #[props(default)] on_send: Option<EventHandler<String>>,
    #[props(default)] on_feed_action: Option<EventHandler<(String, String, bool)>>,
    #[props(default)] on_track: Option<EventHandler<String>>,
) -> Element {
    let mut draft = use_signal(String::new);
    // Chat state: the persisted transcript, the in-flight flag, the last
    // send failure, and the wide layout toggle.
    let mut history = use_resource(get_chat_history);
    let mut sending = use_signal(|| false);
    let mut chat_error = use_signal(|| Option::<String>::None);
    let mut wide = use_signal(|| false);

    let panel_cls = format!(
        "ai-panel{}{}",
        if collapsed { " collapsed" } else { "" },
        if wide() { " wide" } else { "" }
    );
    let pulse_style = if online {
        "width:6px;height:6px;border-radius:999px;background:var(--ok);box-shadow:0 0 8px var(--ok)"
    } else {
        "width:6px;height:6px;border-radius:999px;background:var(--ink-3);box-shadow:none"
    };

    // A `Callback` (Copy) so both the send button and Enter key can fire it.
    // The server validates and answers; the draft is cleared only on success,
    // so a failed message stays recoverable in the input.
    let send = use_callback(move |()| {
        let q = draft().trim().to_string();
        if q.is_empty() || *sending.peek() {
            return;
        }
        if let Some(h) = &on_send {
            h.call(q.clone());
        }
        sending.set(true);
        chat_error.set(None);
        spawn(async move {
            match send_chat_message(q).await {
                Ok(_) => draft.set(String::new()),
                Err(e) => chat_error.set(Some(chat_error_message(&e))),
            }
            sending.set(false);
            history.restart();
        });
    });

    // The transcript to render: the persisted history once loaded, the
    // page-provided placeholder until then. Speakers collapse to usr/sys.
    let (lines, history_err): (Vec<ChatMsg>, Option<String>) = match &*history.read() {
        Some(Ok(h)) => (
            h.iter()
                .map(|m| ChatMsg {
                    who: m.who.clone(),
                    text: m.text.clone(),
                })
                .collect(),
            None,
        ),
        Some(Err(_)) => (
            Vec::new(),
            Some("The chat history could not be loaded.".to_string()),
        ),
        None => (msgs.clone(), None),
    };
    let history_loading = history.read().is_none() && lines.is_empty();
    // Re-keyed on every change so the end marker remounts and scrolls into view.
    let chat_end_key = format!("{}-{}-{}", lines.len(), sending(), chat_error().is_some());

    let icon = |k: &str| match k {
        "categorize" => "✓",
        "reprocess" => "⟳",
        "suggest" => "⌁",
        _ => "∿",
    };

    rsx! {
        aside { class: "{panel_cls}",
            div { class: "ai-rail",
                span {
                    class: "exp",
                    title: "Open assistant",
                    onclick: move |_| if let Some(h) = &on_toggle { h.call(()) },
                    "▸"
                }
                span { class: "vlabel", "{model} · ASSISTANT" }
                span { class: "pulse", style: "{pulse_style}" }
            }

            div { class: "ai-body",
                div { class: "ai-head",
                    Dot { tone: if online { "ok".to_string() } else { "blue".to_string() }, size: 7 }
                    div {
                        div { class: "who", "Assistant" }
                        div { class: "mdl", "{engine} · {model} · {location}" }
                    }
                    span {
                        class: if wide() { "wid on" } else { "wid" },
                        title: if wide() { "Narrow the assistant" } else { "Widen the assistant" },
                        onclick: move |_| wide.toggle(),
                        "⇔"
                    }
                    span {
                        class: "col",
                        title: "Collapse",
                        onclick: move |_| if let Some(h) = &on_toggle { h.call(()) },
                        "◂"
                    }
                }

                div { class: "ai-feed",
                    div { class: "fh",
                        span { class: "pulse" }
                        span { class: "lbl", "Live · auto-maintenance" }
                    }
                    if feed.is_empty() {
                        div {
                            class: "dim",
                            style: "font-size:11px;padding:10px 2px;letter-spacing:.04em",
                            "No activity yet — awaiting backend (/ai/feed)."
                        }
                    }
                    for f in feed.iter().cloned() {
                        div { key: "{f.id}", class: "fitem {f.kind}",
                            span { class: "ic", "{icon(&f.kind)}" }
                            div { class: "ftx",
                                span { "{f.text}" }
                                div { class: "fmeta",
                                    if let Some(c) = f.conf {
                                        span { class: "conf", "CONF {(c * 100.0).round() as i64}%" }
                                    }
                                    if f.state.as_deref() == Some("running") {
                                        span { class: "conf", style: "color:var(--indigo-neon)", "RUNNING…" }
                                    }
                                    span { class: "tm", "{f.time}" }
                                }
                                if !f.actions.is_empty() {
                                    div { class: "facts",
                                        for (k , act) in f.actions.iter().enumerate() {
                                            button {
                                                key: "{k}",
                                                class: if k == 0 {
                                                    if f.cand { "gbtn p" } else { "gbtn coral" }
                                                } else {
                                                    "gbtn"
                                                },
                                                onclick: {
                                                    let id = f.id.clone();
                                                    let act = act.clone();
                                                    let cand = f.cand;
                                                    move |_| {
                                                        if cand && k == 0 {
                                                            if let Some(h) = &on_track { h.call(id.clone()); }
                                                        }
                                                        if let Some(h) = &on_feed_action {
                                                            h.call((id.clone(), act.clone(), cand && k == 0));
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
                }

                div { class: "ai-chat",
                    if let Some(err) = history_err {
                        Awaiting { label: "CHAT HISTORY", message: err }
                    } else if history_loading {
                        Awaiting { label: "CHAT HISTORY", loading: true }
                    } else if lines.is_empty() {
                        div {
                            class: "dim",
                            style: "font-size:11px;padding:8px 2px;letter-spacing:.04em",
                            "Ask the assistant anything — it replies from the local model."
                        }
                    }
                    for (i , m) in lines.iter().enumerate() {
                        div {
                            key: "{i}",
                            class: if m.who == "usr" { "msg usr" } else { "msg sys" },
                            span { class: "nm", if m.who == "usr" { "YOU" } else { "{model}" } }
                            div { {bound_chat_text(&m.text)} }
                        }
                    }
                    if sending() {
                        Awaiting { label: "{model} · THINKING", loading: true }
                    }
                    if let Some(err) = chat_error() {
                        Awaiting { label: "CHAT · ERROR", message: err }
                    }
                    for k in std::iter::once(chat_end_key) {
                        div {
                            key: "{k}",
                            class: "ai-chat-end",
                            onmounted: move |e: MountedEvent| async move {
                                let _ = e
                                    .data()
                                    .scroll_to_with_options(ScrollToOptions {
                                        behavior: ScrollBehavior::Smooth,
                                        vertical: ScrollLogicalPosition::End,
                                        horizontal: ScrollLogicalPosition::Nearest,
                                    })
                                    .await;
                            },
                        }
                    }
                }

                div { class: "ai-input",
                    input {
                        value: "{draft}",
                        placeholder: "Ask the assistant · /clear wipes the chat",
                        title: "Enter sends · /clear wipes the chat history",
                        maxlength: "{CHAT_INPUT_MAX_CHARS}",
                        readonly: sending(),
                        oninput: move |e| draft.set(e.value()),
                        onkeydown: move |e| if e.key() == Key::Enter { send.call(()) },
                    }
                    button {
                        class: "send",
                        title: "Send",
                        disabled: sending(),
                        onclick: move |_| send.call(()),
                        "→"
                    }
                }
            }
        }
    }
}

// ===================== RIGHT — SIGNAL PANEL =====================

/// A recent occurrence of a tracked item-signal.
///
/// Faithful port of the `o` object in `shell.jsx` `SignalPanel`.
#[derive(Clone, PartialEq)]
pub struct SigOcc {
    /// Date label.
    pub date: String,
    /// Note / line text.
    pub note: String,
    /// Shop.
    pub shop: String,
    /// Total for the occurrence (`qty * price`).
    pub total: Money,
}

/// A tracked (or candidate) item-signal.
///
/// Faithful port of the `sig` object consumed across `shell.jsx`. `delta_pct`
/// of `None` renders as NEW; `candidate` flips the panel to the track/dismiss
/// candidate view.
#[derive(Clone, PartialEq)]
pub struct Sig {
    /// Stable id.
    pub id: String,
    /// Display label.
    pub label: String,
    /// Parent category.
    pub parent: String,
    /// Description (candidate rationale / sub-line).
    pub desc: String,
    /// Cycle-over-cycle delta percent (`None` = NEW).
    pub delta_pct: Option<i64>,
    /// The trend series (presentation).
    pub series: Vec<f64>,
    /// Quantity this cycle.
    pub cycle_qty: f64,
    /// Unit label.
    pub unit: String,
    /// Spend this cycle.
    pub cycle_spend: Money,
    /// Average per unit.
    pub avg_unit: Money,
    /// Receipt count.
    pub txns: i64,
    /// Tracked-since label.
    pub since: String,
    /// Confidence 0..1 (`None` = —).
    pub conf: Option<f64>,
    /// Recent occurrences.
    pub recent: Vec<SigOcc>,
    /// Candidate (untracked suggestion) flag.
    pub candidate: bool,
}

/// Right-hand item-signal inspector.
///
/// Faithful port of `shell.jsx` `SignalPanel`. With no `sig` it renders the
/// empty inspector prompt. For a tracked signal it shows the delta hero, the
/// `SigSpark` chart with axis, the 6-stat grid, recent occurrences and the
/// "tracked distinctly" footer. For a candidate it shows the chart + a
/// track/dismiss footer. `on_close` closes the panel; `on_track`/`on_dismiss`
/// fire for a candidate (the page routes them to its server fns).
#[component]
pub fn SignalPanel(
    #[props(default)] sig: Option<Sig>,
    #[props(default)] variant: Option<String>,
    #[props(default = String::from("THIS CYCLE"))] cycle_label: String,
    #[props(default)] on_close: Option<EventHandler<()>>,
    #[props(default)] on_track: Option<EventHandler<String>>,
    #[props(default)] on_dismiss: Option<EventHandler<String>>,
) -> Element {
    let panel_cls = match &variant {
        Some(v) => format!("sig-panel {v}"),
        None => "sig-panel".to_string(),
    };

    let Some(sig) = sig else {
        return rsx! {
            aside { class: "{panel_cls}",
                div { class: "sig-empty",
                    span { class: "mk", "⌁" }
                    div { class: "tx",
                        "No signal selected."
                        br {}
                        "Click a tracked item "
                        b { "⌁" }
                        " in any receipt to inspect its trend across all time."
                    }
                }
            }
        };
    };

    let up = sig.delta_pct.unwrap_or(0) >= 0;
    let delta = match sig.delta_pct {
        None => "NEW".to_string(),
        Some(d) => format!("{}{}%", if up { "↑" } else { "↓" }, d.abs()),
    };
    let big_cls = if up { "big up" } else { "big down" };
    let cand_id = sig.id.clone();
    let cand_id2 = sig.id.clone();

    rsx! {
        aside { class: "{panel_cls}",
            div { class: "sig-head",
                div { class: "kls", "⌁ ITEM-SIGNAL · {sig.parent}" }
                div { class: "nm", "{sig.label}" }
                div { class: "ds", "{sig.desc}" }
                if on_close.is_some() {
                    span {
                        class: "x",
                        title: "Close",
                        onclick: move |_| if let Some(h) = &on_close { h.call(()) },
                        "✕"
                    }
                }
            }

            if !sig.candidate {
                div { class: "sig-delta",
                    span { class: "{big_cls}", "{delta}" }
                    span { class: "vs", "vs last cycle · {sig.cycle_qty} {sig.unit}" }
                }
                div { class: "sig-chart",
                    SigSpark { data: sig.series.clone() }
                    div { class: "axis",
                        span { "12 MO AGO" }
                        span { "{cycle_label}" }
                    }
                }
                div { class: "sig-stats",
                    div { class: "st",
                        div { class: "k", "This cycle" }
                        div { class: "v",
                            "{sig.cycle_qty} "
                            span { style: "font-size:11px;color:var(--ink-3)", "{sig.unit}" }
                        }
                    }
                    div { class: "st",
                        div { class: "k", "Spend" }
                        div { class: "v coral", "CHF {chf2(sig.cycle_spend)}" }
                    }
                    div { class: "st",
                        div { class: "k", "Avg / unit" }
                        div { class: "v", "CHF {chf2(sig.avg_unit)}" }
                    }
                    div { class: "st",
                        div { class: "k", "Receipts" }
                        div { class: "v", "{sig.txns}" }
                    }
                    div { class: "st",
                        div { class: "k", "Tracked since" }
                        div { class: "v", style: "font-size:15px", "{sig.since}" }
                    }
                    div { class: "st",
                        div { class: "k", "Confidence" }
                        div { class: "v",
                            if let Some(c) = sig.conf {
                                "{(c * 100.0).round() as i64}%"
                            } else {
                                "—"
                            }
                        }
                    }
                }
                div { class: "sig-recent",
                    div { class: "h", "Recent occurrences" }
                    for (i , o) in sig.recent.iter().enumerate() {
                        div { key: "{i}", class: "sig-occ",
                            span { class: "dt", "{o.date}" }
                            span { class: "no", "{o.note}" }
                            span { class: "sh", "{o.shop}" }
                            span { class: "pr", "CHF {chf2(o.total)}" }
                        }
                    }
                }
            }

            if sig.candidate {
                div { class: "sig-chart", style: "padding-top:16px",
                    SigSpark { data: sig.series.clone() }
                    div { class: "axis",
                        span { "12 MO AGO" }
                        span { "{cycle_label}" }
                    }
                    div { class: "sig-foot", style: "margin-top:18px",
                        div { class: "tx",
                            "Candidate signal. The AI noticed "
                            b { "{sig.label}" }
                            " {sig.desc}. Track it to follow it on its own axis from here on."
                        }
                        div { style: "display:flex;gap:7px;margin-top:10px",
                            button {
                                class: "gbtn p",
                                onclick: move |_| if let Some(h) = &on_track { h.call(cand_id.clone()) },
                                "TRACK SIGNAL"
                            }
                            button {
                                class: "gbtn",
                                onclick: move |_| if let Some(h) = &on_dismiss { h.call(cand_id2.clone()) },
                                "DISMISS"
                            }
                        }
                    }
                }
            }

            if !sig.candidate {
                div { class: "sig-foot",
                    div { class: "tx",
                        "Tracked "
                        b { "distinctly" }
                        " from {sig.parent}. Every matching line item rolls into this signal automatically — the AI maintains it."
                    }
                }
            }
        }
    }
}

// ===================== ITEM-SIGNAL STRIP (dashboard) =====================

/// One card in the dashboard item-signal strip.
///
/// Faithful port of `shell.jsx` `SignalCard`: glyph + label, delta + inline
/// `SigSpark`, and a sub-line (candidate desc, or `qty unit · CHF spend this
/// cycle`). `active` adds the `on` class; candidates add `cand`. `on_select`
/// carries the signal id.
#[component]
pub fn SignalCard(
    sig: Sig,
    #[props(default = false)] active: bool,
    #[props(default)] on_select: Option<EventHandler<String>>,
) -> Element {
    let up = sig.delta_pct.unwrap_or(0) >= 0;
    let delta = if sig.candidate {
        "NEW".to_string()
    } else {
        format!(
            "{}{}%",
            if up { "↑" } else { "↓" },
            sig.delta_pct.unwrap_or(0).abs()
        )
    };
    let mut cls = String::from("ss-card");
    if active {
        cls.push_str(" on");
    }
    if sig.candidate {
        cls.push_str(" cand");
    }
    let dl_cls = if up { "dl up" } else { "dl down" };
    let sub = if sig.candidate {
        sig.desc.clone()
    } else {
        format!(
            "{} {} · CHF {} this cycle",
            sig.cycle_qty,
            sig.unit,
            chf2(sig.cycle_spend)
        )
    };
    let id = sig.id.clone();
    rsx! {
        button {
            class: "{cls}",
            onclick: move |_| if let Some(h) = &on_select { h.call(id.clone()) },
            div { class: "ss-top",
                span { class: "g", "⌁" }
                span { class: "nm", "{sig.label}" }
            }
            div { class: "ss-mid",
                span { class: "{dl_cls}", "{delta}" }
                div { class: "ss-spk",
                    SigSpark { data: sig.series.clone(), w: 128.0, h: 36.0 }
                }
            }
            div { class: "ss-sub", "{sub}" }
        }
    }
}

/// Dashboard item-signal strip — the readout header + a row of [`SignalCard`]s.
///
/// Faithful port of `shell.jsx` `SignalStrip`. `signals` is the fetched list
/// (tracked + optional candidate); `sel` is the selected id; `on_select`
/// carries the clicked id.
#[component]
pub fn SignalStrip(
    #[props(default)] signals: Vec<Sig>,
    #[props(default)] sel: Option<String>,
    #[props(default)] on_select: Option<EventHandler<String>>,
) -> Element {
    rsx! {
        section { class: "sig-strip", "data-screen-label": "SIGNALS",
            div { class: "ss-head",
                span { class: "hud", "⌁ ITEM-SIGNALS" }
                span { class: "ss-rule" }
                span { class: "ss-meta",
                    "AI-MAINTAINED · TRACKED DISTINCTLY FROM CATEGORIES · CLICK TO INSPECT"
                }
            }
            div { class: "ss-cards",
                if signals.is_empty() {
                    div {
                        class: "dim",
                        style: "font-size:11px;padding:14px 2px;letter-spacing:.04em",
                        "No tracked item-signals yet — awaiting backend (/signals)."
                    }
                }
                for s in signals.iter().cloned() {
                    SignalCard {
                        key: "{s.id}",
                        active: sel.as_deref() == Some(s.id.as_str()),
                        on_select: on_select,
                        sig: s,
                    }
                }
            }
        }
    }
}
