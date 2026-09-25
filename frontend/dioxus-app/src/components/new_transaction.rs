//! The "NEW transaction" form (T36), opened from the Transactions page header.
//!
//! A manual entry is either a total (TOTAL) or a list of line items (ITEMS),
//! mirroring `phosk_ledger::transactions::create_transaction`. Everything is
//! sent as typed; the server parses the CHF text to exact centimes and answers
//! a problem with a message naming the field
//! ([`create_transaction`](crate::data::new_transaction::create_transaction)).
//! The category choices are the real category list (`get_categories`).
//!
//! States: the category list loads through [`Awaiting`]; while a save is in
//! flight every control is disabled and SAVE ignores clicks (no double
//! submit); a refusal shows as a NOT SAVED block. Indigo only: the page header
//! keeps the view's coral moment. Amount and quantity fields render in
//! Pilowlava (`.num`).

use dioxus::prelude::*;

use crate::components::states::Awaiting;
use crate::data::budgets::get_categories;
use crate::data::new_transaction::{
    create_error_text, create_transaction, NewTxnForm, NewTxnLineForm,
};

/// The form panel. `on_saved` fires after a successful save (the page closes
/// the form and refetches the list); `on_close` on CANCEL.
#[component]
pub fn NewTransactionForm(on_close: EventHandler<()>, on_saved: EventHandler<()>) -> Element {
    let cats = use_resource(get_categories);
    let mut form = use_signal(NewTxnForm::default);
    let mut itemised = use_signal(|| false);
    let mut pending = use_signal(|| false);
    let mut error = use_signal(|| Option::<String>::None);
    // One stable key per line row, parallel to `form.lines`, so REMOVE never
    // hands a row's DOM state to its neighbour.
    let mut keys = use_signal(Vec::<u64>::new);
    let mut next_key = use_signal(|| 0_u64);

    let names: Option<Vec<String>> = match &*cats.read() {
        Some(Ok(list)) => Some(list.iter().map(|c| c.name.clone()).collect()),
        _ => None,
    };
    let cats_failed = matches!(&*cats.read(), Some(Err(_)));

    let Some(names) = names else {
        return rsx! {
            div { class: "lfix osc-bkt blue ntx",
                span { class: "osc-leg", "NEW·TX" }
                Awaiting {
                    label: "NEW·TX · CATEGORIES".to_string(),
                    loading: !cats_failed,
                    message: cats_failed.then(|| "The category list could not be loaded.".to_string()),
                }
                div { class: "lfix-acts",
                    button { class: "gbtn", onclick: move |_| on_close.call(()), "CANCEL" }
                }
            }
        };
    };

    let save = move |_| {
        if pending() {
            return;
        }
        let mut entry = form.read().clone();
        if itemised() {
            entry.total.clear();
        } else {
            entry.lines.clear();
        }
        pending.set(true);
        error.set(None);
        spawn(async move {
            let res = create_transaction(entry).await;
            pending.set(false);
            match res {
                Ok(_) => on_saved.call(()),
                Err(err) => error.set(Some(create_error_text(&err))),
            }
        });
    };

    let busy = pending();
    let items = itemised();
    let f = form.read().clone();
    let row_keys = keys.read().clone();
    let mode_cls = |on: bool| if on { "gbtn p" } else { "gbtn" };

    rsx! {
        div { class: "lfix osc-bkt blue ntx",
            span { class: "osc-leg", "NEW·TX · MANUAL ENTRY" }
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
                    span { class: "k", "Date" }
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
                        option { value: "", "—" }
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
            }
            div { class: "ntx-mode",
                button { class: mode_cls(!items), disabled: busy, onclick: move |_| itemised.set(false), "TOTAL" }
                button { class: mode_cls(items), disabled: busy, onclick: move |_| itemised.set(true), "ITEMS" }
            }
            p { class: "ntx-hint",
                "Amounts in CHF with a dot, e.g. 4.20 (4,20 is refused). Quantity takes 0.5 or 0,5."
            }
            if items {
                for (i , (l , k)) in f.lines.iter().cloned().zip(row_keys.iter().copied()).enumerate() {
                    div { key: "{k}", class: "lfix-grid ntx-line",
                        label { class: "lfix-f",
                            span { class: "k", "Item {i + 1}" }
                            input {
                                value: "{l.name}",
                                disabled: busy,
                                oninput: move |e| if let Some(l) = form.write().lines.get_mut(i) { l.name = e.value(); },
                            }
                        }
                        label { class: "lfix-f",
                            span { class: "k", "Qty" }
                            input {
                                class: "num",
                                inputmode: "decimal",
                                value: "{l.qty}",
                                disabled: busy,
                                oninput: move |e| if let Some(l) = form.write().lines.get_mut(i) { l.qty = e.value(); },
                            }
                        }
                        label { class: "lfix-f",
                            span { class: "k", "Unit · CHF" }
                            input {
                                class: "num",
                                inputmode: "decimal",
                                value: "{l.unit_price}",
                                disabled: busy,
                                oninput: move |e| if let Some(l) = form.write().lines.get_mut(i) { l.unit_price = e.value(); },
                            }
                        }
                        label { class: "lfix-f",
                            span { class: "k", "Line category" }
                            select {
                                disabled: busy,
                                onchange: move |e| if let Some(l) = form.write().lines.get_mut(i) { l.category = e.value(); },
                                option { value: "", selected: l.category.is_empty(), "Same as entry" }
                                for n in names.iter() {
                                    option { key: "{n}", value: "{n}", selected: *n == l.category, "{n}" }
                                }
                            }
                        }
                        div { class: "lfix-f ntx-rm",
                            button {
                                class: "gbtn",
                                disabled: busy,
                                onclick: move |_| {
                                    let mut f = form.write();
                                    if i < f.lines.len() {
                                        f.lines.remove(i);
                                        keys.write().remove(i);
                                    }
                                },
                                "REMOVE"
                            }
                        }
                    }
                }
                div { class: "ntx-mode",
                    button {
                        class: "gbtn",
                        disabled: busy,
                        onclick: move |_| {
                            form.write().lines.push(NewTxnLineForm { qty: "1".to_string(), ..NewTxnLineForm::default() });
                            let key = next_key();
                            next_key.set(key + 1);
                            keys.write().push(key);
                        },
                        "+ LINE"
                    }
                }
            } else {
                div { class: "lfix-grid ntx-line",
                    label { class: "lfix-f",
                        span { class: "k", "Total · CHF" }
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
            if busy {
                div { class: "line-state",
                    Awaiting {
                        label: "NEW·TX · ENTRY".to_string(),
                        loading: true,
                        legend: Some("SAVING".to_string()),
                        message: Some("Saving the transaction…".to_string()),
                    }
                }
            } else if let Some(msg) = error() {
                div { class: "line-state",
                    Awaiting {
                        label: "NEW·TX · ENTRY".to_string(),
                        legend: Some("NOT SAVED".to_string()),
                        message: Some(msg),
                    }
                }
            }
            div { class: "lfix-acts",
                button { class: "gbtn p", disabled: busy, onclick: save, "SAVE" }
                button { class: "gbtn", disabled: busy, onclick: move |_| on_close.call(()), "CANCEL" }
            }
        }
    }
}
