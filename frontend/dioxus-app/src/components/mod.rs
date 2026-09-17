//! Shared component vocabulary (OWNED BY AGENT F2).
//!
//! Mirrors React `src/components/{prims,comps,shell,states}.jsx`. F2 fills the
//! submodules below; pages import these (never raw DOM-vocabulary duplication).
//!
//!   * `prims`  — primitives: ScannerBg, Dot, HudCell, Spark, CatBar, PhoskChart,
//!     SavingsDial (React `prims.jsx`).
//!   * `comps`  — composite UI pieces (React `comps.jsx`).
//!   * `shell`  — TopBar + left Nav sidebar — EVERY page must render these
//!     (React `shell.jsx`).
//!   * `states` — empty / loading states (React `states.jsx`).
//!
//! `ScannerBg` is provided here pre-built by F1 (it is the JS-interop bridge to
//! `window.OscScanner` and is wired into the asset/scanner foundation). F2 may
//! relocate it into `prims` if preferred, keeping the public name `ScannerBg`.

pub mod comps;
pub mod prims;
pub mod shell;
pub mod states;

#[allow(unused_imports)]
pub use prims::ScannerBg;
