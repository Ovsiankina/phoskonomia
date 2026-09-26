# Phoskonomia Roadmap

## Project Overview
**Phoskonomia** — AI-driven micro-budgeting expense tracker. Local-first, receipt-focused, item-level categorization with LLM-powered insights and budget recommendations. Built with Dioxus (web/desktop fullstack).

**Core philosophy:** Not a generic accounting app. Purpose-built for detailed personal spending analysis, category-based budgeting, recurring detection, and trend exploration.

---

## Architecture Decisions (v0.2 — 2026-05-22, current)

> This section supersedes the backend portion of v0.1 (below). The
> Dioxus 0.7 frontend documented in v0.1 continues unchanged. The change
> in v0.2 is structural: the backend moves from Dioxus fullstack
> `#[server]` functions to a **pure-Rust library-crate backend** that
> the desktop app links directly. HTTP appears only at network boundaries.

### Decisions made on 2026-05-22

- **Architectural pattern:** Hexagonal architecture (ports & adapters)
  with a microkernel-style core. Whole-subsystem swappability over
  individual-operation swappability.
- **Backend topology:** Library crates linked into the Dioxus app for
  local calls — **no HTTP for in-process operations**. HTTP retained
  only at genuine network boundaries (Pi queue endpoint, desktop ↔ Pi
  polling, hypothetical future web target).
- **Workspace style:** rustc-inspired. Many small crates with strict
  downward dependency flow. `phosk_*` prefix on every crate (mirrors
  `rustc_*`). Composition roots in `bin/`, library crates in `crates/`,
  build tooling in `xtask/`.
- **Database trait shape:** **One fat `DatabaseAdapter` trait**, not
  split per-domain. Rationale below.
- **Security model:** Pi as untrusted DMZ; desktop never exposes an
  inbound socket; desktop polls Pi outbound; queued photos treated as
  hostile input.
- **HTTP framework:** axum. **Async runtime:** tokio.
- **Repository layout:** **Separate repo** for the new backend
  (`phoskonomia-backend`). The current repo's `phoskonomia-adapters`
  is superseded once the new backend is functional.
- **Brainstorm framing for behaviour design:** deferred (see end of
  section). Lean: "stability surfaces + invariants" — directly serves
  the "use this app for years" goal.

### Architectural pattern — hexagonal / microkernel

The app is structured as a **hexagonal architecture** with a
microkernel-style core. Three layers:

- **Domain core** — stable, rarely changes. Personal-finance concepts:
  receipts, items, categories, budgets, subscriptions, debts, alerts.
  This is the "what" of the product. Years from now, a receipt is still
  a receipt.
- **Ports** — stable trait contracts: `DatabaseAdapter`, `LlmAdapter`,
  `OcrAdapter`, `PhotoStorage`, `Notifier`. Each port is a **fat trait**
  defining a whole-subsystem interface. Ports are the surface against
  which feature code is written.
- **Adapters** — interchangeable concrete impls of each port.
  SurrealDB today / future DB engines / future LLM-mediated storage
  tomorrow. Ollama Gemma4 today / OpenCode deepseek fallback / future
  vision-LLMs / future on-device transformers. PaddleOCR today /
  vision-LLM OCR / future OCR engines. The list of replaceable
  technologies will only grow.

**Guiding principle (locked):** **Make whole subsystems swappable, not
individual operations.** LLMs, OCR engines, MCP servers, the database
itself MUST be swappable as the field evolves. The trait IS the
boundary; swapping a backend technology means writing a new impl,
never touching feature code.

### Workspace structure (rustc-inspired)

**Location:** new separate repository (`phoskonomia-backend`). Naming
mirrors rustc (`phosk_*` for every crate, `rustc_*` style). Many small
crates with strict downward dependency flow.

```
phoskonomia-backend/             # new separate repo
├── Cargo.toml                   # workspace root, [workspace.lints] global
├── rust-toolchain.toml
├── README.md
├── CONTRIBUTING.md               # per-crate purpose chart
│
├── bin/                          # composition roots — pick concrete adapters
│   ├── phosk_daemon/             # desktop daemon: polls Pi, runs OCR+LLM
│   ├── phosk_queue/              # Raspberry Pi queue server (HTTP face)
│   └── phosk_cli/                # admin / migration / inspection CLI
│
├── crates/
│   │ ── L0 · foundation ────────────────────────────────────
│   ├── phosk_macros/             # internal proc-macros
│   ├── phosk_errors/             # error types + diagnostics (thiserror)
│   ├── phosk_data_structures/    # shared collections / helpers
│   ├── phosk_id/                 # typed IDs: ReceiptId, ItemId, …
│   ├── phosk_money/              # CHF-typed Decimal arithmetic
│   ├── phosk_time/               # Period (Day/Week/Month/Quarter/Year/Custom)
│   │
│   │ ── L1 · domain model (pure data; like rustc_ast) ─────
│   ├── phosk_model/              # Receipt, Item, Category, Budget,
│   │                             # Subscription, Debt, Alert, …
│   │                             # NO behavior, NO adapter deps
│   │
│   │ ── L2 · adapter contracts (fat traits, no impls) ─────
│   ├── phosk_adapter_db/         # DatabaseAdapter trait
│   ├── phosk_adapter_llm/        # LlmAdapter trait + prompt registry
│   ├── phosk_adapter_ocr/        # OcrAdapter trait
│   ├── phosk_adapter_storage/    # PhotoStorage trait (encrypted at rest)
│   ├── phosk_adapter_notify/     # Notifier trait
│   │
│   │ ── L3 · concrete adapter impls (interchangeable) ─────
│   ├── phosk_db_surreal/         # SurrealDB impl
│   ├── phosk_db_memory/          # in-memory for tests
│   ├── phosk_llm_ollama/         # local Gemma via Ollama HTTP
│   ├── phosk_llm_opencode/       # OpenCode Go / deepseek fallback
│   ├── phosk_llm_mock/           # canned responses
│   ├── phosk_ocr_paddle/         # PaddleOCR microservice client
│   ├── phosk_ocr_vision_llm/     # vision-LLM OCR alternative
│   ├── phosk_storage_fs/         # encrypted filesystem, EXIF strip
│   ├── phosk_notify_libnotify/   # notify-send (Linux)
│   │
│   │ ── L4 · middle (god-object aggregator) ───────────────
│   ├── phosk_middle/             # Session struct holding Arc<dyn …> stores
│   │                             # every feat_* takes &Session
│   │                             # mirrors rustc_middle::ty::TyCtxt
│   │
│   │ ── L5 · feature crates (one per domain feature) ──────
│   ├── phosk_feat_dashboard/
│   ├── phosk_feat_transactions/  # receipt/item CRUD + queries
│   ├── phosk_feat_categories/    # CRUD + merge/split + retro reprocess
│   ├── phosk_feat_budgets/       # budgets + history + LLM suggestions
│   ├── phosk_feat_subscriptions/ # CRUD + recurring detection
│   ├── phosk_feat_debts/
│   ├── phosk_feat_alerts/        # alert generation engine
│   ├── phosk_feat_analytics/     # trends, rankings, comparisons
│   ├── phosk_feat_settings/
│   │
│   │ ── L6 · pipelines (compose features + adapters) ──────
│   ├── phosk_pipeline_receipt/   # photo → OCR → LLM → reviewable items
│   ├── phosk_pipeline_csv/
│   ├── phosk_pipeline_pdf/
│   │
│   │ ── L6 · exports (heavy deps isolated) ────────────────
│   ├── phosk_export_csv/
│   ├── phosk_export_json/
│   ├── phosk_export_pdf/         # printpdf — heaviest, kept alone
│   │
│   │ ── L7 · HTTP (network boundaries only) ───────────────
│   ├── phosk_http_dto/           # request/response shapes
│   ├── phosk_http_auth/          # API-key middleware
│   ├── phosk_http_queue/         # axum router for Pi queue endpoint
│   │
│   │ ── Pi queue storage ──────────────────────────────────
│   ├── phosk_queue_store/        # filesystem-backed FIFO queue
│   ├── phosk_queue_dedup/        # hash-based duplicate rejection
│   │
│   │ ── Cross-cutting ─────────────────────────────────────
│   ├── phosk_config/             # TOML config parser, env overrides
│   ├── phosk_session/            # runtime bootstrap: paths, flags
│   ├── phosk_telemetry/          # tracing-subscriber, log redaction
│   └── phosk_test_utils/         # fixture builders, mock factories
│
├── xtask/                        # cargo xtask migrate / seed / fmt-all
└── tests/e2e/                    # workspace-level end-to-end
```

**Dependency direction (strictly downward — never reverse):**

```
bin/* ──▶ feat_* / pipelines / exports ──▶ middle ──▶ adapter_* (traits)
                                                          ▲
                                                          │
                                                concrete adapter impls
                                                (db_surreal, llm_*, ocr_*)

every layer ──▶ model, errors, id, money, time, data_structures (L0–L1)
```

**Composition rule:** concrete adapter impls are NEVER imported by
feature crates. The composition root (`bin/*`) picks impls at startup
and wires them into a `phosk_middle::Session`. Every `feat_*` function
takes `&Session` and asks for what it needs (`session.db()`,
`session.llm()`, `session.ocr()`, …).

**Why this many crates:**

- **Each port in its own crate** so mock impls (used by every `feat_*`
  test) don't drag in SurrealDB or Ollama. The cfg-feature trick
  becomes "which crate did I depend on" — much cleaner.
- **Per-feature crates** so the budget logic compiles/tests without
  touching the OCR pipeline. Build times stay sane as features grow.
- **`phosk_money` and `phosk_id` look tiny** but they enforce
  invariants — can't pass a `CategoryId` where `ItemId` is expected,
  CHF math is type-safe. The small size is the point (mirrors
  `rustc_index`).
- **`phosk_export_pdf` isolated** because `printpdf` is a heavy dep
  not worth polluting anything else with.
- **`phosk_http_dto` separate from `phosk_model`** so the REST API
  (when needed) can evolve without forcing domain-model changes
  (mirrors rustc's `rustc_ast` vs interface-stable types).

### Transport — HTTP only at network boundaries

Local desktop calls between the Dioxus app and backend crates are
**direct function calls**. No HTTP for in-process operations — that
would be needless overhead and serialization for same-machine work.

HTTP appears only at three places:

1. **iPhone Shortcut → Pi queue.** Mandatory: cross-machine,
   cross-platform client (iOS Shortcut speaks HTTP). axum HTTPS
   endpoint on Pi, single API key, rate-limited.
2. **Desktop daemon → Pi queue.** Cross-machine. Desktop polls
   outbound. Pi never reaches inward.
3. **Web target.** Future, if ever shipped. Browser can't link to
   Rust code; needs a wire format. Dioxus `#[server]` would handle
   this transparently when the time comes. Out of scope for v0.1.

axum is the chosen HTTP framework. tokio is the runtime. Both apply
only inside `phosk_http_queue/` and `bin/phosk_queue/` for v0.1.

### Security model — Pi as DMZ

The Pi is treated as an **untrusted DMZ**. Trust boundary is the
photo content itself, not the network channel.

**Why this is safe even if the Pi is fully compromised:**

- Desktop never exposes an inbound socket. The desktop daemon
  initiates outbound polls to the Pi. A compromised Pi has **no
  network route** back to the desktop.
- Pi stores **opaque blobs** in a persistent FIFO queue. If
  client-side encryption is enabled (option below), the Pi never
  sees plaintext photos — compromise leaks ciphertext only.
- Desktop drains queue on its own schedule. There is no ACK: the pop
  is destructive (the Pi deletes a blob as it hands it over), so the
  desktop holds the only copy from then on — limits how long a photo
  sits on the Pi. The daemon therefore checks OCR + LLM health before
  each pop and leaves photos queued while either is down; a photo whose
  ingest still fails is dropped (counted as failed, not re-queued).

**Inbound (iPhone Shortcut → Pi):**

- HTTPS only
- Single API key in `Authorization` header (rotatable)
- Rate limiting (per-key)
- Optional: IP allowlist (home/cellular IPs only)
- Optional: client-side AES encryption of photo bytes in the
  Shortcut, with the symmetric key held only on the desktop. Pi
  receives ciphertext.

**Photo handling on desktop (zero-trust input):**

- Validate MIME type + libmagic signature + size cap before any
  processing
- Run PaddleOCR + LLM in a sandbox: systemd-nspawn or container
  with no network, dropped capabilities, ulimit on memory and CPU,
  read-only filesystem outside a scratch directory
- Validate LLM JSON output against schema before persisting

**Net effect:** "Pi compromise" reduces to "malicious photo upload" —
a known, bounded threat surface no worse than what cloud-mediated
upload services already deal with.

**Topology summary:** two binaries (`phosk_queue` on Pi, `phosk_daemon`
on desktop), two repos, two trust zones, one-way data flow on poll.

### Database adapter shape — one fat trait

Decision: **one `DatabaseAdapter` trait covering all domains.**
Not split into `ReceiptStore` + `CategoryStore` + `BudgetStore` + …

Reasoning (this is the one place we consciously diverge from rustc
style):

- The priority is **whole-database swappability**. Future targets:
  a different engine (Postgres, embedded KV), a multi-tier setup
  (hot SurrealDB + cold archive), an LLM-mediated storage layer
  that decides query strategy.
- Splitting into 8 per-domain stores would optimize for swapping
  individual tables — not a real need for a single-user local app.
- The fat trait IS the boundary. Future impls implement the whole
  thing; feature code never changes.

This is the only place where "make whole subsystems swappable, not
individual operations" forces a deliberate departure from "many
small crates."

### Migration from current codebase

The Dioxus frontend (`phoskonomia-app`) continues. The backend
moves out into the new repo. Specific moves:

- `phoskonomia-adapters/src/models.rs` → `phosk_model/` (new repo).
  Domain types remain stable.
- `phoskonomia-adapters/src/db/adapter.rs` → `phosk_adapter_db/`
  (new repo), unchanged shape (one fat trait).
- `phoskonomia-adapters/src/db/surreal.rs` → `phosk_db_surreal/`
  (new repo).
- `phoskonomia-app/src/backend/server_fns/*` → **deleted**. The
  Dioxus app instead path-depends on `phosk_feat_*` crates from the
  new repo and calls them as ordinary async library functions.
- `phoskonomia-app/Cargo.toml` adds `phoskonomia-backend` as a
  workspace path dependency (or git submodule, TBD).

Once migration completes, the current repo's `phoskonomia-adapters`
crate is **removed**. The current `backend/server_fns/` directory is
**removed**. The `mock_data.rs` fixtures move to `phosk_test_utils/`.

### Deferred — behaviour brainstorm

Behaviour-level design (what each `feat_*` crate's public API looks
like, what each pipeline emits, what triggers each alert) is **not
done yet**. Resuming this brainstorm is the next session's work.

Candidate framings to choose from when resuming (any combination
welcome, the list is not exhaustive):

- **Invariants first** — what must always be true about the data,
  regardless of which adapter impls are wired
- **Events first** — what the system reacts to: photo arrives, day
  advances, alert dismissed, recurring rule fires
- **E2E flow first** — whiteboard the hardest single flow (iPhone
  photo → reviewed, categorized, budgeted receipt), generalize from
  there
- **Stability surfaces** — catalog what must never change (domain
  types, port traits, REST contracts) vs what can change freely
  (impl details, internal helpers, query strategies)
- **Background-first** — design unattended workers (queue drainer,
  recurring detector, photo retention cleanup, anomaly scanner)
  before user-facing features
- **User-journey** — walk a day in the app
- **Per-crate spec** — alphabetical through `feat_*`

**Lean:** "stability surfaces + invariants" — they directly serve
the "use this app for years" goal that drove every decision in this
section.

---

## Architecture Decisions (v0.1 — superseded by v0.2 above)

> **Status:** Historical reference only. The backend portions of this
> section (Cargo Workspace Structure, API Design) are superseded by
> v0.2 above. The frontend portions (Dioxus 0.7, design system, photo
> import pipeline, LLM/OCR strategy as design intent) remain accurate.

### Cargo Workspace Structure

Phoskonomia uses a **Cargo workspace** with separate crates to enforce
separation of concerns and enable the adapter pattern:

```
phoskonomia/
├── Cargo.toml                 # workspace root
├── phoskonomia-app/           # Dioxus fullstack (single crate)
│   ├── Cargo.toml             # dioxus 0.7 with fullstack + router
│   ├── Dioxus.toml
│   ├── assets/                # CSS, JS, fonts
│   └── src/
│       ├── main.rs
│       ├── app.rs
│       ├── routes.rs
│       ├── components/        # UI (layout, ui, features)
│       ├── pages/             # route page components
│       └── backend/           # server functions
│           └── server_fns/    # #[server] per domain
├── phoskonomia-adapters/      # Adapter layer (DI boundaries)
│   ├── Cargo.toml
│   └── src/
│       ├── lib.rs
│       ├── models.rs          # shared data types
│       ├── db/                # database adapter trait + impls
│       ├── llm/               # LLM provider adapter trait + impls
│       ├── ocr/               # OCR adapter trait + impls
│       ├── storage/           # file storage adapter trait + impls
│       └── notifications/     # platform notification adapter
└── phoskonomia-cli/           # CLI tools (queue processor, etc.)
    ├── Cargo.toml
    └── src/
```

**Key principle:** Every external dependency (database, LLM, OCR, storage,
notifications) goes through an **adapter trait** defined in
`phoskonomia-adapters`. This allows swapping implementations without
touching application logic:

- Database: adapter trait → concrete impl (SQLite, SurrealDB, etc.)
- LLM: adapter trait → Ollama impl, OpenCode Go impl, mock impl
- OCR: adapter trait → PaddleOCR impl, vision-LLM impl, mock impl
- Storage: adapter trait → local filesystem, encrypted fs, mock
- Notifications: adapter trait → notify-send (Linux), mock

### API Design

- **Dioxus server functions** (`#[server]`) with explicit endpoints:
  `#[server(endpoint = "/api/v1/dashboard")]`
- **One file per domain:** `server_fns/receipts.rs`, `server_fns/budgets.rs`, etc.
- **All endpoints scaffolded first** — unimplemented endpoints return a
  `todo_notif!("Feature not implemented yet")` that triggers an in-app
  toast notification
- **Error handling:** `thiserror` crate for domain error types
- **Mock data:** Available in dev mode with a clear UI indicator.
  Hard-disabled in release builds (compile-time gate).

### LLM Strategy

- **Primary:** Local Gemma4 via Ollama (receipt → JSON extraction)
- **Fallback:** OpenCode Go with deepseek-v4-flash or deepseek-v4-pro
  (same API key as Hermes Agent)
- **Model selection:** Configurable in Settings UI (any provider via adapter)
- **Context window:** ~256K tokens
- **Prompt templates:** Rust `const` strings in dedicated prompt files
  (per task: receipt extraction, category suggestion, budget analysis)
- **Output format:** JSON (parseable, extensible, schema-validated)
- **Audit logging:** All LLM prompts + responses logged in debug mode
- **Offline mode:** App functions without LLM; status indicator in topbar
  shows LLM connectivity
- **Category suggestions:** LLM suggests new categories → queued for user
  approval before activation
- **Confidence:** LLM must self-assess confidence. Overconfident
  predictions must be detectable. Low-confidence items flagged.

### OCR Pipeline

- **Primary:** PaddleOCR via HTTP microservice (Python venv, no user install)
- **Alternative under evaluation:** Vision-capable LLM (might replace
  PaddleOCR entirely)
- **Adapter pattern:** OCR trait allows swapping implementations
- **Preprocessing:** Dependency-injected preprocessing steps (contrast,
  deskew, crop) — configurable per implementation
- **Output:** Text + bounding box coordinates stored per receipt for
  detail-page overlay (see bounding boxes on receipt photo)
- **Original + annotated:** Both the original photo and the OCR-annotated
  version (with bounding boxes drawn) are stored

### Photo Import Pipeline

- **Primary input:** iPhone Shortcut → `POST /api/v1/receipts/upload`
  (hosted on ovsiankina.com, secured by API key)
- **Desktop/web:** Standard file picker import
- **Receipt formats supported:**
  - Paper receipt photos (store register printouts)
  - Email receipts (forwarded or screenshotted)
  - Banking app transaction screenshots (limited detail)
  - Payment terminal photos (alternative to paper receipt)
- **Processing flow:** Upload → OCR/LLM extraction → Item list → User
  review → Approval → Compression → Archive
- **Approval workflow:** Every transaction has an approval state.
  Unapproved transactions are usable but show a pending indicator.
  Only approved photos get compressed.
- **Photo retention:** 180 days default, no global quota. Per-photo
  "keep forever" flag available. No user-installed software required.

### Storage & Privacy

- **Receipt photos:** Encrypted at rest
- **EXIF metadata:** Stripped on import (GPS, device info removed)
- **Telemetry:** Strictly zero — no anonymous usage stats

### Platform Targets

- **Primary:** Linux (only target for v0.1)
- **Desktop:** Dioxus desktop (not Tauri)
- **System tray:** Quick actions (process queue, open app)
- **Auto-start:** On user login
- **Raspberry Pi queue:** Deferrable (host the API on ovsiankina.com,
  compute and storage on the local device)

### Development Practices

- **TDD:** Test-driven development. Write tests before code.
- **Testing:** Unit tests per module + integration tests.
  Database: in-memory/temp. LLM: mock with known receipt JSON output.
  OCR: test receipt images in repo. Frontend: Dioxus test utilities.
- **Coverage target:** 60% minimum
- **Scaffold-first:** All server function endpoints defined before
  implementing individual features. `todo_notif!()` macro for stubs.

### Database

- **Technology:** SurrealDB — embeddable document-graph database, Rust-native
  (`surrealdb` crate)
- **Rationale:** Document-oriented (flexible receipt structures), binary
  storage (photos + OCR annotations), full-text search (MCP/RAG for LLM
  queries), JSON-native, embeddable (no separate process)
- **Access pattern:** Through an **adapter trait** in `phoskonomia-adapters`
  so the concrete database can be swapped without touching application code
- **Encryption:** At rest
- **Migrations:** Schema defined in SurrealQL, versioned migration scripts

---

## Feature Recap

**Phoskonomia is a comprehensive, micro-budgeting personal finance tracker with these core features:**

### Core Transaction Model
- Dashboard with financial snapshot (global budget, spending to date, savings target)
- Main transaction list (date, shop, total, category tags — like banking apps)
- Click "Details" on transaction → see full receipt breakdown (all line items)
- Every line item is labeled, organized, and categorized
- Click any item label/category → view ALL transactions of that type across all time periods
- Filtered transaction view (by category, shop, time period) looks identical to main list (seamless UX)
- Multi-dimensional filtering: time horizon, shop, category, custom ranges
- Sorting: by date, amount, shop, category

### Input Sources (Not just photos)
- **Photo upload:** Primary input via Dioxus file picker (JPG, PNG, WEBP)
- **Manual entry:** Form-based data input (item name, price, quantity, category)
- **CSV import:** Bank exports, personal spreadsheets with column mapping and preview
- **PDF invoices:** Subscriptions, utility bills, tax invoices (table extraction, OCR fallback)
- All inputs feed into same processing pipeline

### Intelligent Categorization
- Local LLM (Llama 3 / Mistral via Ollama) reads receipts and suggests categories on first encounter
- User can accept, modify, or create custom categories on-the-fly
- LLM recognizes shop/vendor names automatically
- Can toggle automatic category creation (on/off)
- Can merge categories (retroactively applies to all past items, async processing)
- Can split categories (creates new categories from existing ones)
- Category creation/merge/split triggers async LLM reprocessing of historical data
- Each item has a "reading confidence score" (0-1)
- Low-confidence items flagged with warning triangle icon until user resolves
- User can correct readings via:
  - Direct edit (user changes text, doesn't trigger LLM re-read)
  - Explanation (user explains error, LLM learns from correction)
  - Re-photograph (delete item, upload new receipt photo)
- Category assignment preserves source info (auto-created, user-created, user-modified)

### Data Quality & OCR
- PaddleOCR extracts text + region coordinates from receipt photos
- Raw OCR stored alongside user-corrected version (audit trail)
- Confidence scoring per item
- Low-confidence items (< threshold) tagged and must be resolved before data is "clean"
- Photo metadata stored: original filename, compressed size, retention schedule
- Photos retained for 6 months (user configurable, can mark "never delete")
- Region-of-interest cropping available for user reference (resolve disputes)

### Smart Budgeting & Alerts
- Global monthly budget setting (total spending cap)
- Global monthly savings target (amount user wants to save each month)
- Per-category budgets (LLM-suggested or user-set, can be "unlimited")
- Budget history tracking (see all budget changes over time)
- LLM analyzes spending patterns and periodically proposes budget adjustments
  - "Your clothing has been 15 CHF/mo over budget. Increase budget from X to Y?"
  - "You're consistently saving above target. Consider raising savings goal?"
  - All suggestions respect global budget + savings constraints
- Persistent sidebar notification system (banking app style):
  - Small icon at bottom right (status: green/yellow/red)
  - Expandable list of actionable alerts
  - Budget overspend warnings (category-level and global)
  - Dismissible notifications
  - Action buttons (approve budget change, snooze, mark as paid, etc.)
  - Notification history view
- Real-time alert recalculation (on new transaction)
- Alert severity levels (info, warning, critical)

### Advanced Insights & Analytics
- Trend analysis across all time horizons (day, week, month, quarter, year, all time, custom range)
- Time period comparison (this month vs. last month)
- Per-shop tracking and trends (spending per shop, shop loyalty analysis)
- Per-item trends ("Pain au Chocolat consumption up 20% vs. last month")
- Category vs. budget comparison (progress bars, variance display)
- Savings rate tracking and visualization
- Custom labels for context ("beer for friend — refund pending", "borrowed from Mom", etc.)
- Code-as-policy (future Phase 4):
  - LLM can call sandbox agent to generate custom data views
  - User writes (or LLM suggests) lightweight scripts in custom DSL
  - Examples: "Show correlation between coffee spending and work days"
  - Scripts persist as "saved views"
  - Scratch pad for testing view scripts before saving
  - Async execution (LLM doesn't block UI)

### Recurring Transactions & Subscriptions
- **Automatic detection (LLM-based):**
  - Analyzes all transactions to detect recurring patterns
  - Identifies recurring charges (e.g., "mobile subscription on 1st of month")
  - Creates suggestions for user confirmation
  - Detects anomalies ("Mobile subscription usually on 1st, but missing this month")
  - Flags overdue subscriptions
- **Manual recurring rules:**
  - User can manually create subscription/recurring rules
  - Specify: name, amount, frequency (daily/weekly/monthly/yearly/custom), expected date, category
  - Mark as active/paused/settled
- **Subscriptions management page:**
  - Unified view: LLM-detected + user-created + user-modified subscriptions
  - Display: name, amount, frequency, next due date, category, source badge (visual distinction)
  - Row actions: edit, pause/activate, mark as paid, delete
  - Total monthly recurring spending display
  - Sortable and filterable by category, status, source
  - Last payment date and next expected date display
  - Missing payment alerts with "mark as paid" or "snooze" options

### Debt Tracking
- Separate debt category (tracks borrowed/lent money)
- Track who you owe, who owes you, amounts, due dates
- Metadata per debt: person name, phone number, description
- Status tracking: pending, partially paid, settled, overdue
- Debt management:
  - Record partial payments (running balance)
  - Mark as settled
  - Archive old debts
  - Phone number linking (click to call, SMS option)
  - Payment history per debt
- Custom labels integration ("borrowed from [person]", "lent to [person]")
- LLM can suggest debt categorization ("This looks like borrowed money. Add to debt tracking?")
- Link debts to corresponding expense entries (audit trail)
- Two-tab view: "I owe" | "Owed to me"
- Total owed/lent summary display

### Queue Management & Infrastructure
- **Raspberry Pi queue server:**
  - Receives photos via HTTP endpoint
  - Stores queue in persistent filesystem
  - Duplicate detection (hash-based, before sending to desktop)
  - Simple `/status` endpoint (returns queue length)
  - Authentication via API key
  - Temporary photo storage (cleared after processing)

- **Desktop daemon:**
  - Polls Raspberry Pi queue when GPU is available
  - Lazy processing (respects GPU load, doesn't hog resources)
  - Processes with local LLM + PaddleOCR
  - Results sent back to GUI for review/approval
  - Async queue processing (doesn't block user interface)
  - CLI command: `phoskonomia process-queue` (manual trigger)

- **Notifications:**
  - notify-send alerts: "N receipts in queue. Process now?"
  - Platform-specific: Linux (notify-send), macOS (NSUserNotification), Windows (toast notifications)
  - Optional: sound/visual effects

- **Edge-AI (experimental, optional):**
  - Lightweight models on Raspberry Pi (Gemma 4:e2b alternatives)
  - Initial OCR + rough category guessing on Pi
  - Photo compression and deduplication on Pi
  - GPU load monitoring (don't process if desktop GPU in use)
  - Optional toggle (can disable if full-processing is fast enough)

### Export & Reporting
- **CSV export:** All transactions or filtered subset, column selection
- **JSON export:** Complete data dump with schema versioning
- **PDF export:** Professional reports (monthly, quarterly, annual summaries)
  - Spending charts embedded
  - Category breakdown tables
  - Budget comparison
  - Savings rate analysis
  - Year-over-year comparisons
- Timestamp and Phoskonomia version in all exports
- Data can be filtered before export (date range, category, shop)

### Handles Everything
- Groceries and food (primary use case)
- Subscriptions and recurring charges (streaming, software, utilities)
- Insurance premiums
- Utilities (water, electricity, internet)
- Debts and loans
- One-time purchases (electronics, furniture, services)
- Travel and transportation
- Work-related expenses
- Gifts and personal items
- Any expense category user defines

### Settings & Administration
- Global budget setting (monthly)
- Global savings target (monthly)
- Currency setting (CHF for now, expandable to multi-currency)
- Photo retention days (default 6 months, user configurable)
- Per-category budget management (set, edit, delete)
- Category management (colors, icons, merging, splitting)
- Preset category templates (for quick setup)
- LLM model selection (show currently running model)
- OCR model info and settings
- Auto-category creation toggle
- Database management (size, backup, restore)
- Export all data option
- Cache clearing
- Edge-AI toggle (if available)
- Debug mode (show LLM prompts, OCR output, etc.)
- Dark mode toggle
- Notification preferences

### User Interface
- Primary: GUI via Dioxus (web/desktop/native via Tauri)
- Secondary: CLI for scripting and batch operations
- Dark mode default (high contrast, professional financial UX)
- Responsive design (desktop/tablet/mobile compatible)
- Keyboard navigation fully supported
- WCAG AA accessibility compliance
- Smooth animations and transitions
- Hover states and focus indicators
- Toast notifications for transient updates
- Modal interactions with focus trapping
- Breadcrumb navigation for nested views

### Data Storage & Privacy
- **Local-first:** All data stored on user's machine (SQLite database)
- **No cloud sync required** (but infrastructure ready for optional backup)
- Photo retention with automatic deletion (6-month default)
- Regional photos preserved for 6 months (for user verification)
- Photo metadata stripping (privacy-first)
- Audit trail of all corrections (user maintains full data history)
- Optional: encrypted backups
- Optional: database encryption (SQLCipher)

### Platform & Tech Stack
- Built with **Dioxus 0.7** fullstack (web + desktop from same codebase)
- **Dioxus desktop** for native Linux app (no Tauri)
- **Cargo workspace** with adapter layer crate for dependency injection
- **LLM:** Primary: Ollama (Gemma4 local). Fallback: OpenCode Go (deepseek-v4).
  Adapter pattern enables any provider.
- **OCR:** PaddleOCR via HTTP microservice. Vision-LLM alternative under evaluation.
- **Database:** SurrealDB (embedded document-graph, Rust-native, accessed via adapter trait)
- **Tokio** async runtime (non-blocking processing)
- **thiserror** for domain error types
- **Storage:** Encrypted local filesystem for photos. EXIF stripped.
- **Notifications:** notify-send (Linux), dependency-injected for future platforms
- **Export:** CSV (built-in), JSON (serde), PDF (library TBD)

---

## Phase 1: Core Receipt Processing & Categorization

### Data Model & Schema

#### Database Schema Design
- [ ] Design receipt table schema (id, date, shop_name, total_amount, photo_path, photo_retention_days, raw_ocr_json, processing_status, created_at, updated_at)
- [ ] Design item table schema (id, receipt_id, name_ocr, name_corrected, unit_price, quantity, category_ids, custom_labels, confidence_score, source, low_confidence_flag, created_at, updated_at)
- [ ] Design category table schema (id, name, color, icon, auto_created, created_by_llm_on_date, parent_category_id, created_at, updated_at)
- [ ] Design item_category_mapping table (item_id, category_id, is_primary)
- [ ] Design user_correction table (id, item_id, field_changed, original_value, corrected_value, user_explanation, created_at)
- [ ] Design photo_metadata table (id, receipt_id, original_filename, compressed_size_bytes, deletion_date_scheduled, keep_forever_flag, created_at)
- [ ] Design budget_config table (id, category_id, monthly_budget_chf, is_global, created_at, updated_at)
- [ ] Create SQLite database initialization script
- [ ] Write SQL migrations for schema versioning
- [ ] Test schema with sample data

#### SQLite Setup
- [ ] Install rusqlite or sqlx dependency
- [ ] Create database connection pool
- [ ] Write database initialization function (create tables if not exist)
- [ ] Implement connection retry logic
- [ ] Add database backup routine (daily snapshots)

#### Receipt Storage
- [ ] Implement photo compression (JPEG, configurable quality)
- [ ] Create photo storage directory structure
- [ ] Implement photo retention logic (6 months default, user configurable)
- [ ] Create function to mark photos as "never delete"
- [ ] Implement automatic photo deletion on schedule
- [ ] Store photo metadata (original path, compressed size, retention schedule)

#### Item Structure
- [ ] Create Item struct with all fields (name, price, quantity, categories, labels, confidence, source)
- [ ] Implement multi-category support (item can belong to multiple categories)
- [ ] Add custom_labels field type (flexible key-value or string array)
- [ ] Implement confidence_score as float (0-1)
- [ ] Add source enum (photo_ocr, manual, csv_import, pdf, llm_inferred)
- [ ] Implement low_confidence_flag tracking

#### Category System
- [ ] Create Category struct (name, color, icon, auto_created flag)
- [ ] Implement auto-creation logic (triggered by LLM suggestion)
- [ ] Create merge_categories function (retroactively applies to all past items)
- [ ] Create split_categories function (splits one category into multiple)
- [ ] Implement category hierarchy (parent/child relationships, optional)
- [ ] Create toggle for automatic category creation (settings)
- [ ] Create admin panel data structure (default/preset categories)
- [ ] Implement category color/icon picker UI data

### Receipt Processing Pipeline

#### Input Sources
- [ ] Implement photo upload via Dioxus file picker
- [ ] Validate photo formats (JPG, PNG, WEBP)
- [ ] Compress photo on client before upload
- [ ] Implement manual item entry form (name, price, quantity)
- [ ] Create CSV import parser (configurable column mapping)
- [ ] Implement CSV preview/validation before import
- [ ] Create PDF invoice parser (extract tables, metadata)
- [ ] Implement drag-and-drop for file uploads

#### Raspberry Pi Queue Server
- [ ] Create HTTP server on Raspberry Pi (Axum or Actix)
- [ ] Implement `/upload` endpoint (receives photos)
- [ ] Create queue storage (filesystem or simple database)
- [ ] Implement queue persistence (survives server restart)
- [ ] Add duplicate detection (compare photo hashes)
- [ ] Create `/status` endpoint (returns queue length)
- [ ] Implement authentication/API key for desktop client
- [ ] Add photo storage on Pi (temporary, cleared after processing)
- [ ] Create cleanup routine for old queue files

#### Desktop Daemon
- [ ] Create Rust daemon process (runs at startup)
- [ ] Implement queue polling loop (configurable interval)
- [ ] Add GPU availability detection (check nvidia-smi or equivalent)
- [ ] Implement lazy processing (respects GPU load, doesn't hog resources)
- [ ] Create connection to Raspberry Pi queue server
- [ ] Implement photo download from queue
- [ ] Add error handling for network failures
- [ ] Create logging for processing events
- [ ] Implement daemon restart capability

#### OCR Integration (PaddleOCR)
- [ ] Install PaddleOCR (Python wrapper or Rust binding)
- [ ] Create OCR processing function (takes photo, returns detected text + regions)
- [ ] Implement region-of-interest cropping (coordinates returned by PaddleOCR)
- [ ] Add confidence scoring for each detected text region
- [ ] Create JSON output format (text, position, confidence)
- [ ] Store raw OCR output in database
- [ ] Implement OCR caching (don't re-process same photo)
- [ ] Add multi-language support (if needed)

#### LLM Integration (Local - Ollama)
- [ ] Install Ollama and pull model (Llama 3 / Mistral)
- [ ] Create LLM connection handler
- [ ] Implement prompt engineering for item extraction
- [ ] Create prompt for category suggestion
- [ ] Implement confidence scoring logic (from LLM output)
- [ ] Add shop name recognition prompt
- [ ] Create retry logic for LLM failures
- [ ] Implement token limit handling (large receipts)
- [ ] Add response parsing and validation

#### Processing Flow
- [ ] Implement receipt creation in database
- [ ] Create OCR processing step (PaddleOCR on photo)
- [ ] Implement LLM prompt for item extraction (prices, names, quantities)
- [ ] Create LLM category suggestion step
- [ ] Implement confidence scoring per item
- [ ] Create database insertion for extracted items
- [ ] Implement notification to GUI (receipt ready for review)
- [ ] Create user review UI (show extracted items)
- [ ] Implement user approval/editing step
- [ ] Create final data persistence after user approval

#### Low-Confidence Handling
- [ ] Implement low_confidence_flag on items (threshold: <0.7 confidence)
- [ ] Add triangle icon UI component for warning display
- [ ] Create low-confidence item list view
- [ ] Implement re-photograph option (delete item, upload new photo)
- [ ] Create direct edit mode (user changes text without LLM re-read)
- [ ] Implement explanation mode (user explains error, LLM learns from it)
- [ ] Store correction explanations in user_correction table
- [ ] Create UI notification that item is "unresolved" until fixed
- [ ] Implement resolution confirmation (user marks as okay)

#### Queue Management
- [ ] Create queue status UI (shows N receipts waiting)
- [ ] Implement notify-send integration (Linux notifications)
- [ ] Add macOS notifications (NSUserNotification or equivalent)
- [ ] Add Windows notifications (toast notifications)
- [ ] Create CLI command: `phoskonomia process-queue`
- [ ] Implement manual processing trigger from GUI
- [ ] Add queue priority system (user can prioritize certain receipts)
- [ ] Create progress indicator during processing
- [ ] Implement background processing without blocking UI
- [ ] Create processing speed metrics (items/sec)

---

## Phase 2: Budgeting & Smart Insights

### Budget System

#### Database Schema
- [ ] Add budget_config table (category_id, monthly_budget_chf, is_global, created_at, updated_at)
- [ ] Add global_settings table (global_monthly_budget, global_monthly_savings_target, updated_at)
- [ ] Add budget_history table (id, category_id, previous_budget, new_budget, llm_suggested, user_accepted, timestamp)

#### Budget Configuration
- [ ] Create global budget input form (initial setup)
- [ ] Create global savings target input form (initial setup)
- [ ] Implement per-category budget setter (UI form)
- [ ] Add "none" option for unlimited category budgets
- [ ] Implement category budget display in category management
- [ ] Create budget edit function
- [ ] Store budget history for tracking changes over time
- [ ] Implement currency formatting (CHF display)

#### LLM Budget Analysis & Suggestions
- [ ] Create LLM prompt for spending pattern analysis
- [ ] Implement function to calculate per-category spending (last 30 days)
- [ ] Create comparison logic (actual vs. budgeted)
- [ ] Implement suggestion generation prompt (overspend detection)
- [ ] Add savings rate calculation (income estimate or historical average)
- [ ] Create LLM prompt for savings suggestions
- [ ] Implement periodic suggestion trigger (e.g., weekly/monthly)
- [ ] Add human-readable suggestion formatting
- [ ] Create suggestion storage in database
- [ ] Implement suggestion age tracking (don't repeat same suggestion)

#### Notification Sidebar
- [ ] Design sidebar component layout (TBD in wireframe)
- [ ] Create budget status icon (on-budget, warning, over-budget)
- [ ] Implement notification list rendering
- [ ] Add dismissible notification UI (X button)
- [ ] Create expandable details panel
- [ ] Implement actionable notifications (approve/reject buttons)
- [ ] Add notification persistence (save dismiss state)
- [ ] Create notification timestamp display
- [ ] Implement notification badges (unread count)
- [ ] Add notification sound/visual effects (optional)
- [ ] Create notification history view

#### Budget Alerts
- [ ] Create alert trigger for category overspend (>100% of budget)
- [ ] Create warning trigger for category at-risk (>80% of budget)
- [ ] Create alert trigger for global budget overspend
- [ ] Create alert trigger for savings target at-risk
- [ ] Implement real-time alert recalculation (on new transaction)
- [ ] Add alert severity levels (info, warning, critical)
- [ ] Create alert dismissal functionality
- [ ] Implement alert re-trigger if condition persists

### Transaction Views & Filtering

#### Main Transaction List UI
- [ ] Create transaction list component
- [ ] Display columns: date, shop, total, category tags
- [ ] Implement row hover styling
- [ ] Create "Details" button per row
- [ ] Add category tag visualization (colored pills/badges)
- [ ] Implement click handler for Details button
- [ ] Add sorting options (by date, amount, shop, category)
- [ ] Create filtering sidebar (not detailed in Phase 2, but prepare structure)
- [ ] Implement pagination or infinite scroll
- [ ] Add empty state UI (no transactions)

#### Receipt Detail View
- [ ] Create detail modal/panel component
- [ ] Display all items from receipt (name, price, quantity, category)
- [ ] Show receipt metadata (date, shop, total, photo)
- [ ] Implement item editing UI (clickable fields)
- [ ] Create category tag UI on items
- [ ] Add "low-confidence" indicator on problematic items
- [ ] Create custom label display
- [ ] Implement back button to transaction list
- [ ] Add close modal functionality

#### Filtered Transaction View
- [ ] Create filtered list component (reuse main list layout)
- [ ] Implement category filter (click item category from receipt detail)
- [ ] Add shop filter functionality
- [ ] Implement time period filter (day, week, month, quarter, year, custom)
- [ ] Create filter combination logic (category + shop + period)
- [ ] Display filtered totals (sum of all items in filter)
- [ ] Create breadcrumb navigation (show active filters)
- [ ] Add clear filters button
- [ ] Implement filter persistence (remember last filter)
- [ ] Create "return to main view" link

#### Time Horizon Selection
- [ ] Create time period selector component (dropdown or tabs)
- [ ] Implement presets: today, last 7 days, this month, last 30 days, this year, all time
- [ ] Add custom date range picker (calendar)
- [ ] Implement date validation (end > start)
- [ ] Create time period display in filtered view header
- [ ] Add quick-select buttons for common ranges
- [ ] Implement period comparison mode (this month vs. last month)

#### Sorting & Filtering
- [ ] Implement sort by date (ascending/descending)
- [ ] Implement sort by amount (ascending/descending)
- [ ] Implement sort by shop (alphabetical)
- [ ] Implement sort by category
- [ ] Create sort indicator UI (arrow icon, selected state)
- [ ] Add multi-sort capability (secondary sort)
- [ ] Implement filter by shop (dropdown or checkboxes)
- [ ] Implement filter by category (checkboxes)
- [ ] Create filter reset functionality
- [ ] Add persistent sort/filter preferences (localStorage or database)

---

## Phase 3: Recurring Transactions & Debt Tracking

### Recurring Transaction Detection

#### Database Schema
- [ ] Create recurring_rule table (id, category_id, name, amount_chf, frequency, expected_day_of_month, is_user_created, created_date, last_detected_date, next_expected_date)
- [ ] Create recurring_rule_history table (id, rule_id, detected_date, matched_transaction_id, is_anomaly, user_confirmed)

#### Automatic Detection (LLM-based)
- [ ] Create LLM prompt for recurring pattern analysis
- [ ] Implement function to scan all transactions (last 3 months minimum)
- [ ] Add pattern recognition logic (same amount, same date or interval)
- [ ] Implement merchant/shop name matching for same vendor
- [ ] Create recurring rule suggestion function
- [ ] Store detected patterns in database
- [ ] Create suggestion confidence scoring (0-1 based on consistency)
- [ ] Implement anomaly detection (expected recurring charge missing)
- [ ] Create LLM prompt for anomaly alerts
- [ ] Add recurring detection trigger (on new transaction, scan for patterns)
- [ ] Implement batched detection (run on schedule, not on every transaction)

#### Manual Recurring Rules
- [ ] Create recurring rule creation form
- [ ] Implement frequency selector (daily, weekly, monthly, yearly, custom)
- [ ] Add amount input field (CHF)
- [ ] Implement name/description field
- [ ] Create expected date selector (day of month, day of week, etc.)
- [ ] Add category assignment to rule
- [ ] Implement rule activation/deactivation toggle
- [ ] Create rule editing form
- [ ] Implement rule deletion (with confirmation)
- [ ] Store user-created flag in database

#### Subscriptions Management Page
- [ ] Create subscriptions list view
- [ ] Display columns: name, amount, frequency, category, source (icon/label)
- [ ] Implement visual distinction (LLM vs. user created vs. user-modified)
- [ ] Create row expansion (show more details)
- [ ] Add edit button per subscription
- [ ] Add delete button per subscription (with confirmation)
- [ ] Implement deactivate/reactivate toggle
- [ ] Create total monthly recurring spending display
- [ ] Add filter by category
- [ ] Create sort options (by name, amount, next date, source)
- [ ] Implement search/filter by subscription name
- [ ] Add "add new subscription" button (opens creation form)
- [ ] Create last payment date display
- [ ] Implement next expected date display
- [ ] Add missing payment indicator (overdue flag)

#### Anomaly Alerts
- [ ] Create alert generation for missing recurring charge
- [ ] Implement alert text: "Mobile subscription usually on 1st, but missing this month. Forgotten?"
- [ ] Add snooze functionality (remind in N days)
- [ ] Create mark-as-paid option (user paid manually)
- [ ] Implement skip option (intentionally skipped this month)
- [ ] Create alert history view
- [ ] Add "never alert for this subscription" toggle
- [ ] Implement alert persistence (survives until user resolves)

#### LLM Integration for Recurring
- [ ] Create system prompt for recurring analysis
- [ ] Implement pattern detection prompt
- [ ] Create anomaly detection prompt
- [ ] Add confidence scoring logic
- [ ] Implement prompt for recurring rule merging (e.g., Netflix Premium + Netflix are same rule)
- [ ] Add human-friendly suggestion formatting

### Debt Tracking

#### Database Schema
- [ ] Create debt table (id, person_name, phone_number, amount_chf, debt_type, status, due_date, created_at, updated_at)
- [ ] Create debt_category_mapping table (debt_id, category_id)
- [ ] Create debt_history table (id, debt_id, action, amount_change, timestamp, notes)

#### Debt Entry UI
- [ ] Create debt creation form
- [ ] Implement person name input
- [ ] Add phone number input (optional, validated)
- [ ] Implement amount input (CHF)
- [ ] Create debt type selector (borrowed_from, lent_to)
- [ ] Add due date picker (optional)
- [ ] Implement description field (notes, reason)
- [ ] Create category assignment (single or multiple)
- [ ] Add form submission and validation

#### Debt List View
- [ ] Create debts list component
- [ ] Display columns: person, amount, type, status, due date, categories
- [ ] Add row styling (color-coded by status: pending, overdue, settled)
- [ ] Create edit button per debt
- [ ] Add delete button per debt
- [ ] Implement mark-as-settled functionality
- [ ] Create partial payment tracking (show amount remaining)
- [ ] Add sorting options (by amount, due date, person name)
- [ ] Implement filter by debt type (owed to me vs. I owe)
- [ ] Create total owed/lent display
- [ ] Add search by person name
- [ ] Create "add new debt" button

#### Debt Management
- [ ] Implement mark-as-paid function
- [ ] Create partial payment recording (user paid $N, remaining $Y)
- [ ] Implement payment history view per debt
- [ ] Add reminder functionality (notify on due date)
- [ ] Create phone number linking (click to call, SMS option)
- [ ] Implement status update (pending → settled → archived)
- [ ] Create debt archival (hide old settled debts)
- [ ] Add search/filter by person
- [ ] Implement due date sorting
- [ ] Create overdue debt highlighting

#### Integration with Expenses
- [ ] Create custom label type: "borrowed from [person]" / "lent to [person]"
- [ ] Implement LLM suggestion detection: "This looks like borrowed money. Add to debt tracking?"
- [ ] Create link between expense item and debt record
- [ ] Implement auto-creation of debt from expense (user confirmation)
- [ ] Add reverse: link debt to corresponding expense entry
- [ ] Display debt reference in transaction detail view

---

## Phase 4: Advanced Analytics & Code-as-Policy

### Flexible Trend Analysis

#### Built-in Chart Views
- [ ] Create line chart component (spending over time by category)
- [ ] Create bar chart component (category comparison)
- [ ] Create pie chart component (budget breakdown)
- [ ] Create stacked bar chart (trends by shop)
- [ ] Create trend visualization (spending trajectory)
- [ ] Implement chart time period selector (day/week/month/quarter/year/all)
- [ ] Add chart export to PNG/SVG
- [ ] Create multiple chart display (dashboard with 2-4 charts)
- [ ] Implement chart filtering (by category, shop, custom filter)
- [ ] Add Y-axis auto-scaling
- [ ] Create legend with category colors
- [ ] Implement hover tooltips (show exact values)

#### Category Spending Over Time
- [ ] Create query function (sum spending by category per period)
- [ ] Implement multi-category display (show multiple categories on one chart)
- [ ] Add category toggle (show/hide individual categories)
- [ ] Create comparison mode (this month vs. last month)
- [ ] Implement moving average line (30-day smoothing)
- [ ] Add budget overlay line (show budgeted amount)
- [ ] Create goal line (savings target)
- [ ] Implement outlier highlighting (unusual spending spike)

#### Shop Spending Trends
- [ ] Create query function (sum spending by shop per period)
- [ ] Implement shop selection (dropdown or checkboxes)
- [ ] Create top-N shops ranking
- [ ] Add shop comparison view
- [ ] Implement shop loyalty tracking (how much spent per shop)
- [ ] Create shop frequency view (visits per month)
- [ ] Add average transaction size per shop

#### Item-Level Trends
- [ ] Create query function (sum spending per item/category per period)
- [ ] Implement item trend visualization
- [ ] Add "pain au chocolat consumption rising/falling" analysis
- [ ] Create growth percentage display
- [ ] Implement volatility metric (consistency of spending)
- [ ] Add item popularity ranking (top spending items)
- [ ] Create item budget comparison (budgeted vs. actual)

#### Category vs. Budget Comparison
- [ ] Create side-by-side view (budgeted amount vs. actual)
- [ ] Implement variance visualization (over/under display)
- [ ] Add percentage utilization (actual / budget)
- [ ] Create traffic light indicator (green/yellow/red)
- [ ] Implement drill-down (click category to see transactions)

#### Savings Rate Tracking
- [ ] Implement savings calculation (income estimate or input)
- [ ] Create savings rate chart over time
- [ ] Add savings goal line
- [ ] Implement savings progress display
- [ ] Create savings rate variance analysis

### Code-as-Policy (Experimental Feature)

#### Sandbox Agent Infrastructure
- [ ] Create sandboxed execution environment (WebAssembly or subprocess)
- [ ] Implement security restrictions (no filesystem write, no network access)
- [ ] Create timeout mechanism (max execution time 10 seconds)
- [ ] Add resource limits (memory, CPU)
- [ ] Implement error handling and crash recovery
- [ ] Create sandbox initialization/teardown

#### View Script DSL / API
- [ ] Define data query language (SQL-like or custom)
- [ ] Create built-in functions:
  - [ ] sum_by_category(period) → list of (category, amount)
  - [ ] sum_by_shop(period) → list of (shop, amount)
  - [ ] get_items(filter) → list of items matching filter
  - [ ] get_spending_trend(category, period) → time series
  - [ ] calculate_percentile(category, value) → percentile rank
  - [ ] detect_anomalies(category, method) → list of outliers
- [ ] Add visualization functions:
  - [ ] chart_line(data, title)
  - [ ] chart_bar(data, title)
  - [ ] chart_pie(data, title)
  - [ ] table(data, columns)
- [ ] Create aggregation functions (mean, median, stddev, min, max)

#### LLM Script Generation
- [ ] Create LLM prompt for script generation ("Show me correlation between coffee and work days")
- [ ] Implement script writing prompt (outputs executable code in DSL)
- [ ] Add script validation before execution
- [ ] Create error recovery (suggest fixes if script fails)
- [ ] Implement natural language to query translation

#### Scratch Pad
- [ ] Create script editing interface (text editor)
- [ ] Implement syntax highlighting (if DSL is code-like)
- [ ] Add run button (execute script in sandbox)
- [ ] Create result display panel (shows chart/table output)
- [ ] Implement error display (show runtime errors)
- [ ] Add undo/redo functionality
- [ ] Create script validation (before execution)
- [ ] Implement auto-save drafts

#### Saved Views
- [ ] Create view persistence (save successfully executed scripts)
- [ ] Implement view naming
- [ ] Create view library (list of saved views)
- [ ] Add view tags/categories
- [ ] Implement view sharing (export as JSON)
- [ ] Create view editing
- [ ] Implement view deletion
- [ ] Add view scheduling (auto-run daily/weekly, show results)
- [ ] Create view templating (generate variants)

#### Advanced Analytics Queries (LLM-driven)
- [ ] Create prompt: "Show me correlation between category A and category B"
- [ ] Create prompt: "Detect anomalies in [category]"
- [ ] Create prompt: "Alert me if [category] exceeds 2σ from mean"
- [ ] Create prompt: "Show spending by day of week"
- [ ] Create prompt: "Compare my spending to historical average"
- [ ] Create prompt: "What categories co-occur (often bought together)?"
- [ ] Create prompt: "Seasonal analysis (spending by season)"
- [ ] Create prompt: "Price sensitivity analysis (do you buy more when on sale?)"

---

## Phase 5: Multi-Currency & Advanced Imports

### Multi-Currency Support

#### Database Schema
- [ ] Extend item table with currency field (original_currency, default CHF)
- [ ] Create exchange_rate_cache table (currency_pair, rate, timestamp)
- [ ] Add currency settings table (display_currency, auto_convert)

#### Currency Detection
- [ ] Implement currency detection from OCR (€, $, £, etc.)
- [ ] Create manual currency override on item entry
- [ ] Implement currency validation (check if valid ISO 4217 code)
- [ ] Add fallback currency (default to CHF if unknown)

#### Exchange Rate Integration
- [ ] Choose exchange rate API (ECB, Fixer, OpenExchangeRates, etc.) — TBD
- [ ] Create API client for exchange rate fetching
- [ ] Implement rate caching (store historical rates)
- [ ] Add automatic rate update (fetch at transaction time)
- [ ] Create fallback rate (use most recent cached rate if API fails)
- [ ] Implement offline rate handling (work without network)
- [ ] Add historical rate tracking (preserve conversion rate used)

#### Multi-Currency Calculation
- [ ] Create conversion function (amount × exchange_rate)
- [ ] Implement CHF-equivalent storage (amount_chf = amount × rate)
- [ ] Add conversion display (show original currency + CHF)
- [ ] Create currency formatting per item (symbol display)
- [ ] Implement summary calculation in multiple currencies
- [ ] Add conversion error handling (API failures, invalid currencies)

#### Multi-Currency Reporting
- [ ] Create view option: display in original currency
- [ ] Create view option: display in CHF
- [ ] Implement per-shop reporting (sum by currency)
- [ ] Add currency breakdown (total by currency)
- [ ] Create conversion rate transparency (show rates used)
- [ ] Implement trend charts in multiple currencies
- [ ] Add currency conversion history view

### Advanced CSV Imports

#### CSV Parser & Mapper
- [ ] Create CSV file detector (validate CSV format)
- [ ] Implement column header detection (auto-detect date/amount/description)
- [ ] Create manual column mapper UI (user selects column → field mapping)
- [ ] Add preview table (show first N rows after mapping)
- [ ] Implement validation (check required fields present)
- [ ] Create delimiter detection (comma, semicolon, tab)
- [ ] Add encoding detection (UTF-8, Latin-1, etc.)

#### Bank CSV Imports
- [ ] Implement Swiss bank format parser (UBS, Credit Suisse, Raiffeisen, etc.)
- [ ] Create date format detection (DD.MM.YYYY, etc.)
- [ ] Implement amount parsing (negative = expense, positive = income)
- [ ] Create description extraction (from transaction notes)
- [ ] Add merchant/shop name extraction from description
- [ ] Implement duplicate detection (compare date + amount + description)
- [ ] Create transaction matching (link CSV import to receipts)

#### Spreadsheet Imports
- [ ] Create generic spreadsheet mapper (date, amount, category, description)
- [ ] Implement row validation (check required fields)
- [ ] Add category pre-assignment (from spreadsheet column)
- [ ] Create import preview (show what will be imported)
- [ ] Implement bulk import (multiple rows at once)
- [ ] Add import history tracking

#### LLM Enrichment of Imports
- [ ] Create LLM prompt: extract shop name from transaction description
- [ ] Implement category suggestion for imported transactions
- [ ] Create merchant standardization (map "MC DONALDS" → "McDonald's")
- [ ] Add confidence scoring for LLM enrichment
- [ ] Implement async enrichment (don't block import)

### PDF Invoice Parsing

#### PDF Text Extraction
- [ ] Choose PDF library (pdfminer, pypdf, etc.)
- [ ] Create PDF text extraction function
- [ ] Implement PDF table detection
- [ ] Add OCR fallback (if PDF is image-based)

#### Invoice Data Extraction
- [ ] Create LLM prompt for invoice parsing
- [ ] Implement invoice date detection
- [ ] Add vendor/company name extraction
- [ ] Create total amount extraction
- [ ] Implement line-item table detection
- [ ] Add item name extraction from tables
- [ ] Create unit price extraction
- [ ] Implement quantity extraction
- [ ] Add tax calculation/extraction
- [ ] Create confidence scoring per field

#### Subscription Invoice Handling
- [ ] Detect recurring invoice patterns (same vendor, similar amounts)
- [ ] Create recurring rule auto-creation from invoice
- [ ] Implement invoice storage (link to recurring rule)
- [ ] Add monthly billing date detection
- [ ] Create subscription frequency suggestion

#### Invoice Management
- [ ] Create invoice file storage
- [ ] Implement invoice archival by vendor
- [ ] Add invoice search (by vendor, date range)
- [ ] Create invoice preview (show extracted data)
- [ ] Implement invoice linking to recurring rules

---

## Phase 6: Edge-AI & Experimental Features

### Edge-AI Pre-processing (Experimental)

#### Lightweight Model Selection
- [ ] Research Gemma 4:e2b and similar lightweight models
- [ ] Benchmark models for Raspberry Pi (memory, latency, accuracy)
- [ ] Test model compatibility with arm64 architecture
- [ ] Create model download/installation script
- [ ] Implement model caching (persist on Pi)

#### Lightweight OCR on Pi
- [ ] Install lightweight OCR option (TinyOCR, MicroOCR alternatives)
- [ ] Create OCR inference on Pi
- [ ] Implement region-of-interest compression
- [ ] Add confidence thresholding (only send high-confidence regions)
- [ ] Create OCR timing metrics
- [ ] Implement fallback (if Pi OCR fails, send raw photo to desktop)

#### Rough Category Guessing
- [ ] Implement lightweight category classifier on Pi
- [ ] Create category prediction from item name
- [ ] Add confidence scoring
- [ ] Implement batch processing (multiple items at once)
- [ ] Create category prediction caching

#### Queue Optimization
- [ ] Implement duplicate photo detection (hash-based)
- [ ] Create photo metadata stripping (remove EXIF for privacy)
- [ ] Add lossless compression on Pi
- [ ] Implement queue deduplication (before sending to desktop)
- [ ] Create queue priority system (low-confidence items first)

#### Load Monitoring
- [ ] Create GPU monitoring function (nvidia-smi parsing)
- [ ] Implement desktop availability detection
- [ ] Add lazy processing trigger (only when GPU free)
- [ ] Create desktop availability polling
- [ ] Implement backoff strategy (don't hammer desktop with requests)

#### Experimental Toggle
- [ ] Create settings UI to enable/disable edge-AI
- [ ] Implement feature flag system
- [ ] Add performance metrics display (time saved by edge-AI)
- [ ] Create fallback behavior (if edge-AI disabled)
- [ ] Implement A/B testing capability (edge-AI vs. full-processing)

### Storage & Optimization

#### Photo Compression
- [ ] Implement JPEG quality optimization (80-90% quality)
- [ ] Add WebP support (better compression)
- [ ] Create lossless PNG option (for important receipts)
- [ ] Implement automatic format selection (balance quality/size)
- [ ] Add resolution downsampling (if too large)

#### Database Optimization
- [ ] Create query indexing (frequent searches: date, category, shop)
- [ ] Implement database cleanup (remove orphaned records)
- [ ] Add VACUUM routine (SQLite optimization)
- [ ] Create backup scheduling (daily snapshots)
- [ ] Implement incremental backups

#### Cloud Backup (Optional)
- [ ] Create export-to-cloud option (encrypted)
- [ ] Implement symmetric encryption (user password-based)
- [ ] Add backup scheduling
- [ ] Create backup restoration function
- [ ] Implement conflict resolution (if multiple clients)

---

## Technical Stack & Infrastructure

### Core Dependencies
- [ ] Initialize Dioxus project (web + desktop targets)
- [ ] Install Tauri (for native desktop wrapper)
- [ ] Add Tokio (async runtime)
- [ ] Add Axum or Actix (web server for Pi queue)
- [ ] Add rusqlite or sqlx (database)
- [ ] Add PaddleOCR Rust binding (or Python wrapper via subprocess)
- [ ] Add Ollama client library
- [ ] Add serde (JSON serialization)
- [ ] Add chrono (date/time handling)
- [ ] Add uuid (unique identifiers)

### Platform Support
- [ ] Test on Linux (primary development platform)
- [ ] Test on macOS (Tauri native build)
- [ ] Test on Windows (Tauri native build)
- [ ] Create platform-specific notification code (notify-send, NSUserNotification, toast)
- [ ] Test on Raspberry Pi (queue server only)
- [ ] Test GPU detection on NVIDIA, AMD, Intel Arc

### Deployment
- [ ] Create installation script (Linux)
- [ ] Create Tauri installer (macOS, Windows)
- [ ] Implement auto-updates
- [ ] Create uninstaller
- [ ] Document system requirements

---

## Export & Reporting

### CSV Export
- [ ] Create CSV export function (all transactions)
- [ ] Implement filtered CSV export (by category, period)
- [ ] Add column selection (user chooses which fields to export)
- [ ] Create CSV formatting (proper quoting, escaping)
- [ ] Add timestamp to export filename
- [ ] Implement export with exchange rates (if multi-currency)

### JSON Export
- [ ] Create JSON export function (all data)
- [ ] Implement JSON schema (define structure)
- [ ] Add versioning (schema version in export)
- [ ] Create pretty-printing option
- [ ] Implement filtered JSON export
- [ ] Add metadata (export date, Phoskonomia version)

### PDF Export
- [ ] Choose PDF library (printpdf, reportlab wrapper)
- [ ] Create PDF layout design
- [ ] Implement table rendering (transactions)
- [ ] Add chart embedding (spending charts)
- [ ] Create page breaks for long reports
- [ ] Implement PDF metadata (title, author)
- [ ] Add filtering to PDF export (date range, category)
- [ ] Create styled report format (professional appearance)

### Report Generation
- [ ] Create monthly report template
- [ ] Implement quarterly report template
- [ ] Create annual summary report
- [ ] Add category breakdown section
- [ ] Implement budget comparison section
- [ ] Create savings rate summary
- [ ] Add top spending categories ranking
- [ ] Implement year-over-year comparison
- [ ] Create custom report builder (select sections)

---

## Data Privacy & Security

### Local-First Principles
- [ ] Ensure no data sent to cloud without user consent
- [ ] Create audit log (what data was accessed when)
- [ ] Implement permission system (which app features need what data)
- [ ] Add data minimization (only store necessary fields)
- [ ] Create GDPR export function (all user data)

### Encryption (Optional)
- [ ] Add database encryption option (SQLCipher)
- [ ] Implement password protection
- [ ] Create master password hashing (Argon2)
- [ ] Add encrypted backups
- [ ] Implement encryption key management

### Sensitive Data Handling
- [ ] Remove phone numbers from logs
- [ ] Implement PII masking (in debug output)
- [ ] Create secure photo deletion (overwrite before delete)
- [ ] Add sensitive data redaction (from exports)

---

## Testing & Quality Assurance

### Unit Tests
- [ ] Test OCR parsing logic
- [ ] Test category matching/suggestion
- [ ] Test budget calculation
- [ ] Test CSV import parsing
- [ ] Test exchange rate conversion
- [ ] Test item confidence scoring
- [ ] Test database queries
- [ ] Test recurring pattern detection

### Integration Tests
- [ ] Test receipt upload → processing → storage flow
- [ ] Test queue management (Pi ↔ Desktop)
- [ ] Test LLM integration (prompt → response)
- [ ] Test category merge/split retroactivity
- [ ] Test import workflows (CSV, PDF)

### UI Tests
- [ ] Test transaction list rendering
- [ ] Test filter/sort functionality
- [ ] Test detail panel opening/closing
- [ ] Test form submission and validation
- [ ] Test notification sidebar interactions

### End-to-End Tests
- [ ] Test complete receipt upload workflow
- [ ] Test budget alert generation
- [ ] Test recurring rule detection
- [ ] Test debt tracking workflow
- [ ] Test export functionality

---

## Documentation

### User Documentation
- [ ] Create user manual (PDF)
- [ ] Write quick start guide
- [ ] Create FAQ section
- [ ] Document keyboard shortcuts
- [ ] Create troubleshooting guide
- [ ] Write budget setup tutorial
- [ ] Document custom label system
- [ ] Create recurring rule setup guide
- [ ] Write debt tracking tutorial
- [ ] Document code-as-policy examples

### Developer Documentation
- [ ] Create architecture diagram
- [ ] Document database schema
- [ ] Write LLM prompt specifications
- [ ] Document API endpoints (if any)
- [ ] Create contribution guidelines
- [ ] Write deployment guide
- [ ] Document configuration options
- [ ] Create troubleshooting guide for developers

### API Documentation
- [ ] Document LLM integration API
- [ ] Write Pi queue server API spec
- [ ] Document Dioxus component structure
- [ ] Create function signatures reference

---

## Performance & Optimization

### UI Responsiveness
- [ ] Implement virtual scrolling (large transaction lists)
- [ ] Add loading states (show progress during processing)
- [ ] Create skeleton screens (while data loads)
- [ ] Implement debouncing (search, filter inputs)
- [ ] Add request cancellation (user navigates away)

### Processing Performance
- [ ] Create batch processing (multiple receipts at once)
- [ ] Implement caching (OCR results, LLM responses)
- [ ] Add worker threads (don't block UI)
- [ ] Optimize database queries (use indexes)
- [ ] Profile code bottlenecks

### Memory Management
- [ ] Implement photo unloading (don't keep all in memory)
- [ ] Create LRU cache for LLM responses
- [ ] Optimize image compression
- [ ] Implement garbage collection awareness

---

## Known Issues & Future Enhancements

### TBD / Undecided
- [ ] Queue sync behavior when desktop goes offline
- [ ] Multi-currency exchange rate API source
- [ ] Code-as-policy DSL syntax
- [ ] Notification sidebar detailed design
- [ ] Cloud backup provider (if needed)
- [ ] Multi-device sync approach

### Future Enhancements (Not in Current Roadmap)
- [ ] Mobile app (separate from desktop)
- [ ] Collaborative budgeting (shared expenses)
- [ ] Receipt OCR training (improve model with user corrections)
- [ ] AI-powered spending coach (personalized advice)
- [ ] Receipt image compression improvements
- [ ] Integration with banks (real-time sync)
- [ ] Integration with crypto (track crypto spending)
- [ ] Automated tax report generation
- [ ] Receipt sharing (send to friend for splitting)
