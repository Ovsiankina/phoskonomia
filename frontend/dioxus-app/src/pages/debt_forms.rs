//! The Debts page's write UI (T39): the debt and IOU create/edit forms, the
//! inline payment fields, and the confirm-before-delete step.
//!
//! Every panel keeps its state in its own page-level [`Panel`] signal, so a
//! save in one panel (and the refetch that follows it) never closes or resets
//! another panel's open form. The page sends what was typed; parsing, the
//! error texts and which actions a row offers all come from the server
//! (`data::debt_actions`, `DebtDto::actions`, `PersonalIouDto::actions`).

use std::future::Future;

use dioxus::prelude::*;

use crate::components::states::InlineStatus;
use crate::data::budgets::cap_input_text;
use crate::data::debt_actions::{
    action_error_text, create_debt, create_iou, edit_debt, edit_iou, DebtForm, IouForm, DEBT_KINDS,
};
use crate::data::debts::{DebtDto, PersonalIouDto};

/// One write panel: what it is open for, the draft, and the save state.
///
/// `open` is `Some("")` for a new record, `Some(id)` for that record, `None`
/// when closed. While a save is in flight the panel can neither be reopened
/// nor closed, so the draft being saved is never swapped out from under it.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Panel<T> {
    /// Target: `""` = new, otherwise the record id.
    pub open: Option<String>,
    /// The typed values.
    pub draft: T,
    /// A save is in flight.
    pub saving: bool,
    /// The last save's fixed error text.
    pub error: Option<String>,
}

impl<T: Default> Panel<T> {
    /// Open for `target` with `draft`. Ignored while saving.
    pub fn open(&mut self, target: &str, draft: T) {
        if !self.saving {
            *self = Self {
                open: Some(target.to_owned()),
                draft,
                saving: false,
                error: None,
            };
        }
    }

    /// Is the panel open for `target`?
    pub fn is_open_for(&self, target: &str) -> bool {
        self.open.as_deref() == Some(target)
    }

    /// Close and drop the draft. Ignored while saving.
    pub fn close(&mut self) {
        if !self.saving {
            *self = Self::default();
        }
    }
}

/// The draft of an inline payment field.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PayDraft {
    /// The typed CHF amount.
    pub amount: String,
    /// An extra (principal-only) debt payment rather than the instalment.
    pub extra: bool,
}

/// Run `action` for `panel`: pending while in flight; success closes the
/// panel and calls `done` (the page's refetch), failure keeps the draft and
/// shows the server's fixed text. A second submit while saving is dropped.
pub fn submit<T, F>(mut panel: Signal<Panel<T>>, action: F, done: Callback<()>)
where
    T: Default + 'static,
    F: Future<Output = Result<(), ServerFnError>> + 'static,
{
    {
        let mut p = panel.write();
        if p.saving || p.open.is_none() {
            return;
        }
        p.saving = true;
        p.error = None;
    }
    spawn(async move {
        let result = action.await;
        let ok = result.is_ok();
        {
            let mut p = panel.write();
            p.saving = false;
            match result {
                Ok(()) => *p = Panel::default(),
                Err(e) => p.error = Some(action_error_text(&e)),
            }
        }
        if ok {
            done.call(());
        }
    });
}

/// The edit form's starting values for `d`. Balance, plan and APR are
/// create-only (see `DebtForm`), so the edit draft leaves them blank.
pub fn debt_draft(d: &DebtDto) -> DebtForm {
    DebtForm {
        name: d.name.clone(),
        lender: d.lender.clone(),
        kind: d.kind.clone(),
        orig: cap_input_text(d.orig),
        note: d.note.clone(),
        ..DebtForm::default()
    }
}

/// The create form's starting values.
pub fn new_debt_draft() -> DebtForm {
    DebtForm {
        kind: "LOAN".into(),
        apr: "0".into(),
        day: "1".into(),
        term: "0".into(),
        ..DebtForm::default()
    }
}

/// The edit form's starting values for `p`. The amount is the ORIGINAL.
pub fn iou_draft(p: &PersonalIouDto) -> IouForm {
    IouForm {
        dir: p.dir.clone(),
        person: p.person.clone(),
        amount: cap_input_text(p.of),
        reason: p.reason.clone(),
    }
}

/// The create form's starting values.
pub fn new_iou_draft() -> IouForm {
    IouForm {
        dir: "in".into(),
        ..IouForm::default()
    }
}

/// One labelled text field. `num` sets it in Pilowlava with a decimal keypad.
#[component]
fn Field(
    label: String,
    value: String,
    #[props(default = false)] num: bool,
    #[props(default = false)] wide: bool,
    busy: bool,
    on_input: EventHandler<String>,
) -> Element {
    rsx! {
        label { class: if wide { "dx-fld wide" } else { "dx-fld" },
            span { "{label}" }
            input {
                class: if num { "num" } else { "" },
                r#type: "text",
                inputmode: if num { "decimal" } else { "text" },
                autocomplete: "off",
                value: "{value}",
                readonly: busy,
                oninput: move |e: Event<FormData>| on_input.call(e.value()),
            }
        }
    }
}

/// SAVE (the form's submit) / CANCEL plus the pending-or-error line.
#[component]
fn FormActions(busy: bool, error: Option<String>, on_cancel: EventHandler<()>) -> Element {
    rsx! {
        div { class: "dx-row",
            button { class: "gbtn p", r#type: "submit", disabled: busy, "SAVE" }
            button { class: "gbtn", r#type: "button", disabled: busy, onclick: move |_| on_cancel.call(()), "CANCEL" }
            InlineStatus { pending: busy, error }
        }
    }
}

/// The debt create/edit form; renders nothing while closed.
#[component]
pub fn DebtFormPanel(panel: Signal<Panel<DebtForm>>, on_saved: Callback<()>) -> Element {
    let p = panel.read().clone();
    let Some(target) = p.open.clone() else {
        return rsx! {};
    };
    let busy = p.saving;
    let f = p.draft;
    let head = if target.is_empty() {
        "∿ NEW DEBT"
    } else {
        "∿ EDIT DEBT"
    };
    let save = move |()| {
        let (id, form) = {
            let p = panel.read();
            (p.open.clone().unwrap_or_default(), p.draft.clone())
        };
        let action = async move {
            if id.is_empty() {
                create_debt(form).await.map(drop)
            } else {
                edit_debt(id, form).await
            }
        };
        submit(panel, action, on_saved);
    };
    // `set` writes one field of the draft.
    let set =
        move |apply: fn(&mut DebtForm, String)| move |v: String| apply(&mut panel.write().draft, v);

    rsx! {
        form {
            class: "dx-form",
            onsubmit: move |e: Event<FormData>| {
                e.prevent_default();
                save(());
            },
            div { class: "dx-h", "{head}" }
            Field { label: "Name", value: f.name, busy, on_input: set(|d, v| d.name = v) }
            Field { label: "Lender", value: f.lender, busy, on_input: set(|d, v| d.lender = v) }
            label { class: "dx-fld",
                span { "Type" }
                select {
                    disabled: busy,
                    onchange: move |e: Event<FormData>| panel.write().draft.kind = e.value(),
                    for k in DEBT_KINDS {
                        option { value: k, selected: f.kind == k, "{k}" }
                    }
                }
            }
            // The balance is set once; afterwards only payments move it. The
            // plan and the APR are set once too: changing them is T17's
            // adjust-plan / refinance, which has no UI yet.
            if target.is_empty() {
                Field { label: "Balance · CHF", value: f.balance, num: true, busy, on_input: set(|d, v| d.balance = v) }
            }
            Field { label: "Original · CHF", value: f.orig, num: true, busy, on_input: set(|d, v| d.orig = v) }
            if target.is_empty() {
                Field { label: "Monthly · CHF", value: f.monthly, num: true, busy, on_input: set(|d, v| d.monthly = v) }
                Field { label: "APR · %", value: f.apr, num: true, busy, on_input: set(|d, v| d.apr = v) }
                Field { label: "Payment day", value: f.day, num: true, busy, on_input: set(|d, v| d.day = v) }
                Field { label: "Term · months", value: f.term, num: true, busy, on_input: set(|d, v| d.term = v) }
            }
            Field { label: "Note", value: f.note, wide: true, busy, on_input: set(|d, v| d.note = v) }
            FormActions { busy, error: p.error, on_cancel: move |()| panel.write().close() }
        }
    }
}

/// The personal-IOU create/edit form; renders nothing while closed.
#[component]
pub fn IouFormPanel(panel: Signal<Panel<IouForm>>, on_saved: Callback<()>) -> Element {
    let p = panel.read().clone();
    let Some(target) = p.open.clone() else {
        return rsx! {};
    };
    let busy = p.saving;
    let f = p.draft;
    let head = if target.is_empty() {
        "⟷ NEW IOU"
    } else {
        "⟷ EDIT IOU"
    };
    let amount_label = if target.is_empty() {
        "Amount · CHF"
    } else {
        "Original amount · CHF"
    };
    let save = move |()| {
        let (id, form) = {
            let p = panel.read();
            (p.open.clone().unwrap_or_default(), p.draft.clone())
        };
        let action = async move {
            if id.is_empty() {
                create_iou(form).await.map(drop)
            } else {
                edit_iou(id, form).await
            }
        };
        submit(panel, action, on_saved);
    };
    let set =
        move |apply: fn(&mut IouForm, String)| move |v: String| apply(&mut panel.write().draft, v);

    rsx! {
        form {
            class: "dx-form",
            onsubmit: move |e: Event<FormData>| {
                e.prevent_default();
                save(());
            },
            div { class: "dx-h", "{head}" }
            label { class: "dx-fld",
                span { "Direction" }
                select {
                    disabled: busy,
                    onchange: move |e: Event<FormData>| panel.write().draft.dir = e.value(),
                    option { value: "in", selected: f.dir == "in", "← OWED TO YOU" }
                    option { value: "out", selected: f.dir == "out", "YOU OWE →" }
                }
            }
            Field { label: "Person", value: f.person, busy, on_input: set(|d, v| d.person = v) }
            Field { label: amount_label, value: f.amount, num: true, busy, on_input: set(|d, v| d.amount = v) }
            Field { label: "Reason", value: f.reason, wide: true, busy, on_input: set(|d, v| d.reason = v) }
            FormActions { busy, error: p.error, on_cancel: move |()| panel.write().close() }
        }
    }
}

/// The inline payment field for `target`; renders nothing unless open for it.
/// `on_pay` receives `(id, draft)` and returns the server call to run.
#[component]
pub fn PayField(
    panel: Signal<Panel<PayDraft>>,
    target: String,
    label: String,
    on_pay: Callback<(String, PayDraft), ()>,
) -> Element {
    let p = panel.read().clone();
    if !p.is_open_for(&target) {
        return rsx! {};
    }
    let busy = p.saving;
    rsx! {
        form {
            class: "dx-pay",
            onclick: move |e: Event<MouseData>| e.stop_propagation(),
            onsubmit: move |e: Event<FormData>| {
                e.prevent_default();
                on_pay.call((target.clone(), panel.read().draft.clone()));
            },
            Field {
                label,
                value: p.draft.amount,
                num: true,
                busy,
                on_input: move |v: String| panel.write().draft.amount = v,
            }
            button { class: "gbtn p", r#type: "submit", disabled: busy, "RECORD" }
            button { class: "gbtn", r#type: "button", disabled: busy, onclick: move |_| panel.write().close(), "CANCEL" }
            InlineStatus { pending: busy, error: p.error, pending_label: "Recording…".to_string() }
        }
    }
}

/// DELETE, then CONFIRM DELETE / KEEP. `panel.open == target` means armed.
#[component]
pub fn DeleteConfirm(
    panel: Signal<Panel<bool>>,
    target: String,
    on_confirm: Callback<String>,
) -> Element {
    let p = panel.read().clone();
    let armed = p.is_open_for(&target) && p.draft;
    let busy = p.is_open_for(&target) && p.saving;
    let error = if p.is_open_for(&target) {
        p.error
    } else {
        None
    };
    let t_arm = target.clone();
    rsx! {
        if armed {
            button { class: "gbtn", r#type: "button", disabled: busy, onclick: move |_| on_confirm.call(target.clone()), "CONFIRM DELETE" }
            button { class: "gbtn", r#type: "button", disabled: busy, onclick: move |_| panel.write().close(), "KEEP" }
        } else {
            button { class: "gbtn", r#type: "button", disabled: busy, onclick: move |_| panel.write().open(&t_arm, true), "DELETE" }
        }
        InlineStatus { pending: busy, error }
    }
}
