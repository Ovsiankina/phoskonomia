//! Empty / loading states (OWNED BY AGENT F2).
//!
//! Faithful port of React `src/components/states.jsx`: the full-width
//! [`Awaiting`] block, the compact [`AwaitingInline`] side-panel body, and the
//! [`Dash`] placeholder numeral. On-brand Oscillocore: corner-bracket readout,
//! indigo structure, VG5000 body.
//!
//! The React originals decoded a REST result (`res = { status, data.todo }`) to
//! pick the message — status 0 meant "backend offline", 501 meant "not
//! implemented". That hand-written REST boundary is gone; with `#[server]` fns a
//! page knows its own pending/empty state directly. So these take a plain
//! `loading` flag and an optional `message` string (the page passes whatever the
//! `use_resource`/`use_server_future` state warrants). [`awaiting_message`] is
//! kept as a tiny helper for the common loading→message default.

use dioxus::prelude::*;

/// Default body text for an awaiting/loading surface.
///
/// Faithful in spirit to `states.jsx` `awaitingMessage`: `loading` wins
/// ("Loading…"), otherwise the optional `message` or the generic fallback.
#[must_use]
pub fn awaiting_message(loading: bool, message: Option<&str>) -> String {
    if loading {
        "Loading…".to_string()
    } else {
        message.unwrap_or("Awaiting backend").to_string()
    }
}

/// Full-width block placeholder for a page section / chart / list.
///
/// Faithful port of `states.jsx` `Awaiting`: an `osc-bkt` corner-bracket box
/// with an `osc-leg` legend (LOADING vs AWAITING BACKEND), an indigo `⌁ LABEL`
/// hud line, the message, and — when offline — a second `.dim` hint line. `tone`
/// is the bracket modifier (default `blue`).
///
/// `hint` carries the JSX's status-0 second `.dim` element (`start it: cargo run
/// -p phosk_api`): the page passes it when its resource resolves to `Err` (the
/// `#[server]` equivalent of REST status 0 — backend offline). `None` → no hint
/// line, faithful to the JSX rendering it only when `status === 0`.
///
/// `legend` overrides the bracket legend (default LOADING / AWAITING BACKEND),
/// e.g. `NOT SAVED` when a write was refused rather than the backend missing.
#[component]
pub fn Awaiting(
    #[props(default = String::from("DATA"))] label: String,
    #[props(default = false)] loading: bool,
    #[props(default)] message: Option<String>,
    #[props(default)] hint: Option<String>,
    #[props(default)] legend: Option<String>,
    #[props(default = String::from("blue"))] tone: String,
    #[props(default)] style: Option<String>,
) -> Element {
    let cls = format!("osc-bkt {tone}");
    let base = "padding:20px 16px;text-align:center";
    let style_attr = match &style {
        Some(s) => format!("{base};{s}"),
        None => base.to_string(),
    };
    let body = awaiting_message(loading, message.as_deref());
    let leg = legend.unwrap_or_else(|| {
        if loading {
            "LOADING"
        } else {
            "AWAITING BACKEND"
        }
        .to_string()
    });
    rsx! {
        div { class: "{cls}", "data-awaiting": "1", style: "{style_attr}",
            span { class: "osc-leg", "{leg}" }
            div { class: "hud sm", style: "justify-content:center;color:var(--indigo-neon)", "⌁ {label}" }
            div { class: "dim", style: "margin-top:7px;font-size:11px;line-height:1.5", "{body}" }
            if let Some(h) = hint {
                div { class: "dim", style: "margin-top:4px;font-size:10px", "{h}" }
            }
        }
    }
}

/// Compact inline placeholder for a side-panel / dock empty body.
///
/// Faithful port of `states.jsx` `AwaitingInline`: a `sig-panel` empty body with
/// a `mk` glyph and the label + message.
#[component]
pub fn AwaitingInline(
    #[props(default = String::from("⌁"))] glyph: String,
    #[props(default = String::from("No data"))] label: String,
    #[props(default = false)] loading: bool,
    #[props(default)] message: Option<String>,
    #[props(default)] variant: Option<String>,
) -> Element {
    let panel_cls = match &variant {
        Some(v) => format!("sig-panel {v}"),
        None => "sig-panel".to_string(),
    };
    let body = awaiting_message(loading, message.as_deref());
    rsx! {
        aside { class: "{panel_cls}",
            div { class: "sig-empty",
                span { class: "mk", "{glyph}" }
                div { class: "tx",
                    "{label}"
                    br {}
                    span { class: "dim", "{body}" }
                }
            }
        }
    }
}

/// One-line status under a control that edits in place (a cap field, a row
/// action): the pending note while a save is in flight, otherwise the error the
/// save returned, otherwise nothing.
///
/// The compact sibling of [`Awaiting`]: HUD micro caps in VG5000, indigo while
/// pending, the `--alert` token on failure. `role` lets assistive tech announce
/// the change.
#[component]
pub fn InlineStatus(
    #[props(default = false)] pending: bool,
    #[props(default)] error: Option<String>,
    #[props(default = String::from("Saving…"))] pending_label: String,
) -> Element {
    let base = "display:block;margin-top:var(--s-1);font-family:var(--font-body);\
                font-size:var(--t-xs);line-height:var(--lh-snug);\
                letter-spacing:var(--tracking-tag);text-transform:uppercase";
    if pending {
        rsx! {
            span { role: "status", style: "{base};color:var(--indigo-3)", "⌁ {pending_label}" }
        }
    } else if let Some(msg) = error {
        rsx! {
            span { role: "alert", style: "{base};color:var(--alert)", "⚠ {msg}" }
        }
    } else {
        rsx! {}
    }
}

/// A single placeholder value for a KPI numeral when totals haven't loaded.
///
/// Faithful port of `states.jsx` `Dash` (exported as `PhoskDash` on `window`):
/// an em-dash in the muted ink-3 token.
#[component]
pub fn Dash() -> Element {
    rsx! {
        span { style: "color:var(--ink-3)", "—" }
    }
}
