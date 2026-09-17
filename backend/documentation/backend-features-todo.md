# Phoskonomia Backend — Exhaustive Features TODO

> **SUPERSEDED by [`ROADMAP.md`](../../ROADMAP.md).** This list is kept for
> historical context only. Its checkboxes were never maintained and do not
> reflect what is built, and it describes the `phosk_api` REST surface, which has
> been deleted (front↔back is Dioxus `#[server]` fns). Track work in
> `ROADMAP.md`; do not tick boxes here.

Everything the backend must implement to turn the `phosk_api` `501`s green and
serve the frontend real data. Organised by **bounded-context crate** (ADR-005).
Each `501` response carries a `todo: "<context>: <feature>"` tag that matches a
section below. UI tasks are out of scope — these are backend features only.

**Legend:** `[ ]` not started · each item ≈ a service + repo (+ AI/pipeline)
unit, built test-first. "Derived" = computed by the backend, never stored raw.

---

## 0. Cross-cutting foundations (must exist before most features)

### Persistence (`phosk_db_surreal` impl of `DatabaseAdapter`)
- [ ] SurrealDB adapter implementing the fat `DatabaseAdapter` port (ADR-000).
- [ ] **`Thing` ↔ typed-id conversion confined to the adapter** (ADR-010 — the
      ROADMAP 500 was this leaking; never let `surrealdb::sql::Thing` escape).
- [ ] Schema definition + versioned migrations (SurrealQL).
- [ ] Encryption at rest; daily backup routine.
- [ ] `phosk_db_memory` parity (test mock) kept in lockstep as the port grows.

### Domain & edge plumbing
- [ ] `phosk_core`: error taxonomy (`PhoskError` → HTTP status mapping, PII-redacted
      logs), typed IDs, `Money` (CHF), `Provenance`, **cycle/period engine**
      (`day|week|month|quarter|year|custom` + current-cycle resolution — used by
      almost every read endpoint).
- [ ] DTO layer: per-feature `api/` modules (behind `http` cargo feature),
      domain↔DTO mapping, Swiss CHF formatting (`CHF 1'234.50`) at the edge only.
- [ ] Provenance stamping on every create/edit (`Ocr|LlmInferred|UserEntered|UserModified|Imported|RuleGenerated`).
- [ ] Idempotency for all spine/worker writes (ADR-010).

### Security & transport (ADR-007)
- [ ] Local auth secret on every endpoint; restrict CORS to the frontend origin
      (currently permissive in dev).
- [ ] Keep the API loopback-bound; no inbound surface beyond the Pi DMZ.

---

## 1. `phosk_ledger` — transactions · line items · categories · shops · signals

### Transactions  → `/transactions*`
- [ ] List with filters/sort/search/pagination (`period,from,to,shop,category,signal,flagged,q,sort,order,limit,offset`) + `summary{entry_count,total_amount,period_label}` + `available_shops/categories`.
- [ ] Detail with line items, `avg_confidence`, `source{type,pipeline,ocr_engine}`, `ocr_regions[]`.
- [ ] Create — manual entry; **and** photo upload → receipt pipeline (§8).
- [ ] Edit (subset of `category,shop,fixed,date,amount`); delete.
- [ ] Derived per receipt: `low_conf_count` (lines < 0.7), `signal_ids[]` touched, `fixed` flag.

### Line items  → `/transactions/{id}/lines*`
- [ ] List lines; review/correct a line (`name,qty,unit_price,category,signal_id,confirmed`).
- [ ] Direct-edit vs explanation-correction (audit original; ADR-011 correction events).

### Categories  → `/categories*`
- [ ] List with budget rollup (spent/cap/used%/remaining/items/proj/spark/status).
- [ ] Detail (`overCapAmount`, `projectedSpend`, history, `histAvg`).
- [ ] Create; merge; split (retro reprocess of historical items — async).
- [ ] Per-category transactions feed.
- [ ] Spent/projection computation scoped to the active cycle.

### Shops  → `/shops`
- [ ] Derive shop directory from transactions (`txn_count`, `total_amount`); per-shop trends feed insights.

### Item-signals  → `/signals*` (AI-maintained micro-categories: Coffee, Pain au chocolat, Beer)
- [ ] List tracked signals (+ candidates) with 12-month `series`, `deltaPct`, `conf`.
- [ ] Detail with `recent[]` line occurrences.
- [ ] Track (from candidate or free-text name) / untrack / pause.
- [ ] Auto-attach matching line items to their signal; roll up qty/spend per cycle.
- [ ] Movers ranking (riser/faller) → `/signals/movers` (insights).

---

## 2. `phosk_planning` — budgets · allocation · alerts

### Budgets & allocation  → `/budget/*`, `/categories/{name}` (PATCH), `/signals/{id}/cap`
- [ ] Global monthly budget + savings target; per-category caps (with "unlimited").
- [ ] Cap edits (absolute or delta) + **budget history** tracking.
- [ ] Totals (`budget,allocated,spent,projected,remaining,overAllocated,unallocated`).
- [ ] Allocation breakdown + segments; over-/under-allocation math.
- [ ] Signal soft caps + nudge-at-% threshold.
- [ ] Savings rate + projection.

### Alerts engine  → `/alerts*`
- [ ] Generate: over-budget (>100%), at-risk/near-cap (>80%), global overspend,
      on-pace-to-exceed, savings-at-risk, recurring-missing, budget-cut-suggestion.
- [ ] Severity/tone (`alert|warn|info|llm`); `actions[]`; `kind`; `source(rule|llm)`.
- [ ] **Real-time recalculation on each new transaction.**
- [ ] Lifecycle: dismiss, snooze (re-trigger if condition persists), apply
      (execute the carried suggestion), `target` deep-link resolution.
- [ ] Notification history.

---

## 3. `phosk_recurring` — subscriptions · recurring detection

> `/recurring` (dashboard summary) and `/subscriptions` (full page) are **one
> backend domain**; expose both views over the same store.

### Subscriptions  → `/subscriptions*`, `/recurring*`
- [ ] CRUD; cadence (monthly day-of-month / yearly month); status (`ok|soon|watch|due|paused`).
- [ ] Derived: `monthlyEquiv`, `annual`, `daysUntil`, `nextLabel`, `firedThisCycle`, `priceRose` (hist creep).
- [ ] Stats roll-up (`monthly,annual,autoCount,chargedThisCycle,upcomingThisCycle,next30,flagged`).
- [ ] Billing-sweep timeline (impulse train per charge day).
- [ ] Lifecycle: pause/resume/cancel/mark-paid; charges (record + list); per-cadence next-due.
- [ ] Price-history series (last 6 charges monthly / 3 years yearly).

### AI recurring detection  → `/subscriptions/detect`, `/recurring/{name}/confirm`, `/subscriptions/{id}/confirm|dismiss`
- [ ] Detect standing charges from transaction history (`src=llm` candidates).
- [ ] Recurring-not-seen anomaly (expected charge missing this cycle → alert).
- [ ] Confirm (→ `src=user`) / dismiss; usage-based review flags (e.g. "0 watch hours").

---

## 4. `phosk_debts` — institutional debts · personal IOUs

### Debts  → `/debts*`
- [ ] CRUD (`type LEASE|LOAN|CARD|TAX|BNPL|MEDICAL`, apr, balance, monthly, term, day).
- [ ] **Amortization engine** (derived): `monthsToPayoff` (cap 600), `forwardSeries[]`,
      `interestRemaining` (∞ when payment ≤ interest), `monthlyRate`, `annualInterest`, `paidOffPct`.
- [ ] Stats: `totalOwed,totalOrig,totalMonthly,totalInterestYr,weightedApr,horizon,debtFreeLabel,flagged[]`.
- [ ] Combined trajectory (history + projection, `xTicks`, debt-free date).
- [ ] **Strategy engine**: avalanche (highest APR) / snowball (smallest balance) target + note.
- [ ] Plan adjust (monthly/day/term); refinance; extra payments; payment history.
- [ ] AI debt detection (`src=llm`) → approve/dismiss.

### Personal IOUs  → `/personal-ious*` (kept separate from institutional debt)
- [ ] CRUD; `dir in|out`; partial-repayment (`of` → `repaidPct`).
- [ ] Stats (`owedToYou,youOwe,net,countIn,countOut,maxSingle`).
- [ ] Record payment; settle; settle-up; remind; (optional phone link).

---

## 5. `phosk_insights` — dashboard aggregation · analytics · exports

### Dashboard  → `/cycle/current*`, `/insights/dashboard`
- [ ] Cycle window + KPI totals (budget/spent/remaining/savings/rates/vs-last-cycle/per-day).
- [ ] Spend-series: `daily[]`, `cumulative[]`, `pace[]`, last-cycle compare, `todayIndex`.
- [ ] Top shops this cycle.

### Analytics  → `/analytics/*`
- [ ] 12-cycle spend/savings history + stats (avg/peak/low/vs-avg/vs-prev/totalSaved/income).
- [ ] Per-category momentum (now vs 3-cycle avg, 12-cycle series).
- [ ] Weekday rhythm (discretionary; weekend share).
- [ ] Signal movers + GEMMA4 narrative read (→ AI spine §7).

### Exports  → `/exports/*.csv`
- [ ] CSV (transactions / budget / subscriptions), mirroring list filters; timestamp + version.
- [ ] Later: JSON dump (schema-versioned), PDF reports (`phosk_export_pdf`, heavy dep isolated).

---

## 6. `phosk_settings` — preferences · account · shell

- [ ] Preferences store (~30 keys; get/update/reset/defaults; per-surface; `storedOnDevice`) → `/settings/preferences*`, `/config`.
- [ ] Summary (`totalPreferences,changedCount,engine,model`).
- [ ] Account profile (holder, IBAN masking) → `/account` (GET/PATCH).
- [ ] AI engine/model selection + engines list with reachability → `/account/ai/engine(s)`.
- [ ] Nav pages catalog → `/nav/pages`.

---

## 7. `phosk_ai` — the spine + safe module (touches every context)

### Ports & runtime
- [ ] `LlmAdapter` port + Ollama/GEMMA4 impl with **constrained/structured generation** (ADR-008).
- [ ] Tool registry: per-feature tools, manifest, schema-validate, **effect-typed gate**
      (read=execute, write=approval-queue) — ADR-006/007. "Vocabulary, not a connection."
- [ ] Per-receipt **approval queue** (bulk approve; ADR-011); audit log of every tool call.

### AI features surfaced in the API
- [ ] Auto-categorize (line → category + signal + confidence) → feed `kind=categorize`.
- [ ] Reprocess low-confidence (OCR+LLM re-read) → `/transactions/{id}/reprocess`, `/ai/reprocess`, feed `kind=reprocess`.
- [ ] Suggestions: budget cut, recurring detect, signal candidates → alerts / approval queue, feed `kind=suggest`.
- [ ] Signal-candidate detection ("seen N× across M shops") → `/signals/candidates`, track/dismiss.
- [ ] Trend/anomaly detection (movers, recurring-not-seen) → feed `kind=detect`.
- [ ] Activity feed → `/ai/feed` (+ dismiss).
- [ ] Narrative insights (dashboard, movers) → `/insights/dashboard`, `/analytics/insights/movers`.
- [ ] **LLM chat** → `/ai/chat` (history + send; intents `track`/`cap`; references; **persistence + `/clear`** — frontend ROADMAP gap).
- [ ] AI status (`model,engine,online,watchedSignals`) → `/ai/status`.

---

## 8. Receipt pipeline (`phosk_pipeline_receipt` + `OcrAdapter` + `PhotoStorage`)

- [ ] Photo intake: validate MIME + libmagic signature + size cap (zero-trust, ADR philosophy #4).
- [ ] EXIF strip; encrypt at rest (`PhotoStorage` port); retention (180d default, keep-forever flag, auto-delete).
- [ ] OCR (`phosk_ocr_paddle`): text + regions + per-region confidence; store raw + annotated.
- [ ] LLM extraction: lines → items, schema-validated before persist; confidence per item.
- [ ] Review → approval → compression-on-approval flow.
- [ ] Sandbox the OCR/LLM process (no net but Ollama, scratch-only fs, dropped caps).
- [ ] Other inputs: CSV import (column mapping, Swiss bank formats, dedupe); PDF invoice extraction.

---

## 9. Out of frontend scope (tracked, not part of this API)

- [ ] `phosk_queue` (Raspberry Pi DMZ queue server) + `phosk_daemon` (desktop poller) — separate bins, separate trust zone (ADR security model). Not frontend-facing.

---

## 10. API integration (wiring `phosk_api` to the real backend)

- [ ] Per feature: define DTOs, map domain↔DTO, **replace the `501` stub** in
      `bin/phosk_api/src/routes.rs` with a call into the feature service.
- [ ] Add feature-crate dependencies to `phosk_api` as handlers are wired
      (it intentionally has none today).
- [ ] Add auth middleware + CORS lockdown (§0 security).
- [ ] Keep `API.md` in sync as response shapes firm up through TDD.
