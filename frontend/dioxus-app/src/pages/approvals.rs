//! `/approvals` — the AI approval queue (T42).
//!
//! Every receipt the intake pipeline read from a photo waits here as a staged
//! PROPOSAL until a human decides: APPROVE books it, REJECT drops it, APPROVE
//! ALL books a receipt's open proposal after a confirm step. Nothing on this
//! page books on its own; the only path to the ledger is the `phosk_ai`
//! approval service behind the `data::approvals` server fns.
//!
//! Model output is hostile: every proposed text reaches the DOM as a text node
//! (RSX interpolation escapes it) and was already clipped server-side. Low
//! confidence lines take the `--warn` flag (as on /transactions) so several
//! of them never multiply the view's one coral moment — the bulk CONFIRM.

use dioxus::prelude::*;

use crate::components::prims::ScannerBg;
use crate::components::shell::{AiPanel, TopBar};
use crate::components::states::{Awaiting, InlineStatus};
use crate::data::approvals::{
    approve_proposal, approve_receipt_proposals, list_pending_proposals, reject_proposal,
    ProposalDto, ProposedReceiptDto, ReceiptGroupDto,
};
use crate::data::chf2;
use crate::data::cycle::get_cycle;

/// A decision the user can take on the queue.
#[derive(Clone, PartialEq)]
enum Decision {
    /// Book one proposal (by suggestion id).
    Approve(String),
    /// Drop one proposal (by suggestion id).
    Reject(String),
    /// Book the open proposal of one receipt (by receipt slug), confirmed.
    ApproveAll(String),
}

impl Decision {
    /// The suggestion id or receipt slug the decision targets.
    fn target(&self) -> &str {
        match self {
            Self::Approve(t) | Self::Reject(t) | Self::ApproveAll(t) => t,
        }
    }
}

/// The client-side text of a failed call: the server's fixed user-facing
/// message, or a generic transport note (never a raw transport error).
pub(crate) fn error_text(e: &ServerFnError) -> String {
    match e {
        ServerFnError::ServerError { message, .. } => message.clone(),
        _ => "Could not reach the server. Refresh to see the current state.".to_string(),
    }
}

/// `/approvals` — pending AI receipt proposals, one card per receipt.
#[component]
pub fn ApprovalsPage() -> Element {
    let mut ai_collapsed = use_signal(|| false);
    let cycle = use_resource(get_cycle);
    let queue = use_resource(list_pending_proposals);
    let mut pending = use_signal(|| Option::<Decision>::None);
    // `(target, message)` of the last failed decision.
    let mut failed = use_signal(|| Option::<(String, String)>::None);
    // The receipt slug whose APPROVE ALL awaits confirmation.
    let mut confirming = use_signal(|| Option::<String>::None);
    // Set when the last approval wrote nothing new (already booked).
    let mut unchanged = use_signal(|| false);

    let on_decide = use_callback(move |d: Decision| {
        // One decision at a time, never against a stale list.
        if pending.peek().is_some() || queue.pending() {
            return;
        }
        pending.set(Some(d.clone()));
        failed.set(None);
        confirming.set(None);
        unchanged.set(false);
        let mut queue = queue;
        spawn(async move {
            // `Ok(true)`: the call wrote nothing new to the ledger.
            let result = match &d {
                Decision::Approve(id) => approve_proposal(id.clone()).await.map(|o| !o.applied),
                Decision::Reject(id) => reject_proposal(id.clone()).await.map(|()| false),
                Decision::ApproveAll(slug) => approve_receipt_proposals(slug.clone())
                    .await
                    .map(|os| !os.is_empty() && os.iter().all(|o| !o.applied)),
            };
            match result {
                Ok(nothing_new) => unchanged.set(nothing_new),
                Err(e) => failed.set(Some((d.target().to_string(), error_text(&e)))),
            }
            // `restart` flips the list to pending synchronously, so every
            // button stays disabled until the fresh queue lands.
            queue.restart();
            pending.set(None);
        });
    });

    let (cyc_label, cyc_day, cyc_days, cyc_as_of) = match &*cycle.read() {
        Some(Ok(c)) if c.days > 0 => (
            c.label.clone(),
            c.day.to_string(),
            c.days.to_string(),
            c.as_of.clone(),
        ),
        _ => (String::new(), String::new(), String::new(), String::new()),
    };
    let refreshing = *queue.state().read() == UseResourceState::Pending;
    let busy = pending().is_some() || refreshing;
    let groups: Option<Vec<ReceiptGroupDto>> =
        queue.read().as_ref().and_then(|r| r.as_ref().ok()).cloned();
    let load_error = queue
        .read()
        .as_ref()
        .and_then(|r| r.as_ref().err())
        .map(error_text);
    let count = groups.as_ref().map_or_else(
        || "—".to_string(),
        |g| {
            g.iter()
                .map(|x| x.proposals.len())
                .sum::<usize>()
                .to_string()
        },
    );

    rsx! {
        div { class: "pk", style: "height:100vh;min-height:0",
            ScannerBg {
                class: "pk-bg".to_string(),
                seed: 42,
                shapes: r#"[
                    { char: "4", cx: .9, cy: .8, scale: .36, style: "red", morph: "vein", live: true, fill: .46 },
                    { char: "2", cx: .1, cy: .26, scale: .26, style: "faint", morph: "blob", live: false, fill: .42 }
                ]"#.to_string(),
            }
            div { class: "app-shell swap",
                AiPanel { collapsed: ai_collapsed(), on_toggle: move |()| ai_collapsed.toggle() }
                div { class: "app-main",
                    TopBar {
                        active: "APPROVALS".to_string(),
                        label: cyc_label,
                        day: cyc_day,
                        days: cyc_days,
                        as_of: cyc_as_of,
                    }
                    div { class: "app-scroll", "data-screen-label": "APPROVALS",
                        div { class: "subs-wrap",
                            div { class: "subs-top",
                                div {
                                    div { class: "ttl", "Approvals" }
                                    div { class: "sum",
                                        b { "{count}" }
                                        " receipt proposals read by the model · nothing is booked until you approve"
                                    }
                                }
                            }
                            if unchanged() {
                                span { role: "status", class: "cand-ev",
                                    "Already booked · nothing new was written to the ledger"
                                }
                            }
                            div { class: "subs-sec",
                                span { class: "lbl", "⌁ PENDING PROPOSALS" }
                                span { class: "ct ind", "{count}" }
                                span { class: "rule" }
                                span { class: "meta", "AI PROPOSES · YOU APPROVE · ONLY THEN THE LEDGER CHANGES" }
                            }
                            match (groups, load_error) {
                                (None, Some(err)) => rsx! {
                                    Awaiting { label: "APPROVAL QUEUE", message: err }
                                },
                                (None, None) => rsx! {
                                    Awaiting { label: "APPROVAL QUEUE", loading: true }
                                },
                                (Some(list), _) if list.is_empty() => rsx! {
                                    Awaiting {
                                        label: "APPROVAL QUEUE",
                                        legend: "ALL CLEAR",
                                        message: "No proposals waiting. Receipts read from photos land here for review.",
                                    }
                                },
                                (Some(list), _) => rsx! {
                                    for g in list {
                                        ReceiptCard {
                                            key: "{g.receipt_slug}",
                                            group: g.clone(),
                                            busy,
                                            pending: pending(),
                                            failed: failed(),
                                            confirming: confirming() == Some(g.receipt_slug.clone()),
                                            on_decide: move |d| on_decide.call(d),
                                            on_confirm: move |slug: Option<String>| confirming.set(slug),
                                        }
                                    }
                                },
                            }
                        }
                    }
                }
            }
        }
    }
}

/// One receipt's open proposals plus its APPROVE ALL (with a confirm step).
#[component]
fn ReceiptCard(
    group: ReceiptGroupDto,
    busy: bool,
    pending: Option<Decision>,
    failed: Option<(String, String)>,
    confirming: bool,
    on_decide: EventHandler<Decision>,
    on_confirm: EventHandler<Option<String>>,
) -> Element {
    let slug = group.receipt_slug.clone();
    let n = group.proposals.len();
    // At most one proposal per receipt may be open at approval time.
    let conflicting = n > 1;
    let bulk_ok = !conflicting && group.proposals.iter().all(|p| p.bookable);
    let bulk_pending = pending == Some(Decision::ApproveAll(slug.clone()));
    let bulk_error = failed
        .as_ref()
        .filter(|(t, _)| *t == slug)
        .map(|(_, m)| m.clone());
    let lines: usize = group
        .proposals
        .iter()
        .filter_map(|p| p.receipt.as_ref())
        .map(|r| r.lines.len())
        .sum();
    let (s_ask, s_ok) = (slug.clone(), slug);

    rsx! {
        div { class: "cand-list osc-glass hair osc-bkt blue",
            span { class: "osc-leg", "MOD·AI · RECEIPT PROPOSAL" }
            for p in group.proposals.iter().cloned() {
                ProposalRow {
                    key: "{p.suggestion_id}",
                    busy,
                    conflicting,
                    pending: pending.clone(),
                    error: failed.as_ref().filter(|(t, _)| *t == p.suggestion_id).map(|(_, m)| m.clone()),
                    on_decide: move |d| on_decide.call(d),
                    proposal: p,
                }
            }
            div { class: "cand-row",
                span { class: "cand-ev", style: "flex:1 1 auto",
                    if conflicting {
                        b { "{n}" }
                        " conflicting proposals for this receipt · reject all but one"
                    } else {
                        "Bulk approve books this receipt's open proposal"
                    }
                }
                div { class: "cand-acts",
                    if confirming {
                        span { class: "cand-ev",
                            "Book "
                            b { "{n}" }
                            " proposal · "
                            b { "{lines}" }
                            " lines to the ledger?"
                        }
                        button {
                            class: "gbtn coral",
                            disabled: busy,
                            onclick: move |_| on_decide.call(Decision::ApproveAll(s_ok.clone())),
                            "CONFIRM"
                        }
                        button {
                            class: "gbtn",
                            disabled: busy,
                            onclick: move |_| on_confirm.call(None),
                            "CANCEL"
                        }
                    } else {
                        button {
                            class: "gbtn p",
                            disabled: busy || !bulk_ok,
                            onclick: move |_| on_confirm.call(Some(s_ask.clone())),
                            if bulk_pending { "BOOKING…" } else { "APPROVE ALL" }
                        }
                    }
                }
            }
            if bulk_pending || bulk_error.is_some() {
                InlineStatus { pending: bulk_pending, error: bulk_error, pending_label: "Booking…" }
            }
        }
    }
}

/// One proposal: receipt head, its lines, and APPROVE / REJECT.
#[component]
fn ProposalRow(
    proposal: ProposalDto,
    busy: bool,
    conflicting: bool,
    pending: Option<Decision>,
    error: Option<String>,
    on_decide: EventHandler<Decision>,
) -> Element {
    let id = proposal.suggestion_id.clone();
    let approving = pending == Some(Decision::Approve(id.clone()));
    let rejecting = pending == Some(Decision::Reject(id.clone()));
    let (id_ok, id_no) = (id.clone(), id);
    let bookable = proposal.bookable;
    let mismatch = proposal
        .receipt
        .as_ref()
        .is_some_and(|r| r.lines.iter().any(|l| l.mismatch));
    let approve_title = if !bookable {
        "This proposal failed validation · reject it"
    } else if conflicting {
        "Another open proposal targets this receipt · reject all but one"
    } else {
        ""
    };

    rsx! {
        div { class: "cand-row",
            match proposal.receipt {
                Some(r) => rsx! { ReceiptHead { receipt: r } },
                None => rsx! {
                    div { class: "cand-id",
                        span { class: "cand-nm", "Proposal missing" }
                        span { class: "cand-ev", "The staged receipt could not be read · reject it" }
                    }
                },
            }
            div { class: "cand-acts",
                button {
                    class: "gbtn p",
                    disabled: busy || !bookable || conflicting,
                    title: approve_title,
                    onclick: move |_| on_decide.call(Decision::Approve(id_ok.clone())),
                    if approving { "BOOKING…" } else { "APPROVE" }
                }
                button {
                    class: "gbtn",
                    disabled: busy,
                    onclick: move |_| on_decide.call(Decision::Reject(id_no.clone())),
                    if rejecting { "REJECTING…" } else { "REJECT" }
                }
            }
            if !bookable {
                span { class: "cand-conf low", style: "flex-basis:100%",
                    "INVALID · this proposal can't be booked as read · reject it"
                }
            }
            if mismatch {
                span { class: "cand-conf low", style: "flex-basis:100%",
                    "CHECK TOTALS · a line books a different amount than qty × unit price"
                }
            }
            if approving || rejecting || error.is_some() {
                span { style: "flex-basis:100%",
                    InlineStatus {
                        pending: approving || rejecting,
                        error,
                        pending_label: if approving { "Booking…" } else { "Rejecting…" },
                    }
                }
            }
        }
    }
}

/// Shop, date, total, then one line per proposed item with the amount it
/// books (`line_total`), flagged when that is not `qty × unit`.
#[component]
pub(crate) fn ReceiptHead(receipt: ProposedReceiptDto) -> Element {
    let total = chf2(receipt.total);
    rsx! {
        div { class: "cand-id", style: "cursor:default",
            span { class: "cand-nm", "{receipt.shop}" }
            span { class: "cand-ev",
                b { "{receipt.date}" }
                " · {receipt.category} · CHF "
                b { "{total}" }
            }
            for (i, l) in receipt.lines.into_iter().enumerate() {
                {
                    let conf = format!("{:.0}", l.confidence * 100.0);
                    let unit = chf2(l.unit_price);
                    let booked = chf2(l.line_total);
                    let (cls, tag) = if l.low_confidence {
                        ("cand-conf low", "LOW CONF · REVIEW")
                    } else {
                        ("cand-conf", "CONF")
                    };
                    rsx! {
                        div { key: "{i}", class: "cand-row", style: "padding:var(--s-1) 0",
                            span { class: "cand-ev", style: "flex:1 1 auto", "{l.name}" }
                            span { class: "cand-ev", "{l.category}" }
                            span { class: "cand-amt",
                                b { "{l.qty}" }
                                " × CHF "
                                b { "{unit}" }
                                " = CHF "
                                b { "{booked}" }
                            }
                            if l.mismatch {
                                span { class: "cand-conf low", "≠ QTY × UNIT" }
                            }
                            span { class: "{cls}", "{tag} " b { "{conf}%" } }
                        }
                    }
                }
            }
        }
    }
}
