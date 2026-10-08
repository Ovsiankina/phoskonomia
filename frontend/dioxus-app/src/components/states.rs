//! Empty / loading states (OWNED BY AGENT F2).
//!
//! Faithful port of React `src/components/states.jsx`: the full-width
//! [`Awaiting`] block, the compact [`AwaitingInline`] side-panel body, and the
//! [`Dash`] placeholder numeral. On-brand Oscillocore: corner-bracket readout,
//! indigo structure, VG5000 body.
//!
//! The React originals decoded a REST result (`res = { status, data.todo }`) to
//! pick the message. That hand-written REST boundary is gone; with `#[server]`
//! fns a page knows its own state directly, and [`Awaiting`] shows one of three
//! clearly separate states:
//!
//! - **loading** (`loading: true`): "Loading…", legend LOADING;
//! - **error** (`error: Some(text)`): the real failure text in the caution
//!   colour, legend ERROR;
//! - **empty** (neither): the page's own `message` (e.g. "No transactions yet —
//!   add one with + NEW"), or the neutral [`EMPTY_MESSAGE`], legend EMPTY.
//!
//! A `message` without an `error` is a plain notice (legend NOTICE) — some
//! callers still pass a failure text that way. `legend` always wins.
//! [`awaiting_message`] is kept as a tiny helper for the loading→message default.

use dioxus::prelude::*;

/// The neutral body of an empty surface whose page gave no message of its own.
pub const EMPTY_MESSAGE: &str = "Nothing here yet";

/// Default body text for an awaiting/loading surface.
///
/// Faithful in spirit to `states.jsx` `awaitingMessage`: `loading` wins
/// ("Loading…"), otherwise the optional `message` or the neutral
/// [`EMPTY_MESSAGE`].
#[must_use]
pub fn awaiting_message(loading: bool, message: Option<&str>) -> String {
    if loading {
        "Loading…".to_string()
    } else {
        message.unwrap_or(EMPTY_MESSAGE).to_string()
    }
}

/// The bracket legend for a state: an explicit `legend` wins, then LOADING,
/// ERROR, NOTICE (a message with no error) and EMPTY.
#[must_use]
pub fn awaiting_legend(legend: Option<&str>, loading: bool, error: bool, message: bool) -> String {
    if let Some(l) = legend {
        return l.to_string();
    }
    if loading {
        "LOADING"
    } else if error {
        "ERROR"
    } else if message {
        "NOTICE"
    } else {
        "EMPTY"
    }
    .to_string()
}

/// Full-width block placeholder for a page section / chart / list.
///
/// Faithful port of `states.jsx` `Awaiting`: an `osc-bkt` corner-bracket box
/// with an `osc-leg` legend (see [`awaiting_legend`]), an indigo `⌁ LABEL` hud
/// line, the body, and optionally a second `.dim` hint line. `tone` is the
/// bracket modifier (default `blue`).
///
/// The body is "Loading…" while `loading`, else the `error` text (in the
/// `--warn` caution colour — the page keeps its one coral moment), else the
/// `message`, else [`EMPTY_MESSAGE`].
///
/// `hint` carries the JSX's status-0 second `.dim` element (`start it: cargo run
/// -p phosk_api`): the page passes it when its resource resolves to `Err` (the
/// `#[server]` equivalent of REST status 0 — backend offline). `None` → no hint
/// line, faithful to the JSX rendering it only when `status === 0`.
///
/// `legend` overrides the bracket legend, e.g. `NOT SAVED` when a write was
/// refused.
#[component]
pub fn Awaiting(
    #[props(default = String::from("DATA"))] label: String,
    #[props(default = false)] loading: bool,
    #[props(default)] message: Option<String>,
    #[props(default)] error: Option<String>,
    #[props(default)] hint: Option<String>,
    #[props(default)] legend: Option<String>,
    #[props(default = String::from("blue"))] tone: String,
    #[props(default)] style: Option<String>,
) -> Element {
    let cls = format!("osc-bkt {tone}");
    let base = "padding:var(--s-5) var(--s-4);text-align:center";
    let style_attr = match &style {
        Some(s) => format!("{base};{s}"),
        None => base.to_string(),
    };
    let failed = !loading && error.is_some();
    let leg = awaiting_legend(legend.as_deref(), loading, failed, message.is_some());
    let body = if failed {
        error.unwrap_or_default()
    } else {
        awaiting_message(loading, message.as_deref())
    };
    let body_style = if failed {
        "margin-top:var(--s-2);font-size:var(--t-xs);line-height:var(--lh-body);color:var(--warn)"
    } else {
        "margin-top:var(--s-2);font-size:var(--t-xs);line-height:var(--lh-body)"
    };
    rsx! {
        div { class: "{cls}", "data-awaiting": "1", style: "{style_attr}",
            span { class: "osc-leg", "{leg}" }
            div { class: "hud sm", style: "justify-content:center;color:var(--indigo-neon)", "⌁ {label}" }
            div { class: "dim", role: if failed { "alert" } else { "status" }, style: "{body_style}", "{body}" }
            if let Some(h) = hint {
                div { class: "dim", style: "margin-top:var(--s-1);font-size:var(--t-xs)", "{h}" }
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

#[cfg(test)]
mod tests {
    use super::{awaiting_legend, awaiting_message, EMPTY_MESSAGE};

    #[test]
    fn loading_wins_over_any_message() {
        assert_eq!(awaiting_message(true, Some("x")), "Loading…");
        assert_eq!(awaiting_legend(None, true, true, true), "LOADING");
    }

    #[test]
    fn an_empty_state_never_blames_the_backend() {
        assert_eq!(awaiting_message(false, None), EMPTY_MESSAGE);
        assert_eq!(awaiting_legend(None, false, false, false), "EMPTY");
        assert!(!EMPTY_MESSAGE.to_lowercase().contains("backend"));
    }

    #[test]
    fn errors_and_notices_have_their_own_legends() {
        assert_eq!(awaiting_legend(None, false, true, false), "ERROR");
        assert_eq!(awaiting_legend(None, false, false, true), "NOTICE");
        assert_eq!(
            awaiting_legend(Some("NOT SAVED"), true, true, true),
            "NOT SAVED"
        );
    }
}
