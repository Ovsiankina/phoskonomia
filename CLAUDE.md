# CLAUDE.md

Guidance for Claude Code (and the unattended agents in `agentic-loop/`) working in this repository.
**Read `CONTRIBUTION.md` before changing anything** — it is the security + workflow contract.

> State verified by healthcheck on **2026-09-17**. Docs in this repo have been badly wrong before:
> code is truth, docs are hints. `ls`/`grep` before you reference a path; fix a doc line when you prove it wrong.

## What this repository is

Phoskonomia is an **AI-driven, local-first micro-budgeting / receipt expense tracker** (item-level
categorization, LLM insights, CHF). One unified Cargo workspace (root `Cargo.toml`), all Rust, **no
JavaScript in new work**.

| Area | State |
|---|---|
| `backend/crates/` (21 crates, ~29k LOC) | Builds; **671 tests green**. Domain model, fat DB port, memory + SurrealDB adapters, Ollama/PaddleOCR/vision-OCR/encrypted-fs adapters, receipt intake pipeline, and the **read/analytics side of all 7 contexts** are done. **The write side is mostly missing** (no create/edit/delete for transactions, subscriptions, debts, IOUs, categories; nothing applies an approved AI suggestion to the ledger). |
| `frontend/dioxus-app/` (~13k LOC) | Dioxus 0.7.6 fullstack. 7 pages ported, 39 `#[server]` fns into the real services. Compiles for `server` and `wasm32`. **Effectively a read-only viewer**: only 3 mutations wired; no receipt upload, approval queue, `/receipt`, `/categories`; 0 tests. |
| `backend/bin/phosk_daemon`, `phosk_queue` | Pi-DMZ poller + queue server. Work, but the daemon still ingests through `NullIngest` instead of the real pipeline. |
| `backend/bin/phosk_api` | **Frozen legacy.** axum REST surface, every handler a `501`. Superseded by `#[server]` fns (ADR-001 reversed). Do not extend. |
| `frontend/app/` | **OBSOLETE** (see `frontend/app/OBSOLETE.md`). React prototype of the same 7 pages, built for the dead REST API; superseded by `frontend/dioxus-app/`. Do not run, read for guidance, or extend. Deleted by T05. |
| `frontend/.claude-design-export/` | Static design mockup — the **visual source of truth**. Read-only. |
| `README.md` (1610 lines) | Aspirational spec with conflicting v0.1/v0.2 sections. A plan, not a description. |
| `backend/documentation/` | ADRs (`architectural-design-and-philosophy.md`) and `build-contract.md` are authoritative for design. `backend-features-todo.md` is **superseded by the task list below** (its checkboxes were never maintained). |

Overall ≈ 50–55 % to a usable v1. Never run against live SurrealDB / Ollama / PaddleOCR so far — the
default stack is a seeded in-memory DB with a fake OCR.

## Commands

```bash
# backend (from repo root)
cargo test  --workspace --exclude dioxus-app
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all --check

# frontend (Dioxus; `dx` 0.7.6 must match the pinned crate version)
cd frontend/dioxus-app
dx serve                                            # web (default)
dx serve --platform desktop
cargo check --no-default-features --features server # server side type-check
cargo check --target wasm32-unknown-unknown         # client side type-check
```

Stack selection is by env at the composition root (`frontend/dioxus-app/src/data/mod.rs`):
`PHOSK_DB=memory|surreal` · `PHOSK_OCR=auto|paddle|vision` · `PHOSK_LLM_MODEL=<ollama tag>` ·
`PHOSK_DATA_DIR=./phosk-data`. Dev ports: 3000–3002 are always taken on this machine.

## Architecture rules (what actually exists — keep it this way)

- **Hexagonal, "swap whole subsystems".** Ports are fat traits, one per subsystem: `DatabaseAdapter`
  (deliberately ONE fat trait for all domains — do not split it), `LlmAdapter`, `OcrAdapter`, `PhotoStorage`.
- **Layers flow downward:** `phosk_core`, `phosk_id` → `phosk_model` (pure data) → `phosk_adapter_*` (port
  traits) → concrete adapters (`phosk_db_memory`, `phosk_db_surreal`, `phosk_llm_ollama`, `phosk_ocr_*`,
  `phosk_storage_fs`) → feature crates (`phosk_ledger`, `phosk_planning`, `phosk_recurring`, `phosk_debts`,
  `phosk_insights`, `phosk_settings`, `phosk_ai`, `phosk_pipeline_receipt`) → UI.
- **Feature crates see only port traits.** Concrete adapters are wired **only** at composition roots
  (`dioxus-app/src/data/mod.rs`, `backend/bin/*`). SurrealDB types never escape `phosk_db_surreal`.
- Every port method is implemented in **both** DB adapters with the same behaviour.
- **Front↔back = Dioxus `#[server]` fns** with shared Rust types; in-process on desktop. HTTP exists only
  at the real network boundary (Pi queue). `Money` crosses as exact `i64` centimes — never a float.
- Conventions (clippy-enforced in root `Cargo.toml`): `unsafe` forbidden; `unwrap`/`expect`/`panic` denied
  outside tests; single `PhoskError` taxonomy (`thiserror`); `tracing` compiled out of release; every
  create/edit stamps `Provenance`.
- **AI safety model:** "a vocabulary, not a connection". Reads execute; **writes go to an approval
  queue**; a model can propose, only a human-approved suggestion changes the ledger. JSON output is
  schema-validated; confidence < 0.7 is flagged, never dropped.
- **Security model:** Pi is an untrusted DMZ; the desktop never opens an inbound socket (it polls
  outbound). Queued photos are hostile input. Photos encrypted at rest, EXIF stripped, zero telemetry.

---

## TODO — the task list the agentic loop works through

**Format is machine-parsed — keep it exact:** `- [ ] **T12** — title (needs: T10, T11)`.
`T*` = agent-doable, one PR each. `H*` = needs the human (credentials, live infra, product/visual
judgement) — agents skip them. An agent ticks **only its own** box, and only when every acceptance
criterion under it is met. A task is eligible when all its `needs` are ticked on `main`.
Every task implies: TDD, all gates green, `CONTRIBUTION.md` respected, docs touched by the change updated.

### Phase 0 — hygiene (unblocks the gates)

- [ ] **T01** — Make the gates green with zero behaviour change
  - Formatting is done by `agentic-loop/bootstrap.sh` before the first commit; if `cargo fmt --all --check` is still dirty, format only the files you touch and say so.
  - Fix the 7 existing clippy warnings (`phosk_debts`, `phosk_ledger`, `phosk_recurring`, `phosk_llm_ollama`, `phosk_db_surreal` tests). No `#[allow]`.
  - Done when fmt/clippy(`-D warnings`)/test are all clean.
- [ ] **T02** — Make the docs tell the truth (needs: T01)
  - Replace the `dx new` boilerplate in `frontend/dioxus-app/README.md` with real run/check instructions.
  - Fix `phosk_daemon` comments claiming `phosk_pipeline_receipt` "doesn't exist yet".
  - Rewrite `run.sh` to launch the Dioxus app (`dx serve`) instead of `phosk_api` + React.
  - Add a "superseded by CLAUDE.md TODO" banner to `backend/documentation/backend-features-todo.md`.
- [ ] **T03** — Remove the dead REST surface (needs: T01)
  - Delete `backend/bin/phosk_api` (all-`501` stubs) and its `API.md`; drop now-unused workspace deps; update docs that mention it. Leave `frontend/app/` alone — T05 deletes it.
- [ ] **T05** — Delete the obsolete React frontend `frontend/app/` (needs: T02)
  - `git rm -r frontend/app`; remove every remaining reference (root `Cargo.toml` comments, `CLAUDE.md` table row, docs). Do NOT touch `.gitignore` — it is a protected path; list its now-dead entries under *Noticed, not fixed*. It stays recoverable from git history — say how in the PR body.
  - This PR is almost pure deletion and will exceed the usual size guidance; that is expected. No Rust code may change.
- [ ] **T04** — Shared `DatabaseAdapter` conformance suite (needs: T01)
  - One generic test suite exercising every port method, instantiated for `phosk_db_memory` **and** `phosk_db_surreal` (`kv-mem`). Surreal currently has 6 tests vs memory's 18 — close the gap. Must include a test that category/record ids round-trip as typed ids (the old `Thing` deserialisation 500).

### Phase 1 — backend write side

Each task: service fn(s) in the feature crate + any new port method in **both** adapters (+ conformance
test) + input validation + `Provenance` + errors via `PhoskError`. No UI.

- [ ] **T10** — `phosk_ledger`: `create_transaction` (manual entry, with line items; `UserEntered`) (needs: T04)
- [ ] **T11** — `phosk_ledger`: `edit_transaction` (category, shop, fixed, date, amount → `UserModified`) and `delete_transaction` (needs: T10)
- [ ] **T12** — `phosk_ledger`: category create / rename / delete-if-empty (needs: T04)
- [ ] **T13** — `phosk_ledger`: category merge and split, re-pointing historical line items atomically (needs: T12)
- [ ] **T14** — `phosk_recurring`: subscription create / edit / delete (needs: T04)
- [ ] **T15** — `phosk_recurring`: lifecycle — pause, resume, cancel, mark-paid, record charge; derived status stays correct (needs: T14)
- [ ] **T16** — `phosk_debts`: debt create / edit / delete, record payment, extra payment; amortisation outputs stay correct (needs: T04)
- [ ] **T17** — `phosk_debts`: plan adjust (monthly/day/term) and refinance (needs: T16)
- [ ] **T18** — `phosk_debts`: personal IOUs — create / edit / delete, record partial payment, settle (needs: T04)
- [ ] **T19** — `phosk_planning`: set global monthly budget + savings target, with budget-history entry; alert snooze with re-trigger (needs: T04)
- [ ] **T20** — `phosk_ai`: **approval service** — list pending suggestions, approve (applies the receipt proposal to the ledger via `insert_receipt`, idempotent, provenance preserved), reject, bulk-approve per receipt. This is the ONLY path from a model proposal to the ledger. (needs: T10)
- [ ] **T21** — `phosk_daemon`: compose the real `phosk_pipeline_receipt` ingest behind the existing seam; keep `NullIngest` for tests; selection by env (needs: T20)
- [ ] **T22** — CSV import: column mapping, Swiss bank date/amount formats, dedupe by content hash, `Imported` provenance (needs: T10)

### Phase 2 — wire the UI (`frontend/dioxus-app`)

Each task: `#[server]` fn(s) in `src/data/` + the page interaction + loading/error states using
`components/states.rs`. Oscillocore rules apply (tokens only, numbers in Pilowlava). Must pass both
`cargo check` targets.

- [ ] **T30** — Budgets: edit a category cap (`set_cap`) inline (needs: T01)
- [ ] **T31** — Transactions: review/correct a line item (`correct_line`), low-confidence lines highlighted (needs: T01)
- [ ] **T32** — Subscriptions: confirm / dismiss AI-detected recurring candidates (needs: T01)
- [ ] **T33** — AI chat: send, persisted history, `/clear` command; panel wide + sticky per the old ROADMAP (needs: T01)
- [ ] **T34** — Config page on real `phosk_settings`: get / set / reset preferences (needs: T01)
- [ ] **T35** — CSV export buttons for transactions / budget / subscriptions (file download on web + desktop) (needs: T01)
- [ ] **T36** — "NEW transaction" form (needs: T10)
- [ ] **T37** — Edit / delete transaction from the detail view, with confirm step (needs: T11, T36)
- [ ] **T38** — Subscriptions create / edit / lifecycle actions (needs: T15)
- [ ] **T39** — Debts + personal IOUs create / edit / payments (needs: T17, T18)
- [ ] **T40** — `/categories` route: list, create, rename, merge; token-seeded colour picker (no raw hex input) (needs: T13)
- [ ] **T41** — `/receipt` route: photo upload → `intake_receipt` → per-line review screen (needs: T20)
- [ ] **T42** — Approval queue UI: pending AI proposals, approve / reject / bulk-approve (needs: T20)
- [ ] **T43** — Frontend tests: unit tests for `chf()`, data-layer view-struct mapping, and every `#[server]` fn against the memory stack (needs: T01)

### Human tasks (agents skip these)

- [ ] **H01** — Git bootstrap: first commit, push to `origin`, protect `main` (`agentic-loop/bootstrap.sh`)
- [ ] **H02** — Live-stack smoke test: `PHOSK_DB=surreal` + real Ollama + PaddleOCR, one real receipt end to end
- [ ] **H03** — Design-fidelity pass in a browser against `.claude-design-export/` (chrome on every page, module tags, grid not fading, one coral moment)
- [ ] **H04** — Decide + build the OCR/LLM process sandbox (no net but Ollama, scratch-only fs)
- [ ] **H05** — SurrealDB encryption at rest + daily backup routine
- [ ] **H07** — Deploy `phosk_queue` on the Pi; pair the daemon
- [ ] **H08** — Desktop packaging (`dx bundle`), first installable build

---

## Oscillocore design system (the frontend's law)

The design language is **Oscillocore** — a CRT-oscilloscope / molten-lava aesthetic. It lives in
`frontend/.claude-design-export/`. Two copies of the foundation: the **packaged, tokenized, canonical**
bundle under `frontend/.claude-design-export/_ds/oscillocore-design-system-*/` (source of truth — `osc-*`
classes + tokens) vs the **loose app files** at the export root (`phosk.css`, `shell.css`, `*.jsx` using
`phosk-`/`pk-`/`ai-`/`sig-` prefixes). When they disagree, the packaged DS + `DESIGN-SYSTEM-CANONICAL.md` win.

**Enforceable rules:**

- **No raw hex colors, no raw `px` values.** Use design tokens via `var(--…)` (defined in
  `_ds/.../colors_and_type.css`): `--neon`, `--indigo*`, `--bg*`, the `--t-*` type scale, `--s-*`
  spacing, the `--grid-unit: 39px` module.
- **Only three font families are legal:** `Pilowlava`, `Pilowlava Atome`, `VG5000`.

**Two-font roles (strict):**
- **Pilowlava** (`var(--font-display)`) — **numbers/numerals, headers, tags, hero only.**
- **VG5000** (`var(--font-body)`) — everything else (body, UI labels, data).
- **Every numeric value (amount/count/percent/delta) must render in Pilowlava.** A number in VG5000 is a bug.

**Aesthetic invariants** (from `DESIGN-SYSTEM-CANONICAL.md`): near-black violet void (`--bg #06040c`),
no white page ever; sharp corners (`border-radius: 0` default); depth via **glow, not shadow**
(`--glow-*`); **one coral (`--neon`) moment per view** — coral is the signal, indigo is structure and
never emphasis (and coral must never warm toward orange). Every surface wears the permanent CRT skin
(`.osc-scan` scanline + `.osc-vig` vignette), rendered once at the app root — never per page. The 39px
grid background **must not fade/mask** and must be broken on purpose with cells/rulers/readouts.
**Every page** needs the sticky top bar + left nav sidebar.

Canonical surface classes: `.osc-glass` (frosted translucent panel over the scanner — needs a
translucent fill + `-webkit-` prefix or the blur is wasted), `.osc-frost` (text-legibility frost),
`.osc-readout` (corner-bracket box with a Pilowlava **module tag** in its top border, e.g.
`MOD·TX · LIST` — keep it visible, not clipped behind headers).

The export is a static mockup: no module system (`window.*` globals, load order matters), hardcoded mock
data, canned "AI" replies. `design-canvas.jsx` / `tweaks-panel.jsx` are authoring tooling that
intentionally violate the design system — never copy them into the app.

## Traps worth repeating

- `README.md` is aspirational and self-contradictory (v0.1 vs v0.2). ADRs in `backend/documentation/` win;
  where an ADR and the code disagree, the code + root `Cargo.toml` comments reflect the latest decision
  (e.g. ADR-001's REST transport was reversed in favour of `#[server]` fns).
- `frontend/dioxus-app/AGENTS.md` is generic Dioxus 0.7 API notes — useful, since 0.7 changed every API
  (`cx`, `Scope`, `use_state` are gone). Check the pinned version before trusting memory of an API.
- The root `target/` is ~48 GB. Never `cargo clean` casually; never commit it.
- `phosk-data/` holds encrypted photos + keys at runtime. It must never be committed.
