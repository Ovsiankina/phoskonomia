# Phoskonomia Backend — Architectural Design & Philosophy

> Living document. Every architectural decision is logged here as an ADR.
> Prose is kept terse on purpose. The README (v0.2 section) is the prior
> baseline; this file records the deltas and the decisions made from
> 2026-06-17 onward, when the backend became a standalone web API.

---

## 1. Philosophy (carried forward, condensed)

1. **Swap whole subsystems, not individual operations.** The trait *is*
   the boundary. New DB / LLM / OCR engine = new impl, never touched
   feature code.
2. **Stability surfaces.** Domain types and port traits change rarely;
   everything behind them changes freely. Designed to be used for years.
3. **Local-first & private.** Data lives on the user's machine. Telemetry
   is zero. Photos encrypted at rest, EXIF stripped.
4. **Zero-trust hostile input.** A receipt photo is attacker-controlled.
   So is anything an LLM emits after reading one. Validate at every seam.
5. **AI is assistive, not authoritative.** The model proposes; the user
   (or an explicit policy) disposes. No model output mutates persisted
   financial state without passing a typed schema + a gate.

---

## 2. ADR log

ADR format: **Decision · Status · Context · Consequence.** Status ∈
{LOCKED, PROPOSED, OPEN, SUPERSEDED}.

### ADR-000 · Carried-forward LOCKED decisions (from README v0.2)
- Workspace of many small Rust crates, `phosk_*` prefix, strict downward
  dep flow. — LOCKED
- SurrealDB behind **one fat `DatabaseAdapter` trait** (whole-DB
  swappability beats per-table). — LOCKED
- `thiserror` for domain errors; `tokio` async runtime; `axum` for HTTP. — LOCKED
- LLM via adapter (Ollama/Gemma local primary, fallback impls); OCR via
  adapter (PaddleOCR primary). — LOCKED
- Pi queue = untrusted DMZ; desktop never exposes an inbound socket. — LOCKED

### ADR-001 · Transport & topology
- **Decision: Web HTTP API server (REST/JSON).** Frontend is a separate
  client (currently `localhost:3001`). — **LOCKED 2026-06-17.**
- Context: README v0.2's "link crates directly, no local HTTP" model is
  **SUPERSEDED**. The product is a web app with a real client-server
  boundary.
- Consequence: the HTTP layer sits at the top of the stack and must stay
  thin. The service layer beneath it is kept **transport-agnostic** so a
  future linked-lib / Dioxus-desktop target remains possible without
  rewriting business logic.

### ADR-002 · Architecture style
- **Modular monolith, vertical feature slices.** n-layer *within* a slice;
  hexagonal (ports/adapters) **only** at external seams (DB / LLM / OCR /
  storage / notify); a **command + worker spine** for async + AI work.
  — **LOCKED 2026-06-17.** Supersedes README v0.2's "full hexagonal
  everywhere."
- Layer stack (top→bottom): 1 HTTP edge · 2 application/service · 3
  command+worker spine (incl. `phosk_ai`) · 4 domain core · 5 ports ·
  6 adapters.
- **Crate vs module:** layers 4–6 + the spine are **shared crates**;
  layers 1–2 are **per-feature modules** inside a bounded-context crate.
  A feature is one vertical module, never spread across layer-crates.

### ADR-003 · AI agentic-tools safe module
- **Scope: LOCKED 2026-06-17.** The AI manages transaction *detail*:
  organizes, labels, (re)categorizes items, generates alerts, detects
  recurring/anomalies. It is a mutating actor, not read-only.
- **Safety invariant:** AI never writes persisted state directly. It
  emits **typed, schema-validated commands** through a constrained tool
  surface; mutations land in an **approval queue** (user disposes) or, for
  background scanners, surface as alerts — never silent writes. Model
  process runs sandboxed (zero-trust: photo + LLM output are hostile).
- **Tool-surface & sandbox design:** governed by ADR-006 / ADR-007 below;
  internals (loop, manifest, queue) are the next design block.

### ADR-004 · Deployment model
- **Single-tenant, self-hosted/local.** One backend per user; embedded
  SurrealDB, local GPU/files. — **LOCKED 2026-06-17.**
- Horizontal scaling is a **non-goal**; the real scale axes are
  background-work throughput and codebase growth. API server **binds to
  loopback** + a local auth secret (no inbound network surface beyond the
  Pi DMZ).

### ADR-005 · Crate granularity ("when does X earn a crate?")
- **Rule — a crate boundary needs ≥1 of three forces:** (1) build
  isolation, (2) dependency isolation (quarantine a heavy/optional dep),
  (3) compiler-enforced architectural boundary. — **LOCKED 2026-06-17.**
- **Crate = bounded context** (`phosk_ledger`, `phosk_planning`,
  `phosk_recurring`, `phosk_debts`, `phosk_insights`, `phosk_settings`)
  **or shared infra layer** (foundation, ports, adapters, spine).
- **Feature = plain Rust module** inside its context crate, uniform
  anatomy: `domain / repo / service / commands / ai / api` (`api` behind a
  `http` cargo feature so the service stays linkable).
- Cheap TDD mocks come from **ports-as-crates**, NOT features-as-crates —
  a feature's test build never pulls SurrealDB/Ollama.
- **Grow-into-crates:** promote a module to a crate the moment it trips a
  force. Born as crates for that reason: `phosk_pipeline_receipt`
  (OCR+LLM), `phosk_export_pdf` (printpdf). Pre-splitting ~50 feature
  crates is a standing tax — rejected.

### ADR-006 · AI tool model — "a vocabulary, not a connection"
- Each feature **co-locates its AI tool(s)** in its `ai` module:
  `{ name, arg JSON-schema, effect-class, handler → service }`.
  — **LOCKED 2026-06-17.**
- `phosk_ai` aggregates all tools into one **manifest**, runs the agent
  loop, enforces governance. The model sees only the manifest and emits
  **typed JSON calls**; it never writes code/SQL, never holds a DB handle.
- Distributed ownership (tools live in slices) + central control (one
  safe-module validates / gates / audits).

### ADR-007 · AI / financial-data security model
- **The AI tool surface is in-process, not networked** — no socket,
  nothing to reverse-engineer. — **LOCKED 2026-06-17.**
- Only network doors: public REST API (frontend) + Pi queue. The public
  API **binds to loopback + requires a local auth secret/session**,
  protecting *all* financial data, AI path or not.
- **Effect-typed gating:** `read` tools execute after schema-validation;
  `mutate` tools **enqueue an approval action** — never a direct write. A
  prompt-injected model (hostile receipt) can at worst *propose* a change
  the user rejects.
- A remote fallback model never receives a tool handle — only the right to
  *request* a call the backend validates / gates / executes locally.
- Model + OCR run in a **sandboxed process** (no net except Ollama,
  scratch-only fs, dropped caps). Every tool call is audited.

### ADR-008 · Small-model-first tool design
- **Driver:** tools run on *local* models (Gemma-e2b / 7–8B class), which
  fail on complex calls. Reliability for weak models is **co-equal** with
  security — and here the two **align**. — **LOCKED 2026-06-17.**
- **Rules:**
  - **Flat scalar args** (string / number / bool / short-enum). No nested
    objects, no arrays-of-objects. ≤ ~3 params per tool.
  - **Human-term references, never raw IDs.** A tool takes a category
    *name* / shop *name*; the backend resolves it to an ID **within the
    authorized scope**. Serves simplicity (no UUID hallucination) AND
    security (model can't address arbitrary records). The crown-jewel rule.
  - **Curated, minimal manifest.** A tool exists only where AI help adds
    value AND it is small-model-simple. Most CRUD stays UI/REST-only.
    Scope / progressively disclose if the active set grows.
  - **One tool call per step**, sequential loop — no parallel multi-tool plans.
  - Verb-first, obvious names; short descriptions.
- **Reliability lever — constrained decoding.** Generate calls under a
  grammar / JSON-schema constraint (Ollama structured output / llama.cpp
  GBNF) **derived from the same Rust type** (ADR-006). Schema-valid *at
  generation time*, not merely validated after. → the `LlmAdapter` port
  MUST expose constrained/structured generation, not only free text.
- **Reject-vs-repair:** constrained decoding makes ADR-007's hard gates a
  rarely-firing backstop. Where a model can't be constrained: lenient on
  *form* (coerce `"4.5"`→number, enum synonyms, trailing commas) + one
  bounded repair retry; strict on *meaning* (never invent an ID, never
  widen effect-class).

### ADR-009 · Open vs closed world — the asymmetry
- **Closed types, open data, closed capabilities.** — **LOCKED 2026-06-17.**
- **Types closed:** domain enums are exhaustive; a new concept is a
  deliberate code change. Total `match`, no catch-all on known types.
- **Data open:** categories / labels / shops / rules are born at runtime;
  code never assumes a complete set — always an `Unknown/Other` branch,
  never a panic on an unseen value.
- **Capabilities (tools) closed-at-build:** the manifest is fixed per
  binary. No runtime-loaded tools (security + small-model hazard). New tool
  = recompile.
- **Code-as-policy (Phase 4)** is the *only* open-world execution surface —
  sandboxed, read-only data snapshot, no tool/DB handle.
- The stance: an open world of **values**, a closed world of **powers**.

### ADR-010 · Module charter — contracts every module signs
- **One service gateway.** HTTP, AI tool, CLI, worker all enter a feature
  through its **service** API. No transport gets a privileged path; the AI
  is just another caller. The service is the single place invariants +
  authorization + audit are enforced. — **LOCKED 2026-06-17.**
- **Layer ownership, no skipping.** `domain`=invariants (pure) ·
  `service`=use-cases + authz · `repo`=persistence translation ·
  `ports`=external contracts · `adapters`=one vendor's quirks. Flow is
  HTTP→service→repo→port→adapter; skipping is a violation.
- **Vendor types die at the adapter.** A SurrealDB `Thing` / Ollama JSON
  never escapes its adapter. (The ROADMAP 500 — `Thing` leaking into
  deserialization — is exactly this rule broken.)
- **Security is a boundary invariant, not a module.** At the service door,
  uniformly: authenticate caller · resolve human-terms→IDs within the
  authorized scope · gate AI writes → approval queue · audit. A
  `RequestContext` (caller identity + capability) threads into every
  service call.
- **One error taxonomy** (`phosk_errors`): every layer maps in; an error
  yields a user message + a PII-redacted log line.
- **Idempotency:** everything the spine runs (workers, commands,
  reprocessing) is idempotent — re-runs never double-write.

### ADR-011 · Provenance & the correction-feedback loop
- **Every datum carries provenance + confidence.** `source ∈ {Ocr,
  LlmInferred, UserEntered, UserModified, Imported, RuleGenerated}` +
  confidence (0–1) + correction history. Shared `Provenance` in foundation,
  embedded in every entity. Powers audit trail, low-confidence flag, trust.
  — **LOCKED 2026-06-17.**
- **Corrections are first-class events.** A user fix preserves the original,
  stamps `UserModified`, and **emits a correction event** the spine reacts
  to → retroactive reprocessing (merge/split reapply; future parses
  improve). The "check receipt photo vs AI items → app auto-adapts" loop.
- **Approval is per-receipt bulk**, never per-item: AI is not trusted to
  auto-write; verifying one receipt is a single quick visual check. Settles
  the effect-class UX (ADR-007): binary `Read`/`Write`, all writes gated,
  no auto-apply tier.

---

## 3. Open questions (tracked)

Resolved 2026-06-17 — all LOCKED: ADR-001 transport · ADR-002 layering ·
ADR-003 AI scope · ADR-004 deployment · ADR-005 crate granularity ·
ADR-006 AI tool model · ADR-007 AI/financial security · ADR-008
small-model-first tools · ADR-009 open/closed world · ADR-010 module
charter · ADR-011 provenance + feedback loop.

Deferred to implementation time (not architecture):
- `phosk_ai` loop / manifest format / approval-queue schema — details that
  firm up as we build.
- ~~Chat persistence + `/clear` (frontend ROADMAP gap).~~ Done: the Dioxus
  `AiPanel` reads the persisted transcript, sends through
  `phosk_ai::ai_tools::chat_reply` (which saves a turn only once the model
  has answered) and runs `/clear` via `phosk_ai::ai_spine::clear_chat`. Both
  DB adapters return the transcript in append order (the shared contract
  checks it).
- SurrealDB `Thing` (de)serialization seam — DB-adapter impl (ADR-010
  covers the principle).

Next architecture themes (candidates):
- [ ] Concrete crate-map refresh (supersede README v0.2 tree).
- [ ] Cross-cutting: config, telemetry/observability, schema migration &
      data versioning, concurrency model.
