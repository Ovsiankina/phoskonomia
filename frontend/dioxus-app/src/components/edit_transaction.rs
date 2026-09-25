//! EDIT and DELETE of a recorded transaction (T37), shown in the receipt
//! detail (`ReceiptScreen`).
//!
//! EDIT opens a form pre-filled from the detail: shop, category, fixed flag, a
//! new date (blank keeps it) and — only for a total-only receipt — a new
//! total. An itemised receipt's total comes from its lines, which are
//! corrected one by one below. DELETE asks first, naming the shop, date,
//! amount and line count it removes.
//!
//! States: the category list loads through [`Awaiting`]; while a request is in
//! flight every control is disabled and the submit button ignores clicks (no
//! double submit); a refusal shows as a NOT SAVED / NOT DELETED block with the
//! server's fixed message. Indigo only: the page header keeps the view's coral
//! moment. Amount and date fields render in Pilowlava (`.num`).

use dioxus::prelude::*;

use crate::components::states::Awaiting;
use crate::data::budgets::get_categories;
use crate::data::chf2;
use crate::data::edit_transaction::{
    delete_transaction, edit_transaction, txn_action_error_text, EditTxnForm,
};
use crate::data::transactions::TransactionDto;

#[derive(Clone, Copy, PartialEq)]
enum Mode {
    Idle,
    Edit,
    ConfirmDelete,
}

/// The EDIT / DELETE bar and its panels. `itemised` hides the total field.
/// `line_editing` disables EDIT/DELETE while a line correction is open below
/// (T11's `edit_transaction` re-reads the line list before writing it back, so
/// a concurrent line save here would silently revert it). `on_open_change`
/// reports whenever a panel opens or closes, so the page can disable line
/// correction while this is open (the same race, the other direction) and
/// `on_busy_change` while a request is in flight, so the page can keep the
/// whole detail open until it settles (closing it early would drop the
/// pending EDIT/DELETE with no refresh to show for it). `on_edited` gets the
/// detail row patched with the saved values (the page also refetches the
/// list); `on_deleted` fires after a delete (the page closes the detail and
/// refetches).
#[component]
pub fn TxnActions(
    t: TransactionDto,
    itemised: bool,
    #[props(default = false)] line_editing: bool,
    #[props(default)] on_open_change: EventHandler<bool>,
    #[props(default)] on_busy_change: EventHandler<bool>,
    on_edited: EventHandler<TransactionDto>,
    on_deleted: EventHandler<()>,
) -> Element {
    let mut mode = use_signal(|| Mode::Idle);
    let mut form = use_signal(EditTxnForm::default);
    let mut pending = use_signal(|| false);
    let mut error = use_signal(|| Option::<String>::None);
    let mut cats_ready = use_signal(|| false);

    let busy = pending();
    let mut set_pending = move |value: bool| {
        pending.set(value);
        on_busy_change.call(value);
    };
    let t_open = t.clone();
    let open_edit = move |_| {
        form.set(EditTxnForm {
            id: t_open.id.clone(),
            shop: t_open.shop.clone(),
            category: t_open.category.clone(),
            fixed: t_open.fixed,
            ..EditTxnForm::default()
        });
        error.set(None);
        cats_ready.set(false);
        mode.set(Mode::Edit);
        on_open_change.call(true);
    };
    let open_delete = move |_| {
        error.set(None);
        mode.set(Mode::ConfirmDelete);
        on_open_change.call(true);
    };
    let close = move |_| {
        if pending() {
            return;
        }
        error.set(None);
        mode.set(Mode::Idle);
        on_open_change.call(false);
    };

    let t_save = t.clone();
    let save = move |_| {
        if pending() || !cats_ready() {
            return;
        }
        let mut entry = form.read().clone();
        if itemised {
            entry.total.clear();
        }
        let base = t_save.clone();
        set_pending(true);
        error.set(None);
        spawn(async move {
            let res = edit_transaction(entry.clone()).await;
            set_pending(false);
            match res {
                Ok(saved) => {
                    mode.set(Mode::Idle);
                    on_open_change.call(false);
                    on_edited.call(TransactionDto {
                        shop: entry.shop.trim().to_owned(),
                        category: entry.category.trim().to_owned(),
                        fixed: entry.fixed,
                        amount: saved.amount,
                        date: saved.date,
                        ..base
                    });
                }
                Err(err) => error.set(Some(txn_action_error_text(
                    &err,
                    "Could not reach the server, changes not saved.",
                ))),
            }
        });
    };

    let del_id = t.id.clone();
    let confirm_delete = move |_| {
        if pending() {
            return;
        }
        let id = del_id.clone();
        set_pending(true);
        error.set(None);
        spawn(async move {
            let res = delete_transaction(id).await;
            set_pending(false);
            match res {
                Ok(()) => on_deleted.call(()),
                // A 404 means the record is already gone (deleted elsewhere,
                // or this is a retry after a dropped response): treat it the
                // same as a successful delete rather than leaving the detail
                // open on a record that no longer exists.
                Err(ServerFnError::ServerError { code: 404, .. }) => on_deleted.call(()),
                Err(err) => error.set(Some(txn_action_error_text(
                    &err,
                    "Could not reach the server, nothing was deleted.",
                ))),
            }
        });
    };

    let lines_word = if t.item_count == 1 {
        "line item"
    } else {
        "line items"
    };
    let amount = chf2(t.amount);

    rsx! {
        match mode() {
            Mode::Idle => rsx! {
                div { class: "lfix-acts txa",
                    button { class: "gbtn", disabled: line_editing, onclick: open_edit, "EDIT" }
                    button { class: "gbtn", disabled: line_editing, onclick: open_delete, "DELETE" }
                }
            },
            Mode::Edit => rsx! {
                EditPanel { form, itemised, busy, ready: cats_ready }
                ActionState { busy, error: error(), what: "EDIT", failed: "NOT SAVED", doing: "Saving the changes…" }
                div { class: "lfix-acts",
                    button { class: "gbtn p", disabled: busy || !cats_ready(), onclick: save, "SAVE" }
                    button { class: "gbtn", disabled: busy, onclick: close, "CANCEL" }
                }
            },
            Mode::ConfirmDelete => rsx! {
                div { class: "lfix osc-bkt blue txa-del",
                    span { class: "osc-leg", "TX · DELETE" }
                    p { class: "ntx-hint",
                        "Delete {t.shop} · {t.date} · CHF "
                        span { class: "num", "{amount}" }
                        if t.item_count > 0 {
                            " and its "
                            span { class: "num", "{t.item_count}" }
                            " {lines_word}"
                        }
                        "? This cannot be undone."
                    }
                    ActionState { busy, error: error(), what: "DELETE", failed: "NOT DELETED", doing: "Deleting the transaction…" }
                    div { class: "lfix-acts",
                        button { class: "gbtn p", disabled: busy, onclick: confirm_delete, "CONFIRM DELETE" }
                        button { class: "gbtn", disabled: busy, onclick: close, "KEEP" }
                    }
                }
            },
        }
    }
}

/// The EDIT fields. The category choices are the real category list.
/// `ready` tells the parent's SAVE button once the list has loaded, so a save
/// can't fire against a form the category `<select>` was never rendered for.
#[component]
fn EditPanel(
    form: Signal<EditTxnForm>,
    itemised: bool,
    busy: bool,
    mut ready: Signal<bool>,
) -> Element {
    let cats = use_resource(get_categories);
    let mut form = form;
    let f = form.read().clone();
    let names: Option<Vec<String>> = match &*cats.read() {
        Some(Ok(list)) => Some(list.iter().map(|c| c.name.clone()).collect()),
        _ => None,
    };
    let cats_failed = matches!(&*cats.read(), Some(Err(_)));
    use_effect(move || {
        ready.set(matches!(&*cats.read(), Some(Ok(_))));
    });
    let Some(mut names) = names else {
        return rsx! {
            Awaiting {
                label: "TX·EDIT · CATEGORIES".to_string(),
                loading: !cats_failed,
                message: cats_failed.then(|| "The category list could not be loaded.".to_string()),
            }
        };
    };
    // The stored category stays selectable even if it is not in the list.
    if !f.category.is_empty() && !names.contains(&f.category) {
        names.insert(0, f.category.clone());
    }

    rsx! {
        div { class: "lfix osc-bkt blue txa-edit",
            span { class: "osc-leg", "TX·EDIT" }
            div { class: "lfix-grid",
                label { class: "lfix-f",
                    span { class: "k", "Shop" }
                    input {
                        value: "{f.shop}",
                        disabled: busy,
                        oninput: move |e| form.write().shop = e.value(),
                    }
                }
                label { class: "lfix-f",
                    span { class: "k", "New date · blank keeps it" }
                    input {
                        r#type: "date",
                        class: "num",
                        value: "{f.date}",
                        disabled: busy,
                        oninput: move |e| form.write().date = e.value(),
                    }
                }
                label { class: "lfix-f",
                    span { class: "k", "Category" }
                    select {
                        value: "{f.category}",
                        disabled: busy,
                        onchange: move |e| form.write().category = e.value(),
                        for n in names.iter() {
                            option { key: "{n}", value: "{n}", selected: *n == f.category, "{n}" }
                        }
                    }
                }
                label { class: "lfix-f ntx-check",
                    span { class: "k", "Fixed charge" }
                    input {
                        r#type: "checkbox",
                        checked: f.fixed,
                        disabled: busy,
                        onchange: move |e| form.write().fixed = e.checked(),
                    }
                }
                if !itemised {
                    label { class: "lfix-f",
                        span { class: "k", "New total · CHF · blank keeps it" }
                        input {
                            class: "num",
                            inputmode: "decimal",
                            value: "{f.total}",
                            disabled: busy,
                            oninput: move |e| form.write().total = e.value(),
                        }
                    }
                }
            }
            if itemised {
                p { class: "ntx-hint", "The total comes from the line items; correct a line below to change it." }
            } else {
                p { class: "ntx-hint", "Amounts in CHF with a dot, e.g. 4.20." }
            }
        }
    }
}

/// The in-flight / refused block of an action.
#[component]
fn ActionState(
    busy: bool,
    error: Option<String>,
    what: &'static str,
    failed: &'static str,
    doing: &'static str,
) -> Element {
    let label = format!("TX · {what}");
    if busy {
        rsx! {
            div { class: "line-state",
                Awaiting { label, loading: true, legend: Some("WORKING".to_string()), message: Some(doing.to_string()) }
            }
        }
    } else if let Some(msg) = error {
        rsx! {
            div { class: "line-state",
                Awaiting { label, legend: Some(failed.to_string()), message: Some(msg) }
            }
        }
    } else {
        rsx! {}
    }
}
