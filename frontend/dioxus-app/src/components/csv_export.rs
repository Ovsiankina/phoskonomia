//! CSV export button (T35), dropped into the Transactions, Budgets and
//! Subscriptions page headers.
//!
//! Every click asks the server for a fresh file
//! ([`export_csv`](crate::data::csv_export::export_csv)), then hands it over:
//!
//! * **web** (and the server-rendered pass): a second, indigo `SAVE · N ROWS`
//!   link appears, an `a[download]` pointing at the percent-encoded `data:` URI
//!   ([`csv_data_uri`](crate::data::csv_export::csv_data_uri)). The browser
//!   saves it; nothing touches the server's disk.
//! * **desktop**: a native "save as" dialog (`rfd`), then the client writes the
//!   chosen file. A `data:` link cannot be used there: the Dioxus desktop
//!   interpreter cancels every `<a>` click and passes the `href` to
//!   `webbrowser::open`, which would hand the whole CSV to the OS URL opener
//!   as a command-line argument instead of downloading it.
//!
//! Indigo only (the page header already owns the view's coral moment); the row
//! count renders in Pilowlava; pending / error text comes from
//! [`awaiting_message`].

use dioxus::prelude::*;

use crate::components::states::awaiting_message;
use crate::data::csv_export::{export_csv, CsvExportKind};

/// Inline style of the status text next to the button.
const STATUS_STYLE: &str =
    "font-size:var(--t-xs);letter-spacing:var(--tracking-tag);text-transform:uppercase";
/// Numerals (the row count) are Pilowlava.
const NUM_STYLE: &str = "font-family:var(--font-display);color:var(--text-blue)";

/// Where one export stands.
#[derive(Clone, PartialEq)]
enum Phase {
    /// Nothing requested yet (or the save dialog was cancelled).
    Idle,
    /// Waiting for the server (or the save dialog).
    Pending,
    /// Web: the file is ready behind a download link.
    #[cfg(not(feature = "desktop"))]
    Ready {
        href: String,
        filename: String,
        rows: u32,
    },
    /// Desktop: the file was written where the user chose.
    #[cfg(feature = "desktop")]
    Saved { rows: u32 },
    /// The export failed; the text is safe to show.
    Failed(String),
}

/// What the list is called in the button tooltip.
const fn list_name(kind: CsvExportKind) -> &'static str {
    match kind {
        CsvExportKind::Transactions => "transactions",
        CsvExportKind::Budget => "budget envelopes",
        CsvExportKind::Subscriptions => "subscriptions",
    }
}

/// The server's own message is already scrubbed (see `data::csv_export`);
/// transport errors get a fixed line.
fn failure_text(e: &ServerFnError) -> String {
    match e {
        ServerFnError::ServerError { message, .. } => message.clone(),
        _ => "CSV export unavailable".to_string(),
    }
}

/// Export button for one list. Renders fresh data on every click.
#[component]
pub fn CsvExport(kind: CsvExportKind) -> Element {
    let mut phase = use_signal(|| Phase::Idle);
    let pending = matches!(*phase.read(), Phase::Pending);
    let title = format!("Export this cycle's {} as CSV", list_name(kind));

    let start = move |_| {
        phase.set(Phase::Pending);
        spawn(async move {
            let next = match export_csv(kind).await {
                #[cfg(not(feature = "desktop"))]
                Ok(file) => link_to(file),
                #[cfg(feature = "desktop")]
                Ok(file) => save_native(file).await,
                Err(e) => Phase::Failed(failure_text(&e)),
            };
            phase.set(next);
        });
    };

    let status = match phase() {
        Phase::Idle => rsx! {},
        Phase::Pending => {
            let text = awaiting_message(true, None);
            rsx! {
                span { class: "dim", style: STATUS_STYLE, "{text}" }
            }
        }
        #[cfg(not(feature = "desktop"))]
        Phase::Ready {
            href,
            filename,
            rows,
        } => rsx! {
            a {
                class: "gbtn p",
                style: "text-decoration:none",
                href: "{href}",
                download: "{filename}",
                title: "{filename}",
                "SAVE · "
                span { style: NUM_STYLE, "{rows}" }
                " ROWS"
            }
        },
        #[cfg(feature = "desktop")]
        Phase::Saved { rows } => rsx! {
            span { class: "dim", style: STATUS_STYLE,
                "SAVED · "
                span { style: NUM_STYLE, "{rows}" }
                " ROWS"
            }
        },
        Phase::Failed(msg) => {
            let text = awaiting_message(false, Some(&msg));
            rsx! {
                span { class: "dim", style: STATUS_STYLE, "{text}" }
            }
        }
    };

    rsx! {
        div { class: "csv-export", style: "display:flex;align-items:center;gap:var(--s-2)",
            button {
                class: "gbtn",
                r#type: "button",
                title: "{title}",
                disabled: pending,
                onclick: start,
                "⇩ EXPORT CSV"
            }
            {status}
        }
    }
}

/// Web: keep the file behind a download link (the `data:` URI is built once).
#[cfg(not(feature = "desktop"))]
fn link_to(file: crate::data::csv_export::CsvFileDto) -> Phase {
    Phase::Ready {
        href: crate::data::csv_export::csv_data_uri(&file.csv),
        filename: file.filename,
        rows: file.row_count,
    }
}

/// Desktop: ask where to save, then write the file. Cancelling is not an error.
#[cfg(feature = "desktop")]
async fn save_native(file: crate::data::csv_export::CsvFileDto) -> Phase {
    let picked = rfd::AsyncFileDialog::new()
        .set_title("Export CSV")
        .set_file_name(file.filename.as_str())
        .add_filter("CSV", &["csv"])
        .save_file()
        .await;
    match picked {
        None => Phase::Idle,
        Some(target) => match target.write(file.csv.as_bytes()).await {
            Ok(()) => Phase::Saved {
                rows: file.row_count,
            },
            Err(_) => Phase::Failed("Could not write the file".to_string()),
        },
    }
}
