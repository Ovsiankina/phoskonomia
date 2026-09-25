//! Data layer (OWNED BY AGENT F3).
//!
//! This is where the hand-written REST boundary dies. F3 fills this module with:
//!   * `#[server]` async fns (in-process on desktop/mobile; auto-transported on
//!     web) that call into the server-only crates (`phosk_insights`,
//!     `phosk_ledger`, `phosk_planning`) behind the `server-deps` feature, and
//!   * the shared view structs they return (pure, `serde`-able, WASM-safe — they
//!     may use `phosk_core::Money` / `phosk_model` types since those compile to
//!     WASM too).
//!
//! Pages consume these via `use_resource` / `use_server_future`.
//!
//! ## Layout (F3)
//!
//! One submodule per page + two shared ones. Each declares its `serde` view
//! structs (money as exact [`Money`] via `phosk_model::money_centimes`) and the
//! `#[server]` async fn(s) a page calls through `use_resource`:
//!
//! * [`cycle`]        — shared current-cycle window (`CycleDto`); every page top-bar uses it.
//! * [`ai`]           — shared assistant (feed/status read, live chat) for the left `AiPanel`.
//! * [`approvals`]    — the AI approval queue: pending receipt proposals, approve / reject / bulk.
//! * [`signals`]      — shared item-signal vocabulary (`SignalDto`, candidates, movers).
//! * [`dashboard`]    — the composed dashboard read (REAL backend via `phosk_insights`).
//! * [`transactions`] — receipt list, lines, receipt detail.
//! * [`budgets`]      — envelopes, budget totals, allocation, category inspector.
//! * [`csv_export`]   — CSV export of the transactions / budget / subscriptions lists.
//! * [`subscriptions`]— standing charges, stats, billing sweep, detail.
//! * [`debts`]        — open balances, stats, payoff trajectory, IOU ledger, detail.
//! * [`settings`]     — `/config` preferences: get / set / reset (REAL via `phosk_settings`).
//! * [`analytics`]    — spend history, momentum, weekday rhythm, movers, insights.
//!
//! ## Money on the wire
//!
//! Monetary fields are the exact [`Money`] newtype (i64 centimes), serialized via
//! `#[serde(with = "phosk_model::money_centimes")]` (`Option` via
//! `opt_money_centimes`; `Vec` via the local `dashboard::money_vec_centimes`). No
//! CHF `f64` ever crosses the boundary; the page renders amounts through [`chf`] /
//! [`chf2`] in the Pilowlava display font.
//!
//! ## "Today" & the seed
//!
//! The deterministic Swiss seed pins the demo clock to **2026-06-18** (day 18 of
//! the June cycle). Server fns resolve `as_of` via [`today`] and drive the real
//! feature services through the `Session` ports (the seeded `MemoryDb` by
//! default). Tests live in `tests/` (see its module doc).
//!
//! F1 provides only the [`chf`] presentation formatter below (the one thing
//! carried from React `data/phosk.js`); everything else is F3's.

use phosk_core::money::Money;

pub mod ai;
pub mod analytics;
pub mod approvals;
pub mod budgets;
pub mod csv_export;
pub mod cycle;
pub mod dashboard;
pub mod debts;
pub mod settings;
pub mod signals;
pub mod subscriptions;
pub mod transactions;

/// The demo "today": 2026-06-18 (day 18 of the seeded June cycle). Server fns
/// resolve cycle windows against this so the seeded reads line up with the
/// `phosk_insights` dashboard logic. The literal is always valid, so the
/// `unwrap_or` arm ([`NaiveDate::MIN`]) is unreachable and this never panics.
#[cfg(feature = "server-deps")]
#[must_use]
pub fn today() -> chrono::NaiveDate {
    chrono::NaiveDate::from_ymd_opt(2026, 6, 18).unwrap_or(chrono::NaiveDate::MIN)
}

// ── composition root (ADR-010) ───────────────────────────────────────────────
//
// This module is the SINGLE place concrete adapters (`phosk_db_surreal`,
// `phosk_llm_ollama`, `phosk_storage_fs`, `phosk_ocr_*`) are named and injected.
// It is gated behind `server-deps`, so none of these crates — nor their vendor
// dependencies (surrealdb, reqwest, chacha20poly1305) — ever enter the WASM
// client bundle (the `web` build does not enable `server-deps`). Feature
// services and `#[server]` fn bodies only ever see the L2 PORT trait objects
// (`&dyn DatabaseAdapter`, `&dyn LlmAdapter`, …) via [`Session`]; swapping a
// technology is a change here and nowhere else.
//
// The whole stack is assembled ONCE per process (lazily, on the first
// `#[server]` invocation) and cached in [`STACK`]. `build_session()` then hands
// out cheap [`Session`] handles that clone the cached `Arc`s — opening the
// SurrealDB file / probing OCR reachability happens a single time, not per
// request.

#[cfg(feature = "server-deps")]
mod composition {
    use std::sync::Arc;

    use phosk_adapter_db::DatabaseAdapter;
    use phosk_adapter_llm::LlmAdapter;
    use phosk_adapter_ocr::OcrAdapter;
    use phosk_adapter_storage::PhotoStorage;
    use tokio::sync::OnceCell;

    /// The process-wide assembled adapter stack. Built once (see [`stack`]).
    static STACK: OnceCell<Stack> = OnceCell::const_new();

    /// The wired-up, process-global adapter set. Each port is held behind an
    /// `Arc<dyn _>` so a per-request [`super::Session`] is a cheap clone of four
    /// refcounts — never a rebuild of the underlying store/clients.
    #[derive(Clone)]
    pub(crate) struct Stack {
        pub(crate) db: Arc<dyn DatabaseAdapter>,
        pub(crate) llm: Arc<dyn LlmAdapter>,
        pub(crate) storage: Arc<dyn PhotoStorage>,
        pub(crate) ocr: Arc<dyn OcrAdapter>,
    }

    /// Resolve the data directory for the file-backed adapters
    /// (`PHOSK_DATA_DIR`, default `./phosk-data`).
    fn data_dir() -> std::path::PathBuf {
        std::env::var_os("PHOSK_DATA_DIR")
            .map_or_else(|| std::path::PathBuf::from("phosk-data"), Into::into)
    }

    /// Select + open the DATABASE adapter.
    ///
    /// * `PHOSK_DB=surreal` → file-backed [`phosk_db_surreal::SurrealDb`], seeded
    ///   on first run (idempotent re-open), stored under `<data_dir>/surreal`.
    /// * anything else (default `memory`) → seeded
    ///   [`phosk_db_memory::MemoryDb`] (deterministic 2026-06-18 Swiss seed).
    async fn build_db() -> Result<Arc<dyn DatabaseAdapter>, phosk_core::error::PhoskError> {
        let kind = std::env::var("PHOSK_DB").unwrap_or_default();
        if kind.eq_ignore_ascii_case("surreal") {
            let dir = data_dir().join("surreal");
            std::fs::create_dir_all(&dir).map_err(|e| {
                phosk_core::error::PhoskError::Invalid(format!("create surreal dir: {e}"))
            })?;
            let path = dir.join("phosk.db");
            let db = phosk_db_surreal::SurrealDb::file_seeded(&path.to_string_lossy()).await?;
            Ok(Arc::new(db))
        } else {
            let db = phosk_db_memory::MemoryDb::seeded()?;
            Ok(Arc::new(db))
        }
    }

    /// Build the LLM adapter: local Ollama via [`phosk_llm_ollama::OllamaLlm`]
    /// (`PHOSK_LLM_MODEL` overrides the default `qwen3.6:35b-custom`; base URL
    /// is the Ollama default `http://localhost:11434`). Construction does not
    /// contact the server, so it never blocks here on a down model.
    fn build_llm() -> Result<Arc<dyn LlmAdapter>, phosk_core::error::PhoskError> {
        let llm = phosk_llm_ollama::OllamaLlm::from_env()?;
        Ok(Arc::new(llm))
    }

    /// Build the photo STORAGE adapter: encrypted-at-rest
    /// [`phosk_storage_fs::FsPhotoStorage`] under `<data_dir>/photos`.
    fn build_storage() -> Result<Arc<dyn PhotoStorage>, phosk_core::error::PhoskError> {
        let dir = data_dir().join("photos");
        let storage = phosk_storage_fs::FsPhotoStorage::open(&dir)?;
        Ok(Arc::new(storage))
    }

    /// Select the OCR adapter, falling back to the deterministic fake when no
    /// live engine is reachable on THIS machine (PaddleOCR not installed, no
    /// vision model pulled):
    ///
    /// * `PHOSK_OCR=paddle` and the service is reachable → [`phosk_ocr_paddle`].
    /// * `PHOSK_OCR=vision` (or default `auto`) and Ollama has a vision model →
    ///   [`phosk_ocr_vision`].
    /// * otherwise → [`phosk_adapter_ocr::FakeOcr`] (canned Swiss receipt).
    async fn build_ocr() -> Arc<dyn OcrAdapter> {
        let kind = std::env::var("PHOSK_OCR").unwrap_or_else(|_| "auto".to_owned());

        if kind.eq_ignore_ascii_case("paddle") || kind.eq_ignore_ascii_case("auto") {
            if let Ok(p) = phosk_ocr_paddle::PaddleOcr::from_env() {
                if p.is_reachable().await {
                    return Arc::new(p);
                }
            }
        }
        if kind.eq_ignore_ascii_case("vision") || kind.eq_ignore_ascii_case("auto") {
            if let Ok(v) = phosk_ocr_vision::OllamaVisionOcr::from_env() {
                if matches!(v.has_vision_model().await, Ok(true)) {
                    return Arc::new(v);
                }
            }
        }
        Arc::new(phosk_adapter_ocr::FakeOcr::new())
    }

    /// The stack unit tests run against: the seeded memory DB plus the
    /// in-process fakes. It reads no `PHOSK_*` env, opens no socket and writes
    /// no file, so `#[server]` fn tests are deterministic on any machine.
    fn hermetic_stack() -> Result<Stack, phosk_core::error::PhoskError> {
        Ok(Stack {
            db: Arc::new(phosk_db_memory::MemoryDb::seeded()?),
            llm: Arc::new(phosk_adapter_llm::FakeLlm::new()),
            storage: Arc::new(phosk_adapter_storage::InMemoryStorage::new()),
            ocr: Arc::new(phosk_adapter_ocr::FakeOcr::new()),
        })
    }

    /// Assemble (or return the cached) process-wide [`Stack`]. The first call
    /// opens the store / probes OCR; every later call is a cheap cache hit.
    /// Test builds get [`hermetic_stack`] instead.
    pub(crate) async fn stack() -> Result<&'static Stack, phosk_core::error::PhoskError> {
        STACK
            .get_or_try_init(|| async {
                if cfg!(test) {
                    return hermetic_stack();
                }
                Ok(Stack {
                    db: build_db().await?,
                    llm: build_llm()?,
                    storage: build_storage()?,
                    ocr: build_ocr().await,
                })
            })
            .await
    }
}

/// The server-side composition root for one `#[server]` invocation.
///
/// Holds cheap `Arc` clones of the four process-global ports (DB, LLM, photo
/// storage, OCR). Feature services and pipelines are called through the
/// accessors below, which hand them `&dyn _` PORT objects — they never see a
/// concrete adapter type, so swapping a backend technology is a change in
/// [`composition`] alone (ADR-005/010).
#[cfg(feature = "server-deps")]
pub(crate) struct Session {
    stack: composition::Stack,
}

#[cfg(feature = "server-deps")]
impl Session {
    /// The database port the feature services read/write through.
    pub(crate) fn db(&self) -> &dyn phosk_adapter_db::DatabaseAdapter {
        self.stack.db.as_ref()
    }

    /// The LLM port (Ollama) the AI chat, write-tools and narrative insights use.
    pub(crate) fn llm(&self) -> &dyn phosk_adapter_llm::LlmAdapter {
        self.stack.llm.as_ref()
    }

    /// The encrypted photo-storage port the receipt pipeline writes to.
    #[allow(dead_code)]
    pub(crate) fn storage(&self) -> &dyn phosk_adapter_storage::PhotoStorage {
        self.stack.storage.as_ref()
    }

    /// The OCR port the receipt pipeline transcribes with.
    #[allow(dead_code)]
    pub(crate) fn ocr(&self) -> &dyn phosk_adapter_ocr::OcrAdapter {
        self.stack.ocr.as_ref()
    }
}

/// Build the per-request [`Session`] — the single composition seam.
///
/// **This is the swap point.** It returns a handle onto the process-global
/// adapter stack assembled by [`composition::stack`] (built once, lazily, on the
/// first `#[server]` call). The concrete stack is selected by environment:
///
/// | env var          | values                              | default                  |
/// |------------------|-------------------------------------|--------------------------|
/// | `PHOSK_DB`       | `surreal` (file) \| `memory`        | `memory` (seeded)        |
/// | `PHOSK_LLM_MODEL`| any Ollama model tag                | `qwen3.6:35b-custom`     |
/// | `PHOSK_OCR`      | `paddle` \| `vision` \| `auto`      | `auto` (→ fake if none)  |
/// | `PHOSK_DATA_DIR` | a path for file-backed adapters     | `./phosk-data`           |
///
/// Server fns and feature services are untouched by a swap because they only
/// ever see the `&dyn _` PORT objects [`Session`] exposes.
///
/// # Errors
/// Propagates any [`phosk_core::error::PhoskError`] from adapter construction
/// (store open, key setup, model-name validation), surfaced to the client as a
/// [`dioxus::prelude::ServerFnError`].
#[cfg(feature = "server-deps")]
pub(crate) async fn build_session() -> Result<Session, dioxus::prelude::ServerFnError> {
    let stack = composition::stack()
        .await
        .map_err(|e| dioxus::prelude::ServerFnError::new(e.to_string()))?
        .clone();
    Ok(Session { stack })
}

// Foundation API consumed by page/data agents; unused until they land.
#[allow(dead_code)]
/// Swiss-currency formatter: apostrophe thousands, dot decimal, `−` for negatives
/// (e.g. `CHF 1'234.50` renders `1’234.50`, `-12.5` renders `−12.50`).
///
/// Port of the design export's `chf(n, dp)`. Takes exact [`Money`] (never a
/// float) and formats to `dp` decimal places (default 2), rounding half away
/// from zero like its `toFixed(dp)` (so `12.50` at `dp = 0` is `13`, not `12`),
/// but in exact integer arithmetic. NOTE: every numeric
/// value the UI shows must render in the Pilowlava display font (design law) —
/// this only produces the string; the page applies the font.
#[must_use]
pub fn chf(amount: Money, dp: usize) -> String {
    let centimes = amount.centimes();
    let neg = centimes < 0;
    // `u64` holds |i64::MIN| + 50, so neither the abs nor the rounding overflows.
    let abs = centimes.unsigned_abs();

    // Round the exact centimes to `min(dp, 2)` decimals; `dp > 2` pads zeros.
    let (whole, frac) = match dp {
        0 => ((abs + 50) / 100, String::new()),
        1 => {
            let tenths = (abs + 5) / 10;
            (tenths / 10, (tenths % 10).to_string())
        }
        _ => (abs / 100, format!("{:02}{}", abs % 100, "0".repeat(dp - 2))),
    };

    // Group the integer part with apostrophe thousands separators.
    let int_str = whole.to_string();
    let mut grouped = String::with_capacity(int_str.len() + int_str.len() / 3);
    let bytes = int_str.as_bytes();
    let len = bytes.len();
    for (i, b) in bytes.iter().enumerate() {
        if i > 0 && (len - i).is_multiple_of(3) {
            grouped.push('\u{2019}'); // ’ right single quotation mark
        }
        grouped.push(*b as char);
    }

    let mut out = String::new();
    if neg {
        out.push('\u{2212}'); // − minus sign
    }
    out.push_str(&grouped);
    if dp > 0 {
        out.push('.');
        out.push_str(&frac);
    }
    out
}

/// `chf` with the default 2 decimal places (the common case).
#[allow(dead_code)]
#[must_use]
pub fn chf2(amount: Money) -> String {
    chf(amount, 2)
}

#[cfg(test)]
mod tests;
