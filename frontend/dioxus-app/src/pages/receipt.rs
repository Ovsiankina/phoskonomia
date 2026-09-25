//! `/receipt` — photo upload → intake → per-line review (T41).
//!
//! One photo is streamed to `data::receipt::upload_receipt`, which runs the
//! zero-trust intake and STAGES a proposal. The review below shows that
//! proposal line by line (low-confidence lines take the `--warn` flag, as on
//! /approvals) and decides it through the T42 fns: APPROVE books it, REJECT
//! drops it. Nothing on this page books on its own. Model text reaches the DOM
//! only as escaped text nodes, already clipped server-side.

use dioxus::fullstack::FileStream;
use dioxus::prelude::*;

use crate::components::prims::ScannerBg;
use crate::components::shell::{AiPanel, TopBar};
use crate::components::states::{Awaiting, InlineStatus};
use crate::data::approvals::{approve_proposal, reject_proposal};
use crate::data::cycle::get_cycle;
use crate::data::receipt::{upload_receipt, IntakeStatus, ReceiptIntakeDto, MAX_UPLOAD_BYTES};
use crate::pages::approvals::{error_text, ReceiptHead};
use crate::Route;

/// Where the review stands after the user's decision.
#[derive(Clone, Copy, PartialEq)]
enum Decided {
    Booked,
    Rejected,
}

/// `/receipt` — upload one receipt photo and review what the model read.
#[component]
pub fn ReceiptPage() -> Element {
    let mut ai_collapsed = use_signal(|| false);
    let cycle = use_resource(get_cycle);
    let mut uploading = use_signal(|| false);
    let mut upload_error = use_signal(|| Option::<String>::None);
    let mut intake = use_signal(|| Option::<ReceiptIntakeDto>::None);
    // `true` = approving, `false` = rejecting, while a decision is in flight.
    let mut deciding = use_signal(|| Option::<bool>::None);
    let mut decide_error = use_signal(|| Option::<String>::None);
    let mut decided = use_signal(|| Option::<Decided>::None);

    let on_pick = move |evt: FormEvent| {
        // One upload at a time; a decision in flight also locks the picker.
        if *uploading.peek() || deciding.peek().is_some() {
            return;
        }
        let Some(file) = evt.files().into_iter().next() else {
            return;
        };
        upload_error.set(None);
        decide_error.set(None);
        if usize::try_from(file.size()).map_or(true, |n| n > MAX_UPLOAD_BYTES) {
            upload_error.set(Some(
                "This photo is larger than 12 MB. Take a smaller photo and try again.".into(),
            ));
            return;
        }
        uploading.set(true);
        intake.set(None);
        decided.set(None);
        spawn(async move {
            match upload_receipt(FileStream::from(file)).await {
                Ok(out) => intake.set(Some(out)),
                Err(e) => upload_error.set(Some(error_text(&e))),
            }
            uploading.set(false);
        });
    };

    let mut decide = move |approve: bool| {
        if deciding.peek().is_some() || decided.peek().is_some() || *uploading.peek() {
            return;
        }
        let Some(id) = intake.peek().as_ref().map(|i| i.suggestion_id.clone()) else {
            return;
        };
        deciding.set(Some(approve));
        decide_error.set(None);
        spawn(async move {
            let result = if approve {
                approve_proposal(id).await.map(|_| Decided::Booked)
            } else {
                reject_proposal(id).await.map(|()| Decided::Rejected)
            };
            match result {
                Ok(d) => decided.set(Some(d)),
                Err(e) => decide_error.set(Some(error_text(&e))),
            }
            deciding.set(None);
        });
    };

    let (cyc_label, cyc_day, cyc_days, cyc_as_of) = match &*cycle.read() {
        Some(Ok(c)) if c.days > 0 => (
            c.label.clone(),
            c.day.to_string(),
            c.days.to_string(),
            c.as_of.clone(),
        ),
        _ => (String::new(), String::new(), String::new(), String::new()),
    };
    let busy = uploading() || deciding().is_some();
    let current = intake();

    rsx! {
        div { class: "pk", style: "height:100vh;min-height:0",
            ScannerBg {
                class: "pk-bg".to_string(),
                seed: 41,
                shapes: r#"[
                    { char: "4", cx: .88, cy: .78, scale: .34, style: "red", morph: "vein", live: true, fill: .46 },
                    { char: "1", cx: .12, cy: .24, scale: .26, style: "faint", morph: "blob", live: false, fill: .42 }
                ]"#.to_string(),
            }
            div { class: "app-shell swap",
                AiPanel { collapsed: ai_collapsed(), on_toggle: move |()| ai_collapsed.toggle() }
                div { class: "app-main",
                    TopBar {
                        active: "RECEIPT".to_string(),
                        label: cyc_label,
                        day: cyc_day,
                        days: cyc_days,
                        as_of: cyc_as_of,
                    }
                    div { class: "app-scroll", "data-screen-label": "RECEIPT",
                        div { class: "subs-wrap",
                            div { class: "subs-top",
                                div {
                                    div { class: "ttl", "Receipt" }
                                    div { class: "sum",
                                        "Upload a photo · the model reads it · nothing is booked until you approve"
                                    }
                                }
                            }
                            div { class: "subs-sec",
                                span { class: "lbl", "⌁ PHOTO" }
                                span { class: "rule" }
                                span { class: "meta", "JPEG OR PNG · MAX 12 MB · EXIF STRIPPED · STORED ENCRYPTED" }
                            }
                            div { class: "cand-list osc-glass hair osc-bkt blue",
                                span { class: "osc-leg", "MOD·INTAKE" }
                                div { class: "cand-row",
                                    input {
                                        r#type: "file",
                                        accept: "image/jpeg,image/png",
                                        disabled: busy,
                                        aria_label: "Receipt photo",
                                        onchange: on_pick,
                                    }
                                }
                                if uploading() || upload_error().is_some() {
                                    InlineStatus {
                                        pending: uploading(),
                                        error: upload_error(),
                                        pending_label: "Reading the receipt…",
                                    }
                                }
                            }
                            div { class: "subs-sec",
                                span { class: "lbl", "⌁ REVIEW" }
                                span { class: "rule" }
                                span { class: "meta", "AI PROPOSES · YOU APPROVE · ONLY THEN THE LEDGER CHANGES" }
                            }
                            match current {
                                None => rsx! {
                                    Awaiting {
                                        label: "RECEIPT REVIEW",
                                        legend: "NO PHOTO",
                                        message: "Upload a receipt photo to review what the model read.",
                                    }
                                },
                                Some(ReceiptIntakeDto { proposal: None, .. }) => rsx! {
                                    Awaiting {
                                        label: "RECEIPT REVIEW",
                                        legend: "ALREADY BOOKED",
                                        message: "This photo was submitted before and is no longer pending.",
                                    }
                                    Link { class: "gbtn", to: Route::TransactionsPage {}, "OPEN TRANSACTIONS" }
                                },
                                Some(ReceiptIntakeDto { status, proposal: Some(p), .. }) => rsx! {
                                    div { class: "cand-list osc-glass hair osc-bkt blue",
                                        span { class: "osc-leg", "MOD·AI · RECEIPT PROPOSAL" }
                                        if status == IntakeStatus::Duplicate {
                                            span { class: "cand-ev",
                                                "This photo was already submitted · showing its pending review"
                                            }
                                        }
                                        div { class: "cand-row",
                                            match p.receipt {
                                                Some(r) => rsx! { ReceiptHead { receipt: r } },
                                                None => rsx! {
                                                    span { class: "cand-ev", "The staged receipt could not be read · reject it" }
                                                },
                                            }
                                        }
                                        match decided() {
                                            Some(Decided::Booked) => rsx! {
                                                div { class: "cand-row",
                                                    span { class: "cand-ev", style: "flex:1 1 auto", "Booked to the ledger." }
                                                    Link { class: "gbtn p", to: Route::TransactionsPage {}, "OPEN TRANSACTIONS" }
                                                }
                                            },
                                            Some(Decided::Rejected) => rsx! {
                                                div { class: "cand-row",
                                                    span { class: "cand-ev", "Rejected · nothing was booked." }
                                                }
                                            },
                                            None => rsx! {
                                                div { class: "cand-row",
                                                    if !p.bookable {
                                                        span { class: "cand-conf low", style: "flex:1 1 auto",
                                                            "INVALID · this proposal can't be booked as read · reject it"
                                                        }
                                                    }
                                                    div { class: "cand-acts",
                                                        button {
                                                            class: "gbtn p",
                                                            disabled: busy || !p.bookable,
                                                            onclick: move |_| decide(true),
                                                            if deciding() == Some(true) { "BOOKING…" } else { "APPROVE" }
                                                        }
                                                        button {
                                                            class: "gbtn",
                                                            disabled: busy,
                                                            onclick: move |_| decide(false),
                                                            if deciding() == Some(false) { "REJECTING…" } else { "REJECT" }
                                                        }
                                                    }
                                                }
                                            },
                                        }
                                        if deciding().is_some() || decide_error().is_some() {
                                            InlineStatus {
                                                pending: deciding().is_some(),
                                                error: decide_error(),
                                                pending_label: if deciding() == Some(true) { "Booking…" } else { "Rejecting…" },
                                            }
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
