# ROADMAP

The task list for finishing Phoskonomia. Contribution rules: `CONTRIBUTION.md`.

## Tasks

**Format is machine-parsed — keep it exact:** `- [ ] **T12** — title (needs: T10, T11)`.
`T*` = agent-doable, one PR each. `H*` = needs the human (credentials, live infra, product/visual
judgement) — agents skip them. An agent ticks **only its own** box, and only when every acceptance
criterion under it is met. A task is eligible when all its `needs` are ticked on `main`.
Every task implies: TDD, all gates green, `CONTRIBUTION.md` respected, docs touched by the change updated.

### Phase 0 — hygiene (unblocks the gates)

- [x] **T01** — Make the gates green with zero behaviour change
  - Formatting was done before the first commit; if `cargo fmt --all --check` is still dirty, format only the files you touch and say so.
  - Fix the 7 existing clippy warnings (`phosk_debts`, `phosk_ledger`, `phosk_recurring`, `phosk_llm_ollama`, `phosk_db_surreal` tests). No `#[allow]`.
  - Done when fmt/clippy(`-D warnings`)/test are all clean.
- [x] **T02** — Make the docs tell the truth (needs: T01)
  - Replace the `dx new` boilerplate in `frontend/dioxus-app/README.md` with real run/check instructions.
  - Fix `phosk_daemon` comments claiming `phosk_pipeline_receipt` "doesn't exist yet".
  - Rewrite `run.sh` to launch the Dioxus app (`dx serve`) instead of `phosk_api` + React.
  - Add a "superseded by ROADMAP.md" banner to `backend/documentation/backend-features-todo.md`.
- [x] **T03** — Remove the dead REST surface (needs: T01)
  - Delete `backend/bin/phosk_api` (all-`501` stubs) and its `API.md`; drop now-unused workspace deps; update docs that mention it. Leave `frontend/app/` alone — T05 deletes it.
- [x] **T05** — Delete the obsolete React frontend `frontend/app/` (needs: T02)
  - `git rm -r frontend/app`; remove every remaining reference (root `Cargo.toml` comments, docs). Do NOT touch `.gitignore` — it is a protected path; list its now-dead entries under *Noticed, not fixed*. It stays recoverable from git history — say how in the PR body.
  - This PR is almost pure deletion and will exceed the usual size guidance; that is expected. No Rust code may change.
- [x] **T04** — Shared `DatabaseAdapter` conformance suite (needs: T01)
  - One generic test suite exercising every port method, instantiated for `phosk_db_memory` **and** `phosk_db_surreal` (`kv-mem`). Surreal currently has 6 tests vs memory's 18 — close the gap. Must include a test that category/record ids round-trip as typed ids (the old `Thing` deserialisation 500).

### Phase 1 — backend write side

Each task: service fn(s) in the feature crate + any new port method in **both** adapters (+ conformance
test) + input validation + `Provenance` + errors via `PhoskError`. No UI.

- [x] **T10** — `phosk_ledger`: `create_transaction` (manual entry, with line items; `UserEntered`) (needs: T04)
- [ ] **T11** — `phosk_ledger`: `edit_transaction` (category, shop, fixed, date, amount → `UserModified`) and `delete_transaction` (needs: T10)
- [x] **T12** — `phosk_ledger`: category create / rename / delete-if-empty (needs: T04)
- [x] **T13** — `phosk_ledger`: category merge and split, re-pointing historical line items atomically (needs: T12)
- [x] **T14** — `phosk_recurring`: subscription create / edit / delete (needs: T04)
- [x] **T15** — `phosk_recurring`: lifecycle — pause, resume, cancel, mark-paid, record charge; derived status stays correct (needs: T14)
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

- [x] **T30** — Budgets: edit a category cap (`set_cap`) inline (needs: T01)
- [x] **T31** — Transactions: review/correct a line item (`correct_line`), low-confidence lines highlighted (needs: T01)
- [x] **T32** — Subscriptions: confirm / dismiss AI-detected recurring candidates (needs: T01)
- [ ] **T33** — AI chat: send, persisted history, `/clear` command; panel wide + sticky per the old ROADMAP (needs: T01)
- [x] **T34** — Config page on real `phosk_settings`: get / set / reset preferences (needs: T01)
- [x] **T35** — CSV export buttons for transactions / budget / subscriptions (file download on web + desktop) (needs: T01)
- [ ] **T36** — "NEW transaction" form (needs: T10)
- [ ] **T37** — Edit / delete transaction from the detail view, with confirm step (needs: T11, T36)
- [x] **T38** — Subscriptions create / edit / lifecycle actions (needs: T15)
- [ ] **T39** — Debts + personal IOUs create / edit / payments (needs: T17, T18)
- [ ] **T40** — `/categories` route: list, create, rename, merge; token-seeded colour picker (no raw hex input) (needs: T13)
- [ ] **T41** — `/receipt` route: photo upload → `intake_receipt` → per-line review screen (needs: T20)
- [ ] **T42** — Approval queue UI: pending AI proposals, approve / reject / bulk-approve (needs: T20)
- [x] **T43** — Frontend tests: unit tests for `chf()`, data-layer view-struct mapping, and every `#[server]` fn against the memory stack (needs: T01)

### Human tasks (agents skip these)

- [ ] **H01** — Git bootstrap: first commit, push to `origin`, protect `main` 
- [ ] **H02** — Live-stack smoke test: `PHOSK_DB=surreal` + real Ollama + PaddleOCR, one real receipt end to end
- [ ] **H03** — Design-fidelity pass in a browser against `.claude-design-export/` (chrome on every page, module tags, grid not fading, one coral moment)
- [ ] **H04** — Decide + build the OCR/LLM process sandbox (no net but Ollama, scratch-only fs)
- [ ] **H05** — SurrealDB encryption at rest + daily backup routine
- [ ] **H07** — Deploy `phosk_queue` on the Pi; pair the daemon
- [ ] **H08** — Desktop packaging (`dx bundle`), first installable build
