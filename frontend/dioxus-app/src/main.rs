//! Phoskonomia — Dioxus 0.7 fullstack entry point.
//!
//! One Rust codebase -> web (default) + desktop + mobile. The UI talks to the
//! backend through `#[server]` functions (see `mod data`); there is NO hand-written
//! REST boundary. This file is the FOUNDATION SHELL every page plugs into:
//!
//!   * the `Route` enum (9 routes + index/unknown -> /dashboard),
//!   * the `Layout` that renders the 3 permanent CRT overlays once + `Outlet`,
//!   * all Oscillocore CSS linked in the EXACT cascade order from React `main.jsx`,
//!   * `@font-face` injected with `asset!()`'d font URLs so Pilowlava + VG5000
//!     actually resolve (Dioxus hashes asset filenames; relative `url()` in a
//!     linked stylesheet would 404 — we side-step that, see `FontFaces`),
//!   * the `ScannerBg` JS-interop bridge (drives `window.OscScanner`).
//!
//! Module tree (each filled by a DIFFERENT agent, disjoint files):
//!   * `components` — F2: shared RSX vocabulary (TopBar + Nav live here).
//!   * `data`       — F3: `#[server]` fns + shared view structs.
//!   * `pages`      — one self-contained file per page; the Router points here.

use dioxus::prelude::*;

mod components;
mod data;
mod pages;

use pages::{
    analytics::AnalyticsPage, approvals::ApprovalsPage, budgets::BudgetsPage, config::ConfigPage,
    dashboard::DashboardPage, debts::DebtsPage, receipt::ReceiptPage,
    subscriptions::SubscriptionsPage, transactions::TransactionsPage,
};

// ---------------------------------------------------------------------------
// Assets. `asset!()` content-hashes filenames at build time and returns the
// hashed, served URL. CSS is linked in the SAME order as React `main.jsx`
// (cascade-critical): fonts first, then tokens, CRT skin, app, then per-page.
// ---------------------------------------------------------------------------
const FAVICON: Asset = asset!("/assets/favicon.ico");

const CSS_COLORS_TYPE: Asset = asset!("/assets/styles/colors_and_type.css");
const CSS_OSC_CRT: Asset = asset!("/assets/styles/osc-crt.css");
const CSS_PHOSK: Asset = asset!("/assets/styles/phosk.css");
const CSS_SHELL: Asset = asset!("/assets/styles/shell.css");
const CSS_TXN: Asset = asset!("/assets/styles/txn.css");
const CSS_BUDGET: Asset = asset!("/assets/styles/budget.css");
const CSS_SUBS: Asset = asset!("/assets/styles/subs.css");
const CSS_DEBT: Asset = asset!("/assets/styles/debt.css");
const CSS_ANALYTICS: Asset = asset!("/assets/styles/analytics.css");
const CSS_CONFIG: Asset = asset!("/assets/styles/config.css");

// Font binaries. We do NOT link `fonts/fonts.css` directly: its `@font-face`
// `url("Pilowlava-Regular.woff2")` refs are relative and would resolve against
// the hashed CSS URL, 404-ing. Instead we hash each font file here and inject a
// hand-built `@font-face` block pointing at these resolved URLs (see FontFaces).
const FONT_PILOWLAVA_WOFF2: Asset = asset!("/assets/styles/fonts/Pilowlava-Regular.woff2");
const FONT_PILOWLAVA_WOFF: Asset = asset!("/assets/styles/fonts/Pilowlava-Regular.woff");
const FONT_PILOWLAVA_OTF: Asset = asset!("/assets/styles/fonts/Pilowlava-Regular.otf");
const FONT_PILOWLAVA_ATOME_WOFF2: Asset = asset!("/assets/styles/fonts/Pilowlava-Atome.woff2");
const FONT_PILOWLAVA_ATOME_WOFF: Asset = asset!("/assets/styles/fonts/Pilowlava-Atome.woff");
const FONT_VG5000_WOFF2: Asset = asset!("/assets/styles/fonts/VG5000-Regular.woff2");
const FONT_VG5000_WOFF: Asset = asset!("/assets/styles/fonts/VG5000-Regular.woff");
const FONT_VG5000_OTF: Asset = asset!("/assets/styles/fonts/VG5000-Regular.otf");

fn main() {
    dioxus::launch(App);
}

/// Root component. Mounts the router; the router renders [`Layout`] (chrome +
/// overlays) and swaps the active page into the `Outlet`.
#[component]
fn App() -> Element {
    rsx! {
        Router::<Route> {}
    }
}

// ---------------------------------------------------------------------------
// Routes. Index "/" and any unknown path redirect to /dashboard. Every page is
// nested under `Layout` so it inherits the CRT overlays + (later) TopBar/Nav.
// ---------------------------------------------------------------------------
#[derive(Routable, Clone, PartialEq)]
#[rustfmt::skip]
pub enum Route {
    #[layout(Layout)]
        #[route("/dashboard")]
        DashboardPage {},
        #[route("/transactions")]
        TransactionsPage {},
        #[route("/budgets")]
        BudgetsPage {},
        #[route("/subscriptions")]
        SubscriptionsPage {},
        #[route("/debts")]
        DebtsPage {},
        #[route("/analytics")]
        AnalyticsPage {},
        #[route("/approvals")]
        ApprovalsPage {},
        #[route("/receipt")]
        ReceiptPage {},
        #[route("/config")]
        ConfigPage {},
        // Index + any unknown path -> /dashboard.
        #[route("/")]
        Index {},
        #[route("/:..segments")]
        NotFound { segments: Vec<String> },
}

/// Index route: redirect "/" to the dashboard.
#[component]
fn Index() -> Element {
    let nav = use_navigator();
    use_effect(move || {
        nav.replace(Route::DashboardPage {});
    });
    rsx! {}
}

/// Catch-all: any unknown path redirects to the dashboard.
#[component]
fn NotFound(segments: Vec<String>) -> Element {
    let nav = use_navigator();
    use_effect(move || {
        nav.replace(Route::DashboardPage {});
    });
    rsx! {}
}

// ---------------------------------------------------------------------------
// Layout — the persistent shell. Mirrors React `App.jsx`: inject the base reset
// + @font-face, link all CSS once, render the 3 CRT overlay divs ONCE, then the
// active page via `Outlet`. Pages must NOT re-link CSS or re-render overlays.
// ---------------------------------------------------------------------------
#[component]
fn Layout() -> Element {
    rsx! {
        // Favicon + global stylesheets, injected into <head>, in cascade order.
        document::Link { rel: "icon", href: FAVICON }
        FontFaces {}
        BaseReset {}
        document::Stylesheet { href: CSS_COLORS_TYPE }
        document::Stylesheet { href: CSS_OSC_CRT }
        document::Stylesheet { href: CSS_PHOSK }
        document::Stylesheet { href: CSS_SHELL }
        document::Stylesheet { href: CSS_TXN }
        document::Stylesheet { href: CSS_BUDGET }
        document::Stylesheet { href: CSS_SUBS }
        document::Stylesheet { href: CSS_DEBT }
        document::Stylesheet { href: CSS_ANALYTICS }
        document::Stylesheet { href: CSS_CONFIG }

        // Permanent CRT skin — rendered ONCE here, never per page (React App.jsx).
        div { class: "osc-scan" }
        div { class: "osc-roll" }
        div { class: "osc-vig" }

        // Active page.
        Outlet::<Route> {}
    }
}

/// Injects the `@font-face` rules with content-hashed font URLs.
///
/// This is the load-bearing fix for "fonts must actually load": we cannot link
/// the verbatim `fonts/fonts.css` because its `url(...)` references are relative
/// and Dioxus does not rewrite them, so they 404 against the hashed CSS path.
/// We rebuild the exact same `@font-face` declarations here with `asset!()`'d
/// URLs. Roles match the design system: Pilowlava = display/numerals/headers,
/// VG5000 = body/UI/data.
#[component]
fn FontFaces() -> Element {
    let css = format!(
        r#"
@font-face {{
  font-family: "Pilowlava";
  src: url("{pil_woff2}") format("woff2"),
       url("{pil_woff}") format("woff"),
       url("{pil_otf}") format("opentype");
  font-weight: 400; font-style: normal; font-display: swap;
}}
@font-face {{
  font-family: "Pilowlava Atome";
  src: url("{atome_woff2}") format("woff2"),
       url("{atome_woff}") format("woff");
  font-weight: 400; font-style: normal; font-display: swap;
}}
@font-face {{
  font-family: "VG5000";
  src: url("{vg_woff2}") format("woff2"),
       url("{vg_woff}") format("woff"),
       url("{vg_otf}") format("opentype");
  font-weight: 400; font-style: normal; font-display: swap;
}}
"#,
        pil_woff2 = FONT_PILOWLAVA_WOFF2,
        pil_woff = FONT_PILOWLAVA_WOFF,
        pil_otf = FONT_PILOWLAVA_OTF,
        atome_woff2 = FONT_PILOWLAVA_ATOME_WOFF2,
        atome_woff = FONT_PILOWLAVA_ATOME_WOFF,
        vg_woff2 = FONT_VG5000_WOFF2,
        vg_woff = FONT_VG5000_WOFF,
        vg_otf = FONT_VG5000_OTF,
    );
    rsx! {
        document::Style { {css} }
    }
}

/// Base page reset, matching React `index.html`'s inline `<body>` style, plus the
/// `osc` body class hook (the design system applies the VG5000 body font + void
/// background via `body.osc` / `.osc-body`, not bare `body`).
#[component]
fn BaseReset() -> Element {
    rsx! {
        document::Style {
            {r#"
html, body, #main { height: 100%; }
body { margin: 0; background: #06040c; overflow: hidden; }
"#}
        }
        // Ensure the design-system body class is present so var(--font-body) etc. apply.
        document::Style { {"body { font-family: var(--font-body); color: var(--ink); background: var(--bg); }"} }
    }
}
