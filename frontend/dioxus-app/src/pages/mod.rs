//! The 7 pages — one self-contained file each (mirrors React `src/pages/*.jsx`).
//!
//! Each page agent REPLACES the body of its own `pages/<page>.rs` ONLY, and must
//! NOT touch shared files (`main.rs`, `components/*`, `data/*`, this `mod.rs`).
//!
//! Each module exposes `pub fn <Page>Page() -> Element`. The `Route` enum in
//! `main.rs` points at these directly. A page file should:
//!   * `use dioxus::prelude::*;` and `use crate::components::{...};` (TopBar, Nav,
//!     ScannerBg, ...) and `use crate::data::{chf, ...}` / its `#[server]` fns;
//!   * render its own `TopBar` + left `Nav` + background `ScannerBg` + panels
//!     (faithful to the export's one-HTML-per-page shape — the page owns its
//!     chrome, while the CRT overlays live once in `Layout`);
//!   * fetch data via `use_resource`/`use_server_future` over `crate::data` fns;
//!   * obey Oscillocore design law (tokens via `var(--…)`, numbers in Pilowlava,
//!     `.osc-readout` module tag, etc.).

pub mod analytics;
pub mod budgets;
pub mod config;
pub mod dashboard;
pub mod debts;
pub mod subscriptions;
pub mod transactions;
