# Phoskonomia frontend API contract

> **What this is.** The exact HTTP/JSON contract the web frontend (`frontend/app/`) expects from
> the backend (`phosk_api`). The backend today is honest 501 stubs — **the frontend is the authority**:
> every shape below was extracted from how the React app actually *sends* and *reads* data, then
> verified field-by-field against that code. This is a description of what the client needs, not a
> mandate — **you decide** whether to implement each endpoint as-is or change the contract (and the app).
>
> **How it was derived.** For each endpoint: the route file (`backend/bin/phosk_api/src/routes/*.rs`)
> gives the path + verb + a one-line `todo` hint; the app (`frontend/app/src/`) gives the JSON shape —
> from `useGet(path, params)`, `api.{get,post,patch,put,del}(path, body)`, and how the response is
> destructured. `Consumed by` cites the app file:line so you can re-check any claim yourself.

## Conventions

- **Base URL:** `http://127.0.0.1:3000/api/v1` (override via `VITE_API_BASE`). Every path below is relative to it.
- **Format:** JSON in, JSON out. The client sets `Content-Type: application/json` whenever it sends a body.
- **Client:** `frontend/app/src/lib/api.js`. `useGet` does background GETs (quiet); `api.post/patch/put/del`
  are user actions that toast the result. Query strings are built by a `qs()` helper that **drops `undefined`/`null`/`''`**, so optional params simply vanish when unset.
- **Status semantics the app reacts to:** `200/2xx` = OK · `501` = not implemented (shows "NO IMPL YET" + the `todo` tag) · `0` = backend unreachable. Pages tolerate a null body and render an empty "awaiting backend" state, so you can ship endpoints one at a time.
- **IDs & references (ADR-008):** path params and body refs are **human / stable terms** — category NAME, shop NAME, slug ids — **never UUIDs**. Examples reflect this.
- **Money:** CHF, decimal. **Cycle:** one calendar month (see `/cycle/current`).
- **Confidence** on each endpoint: `high` = fields are directly read in app code · `medium` = inferred from the todo + sibling shapes · `low` = the frontend never reads the response (so the shape is your call).
- **Fire-and-forget mutations:** many POST/PATCH actions are followed by a `reload()` of a GET — the app ignores their response body. Those are marked; return any 2xx.

## Endpoint index

### Cycle & dashboard insights

- `GET /cycle/current` — The current budgeting-cycle window: human label, current day index, total days, days left, and an as-of stamp.
- `GET /cycle/current/totals` — KPI roll-ups for the current cycle: budget, spent, remaining, saved/target/projected savings, spent %, savings rate, per-day-to-stay-on-budget, and last-cycle comparison.
- `GET /cycle/current/spend-series` — Spend-trace time series for the dashboard chart: per-day spend, cumulative spend, budget pace line, prior-cycle cumulative, and today's index.
- `GET /cycle/current/top-shops` — Top shops by spend this cycle, for the dashboard 'TOP SHOPS' bar list.
- `GET /insights/dashboard` — AI-generated one-line dashboard narrative insight (model name shown, default 'GEMMA4'), optionally with an estimated savings figure.

### Transactions & receipt lines

- `GET /transactions` — List transactions (filtered/sorted/searched), with summary totals and the available-filter facet lists the page's dropdowns render.
- `POST /transactions` — Create a transaction (manual entry or the photo-upload OCR+LLM pipeline).
- `GET /transactions/{id}` — Transaction detail used by the full receipt modal: reading confidence, OCR source/engine, and detected OCR regions.
- `PATCH /transactions/{id}` — Edit a transaction's top-level fields.
- `DELETE /transactions/{id}` — Delete a transaction.
- `GET /transactions/{id}/lines` — The receipt's line items (name/qty/unit price/total/category/signal/confidence) plus low-confidence count and the signals this receipt feeds. Drives both the inline accordion body and the full receipt modal.
- `PATCH /transactions/{id}/lines/{lineIndex}` — Review/edit one line item: confirm a low-confidence line, or correct its name/qty/unit price and confirm.
- `POST /transactions/{id}/reprocess` — AI re-read nudge: re-run OCR+LLM over the receipt's low-confidence lines.

### Categories & budget envelopes

- `GET /categories` — List budget envelopes (categories) with their cap + spend rollup; drives the Budgets envelope grid, the Dashboard CatRows table AND the Dashboard hero CHANNELS strip (sparkline + usedPct).
- `POST /categories` — Create a new category / budget envelope.
- `GET /categories/{name}` — Category detail for the budget inspector — projection, history average, AI guidance and over-cap amount layered on top of the list record.
- `PATCH /categories/{name}` — Adjust a category's budget cap by a delta (the +/- CHF 10 cap steppers).
- `GET /categories/{name}/transactions` — Transactions in this category for the current cycle — the inspector's 'This cycle' recent list.
- `GET /budget/totals` — Aggregate monthly budget totals for the Budgets KPI band (budget / allocated / spent / projected / remaining + allocation deltas + envelope count).
- `GET /budget/allocation` — Channel-mix allocation breakdown (per-envelope cap segments) plus GEMMA4 trim/balance advice for the allocation console bar.

### Shops

- `GET /shops` — List every shop in the ledger with its transaction count and lifetime total spend.

### alerts

- `GET /alerts` — List active planning alerts shown on the dashboard (tone/tag/head/body/actions).
- `POST /alerts/{id}/apply` — Apply the alert's carried suggestion (e.g. raise/lower cap, move to savings); fire-and-forget then reload.
- `POST /alerts/{id}/dismiss` — Dismiss (remove) an alert; fire-and-forget then reload the list.
- `POST /alerts/{id}/snooze` — Snooze an alert (hide it for some period); fire-and-forget then reload.
- `GET /alerts/{id}/target` — Resolve an alert's deep-link filter target so the VIEW button can navigate to the relevant filtered view.

### recurring

- `GET /recurring` — List standing recurring charges plus an aggregate monthly total for the dashboard.
- `POST /recurring/{name}/mark-paid` — Mark a standing charge as paid this cycle (resolves a 'missing recurring charge' alert).
- `POST /recurring/{name}/confirm` — Confirm an AI-detected recurring charge (promote a candidate to a tracked standing charge).

### subscriptions

- `GET /subscriptions` — List subscriptions; backend owns ordering/grouping and all derived figures.
- `POST /subscriptions` — Create a subscription (no frontend caller exists yet).
- `GET /subscriptions/stats` — Roll-up KPIs for the header summary line and the 4 KPI tiles.
- `GET /subscriptions/billing-sweep` — Billing-sweep timeline: cycle window + per-charge impulses + footer roll-up for the hero chart.
- `POST /subscriptions/detect` — AI scan of transaction history to detect recurring charges, then the frontend reloads everything.
- `GET /subscriptions/{id}` — Subscription detail for the right-dock inspector: all card fields plus note, recent charges, AI guidance, candidate flag.
- `PATCH /subscriptions/{id}` — Edit a subscription (no frontend caller exists yet).
- `POST /subscriptions/{id}/pause` — Pause a subscription; frontend reloads list/stats/sweep/detail.
- `POST /subscriptions/{id}/resume` — Resume a paused subscription; frontend reloads.
- `POST /subscriptions/{id}/cancel` — Cancel a subscription; frontend reloads.
- `POST /subscriptions/{id}/mark-paid` — Mark this cycle's charge as paid/seen; frontend reloads.
- `GET /subscriptions/{id}/charges` — List a subscription's charges (no dedicated frontend caller — the inspector reads charges off the detail object instead).
- `POST /subscriptions/{id}/charges` — Record a charge/payment for a subscription (no frontend caller exists yet).
- `POST /subscriptions/{id}/confirm` — Confirm an AI-detected candidate subscription; frontend reloads.
- `POST /subscriptions/{id}/dismiss` — Dismiss an AI subscription suggestion; frontend reloads.
- `POST /subscriptions/{id}/snooze` — Snooze a subscription's alert; frontend reloads.

### Item signals

- `GET /signals` — List the tracked item-signals (the AI-maintained micro-categories).
- `POST /signals` — Track a new item-signal (approve an AI candidate, or by name).
- `GET /signals/candidates` — List AI-proposed candidate signals not yet tracked.
- `GET /signals/movers` — Top riser / faller signal-movers leaderboard for Analytics.
- `GET /signals/{id}` — Detail for one tracked signal: stats, 12-mo series, recent occurrences.
- `DELETE /signals/{id}` — Untrack / pause a tracked signal.
- `POST /signals/{id}/cap` — Set a soft spending cap (nudge threshold) on a signal.
- `POST /signals/candidates/{id}/track` — Track (approve) a candidate signal from the AI feed.
- `POST /signals/candidates/{id}/dismiss` — Dismiss (reject) a candidate signal the AI proposed.

### debts

- `GET /debts` — List every institutional debt as a flat array of fully-derived debt records (balance, apr, payoff metrics, decay spark).
- `POST /debts` — Create a new debt. No create UI is wired in the current frontend, so request/response shape is unconstrained by the app.
- `GET /debts/stats` — Portfolio roll-ups for the header sub-line, the 4 KPI tiles, the trajectory footer, and the avalanche/snowball target ids.
- `GET /debts/trajectory` — Combined balance-decay trajectory (history + projection to zero) powering the hero SVG, re-fetched whenever the strategy changes.
- `PUT /debts/strategy` — Persist the selected payoff strategy (avalanche/snowball/none). Fire-and-forget; app reloads trajectory + stats after.
- `GET /debts/{id}` — Per-debt detail for the inspector: decay series for the balance chart plus AI payoff guidance text.
- `PATCH /debts/{id}` — Edit a debt's core fields. No edit-debt UI is wired, so request/response shape is unconstrained by the app.
- `DELETE /debts/{id}` — Delete a debt. No delete UI is wired, so no app consumer.
- `GET /debts/{id}/payments` — Recent payment history for the selected debt, shown in the inspector's 'Recent payments' list.
- `POST /debts/{id}/payments` — Record an extra payment toward a debt. Fire-and-forget; app reloads all debt data after.
- `PATCH /debts/{id}/plan` — Adjust a debt's payment plan (monthly amount, payment day, term). Fire-and-forget; app reloads all debt data after.
- `POST /debts/{id}/refinance` — Refinance a high-APR debt (new apr/lender). Fire-and-forget; app sends an empty body and reloads all debt data.

### Personal IOUs

- `GET /personal-ious` — List all personal IOUs (both directions) — drives the IOU ledger columns and PersonCards.
- `POST /personal-ious` — Create a new personal IOU.
- `GET /personal-ious/stats` — Aggregate IOU stats: totals owed-to-you / you-owe, net position, and per-direction counts.
- `PATCH /personal-ious/{id}` — Edit an existing personal IOU.
- `DELETE /personal-ious/{id}` — Delete a personal IOU.
- `POST /personal-ious/{id}/payments` — Record a payment against a personal IOU (partial settlement).
- `POST /personal-ious/{id}/settle` — Mark a personal IOU fully settled (the MARK SETTLED button on every PersonCard).
- `POST /personal-ious/{id}/settle-up` — Settle up an IOU you owe (the SETTLE UP button shown on YOU-OWE / dir==='out' cards).
- `POST /personal-ious/{id}/remind` — Send a reminder for an inbound IOU (the REMIND button shown on OWED-TO-YOU / dir==='in' cards).

### analytics

- `GET /analytics/spend-history` — 12-cycle (oldest->current) monthly spend/savings history powering the SpendTrend hero scope.
- `GET /analytics/spend-history/stats` — Rolled-up stats over the spend history: avg/peak/low, current & previous cycle, savings rate, vs-avg/vs-prev deltas. Feeds the KPI band and trend footer.
- `GET /analytics/category-momentum` — Per-category momentum cards: current cycle spend vs trailing 3-cycle average, with a 12-point sparkline series.
- `GET /analytics/rhythm/weekday` — Weekday discretionary-spend rhythm heatmap: average CHF per weekday plus rollup stats (peak day, max, weekend share).
- `GET /analytics/insights/movers` — AI (GEMMA4) narrative about the cycle's spending movers, plus an optional suggested soft-cap action the user can apply with one click.

### ai

- `GET /ai/feed` — Activity feed for the assistant panel: auto-maintenance items (categorize/reprocess/suggest/detect) the AI emits, each optionally with action buttons.
- `POST /ai/feed/{id}/dismiss` — Dismiss a single feed item; the panel optimistically hides it locally and then reloads the feed.
- `GET /ai/chat` — Chat message history used to seed the assistant transcript when the panel mounts.
- `POST /ai/chat` — Send a chat message to the local model (GEMMA4 via Ollama); the reply is appended to the transcript. May emit track/cap intents server-side.
- `GET /ai/status` — Assistant status: online flag plus model/engine/location, shown in the AI panel header rail and the Config ASSISTANT tile.
- `POST /ai/reprocess` — Global trigger to reprocess all low-confidence items across the dataset (not scoped to one transaction).

### Settings & account

- `GET /settings/preferences` — Fetch the device's stored UI tweak/preference overrides (flat key->value, mirroring the Config-page tweak set).
- `PATCH /settings/preferences` — Persist one or more changed UI preferences (partial flat object of just the edited keys).
- `DELETE /settings/preferences` — Reset all UI preferences back to defaults (clear the device's stored overrides).
- `GET /settings/preferences/defaults` — (Backend stub) Canonical preference defaults — NOT consumed by the current frontend.
- `GET /settings/summary` — Config-page status roll-up: total/changed preference counts plus current AI engine & model.
- `GET /account` — Account profile — frontend only reads AI engine & model from it (as a fallback for the Config ASSISTANT tile).
- `PATCH /account` — (Backend stub) Edit the account profile (holder/iban) — NOT consumed by the current frontend.
- `GET /account/ai/engines` — (Backend stub) List of available local AI engines/models — NOT consumed by the current frontend.
- `PUT /account/ai/engine` — (Backend stub) Select the active AI engine/model — NOT consumed by the current frontend.

### Shell

- `GET /nav/pages` — Navigation pages for the top-bar/mega-menu chrome (key/abbr/glyph/href/to/desc, plus a backend-only `available`).
- `GET /config` — UI config subset — the per-surface preference store (topDateFmt and the other CFG_DEFAULTS keys) layered under localStorage on each page.
- `PATCH /config` — Save a UI config subset — persist a partial set of preference edits (fire-and-forget; localStorage is authoritative).

### exports

- `GET /exports/transactions.csv` — Download the transaction ledger as CSV, applying the same filters/sort the /transactions list uses.
- `GET /exports/budget.csv` — Download the budget snapshot as CSV — one row per envelope (category) plus alert/recurring context.
- `GET /exports/subscriptions.csv` — Download the subscriptions list as CSV — one row per recurring charge.

## Cycle & dashboard insights

_Source: `backend/bin/phosk_api/src/routes//home/ovsiankina/Documents/phoskonomia/backend/bin/phosk_api/src/routes/cycle.rs`_

Serves the current budgeting-cycle window (label/day count), its KPI totals (budget/spent/saved/rates), the spend-trace time series for the dashboard chart, the top shops this cycle, and an AI-generated dashboard narrative insight. All five endpoints are consumed almost entirely by the Dashboard page (frontend/app/src/pages/Dashboard.jsx); /cycle/current additionally feeds the shared TopBar date readout (comps.jsx) and several other pages' cycle labels (Subscriptions, Debts, Analytics).

### `GET /cycle/current`

The current budgeting-cycle window: human label, current day index, total days, days left, and an as-of stamp.

> backend todo: `insights: current cycle window`  ·  confidence: **high**

**Response `200`**

```json
{
  "label": "JUN 2026",
  "day": 18,
  "days": 30,
  "daysLeft": 12,
  "asOf": "18 JUN"
}
```

**Response fields**

- `label` *(string)* — Human cycle label shown in the hero, top bar, KPI tiles and chart hud. Route builds it as uppercased month abbr + year, e.g. 'JUN 2026'.
- `day` *(number (int))* — Current day index within the cycle (1-based, = day-of-month in the route impl). Rendered as 'day {day}' in the BUDGET KPI tile (Dashboard:225) and as {day} in the top-bar date format (comps.jsx:52).
- `days` *(number (int))* — Total days in the cycle (= days in the month; 30 for June). Passed to PhoskChart as days= for the x-axis span and shown as '{days}-day cycle'. Subscriptions defaults C.days || 30.
- `daysLeft` *(number (int))* — Days remaining in the cycle (route computes days - day). Shown as '{daysLeft} days left' in the hero sub and '{daysLeft} D LEFT' in the REMAINING tile.
- `asOf` *(string)* — Short as-of stamp, e.g. '18 JUN' (route builds it as day + month abbr). Only used by the top-bar date format via the {asOf} placeholder (comps.jsx:54).

**Consumed by** — `Dashboard.jsx:28 (cycle = useGet('/cycle/current'))`, `Dashboard.jsx:40 (C = cycle.data || {})`, `Dashboard.jsx:103 (C.label)`, `Dashboard.jsx:107 (C.daysLeft + ' days left')`, `Dashboard.jsx:156 (C.label in SPEND TRACE hud)`, `Dashboard.jsx:161 (days={C.days})`, `Dashboard.jsx:225 (C.label, C.days + '-day cycle · day ' + C.day)`, `Dashboard.jsx:227 (C.daysLeft + ' D LEFT')`, `comps.jsx:26 (TopBar useGet('/cycle/current'))`, `comps.jsx:51-54 (C.label, C.day, C.days, C.asOf interpolated into date readout)`, `Subscriptions.jsx:442,444 (cycle = useGet; cycleDays = C.days || 30)`, `Subscriptions.jsx:534 (C.label)`, `Debts.jsx:575-576 (cycleGet = useGet; cycle = data || {})`, `Debts.jsx:106 (cycle.label)`, `Analytics.jsx:330-331 (cycle.data.label || 'THIS CYCLE')`

**Notes** — App reads C = cycle.data || {} and guards EVERY field with `!= null` before rendering, so any field may be absent and the UI falls back to '' or '—' (never crashes). The route's own header comment (cycle.rs:27-33), the cycle_window impl (lines 35-47), and the passing test june_19_2026 (lines 65-71) lock the exact key names and types: label, day, days, daysLeft, asOf. No query params, no request body. Response is a single bare object (not wrapped, not an array).

---

### `GET /cycle/current/totals`

KPI roll-ups for the current cycle: budget, spent, remaining, saved/target/projected savings, spent %, savings rate, per-day-to-stay-on-budget, and last-cycle comparison.

> backend todo: `insights: cycle KPI totals (budget/spent/saved/rates)`  ·  confidence: **high**

**Response `200`**

```json
{
  "budget": 4200,
  "spent": 2680.45,
  "remaining": 1519.55,
  "spentPct": 64,
  "saved": 520,
  "savingsTarget": 800,
  "savingsProjected": 690,
  "savingsRate": 0.18,
  "vsLastCyclePct": -7,
  "perDayToStayOnBudget": 126,
  "lastCycleSpent": 2880
}
```

**Response fields**

- `budget` *(number (CHF))* — Total cycle budget. Big number in BUDGET tile and chart hud denominator; also passed to PhoskChart budget= (drives gridlines/maxY).
- `spent` *(number (CHF))* — Amount spent so far. Hero SNAPSHOT 'Spent', chart hud numerator, SPENT KPI tile, and 'CHF {chf(spent)}' detail line (chf = 2 decimals).
- `remaining` *(number (CHF))* — Budget minus spent. The hero big REMAINING number and the REMAINING KPI tile.
- `spentPct` *(number (percent, 0-100))* — Percent of budget spent, expressed 0-100. App divides by 100 before formatting: pct(T.spentPct/100). Used in hero sub, chart hud, and SPENT tile lbl.
- `saved` *(number (CHF))* — Amount saved this cycle. SavingsDial saved= (Dashboard:115) and SNAPSHOT 'Saved'.
- `savingsTarget` *(number (CHF))* — Savings goal for the cycle. SavingsDial target= (denominator of the gauge sweep; SavingsDial defaults target=1).
- `savingsProjected` *(number (CHF))* — Projected end-of-cycle savings. SavingsDial projected= (the secondary/projection arc).
- `savingsRate` *(number (fraction 0-1))* — Savings rate as a fraction; formatted directly via pct() which multiplies by 100 (Dashboard:23). SNAPSHOT 'Savings rate' and RATES tile.
- `vsLastCyclePct` *(number (percent, signed int))* — Change vs last cycle as a signed integer percent; rendered as '+{n}%' / '{n}%' directly with no /100 (Dashboard:127,232). SNAPSHOT 'vs last cycle' and RATES tile.
- `perDayToStayOnBudget` *(number (CHF/day))* — Daily spend allowed to stay on budget. REMAINING tile sub: 'CHF {chf0}/day to stay on budget' (Dashboard:227, chf0 = no decimals).
- `lastCycleSpent` *(number (CHF))* — Total spent in the previous cycle. RATES tile 'Last cycle spent' (Dashboard:233).

**Consumed by** — `Dashboard.jsx:29 (totals = useGet('/cycle/current/totals'))`, `Dashboard.jsx:41 (T = totals.data || {})`, `Dashboard.jsx:104-107 (ok200(totals.status); T.remaining, T.budget, T.spentPct)`, `Dashboard.jsx:115 (SavingsDial saved={T.saved} target={T.savingsTarget} projected={T.savingsProjected})`, `Dashboard.jsx:124-127 (T.spent, T.saved, T.savingsRate, T.vsLastCyclePct)`, `Dashboard.jsx:157 (T.spent, T.budget, T.spentPct)`, `Dashboard.jsx:161 (budget={T.budget} into PhoskChart)`, `Dashboard.jsx:225-227 (T.budget, T.spentPct, T.spent, T.remaining, T.perDayToStayOnBudget)`, `Dashboard.jsx:231-233 (T.savingsRate, T.vsLastCyclePct, T.lastCycleSpent)`

**Notes** — Entire object gated behind ok200(totals.status): until the endpoint returns 200, every consumer shows an <Awaiting/> placeholder or '—'. T = totals.data || {}; each field guarded with `!= null`, so all fields are optional and may be omitted. CRITICAL unit distinction verified in code: spentPct is 0-100 (app divides by 100), savingsRate is a 0-1 fraction (pct() multiplies by 100), and vsLastCyclePct is a signed integer percent used as-is. Amounts are plain numbers in CHF; the app formats them (chf0 = no decimals for big numerals, chf = 2 decimals for detail lines). Single bare object, not wrapped, not an array. NOTE: this is the ni!-stub route file — field shapes are inferred entirely from the frontend (the route has no header comment or test pinning these keys, unlike /cycle/current), but every listed field is directly read by Dashboard.jsx, so confidence stays high.

---

### `GET /cycle/current/spend-series`

Spend-trace time series for the dashboard chart: per-day spend, cumulative spend, budget pace line, prior-cycle cumulative, and today's index.

> backend todo: `insights: spend-trace series (daily/cumulative/pace, compare=lastCycle)`  ·  confidence: **high**

**Query params**

- `compare` *(string, optional)* — Comparison mode; the app always sends compare=lastCycle. When set, the backend should include lastCycleCumulative so the chart can draw the dashed prior-cycle line.

**Response `200`**

```json
{
  "daily": [0, 145.2, 0, 88.6, 210, 0, 47.5, 0, 320.4, 12.9, 0, 64, 198, 0, 0, 176.3, 92.1, 130.5],
  "cumulative": [0, 145.2, 145.2, 233.8, 443.8, 443.8, 491.3, 491.3, 811.7, 824.6, 824.6, 888.6, 1086.6, 1086.6, 1086.6, 1262.9, 1355, 1485.5],
  "pace": [0, 140, 280, 420, 560, 700, 840, 980, 1120, 1260, 1400, 1540, 1680, 1820, 1960, 2100, 2240, 2380],
  "lastCycleCumulative": [0, 96, 96, 240, 380, 380, 520, 600, 880, 940, 940, 1090, 1300, 1300, 1320, 1500, 1640, 1820]
}
```

**Response fields**

- `cumulative` *(number[] (CHF))* — Running cumulative spend; one entry per day. Drives the coral cumulative line + filled area; its LAST point is the 'today' dot and the today vertical marker. The chart renders nothing meaningful (just the grid) until this has points (prims.jsx:84-87,128-133).
- `pace` *(number[] (CHF))* — Budget-pace reference line (linear spend allowance); one entry per day. Drives the dashed indigo BUDGET PACE polyline (prims.jsx:90,126).
- `lastCycleCumulative` *(number[] (CHF))* — Prior-cycle cumulative spend for comparison; one entry per day. Drives the dashed LAST CYCLE polyline (prims.jsx:93,124). Returned because the app sends compare=lastCycle. NOTE: the chart prop is named `lastCumulative` but is fed from the response key `lastCycleCumulative` (Dashboard:162).
- `daily` *(number[] (CHF))* — Per-day spend amounts; one entry per cycle day. Passed to PhoskChart but ONLY rendered when showBars=true (prims.jsx:118) — the dashboard passes showBars={false} (Dashboard:160), so daily is effectively UNUSED on this page. Speculative for now.
- `todayIndex` *(number (int, 0-based))* — Passed to PhoskChart as today= (Dashboard:161), but the chart destructures `today` (prims.jsx:73) and NEVER references it — the today marker/dot is derived purely from the last cumulative point (prims.jsx:87,131-133). So this field is currently UNREAD by rendering. Kept because the prop is wired and a date-aligned today index is plausibly intended. Speculative.

**Consumed by** — `Dashboard.jsx:30 (series = useGet('/cycle/current/spend-series', { compare: 'lastCycle' }))`, `Dashboard.jsx:42 (S = series.data || {})`, `Dashboard.jsx:159 (ok200(series.status))`, `Dashboard.jsx:160-162 (PhoskChart today={S.todayIndex} daily={S.daily} cumulative={S.cumulative} pace={S.pace} lastCumulative={S.lastCycleCumulative})`, `prims.jsx:73-93,118-133 (PhoskChart: cumulative drives coral line+area+today dot, pace drives dashed indigo line, lastCumulative drives dashed prior-cycle line; daily only used when showBars=true; today destructured but unused)`

**Notes** — Read as S = series.data || {}, all gated behind ok200(series.status). All arrays default to [] in PhoskChart, so each field is optional. Array indices align to cycle day positions (0..days-1); x-axis span comes from C.days (the /cycle/current 'days'), NOT from array length. Single bare object. The query key/value (compare=lastCycle) is part of the useGet cache key (qs() in api.js), so the path the backend actually receives is /cycle/current/spend-series?compare=lastCycle. cumulative/pace/lastCycleCumulative are solidly consumed; daily and todayIndex are passed-but-unread on the dashboard (downgraded to speculative).

---

### `GET /cycle/current/top-shops`

Top shops by spend this cycle, for the dashboard 'TOP SHOPS' bar list.

> backend todo: `insights: top shops this cycle`  ·  confidence: **high**

**Response `200`**

```json
{
  "maxTotal": 612.4,
  "shops": [
    { "shop": "Migros", "total": 612.4 },
    { "shop": "Coop", "total": 488.15 },
    { "shop": "Denner", "total": 203.9 },
    { "shop": "SBB", "total": 142 }
  ]
}
```

**Response fields**

- `shops` *(array)* — Array of shop spend objects, sorted descending by total; app shows first 4 (Dashboard:247). If not an array, becomes [] and shows 'No shops this cycle.'.
- `shops[].shop` *(string)* — Shop NAME (human stable id per ADR-008, not a UUID). Row label, uppercased via CSS, and used as the React key (Dashboard:250,252).
- `shops[].total` *(number (CHF))* — CHF spent at the shop this cycle. Shown formatted via chf() (2 decimals); drives the indigo bar width (Dashboard:253,256).
- `maxTotal` *(number (CHF))* — Optional bar-width denominator (largest total). Falls back to shopList[0].total || 1 when missing (Dashboard:248).

**Consumed by** — `Dashboard.jsx:31 (shops = useGet('/cycle/current/top-shops'))`, `Dashboard.jsx:43-44 (shopsData = shops.data || {}; shopList = Array.isArray(shopsData.shops) ? shopsData.shops : [])`, `Dashboard.jsx:241 (ok200(shops.status))`, `Dashboard.jsx:247-256 (shopList.slice(0,4).map(s => s.shop, s.total); denom = shopsData.maxTotal || shopList[0].total || 1)`

**Notes** — Wrapped object — NOT a bare array. App reads shops.data.shops (only the `shops` key; does NOT also try `items`, unlike some other list endpoints on this page). If shopsData.shops is not an array it becomes [] and shows 'No shops this cycle.'. Gated behind ok200(shops.status). maxTotal is optional (fallback chain shopsData.maxTotal || shopList[0].total || 1). shop is a human name per ADR-008.

---

### `GET /insights/dashboard`

AI-generated one-line dashboard narrative insight (model name shown, default 'GEMMA4'), optionally with an estimated savings figure.

> backend todo: `ai: dashboard narrative insight (GEMMA4)`  ·  confidence: **high**

**Response `200`**

```json
{
  "model": "GEMMA4",
  "text": "Groceries are pacing 12% above last cycle — trimming Migros runs to weekly could keep you under budget.",
  "estimatedSavings": 85
}
```

**Response fields**

- `model` *(string)* — Model name shown in the panel header ('{model} · INSIGHT'). Optional; app falls back to literal 'GEMMA4' (Dashboard:312).
- `text` *(string)* — The narrative insight sentence. Optional; app falls back to 'No insight yet.' (Dashboard:316).
- `estimatedSavings` *(number (CHF))* — Optional estimated savings; when present (!= null), appended as ' · est. CHF {chf(estimatedSavings)}' (2 decimals). Hidden when null/absent (Dashboard:316).

**Consumed by** — `Dashboard.jsx:32 (insight = useGet('/insights/dashboard'))`, `Dashboard.jsx:45 (insightData = insight.data || {})`, `Dashboard.jsx:312 (insightData.model || 'GEMMA4')`, `Dashboard.jsx:313 (ok200(insight.status))`, `Dashboard.jsx:316 (insightData.text || 'No insight yet.'; insightData.estimatedSavings != null ? 'est. CHF {chf}' : null)`

**Notes** — Read as insightData = insight.data || {}, gated behind ok200(insight.status). All three fields optional with explicit fallbacks (model→'GEMMA4', text→'No insight yet.', estimatedSavings hidden when null). Single bare object. Confidence high because each field is directly read, though the AI nature means the backend is freer about content/wording. estimatedSavings formatted with chf() (2 decimals).

---

## Transactions & receipt lines

_Source: `backend/bin/phosk_api/src/routes//home/ovsiankina/Documents/phoskonomia/backend/bin/phosk_api/src/routes/transactions.rs`_

The ledger: itemized receipts (transactions) and their OCR/LLM-parsed line items. The web frontend reads this for the Transactions list page (filterable/sortable dated list with inline accordion rows + full receipt modal with OCR confidence) and for the Dashboard "Recent" tape. Each receipt carries per-line item-signal pills, low-confidence flags, and an AI re-read ("reprocess") nudge. Per ADR-008, all refs are human/stable terms (shop NAME, category NAME, slug-style transaction id) — never UUIDs.

### `GET /transactions`

List transactions (filtered/sorted/searched), with summary totals and the available-filter facet lists the page's dropdowns render.

> backend todo: `ledger: list transactions (filters/sort/search)`  ·  confidence: **high**

**Query params**

- `period` *(string, optional)* — Time horizon. Frontend sends one of day|week|month|quarter|year (mapped from the TODAY/7D/MONTH/QUARTER/YEAR horizon buttons; HZ_PERIOD in Transactions.jsx:23). The ALL button maps to '' which qs() DROPS entirely (qs filters out '' / null / undefined, api.js:43), so absence = all-time. Default page state is MONTH → period=month.
- `shop` *(string, optional)* — Filter by shop NAME (the selected option from available_shops). Sent as `shop || undefined`, so omitted when 'ALL SHOPS' (empty) is selected.
- `category` *(string, optional)* — Filter by category NAME (from available_categories). Sent as `cat || undefined`, omitted when 'ALL CATEGORIES'. NOTE: Dashboard alert deep-links navigate to /transactions?category=<name> (comps.jsx:287), but that is a react-router URL on the page, not a value this useGet currently re-reads from the URL — the page's `cat` state starts '' regardless.
- `sort` *(string, optional)* — Sort key, one of date|amount|shop (FilterBar select, Transactions.jsx:170-174). Always sent; default 'date'.
- `q` *(string, optional)* — Free-text search over shop or item name. Debounced 300ms (Transactions.jsx:388-391), sent as `qParam || undefined`. Omitted when empty.
- `limit` *(number, optional)* — Max rows to return. Only the Dashboard sends this (limit=9, Dashboard.jsx:34); the Transactions page never sends it.

**Response `200`**

```json
{
  "transactions": [
    {
      "id": "tx-2026-06-14-migros-001",
      "date": "14 JUN",
      "shop": "Migros",
      "category": "Groceries",
      "amount": 47.85,
      "item_count": 9,
      "low_conf_count": 2,
      "signal_ids": ["coffee", "gruyere"],
      "fixed": false,
      "flag": true
    },
    {
      "id": "tx-2026-06-14-sbb-002",
      "date": "14 JUN",
      "shop": "SBB",
      "category": "Transport",
      "amount": 6.80,
      "item_count": 1,
      "low_conf_count": 0,
      "signal_ids": [],
      "fixed": true,
      "flag": false
    },
    {
      "id": "tx-2026-06-13-coop-003",
      "date": "13 JUN",
      "shop": "Coop",
      "category": "Groceries",
      "amount": 23.40,
      "item_count": 5,
      "low_conf_count": 0,
      "signal_ids": ["pain"],
      "fixed": false,
      "flag": false
    }
  ],
  "summary": {
    "entry_count": 38,
    "period_label": "JUNE 2026",
    "total_amount": 1284.55
  },
  "available_shops": ["Migros", "Coop", "Denner", "SBB", "Manor"],
  "available_categories": ["Groceries", "Transport", "Dining", "Household"]
}
```

**Response fields**

- `transactions` *(array<object>)* — The list rows. Read as body.transactions; if not an array the page falls back to []. Dashboard also accepts a bare array OR {transactions:[...]} (Dashboard.jsx:47), but the Transactions page ONLY reads .transactions — so wrap rows in an object for compatibility with both.
- `transactions[].id` *(string)* — Stable slug-style transaction id (ADR-008, not a UUID). Used as React key and as the path segment for /transactions/{id}, /lines, /reprocess. Also drives the open-accordion Set.
- `transactions[].date` *(string)* — Display date label, e.g. '14 JUN'. The page splits it on a single space into [day, month] for the two-line date cell (Transactions.jsx:130), AND groups consecutive rows by exact-equal date string (groupByDay, line 343) — so identical day-rows must share the identical label and the list must already be in display order.
- `transactions[].shop` *(string)* — Shop NAME (human, ADR-008). Rendered as the primary row label (Transactions.jsx:139, comps.jsx:244); also a search/filter facet value.
- `transactions[].category` *(string)* — Category NAME. Rendered as a tag (Transactions.jsx:141); TxnTape falls back to t.cat if t.category absent (comps.jsx:245, prefer t.category).
- `transactions[].amount` *(number)* — Receipt total in CHF (Decimal-as-number). Rendered via chf() (apostrophe thousands, dot decimal). Non-finite → '—'.
- `transactions[].item_count` *(number)* — Number of line items on the receipt. Shown as 'N items' (Transactions.jsx:142). Defaults to 0 if null (line 132).
- `transactions[].low_conf_count` *(number)* — Count of low-confidence (<0.7) lines. >0 renders a ⚠ mark on the row (line 145) and (as t.low_conf_count) seeds the receipt-body 'N low-confidence — AI re-reading' nudge when the /lines payload omits lowConf (lines 99, 202). Defaults 0 (line 133).
- `transactions[].signal_ids` *(array<string>)* — Item-signal slug ids this receipt feeds (e.g. 'coffee','gruyere'). Each renders a small ⌁ mark on the row (Transactions.jsx:144). Defaults [] (line 131).
- `transactions[].fixed` *(boolean)* — True if a fixed/recurring charge → renders a '· FIXED' tag (Transactions.jsx:146) and a blue dot (Dashboard tape, comps.jsx:245).
- `transactions[].flag` *(boolean)* — Dashboard-only: true renders a ⚠ 'needs review' glyph before the shop in TxnTape (comps.jsx:244). The Transactions page uses low_conf_count/signal_ids instead and does NOT read flag.
- `transactions[].cat` *(string)* — Legacy alias for category, only consulted by TxnTape as a fallback (t.category || t.cat, comps.jsx:245). Backend should emit 'category'; this is optional/ignorable.
- `summary` *(object)* — Header totals. Defaults to {} (then per-field fallbacks apply).
- `summary.entry_count` *(number)* — Total matching entries for the period (header '<N> entries', Transactions.jsx:443). Falls back to rows.length if null (line 408).
- `summary.period_label` *(string)* — Human label for the active period, e.g. 'JUNE 2026' (Transactions.jsx:443). Falls back to the raw horizon button text (e.g. 'MONTH') if null (line 409). Also passed to the signal sheet as cycleLabel (line 469).
- `summary.total_amount` *(number)* — Sum of matching amounts in CHF for the header ('CHF <total>', via chf(), Transactions.jsx:443). '—' if non-finite/absent.
- `available_shops` *(array<string>)* — Distinct shop names to populate the 'ALL SHOPS' filter dropdown (Transactions.jsx:405, 165). Must be an array else [].
- `available_categories` *(array<string>)* — Distinct category names to populate the 'ALL CATEGORIES' filter dropdown (Transactions.jsx:406, 168). Must be an array else [].

**Consumed by** — `Transactions.jsx:401 useGet('/transactions', params)`, `Transactions.jsx:403 rows = body.transactions (Array.isArray else [])`, `Transactions.jsx:404 summary = body.summary || {}`, `Transactions.jsx:405 shops = body.available_shops (array)`, `Transactions.jsx:406 cats = body.available_categories (array)`, `Transactions.jsx:408 summary.entry_count (fallback rows.length)`, `Transactions.jsx:409 summary.period_label (fallback horizon)`, `Transactions.jsx:443 summary.total_amount (via chf())`, `Transactions.jsx:129-130 t.date (string, split on space into day/month)`, `Transactions.jsx:131 t.signal_ids (array)`, `Transactions.jsx:132 t.item_count`, `Transactions.jsx:133 t.low_conf_count`, `Transactions.jsx:139 t.shop`, `Transactions.jsx:141 t.category`, `Transactions.jsx:146 t.fixed`, `Transactions.jsx:150 t.amount (via chf())`, `Transactions.jsx:457 t.id (row key, accordion open-set, /lines+/reprocess+/{id} path)`, `Transactions.jsx:343 t.date (groupByDay key)`, `Dashboard.jsx:34 useGet('/transactions', { limit: 9 })`, `Dashboard.jsx:47 txnList = txns.data.transactions (bare-array OR {transactions:[]})`, `comps.jsx:242 TxnTape t.id`, `comps.jsx:243 t.date`, `comps.jsx:244 t.flag (boolean → 'needs review' ⚠), t.shop`, `comps.jsx:245 t.fixed, t.category || t.cat`, `comps.jsx:246 t.amount (via chf())`

**Notes** — List-vs-object: the Transactions page reads ONLY body.transactions (Transactions.jsx:403); Dashboard tolerates a bare array too (Dashboard.jsx:47). To satisfy both, return an OBJECT with a transactions:[] key. The page renders the empty 'Awaiting' state unless status===200 AND rows.length>0 (Transactions.jsx:420, 451). All amounts are plain numbers (CHF), formatted client-side by chf(); never send pre-formatted strings. 'date' is a pre-formatted display label, not an ISO date — and rows must arrive pre-sorted in display order because groupByDay only coalesces *consecutive* equal labels (line 339-347). Every listed sub-field is optional with a client fallback EXCEPT id+date which are structurally required (key/grouping). signal_ids slugs ('coffee','pain','beer','gruyere' per SIG_SHORT map, line 20) are the same ids consumed by /signals/{id}.

---

### `POST /transactions`

Create a transaction (manual entry or the photo-upload OCR+LLM pipeline).

> backend todo: `ledger: create transaction (manual + photo-upload pipeline)`  ·  confidence: **low**

**Response** — frontend ignores the body (fire-and-forget; the page just calls `reload()`). Any 2xx = success.

**Notes** — NO consumer found anywhere in frontend/app/src/ — there is no manual-add or photo-upload call site wired to POST /transactions in the current React app (grep confirms the only POST under this domain is .../reprocess). Shape is a genuine guess: a manual entry would plausibly carry {shop, date, category, amount, lines?} and a photo import would carry an uploaded blob/multipart, but the frontend defines neither today. Treat as unspecified until a create-UI is built. Per ADR-008 any refs would be shop/category NAMES, not UUIDs.

---

### `GET /transactions/{id}`

Transaction detail used by the full receipt modal: reading confidence, OCR source/engine, and detected OCR regions.

> backend todo: `ledger: transaction detail (lines, OCR regions, confidence)`  ·  confidence: **high**

**Path params**

- `id` — Transaction id (slug-style, ADR-008) — the same id from the list row's transactions[].id.

**Response `200`**

```json
{
  "avg_confidence": 0.82,
  "source": {
    "type": "photo",
    "ocr_engine": "PaddleOCR"
  },
  "ocr_regions": [
    { "x": 0.10, "y": 0.22, "w": 0.80, "h": 0.05 },
    { "x": 0.10, "y": 0.28, "w": 0.80, "h": 0.05 }
  ]
}
```

**Response fields**

- `avg_confidence` *(number)* — Mean OCR/LLM reading confidence 0..1. Drives the 'READING CONFIDENCE' bar (width = round(avg*100)%, Transactions.jsx:266) and its '%' label (line 267); bar turns green (var(--ok)) at >=0.85 else amber (var(--warn)). null → bar at 0% and label '—'.
- `source` *(object)* — Provenance of the receipt. Defaults to {} (Transactions.jsx:206).
- `source.type` *(string)* — Source kind, e.g. 'photo' | 'manual'. Uppercased into the 'SOURCE · <TYPE>' badge (Transactions.jsx:262). Falls back to 'PHOTO' if absent.
- `source.ocr_engine` *(string)* — OCR engine name, e.g. 'PaddleOCR'. Uppercased into the annotated-scan header ('<ENGINE> · N REGIONS', Transactions.jsx:229). Falls back to 'PADDLEOCR' (line 207).
- `ocr_regions` *(array)* — Detected OCR bounding regions. The app only reads .length (regionCount = ocr_regions.length || lines.length, Transactions.jsx:209) for the '<N> REGIONS' header — it does NOT currently render the box geometry, so element shape is unread (any array; example shows plausible normalized boxes). Must be an array else [].

**Consumed by** — `Transactions.jsx:196 detailRes = useGet(`/transactions/${id}`)`, `Transactions.jsx:204 detail = detailRes.data || {}`, `Transactions.jsx:205 detail.avg_confidence`, `Transactions.jsx:206 detail.source || {}`, `Transactions.jsx:207 source.ocr_engine (fallback 'PADDLEOCR')`, `Transactions.jsx:208 detail.ocr_regions (array, else [])`, `Transactions.jsx:209 regionCount = ocr_regions.length || lines.length`, `Transactions.jsx:229 ocrEngine + regionCount → '<ENGINE> · N REGIONS' header`, `Transactions.jsx:262 source.type (fallback 'PHOTO', uppercased) → 'SOURCE · PHOTO' badge`, `Transactions.jsx:266-267 avg_confidence → reading-confidence bar width + '%' label`

**Notes** — Only avg_confidence, source.type, source.ocr_engine, and ocr_regions.length are actually read; everything else in the response is ignored by the current app. The receipt modal pulls the line table from the separate /lines endpoint, NOT from here (despite the route todo mentioning 'lines') — so this detail body does not need a lines array. avg_confidence is a fraction (0..1), not a percent. All gracefully degrade: missing detail just shows '—'/defaults while /lines still renders.

---

### `PATCH /transactions/{id}`

Edit a transaction's top-level fields.

> backend todo: `ledger: edit transaction`  ·  confidence: **low**

**Response** — frontend ignores the body (fire-and-forget; the page just calls `reload()`). Any 2xx = success.

**Notes** — NO consumer in frontend/app/src/ — the app never calls api.patch('/transactions/{id}') (only per-LINE patches at /transactions/{id}/lines/{lineIndex}). No edit-transaction UI exists yet. Request/response shape is unspecified; by analogy with the list row it would carry editable fields like {shop, category, date, amount} (NAMES per ADR-008). Guess only.

---

### `DELETE /transactions/{id}`

Delete a transaction.

> backend todo: `ledger: delete transaction`  ·  confidence: **low**

**Response** — frontend ignores the body (fire-and-forget; the page just calls `reload()`). Any 2xx = success.

**Notes** — NO consumer in frontend/app/src/ — no delete control is wired in the current React app (grep for api.del('/transactions') returns nothing). Response almost certainly ignored (fire-and-forget + reload pattern used elsewhere). Shape unspecified.

---

### `GET /transactions/{id}/lines`

The receipt's line items (name/qty/unit price/total/category/signal/confidence) plus low-confidence count and the signals this receipt feeds. Drives both the inline accordion body and the full receipt modal.

> backend todo: `ledger: list line items for a receipt`  ·  confidence: **high**

**Path params**

- `id` — Transaction id (slug, ADR-008) — same id as the list row.

**Response `200`**

```json
{
  "lines": [
    {
      "name": "Café en grains 500g",
      "qty": 1,
      "unit_price": 8.90,
      "line_total": 8.90,
      "category": "Groceries",
      "signal_id": "coffee",
      "confidence": 0.93
    },
    {
      "name": "Gruyère AOP 250g",
      "qty": 2,
      "unit_price": 4.75,
      "line_total": 9.50,
      "category": "Groceries",
      "signal_id": "gruyere",
      "confidence": 0.61
    },
    {
      "name": "Pain au chocolat",
      "qty": 3,
      "unit_price": 1.20,
      "line_total": 3.60,
      "category": "Dining",
      "signal_id": null,
      "confidence": 0.88
    }
  ],
  "sigs": ["coffee", "gruyere"],
  "lowConf": 1
}
```

**Response fields**

- `lines` *(array<object>)* — The receipt line items. Read as body.lines; not-array → []. The page shows 'Awaiting' unless status===200 AND lines.length>0 (Transactions.jsx:106, 216).
- `lines[].name` *(string)* — Item name (e.g. 'Gruyère AOP 250g'). Shown as the line label (Transactions.jsx:61, 290); prefixed with 'qty× ' in the OCR pane when qty>1 (line 244).
- `lines[].qty` *(number)* — Quantity. Rendered as '<qty>×<unit_price>' (Transactions.jsx:68, 295); in the OCR pane a qty>1 prefixes the name (line 244). Also used as the prefill in the CORRECT prompt (line 83).
- `lines[].unit_price` *(number)* — Per-unit price in CHF (via chf()). Shown in the '<qty>×<unit_price>' subline (Transactions.jsx:68, 295) and prefilled in CORRECT (line 85).
- `lines[].line_total` *(number)* — Line total in CHF, read directly off the record (NOT recomputed client-side; Transactions.jsx:56). chf() renders '—' when absent.
- `lines[].category` *(string)* — Category NAME for this line. Rendered as the quiet indigo CatTag ONLY when signal_id is absent (Transactions.jsx:63-65, 292 — signal_id takes visual precedence).
- `lines[].signal_id` *(string)* — null | Item-signal slug (e.g. 'coffee','gruyere','pain','beer'). If present, the line shows a coral SignalPill (click → inspect /signals/{id}) instead of the category tag; null/absent → CatTag. SIG_SHORT maps known slugs to short labels (Transactions.jsx:20).
- `lines[].confidence` *(number)* — null | Per-line reading confidence 0..1. Drives the ConfDot tone (>=0.85 ok / >=0.7 blue / <0.7 alert, Transactions.jsx:27) and the 'low' (<0.7) styling that surfaces CONFIRM/CORRECT controls and the ⚠ (lines 55, 70-74, 239, 297-301). Shown as conf.toFixed(2) in the OCR pane (line 246); null → '—' and treated as not-low.
- `sigs` *(array)* — The tracked item-signals this whole receipt feeds. ONLY .length is read (receipt modal: 'This receipt feeds N tracked signals' note, Transactions.jsx:305-308). Element shape unread — an array of slugs is sufficient. Not read by the accordion body (only the modal reads lbody.sigs, line 200).
- `lowConf` *(number)* — Count of low-confidence lines for this receipt. Drives the 'N low-confidence — AI re-reading now' nudge (accordion, Transactions.jsx:118-119) and the 'AI is re-reading N blurred item(s)' nudge (modal, lines 270-273). If null, falls back to the row's t.low_conf_count (lines 99, 201-202). Note the camelCase key (lowConf), unlike the list row's snake_case low_conf_count.

**Consumed by** — `Transactions.jsx:96 useGet(`/transactions/${t.id}/lines`) (accordion body)`, `Transactions.jsx:98 lines = body.lines (Array.isArray else [])`, `Transactions.jsx:99 lowConf = body.lowConf (fallback t.low_conf_count)`, `Transactions.jsx:195 useGet(`/transactions/${id}/lines`) (receipt modal)`, `Transactions.jsx:199 lines = lbody.lines`, `Transactions.jsx:200 sigs = lbody.sigs (array else [])`, `Transactions.jsx:201-202 lowConf = lbody.lowConf (fallback t.low_conf_count)`, `Transactions.jsx:54 l.confidence`, `Transactions.jsx:55 low = confidence < 0.7`, `Transactions.jsx:56 l.line_total`, `Transactions.jsx:61 l.name`, `Transactions.jsx:63-64 l.signal_id (→ SignalPill else CatTag)`, `Transactions.jsx:65 l.category`, `Transactions.jsx:68 l.qty, l.unit_price`, `Transactions.jsx:244 l.qty (prefix), l.name (OCR pane)`, `Transactions.jsx:246 conf.toFixed(2) per OCR line`, `Transactions.jsx:305-308 sigs.length (feeds 'N tracked signals' note)`

**Notes** — Wrap lines in an OBJECT: {lines:[...], sigs:[...], lowConf:N}. The app reads body.lines (never a bare array here) — a bare array would render the empty state. Per-line indices are positional: the array index i is what gets sent to PATCH .../lines/{lineIndex}, so order and indexing must be stable across reloads. line_total is authoritative from the backend (not qty*unit_price client-side). signal_id vs category is mutually-presented (signal wins). confidence is a 0..1 fraction; <0.7 is the low-confidence threshold the UI keys on. Both the accordion (uses t.low_conf_count fallback, ignores sigs) and the modal (reads sigs) hit this same endpoint.

---

### `PATCH /transactions/{id}/lines/{lineIndex}`

Review/edit one line item: confirm a low-confidence line, or correct its name/qty/unit price and confirm.

> backend todo: `ledger: review/edit a line item (category/signal/confirm)`  ·  confidence: **high**

**Path params**

- `id` — Transaction id (slug, ADR-008).
- `lineIndex` — ZERO-BASED positional index of the line within the /lines array (the array index i from .map, Transactions.jsx:115/237/282), NOT a line id. Sent as a path segment, e.g. /transactions/<id>/lines/0.

**Request body**

```json
{
  "name": "Gruyère AOP 250g",
  "qty": 2,
  "unit_price": 4.75,
  "confirmed": true
}
```

**Response** — frontend ignores the body (fire-and-forget; the page just calls `reload()`). Any 2xx = success.

**Consumed by** — `Transactions.jsx:101-102 patchLine = api.patch(`/transactions/${t.id}/lines/${lineIndex}`, patch).then(reload)`, `Transactions.jsx:211-212 patchLine = api.patch(`/transactions/${id}/lines/${lineIndex}`, patch).then(linesRes.reload)`, `Transactions.jsx:72 CONFIRM → onPatchLine(idx, { confirmed: true })`, `Transactions.jsx:87-90 onCorrect → patch { name, confirmed:true, qty?, unit_price? }`, `Transactions.jsx:299 CONFIRM (modal) → patchLine(i, { confirmed: true })`, `Transactions.jsx:300 CORRECT (modal) → onCorrect(i, l, patchLine)`

**Notes** — Two request variants. CONFIRM sends exactly {confirmed: true} (Transactions.jsx:72, 299). CORRECT sends {name: <string>, confirmed: true} and conditionally adds qty: <number> and/or unit_price: <number> ONLY when the prompt value parses to a finite number (Transactions.jsx:87-90) — so qty/unit_price are optional and may be omitted. The route todo says it also handles category/signal, but the current UI sends NO category or signal_id field. RESPONSE IS IGNORED: the call is fire-and-forget — on resolution the app just calls reload() to re-fetch /lines (Transactions.jsx:102, 212). Set responseJson empty; any 2xx works. lineIndex is positional, so the backend must map index→line for the receipt as returned by GET /lines.

---

### `POST /transactions/{id}/reprocess`

AI re-read nudge: re-run OCR+LLM over the receipt's low-confidence lines.

> backend todo: `ai: reprocess low-confidence lines (OCR+LLM re-read)`  ·  confidence: **high**

**Path params**

- `id` — Transaction id (slug, ADR-008) to reprocess.

**Request body**

```json
{}
```

**Response** — frontend ignores the body (fire-and-forget; the page just calls `reload()`). Any 2xx = success.

**Consumed by** — `Transactions.jsx:103-104 reprocess = api.post(`/transactions/${t.id}/reprocess`, {}).then(reload)`, `Transactions.jsx:213-214 reprocess = api.post(`/transactions/${id}/reprocess`, {}).then(linesRes.reload)`, `Transactions.jsx:120 'RE-READ' button (accordion nudge)`, `Transactions.jsx:274 'RE-READ' button (modal nudge)`

**Notes** — Request body is an EMPTY object {} — no parameters are sent (Transactions.jsx:104, 214). RESPONSE IS IGNORED: fire-and-forget — the app calls reload() on the /lines GET afterward to pick up re-read results, never inspecting the POST body. Set responseJson empty; any 2xx works. Triggered from the 'RE-READ' button in the low-confidence nudge (shown only when lowConf>0, Transactions.jsx:118/270).

---

## Categories & budget envelopes

_Source: `backend/bin/phosk_api/src/routes//home/ovsiankina/Documents/phoskonomia/backend/bin/phosk_api/src/routes/categories_budget.rs`_

Categories (budget envelopes) and aggregate budget endpoints powering the Budgets console (frontend/app/src/pages/Budgets.jsx) and the Dashboard (category-budgets card via CatRows + the hero CHANNELS strip). Categories ARE the budget envelopes: each carries its cap (budget), spend, projection, history and a sparkline; cap tuning is a relative-delta PATCH. The /budget/* endpoints give the page-level KPI band (totals) and the channel-mix allocation bar with GEMMA4 trim advice. ADR-008: categories keyed by human NAME (e.g. GROCERIES), not UUID.

### `GET /categories`

List budget envelopes (categories) with their cap + spend rollup; drives the Budgets envelope grid, the Dashboard CatRows table AND the Dashboard hero CHANNELS strip (sparkline + usedPct).

> backend todo: `ledger: list categories with budget rollup (spent/cap/used/proj/spark)`  ·  confidence: **high**

**Response `200`**

```json
{"categories":[{"name":"GROCERIES","budget":800,"spent":612.30,"proj":868,"fixed":false,"items":41,"remaining":187.70,"usedPct":76.5,"spark":[42,18,55,31,48,53],"hist":[765,742,810,788,756,802],"note":"On pace ~CHF 70 over cap."},{"name":"HOUSING","budget":1680,"spent":1680.00,"proj":1680,"fixed":true,"items":1,"remaining":0,"usedPct":100,"spark":[100,100,100,100,100,100],"next":"1 JUL","hist":[1680,1680,1680,1680,1680,1680],"note":"Fixed — rent, charged on the 1st."},{"name":"CLOTHING","budget":150,"spent":0.00,"proj":58,"fixed":false,"items":0,"remaining":150,"usedPct":0,"spark":[],"hist":[120,0,210,45,0,88],"note":"Barely touched this cycle."}]}
```

**Response fields**

- `name` *(string)* — Category/envelope name — the stable human ID (ADR-008). Used as React key, sort key, selection key, and {name} path param. E.g. "GROCERIES".
- `budget` *(number (CHF))* — The envelope cap. capOf() reads c.budget; 0/absent treated as 'NO CAP'/'NO BUDGET'. % used = spent/budget.
- `spent` *(number (CHF))* — Amount spent this cycle. Defaulted to 0 if absent.
- `proj` *(number (CHF))* — Projected end-of-cycle spend. Optional — falls back to spent when null (c.proj != null ? c.proj : spent). Drives projection markers + ON PACE OVER status. Read only by Budgets (EnvCard/EnvMeter/over-sort); the Dashboard does not read it.
- `fixed` *(boolean)* — True for fixed charges (e.g. HOUSING/rent). Suppresses cap stepper, shows FIXED tag, uses c.next. Read by Budgets EnvCard and Dashboard CatRows.
- `items` *(number (int))* — Entry/item count for this cycle. Rendered as 'N ENTRIES' (Budgets EnvCard) / 'N items' (Dashboard CatRows). Inspector shows cat.items or '—'. CatRows interpolates c.items + ' items' unconditionally (renders 'undefined items' if absent).
- `remaining` *(number (CHF))* — Cap minus spent. Optional — recomputed as cap - spent when null. Negative => OVER. Read only by Budgets (EnvCard, inspector).
- `usedPct` *(number — UNIT CONFLICT between pages)* — Fraction/percent of cap used. INCONSISTENT across consumers: Budgets.jsx:416 treats it as a 0..1 fraction (fallback is spent/budget) for the 'USED' sort; Dashboard.jsx:186 renders Math.round(c.usedPct) + '%', i.e. expects 0..100. Both read the SAME field with different units — the backend cannot satisfy both. Optional in both (Budgets recomputes spent/budget; Dashboard shows '—'). Flag this as a frontend bug to reconcile; if forced, emitting 0..1 only breaks the Dashboard percent label, emitting 0..100 only breaks the Budgets sort order.
- `spark` *(number[])* — Sparkline series for the Dashboard hero CHANNELS strip. Dashboard.jsx:178 (Array.isArray(c.spark) ? c.spark : null), :182 renders <Spark data={spark}> only when length > 1, else a blank spacer. Optional — Dashboard-only; Budgets ignores it (the inspector uses c.hist instead).
- `hist` *(number[] (CHF per cycle))* — Per-cycle spend history (last ~6 full cycles) for the inspector mini-chart (HistBars). Budgets.jsx:252 (Array.isArray(cat.hist) ? cat.hist : []). Optional — [] when absent; inspector shows <Awaiting label=HISTORY/> when empty and detail !=200.
- `next` *(string)* — Next charge date label for fixed envelopes, e.g. "1 JUL". Budgets.jsx:101 reads c.next only when c.fixed. Optional.
- `note` *(string)* — Short guidance string used by the inspector as fallback when the detail endpoint's guidance is absent (Budgets.jsx:257 guidance = d.guidance || cat.note || ''). Optional.

**Consumed by** — `Budgets.jsx:365 (useGet('/categories'))`, `Budgets.jsx:370 (Array.isArray(catsData) ? catsData : catsData.categories || catsData.items)`, `Budgets.jsx:372 (catsReady = catsStatus===200 && categories.length>0)`, `Budgets.jsx:387 (capOf = c.budget != null ? c.budget : 0)`, `Budgets.jsx:416 (usedPct(c) = c.usedPct != null ? c.usedPct : spent/budget)`, `Budgets.jsx:418 (sort over: c.proj / c.budget)`, `Budgets.jsx:422 (categories.find(c => c.name === sel))`, `Budgets.jsx:73-105 EnvCard (c.name,c.fixed,c.spent,c.proj,c.remaining,c.items,c.next)`, `Budgets.jsx:248-257 inspector reads list record (cat.spent,cat.proj,cat.hist,cat.remaining,cat.items,cat.note,cat.fixed,cat.name)`, `Dashboard.jsx:33,46 (useGet('/categories'); Array.isArray(cats.data)? : cats.data.categories)`, `Dashboard.jsx:176-188 CHANNELS strip (c.name,c.budget,c.spent,c.spark,c.usedPct)`, `Dashboard.jsx:274 (<CatRows cats={categories} />)`, `comps.jsx:215-229 CatRows (c.budget,c.spent,c.fixed,c.name,c.items)`

**Notes** — RESPONSE WRAPPING: the app accepts THREE shapes interchangeably — bare array [...], OR {categories:[...]}, OR {items:[...]} (Budgets.jsx:370 reads catsData.categories || catsData.items; Dashboard.jsx:46 reads only .categories), so to satisfy BOTH pages emit either a bare array or {categories:[...]}. Every numeric field has a null-safe fallback, so all but `name` are technically optional, but a useful rollup supplies budget/spent/proj/items/remaining + spark + usedPct (for the Dashboard channels) and hist/next/note (for the Budgets inspector). usedPct UNIT IS AMBIGUOUS (see field note) — the example uses 0..100 to satisfy the Dashboard's Math.round(usedPct)+'%'; if you adopt 0..1 the Dashboard percent breaks. catsReady requires HTTP 200 AND non-empty list, else the page shows <Awaiting/>; Dashboard guards on cats.status===200 per section.

---

### `POST /categories`

Create a new category / budget envelope.

> backend todo: `planning: create category / budget envelope`  ·  confidence: **low**

**Request body**

```json
{"name":"GIFTS","budget":120,"fixed":false}
```

**Response** — frontend ignores the body (fire-and-forget; the page just calls `reload()`). Any 2xx = success.

**Notes** — NO consumer in frontend/app/src — grep for api.post('/categories' finds nothing; the page only PATCHes existing caps and never creates an envelope. Request shape is a genuine guess inferred from the create todo and the category fields the list endpoint returns (name + budget cap + fixed). If implemented to mirror the PATCH/list pattern, returning the created category record (same shape as a /categories list item) is the safe choice; the current frontend would ignore the body. Low confidence because the request shape is unobserved.

---

### `GET /categories/{name}`

Category detail for the budget inspector — projection, history average, AI guidance and over-cap amount layered on top of the list record.

> backend todo: `ledger: category detail`  ·  confidence: **high**

**Path params**

- `name` — Category name (the same human ID from the list), URI-encoded by the client. E.g. "DINING%20%26%20CAF%C3%89S" for "DINING & CAFÉS".

**Response `200`**

```json
{"name":"DINING & CAFÉS","projectedSpend":592,"histAvg":377,"guidance":"Crept up 3 cycles running — trim or raise the cap.","overCapAmount":60.20}
```

**Response fields**

- `projectedSpend` *(number (CHF))* — Projected end-of-cycle spend; overrides the list record's proj in the inspector. Optional — falls back to cat.proj then spent.
- `histAvg` *(number (CHF))* — Average spend across the history cycles, shown as 'N-cyc avg'. Optional — '—' when absent.
- `guidance` *(string)* — AI/system guidance line shown in the inspector header (ds) + footer. Optional — falls back to the list record's note, then ''.
- `overCapAmount` *(number (CHF))* — How far spend has exceeded the cap; appended to the footer warning when > 0. Optional.

**Consumed by** — `Budgets.jsx:243 (enc = encodeURIComponent(cat.name))`, `Budgets.jsx:244 (useGet(`/categories/${enc}`, undefined, [cat.name]) -> detail/detStatus/detRes)`, `Budgets.jsx:248 (d = detail || {})`, `Budgets.jsx:251 (proj = d.projectedSpend != null ? d.projectedSpend : (cat.proj ?? spent))`, `Budgets.jsx:256 (histAvg = d.histAvg != null ? d.histAvg : null)`, `Budgets.jsx:257 (guidance = d.guidance || cat.note || '')`, `Budgets.jsx:259 (detailEmpty = detStatus != null && detStatus !== 200)`, `Budgets.jsx:322 (d.overCapAmount != null && d.overCapAmount > 0)`

**Notes** — Detail object is merged ON TOP of the already-loaded list record (cat), so it only needs the four extra fields; the inspector still reads spent/remaining/items/hist/fixed/name/note from the list record, NOT from here. All four fields optional with graceful '—'/fallback. The history mini-chart uses cat.hist (list record), not this endpoint, so HISTORY can render even if detail is unavailable; when detStatus !== 200 and the list record has no hist, the inspector shows <Awaiting label=HISTORY/>. `name` echoed in the example is not read off this response (cat.name comes from the list record).

---

### `PATCH /categories/{name}`

Adjust a category's budget cap by a delta (the +/- CHF 10 cap steppers).

> backend todo: `planning: set / raise / lower category cap`  ·  confidence: **high**

**Path params**

- `name` — Category name (human ID), URI-encoded by the client. E.g. "TRANSPORT".

**Request body**

```json
{"delta":10}
```

**Response** — frontend ignores the body (fire-and-forget; the page just calls `reload()`). Any 2xx = success.

**Consumed by** — `Budgets.jsx:390-392 (step = (name,d) => api.patch(`/categories/${encodeURIComponent(name)}`, { delta: d }).then(() => { reloadCats(); reloadTotals(); reloadAlloc(); }))`, `Budgets.jsx:64-66 CapStepper onStep(-10)/(+10)`, `Budgets.jsx:312-314 inspector steppers onStep(cat.name, -10 / +10)`

**Notes** — REQUEST body is exactly {delta:<number>} where delta is +10 or -10 CHF (the stepper increment); the cap is adjusted RELATIVELY, not set absolutely (despite the todo saying 'set/raise/lower'). FIRE-AND-FORGET: the .then() ignores the response body entirely and instead re-fetches /categories, /budget/totals and /budget/allocation — so responseJson is '' (any 2xx with any/no body works). Lowering below 0 is not guarded client-side; backend should clamp.

---

### `GET /categories/{name}/transactions`

Transactions in this category for the current cycle — the inspector's 'This cycle' recent list.

> backend todo: `ledger: transactions within a category`  ·  confidence: **high**

**Path params**

- `name` — Category name (human ID), URI-encoded by the client. E.g. "GROCERIES".

**Response `200`**

```json
{"transactions":[{"id":"txn-2026-06-19-migros","date":"19 JUN","shop":"MIGROS","amount":53.85},{"id":"txn-2026-06-18-coop-pronto","date":"18 JUN","shop":"COOP PRONTO","amount":12.40},{"id":"txn-2026-06-14-coop","date":"14 JUN","shop":"COOP","amount":88.25}]}
```

**Response fields**

- `id` *(string)* — Transaction id — used only as the React list key (falls back to array index). A slug/human id per ADR-008. Optional.
- `date` *(string)* — Display date label, e.g. "19 JUN". Rendered verbatim.
- `shop` *(string)* — Shop/merchant name, e.g. "MIGROS". Rendered verbatim.
- `amount` *(number (CHF))* — Transaction amount, formatted with chf(t.amount) (2 decimals).

**Consumed by** — `Budgets.jsx:245 (useGet(`/categories/${enc}/transactions`, undefined, [cat.name]) -> txnData)`, `Budgets.jsx:258 (txns = Array.isArray(txnData) ? txnData : txnData.transactions || txnData.rows)`, `Budgets.jsx:295-304 (txns.slice(0,5).map(t => key t.id||i, t.date, t.shop, t.amount))`

**Notes** — RESPONSE WRAPPING: bare array [...], OR {transactions:[...]}, OR {rows:[...]} are all accepted (Budgets.jsx:258). App only renders the first 5. Each item needs only date/shop/amount; id is optional (used as key, index fallback). The block is hidden entirely when the list is empty, so an empty array is a valid 'no transactions this cycle' state.

---

### `GET /budget/totals`

Aggregate monthly budget totals for the Budgets KPI band (budget / allocated / spent / projected / remaining + allocation deltas + envelope count).

> backend todo: `planning: budget totals (budget/allocated/spent/projected/remaining)`  ·  confidence: **high**

**Response `200`**

```json
{"budget":4200,"allocated":4910,"spent":2614.40,"projected":4838,"remaining":1585.60,"overAllocated":710,"unallocated":0,"envelopeCount":10}
```

**Response fields**

- `budget` *(number (CHF))* — Total monthly budget. Drives MONTHLY BUDGET KPI, spentPct = spent/budget, projOver = projected - budget, and the allocation-bar BUDGET marker (budget is passed into AllocationBar separately).
- `allocated` *(number (CHF))* — Sum of envelope caps allocated. Shown in ALLOCATED KPI and as the allocation-bar 'ALLOCATED' figure.
- `spent` *(number (CHF))* — Total spent this cycle. Drives SPENT KPI + spentPct.
- `projected` *(number (CHF))* — Projected total end-of-cycle spend. Drives PROJECTED KPI + over/under-pace sub line.
- `remaining` *(number (CHF))* — Budget minus spent; 'CHF X left of monthly budget' under the SPENT KPI.
- `overAllocated` *(number (CHF))* — Amount caps exceed budget by. When > 0 the ALLOCATED KPI flags OVER and shows 'CHF X over budget'; also drives AllocationBar isOver.
- `unallocated` *(number (CHF))* — Unallocated budget headroom; shown as 'CHF X unallocated' when not over-allocated.
- `envelopeCount` *(number (int))* — Number of envelopes; the headline '<N> envelopes'. Optional — falls back to categories.length when null.

**Consumed by** — `Budgets.jsx:366 (useGet('/budget/totals') -> totalsData)`, `Budgets.jsx:371 (totals = totalsData || {})`, `Budgets.jsx:397-404 (num(totals.budget/allocated/spent/projected/remaining/overAllocated/unallocated); totals.envelopeCount)`, `Budgets.jsx:405-406 (spentPct = spent/budget; projOver = projected - budget)`, `Budgets.jsx:450-495 (KPI band rendering of all of the above)`, `Budgets.jsx:141-160 AllocationBar reads T.allocated/overAllocated/unallocated (totals passed in)`

**Notes** — Object (not array). All fields optional and passed through num() => null when absent/non-finite, rendering '—'. overAllocated vs unallocated are mutually exclusive in display: if overAllocated > 0 show OVER, else show unallocated. The AllocationBar derives isOver from totals.overAllocated > 0 (else false if unallocated present, else null/'—'). envelopeCount falls back to the /categories list length. No query params.

---

### `GET /budget/allocation`

Channel-mix allocation breakdown (per-envelope cap segments) plus GEMMA4 trim/balance advice for the allocation console bar.

> backend todo: `planning: allocation breakdown + GEMMA4 trim advice`  ·  confidence: **high**

**Response `200`**

```json
{"segments":[{"name":"HOUSING","cap":1680,"share":0.342,"fixed":true},{"name":"GROCERIES","cap":800,"share":0.163,"fixed":false},{"name":"DINING & CAFÉS","cap":350,"share":0.071,"fixed":false},{"name":"TRANSPORT","cap":280,"share":0.057,"fixed":false},{"name":"CLOTHING","cap":150,"share":0.031,"fixed":false}],"aiAdvice":{"model":"GEMMA4","text":"trim CHF 710 — Transport has run under cap 3 cycles."}}
```

**Response fields**

- `segments` *(array)* — Per-envelope allocation segments for the channel-mix bar. Empty/missing => bar shows <Awaiting/>.
- `segments[].name` *(string)* — Envelope name; first word (split(' ')[0]) used as the in-bar label, full name in the tooltip. React key (falls back to index).
- `segments[].cap` *(number (CHF))* — Envelope cap; segments with cap <= 0 are skipped. Also feeds the ENVELOPES count and the geometry denominator fallback (sum of caps).
- `segments[].share` *(number (0..1 fraction))* — Optional fraction of the bar width. When null, width is computed from cap/domain geometry.
- `segments[].fixed` *(boolean)* — Optional; renders the segment with the muted 'fixed' shade/class.
- `aiAdvice` *(object)* — GEMMA4 advice block. Rendered only when aiAdvice.text is present.
- `aiAdvice.text` *(string)* — The advice sentence shown after the model tag, e.g. 'trim CHF 710 — Transport has run under cap 3 cycles.'
- `aiAdvice.model` *(string)* — Model label prefix; defaults to 'GEMMA4' when absent.

**Consumed by** — `Budgets.jsx:367 (useGet('/budget/allocation') -> allocData/allocLoading/allocRes)`, `Budgets.jsx:499 (<AllocationBar alloc={allocData} totals={totals} budget={budget} res={allocRes} loading={allocLoading} />)`, `Budgets.jsx:135 (segments = Array.isArray(alloc.segments) ? alloc.segments : [])`, `Budgets.jsx:136 (advice = alloc.aiAdvice || null)`, `Budgets.jsx:138 (empty guard: !alloc || res.status!==200 || segments.length===0 -> <Awaiting/>)`, `Budgets.jsx:147 (capSum = segments.reduce(seg.cap))`, `Budgets.jsx:164-171 (seg.cap, seg.share, seg.fixed, seg.name)`, `Budgets.jsx:183 (segments.filter(s => s.cap>0).length -> ENVELOPES count)`, `Budgets.jsx:185-186 (advice.text, advice.model || 'GEMMA4')`

**Notes** — Object with `segments` (required to render anything) and `aiAdvice` (optional). The monetary figures in the bar's footer (allocated / over- / unallocated) come from /budget/totals passed in as `totals`, and the BUDGET marker from `budget` (also from totals) — NOT from this endpoint — so /budget/allocation needs only segments + aiAdvice. `share` is optional presentation; cap is the load-bearing value (segments with cap<=0 dropped). aiAdvice rendered only if aiAdvice.text truthy; model defaults to 'GEMMA4'. Bar replaced with <Awaiting label=ALLOCATION/> when allocRes.status !== 200 or segments empty.

---

## Shops

_Source: `backend/bin/phosk_api/src/routes//home/ovsiankina/Documents/phoskonomia/backend/bin/phosk_api/src/routes/shops.rs`_

Ledger of shops/merchants — the canonical list of every shop the user has transacted with, with per-shop transaction counts and lifetime totals. NOTE: the React app at frontend/app/src never calls GET /shops directly. The shop data the app DOES consume comes from other route files: /cycle/current/top-shops (cycle domain, read on Dashboard.jsx as shopsData.shops[].shop / .total / maxTotal) and the available_shops array embedded in GET /transactions (a bare string[] of shop names used to populate the Transactions filter dropdown at Transactions.jsx:405). This /shops endpoint is an unconsumed ledger endpoint; its shape below is inferred from the route's own todo hint ("name/txn_count/total") and the field naming of its closest sibling top-shops.

### `GET /shops`

List every shop in the ledger with its transaction count and lifetime total spend.

> backend todo: `ledger: list shops (name/txn_count/total)`  ·  confidence: **low**

**Response `200`**

```json
{
  "shops": [
    { "name": "Migros", "txn_count": 23, "total": 842.55 },
    { "name": "Coop", "txn_count": 17, "total": 611.20 },
    { "name": "Denner", "txn_count": 9, "total": 248.90 },
    { "name": "SBB", "txn_count": 6, "total": 187.00 }
  ]
}
```

**Response fields**

- `shops` *(array)* — List of shop ledger entries. Inferred wrapper key; could equally be a bare top-level array — see notes. SPECULATIVE: no frontend reader for this endpoint.
- `shops[].name` *(string)* — Shop / merchant display name (e.g. "Migros", "Coop"). Per ADR-008 (architectural-design-and-philosophy.md:135, LOCKED 2026-06-17) the human shop name is the stable reference, never a raw UUID. Matches the strings the Transactions filter expects in available_shops. SPECULATIVE: no frontend reader for this endpoint.
- `shops[].txn_count` *(integer)* — Number of transactions recorded against this shop. From the todo hint; no frontend reader.
- `shops[].total` *(number (CHF decimal))* — Lifetime total spend at this shop, in CHF. Sibling top-shops renders an analogous `total` via chf() (Dashboard.jsx:253); no reader for this endpoint.

**Notes** — NOT CONSUMED by frontend/app/src — verified there is no useGet('/shops') or api.get/post/patch/put/delete('/shops') call anywhere (re-grepped every call site; the only shop-related useGet is '/cycle/current/top-shops' at Dashboard.jsx:31). Therefore the response shape is a genuine inference, hence low confidence and SPECULATIVE field tags. Shape is derived from (a) the route's own todo label 'name/txn_count/total' giving the three per-shop fields, and (b) the closest real sibling /cycle/current/top-shops which the Dashboard reads as shopsData.shops[].shop and .total (Dashboard.jsx:44,250,252,253) plus shopsData.maxTotal (Dashboard.jsx:248) — that sibling uses key `shop` for the name and wraps the list under `shops`. I kept the wrapper key `shops` and the field name `name` to match this endpoint's own todo wording ('name'), but the backend could legitimately return a BARE top-level array of {name,txn_count,total} OR reuse the top-shops `shop` key — the frontend cannot disambiguate because it never reads this body. The separate `available_shops` consumer (Transactions.jsx:405, `Array.isArray(body.available_shops) ? body.available_shops : []`) wants a flat string[] of shop names and comes from GET /transactions, not this endpoint. ADR-008: identify shops by NAME, not UUID (verified at architectural-design-and-philosophy.md:135-138). Example values (Migros/Coop/Denner/SBB, CHF amounts) borrowed from the Swiss/CHF domain; the exact numbers are illustrative only. Route confirmed: GET /shops, no path/query params (shops.rs:5-8).

---

## alerts

_Source: `backend/bin/phosk_api/src/routes//home/ovsiankina/Documents/phoskonomia/backend/bin/phosk_api/src/routes/alerts.rs`_

Surfaces the dashboard's "needs attention" planning alerts (budget overruns, on-pace warnings, missing recurring charges, LLM budget-cut suggestions) and the actions that resolve them: apply the carried suggestion, dismiss, snooze, or resolve a deep-link target for the VIEW button. Consumed entirely by the Dashboard page (WATCH strip + NEEDS ATTENTION rail) via the AlertItem component.

### `GET /alerts`

List active planning alerts shown on the dashboard (tone/tag/head/body/actions).

> backend todo: `planning: list alerts (tone/tag/head/body/actions/source)`  ·  confidence: **high**

**Response `200`**

```json
{
  "alerts": [
    {
      "id": "dining-over-2026-06",
      "tone": "alert",
      "tag": "DINING & CAFÉS",
      "head": "17% over budget",
      "body": "CHF 60.20 over the CHF 350 cap with 11 days left in the cycle.",
      "actions": ["RAISE CAP", "DISMISS"]
    },
    {
      "id": "groceries-onpace-2026-06",
      "tone": "warn",
      "tag": "GROCERIES",
      "head": "On pace to exceed",
      "body": "Projected CHF 870 by 30 Jun — about CHF 70 above budget at the current rate.",
      "actions": ["VIEW", "DISMISS"]
    },
    {
      "id": "activ-fitness-missing-2026-06",
      "tone": "llm",
      "tag": "ACTIV FITNESS",
      "head": "Recurring charge not seen",
      "body": "Usually charged CHF 89.00 on the 5th. Not recorded this cycle — paused, or a missing receipt?",
      "actions": ["MARK PAID", "SNOOZE"],
      "relatedRecurring": "Activ Fitness"
    },
    {
      "id": "transport-cut-2026-06",
      "tone": "llm",
      "tag": "TRANSPORT",
      "head": "Suggested budget cut",
      "body": "Under budget 3 cycles running. Lower the cap from CHF 280 to CHF 220 and move CHF 60 to savings?",
      "actions": ["APPLY", "DISMISS"]
    }
  ]
}
```

**Response fields**

- `alerts` *(Alert[])* — Array of alert objects. Dashboard accepts EITHER a bare top-level array OR an object wrapping it under the key `alerts` (Dashboard.jsx:50). No other wrapper key is checked.
- `alerts[].id` *(string)* — Stable slug id (ADR-008: human/stable, not a UUID). Used as the React list key (a.id || i, Dashboard.jsx:298) and as the {id} path param for apply/dismiss/snooze/target (comps.jsx:280). Effectively required for the action buttons to work.
- `alerts[].tone` *(string enum: alert)* — warn|llm (any other value falls into the default indigo/⌁ branch) | Severity/source classification. comps.jsx:279 maps 'llm'->⌁, 'warn'->◷, ELSE->⚠. The CSS class is 'alert-i '+tone (comps.jsx:302). The WATCH strip (Dashboard.jsx:81-82) maps 'alert'->coral/⚠, 'warn'->warn/◷, ELSE->indigo/⌁. Only 'alert', 'warn', 'llm' are matched explicitly; everything else hits the default branch. The draft's 'info' value is NOT referenced anywhere in the frontend — removed.
- `alerts[].tag` *(string)* — Short uppercase label (category or shop NAME), e.g. 'DINING & CAFÉS', 'ACTIV FITNESS'. Rendered as the .tg chip (comps.jsx:305) and prefixes the WATCH-strip head (Dashboard.jsx:203). Optional — WATCH strip guards `a.tag ? a.tag+' ' : ''`.
- `alerts[].head` *(string)* — One-line headline, e.g. '17% over budget'. Rendered as .hd (comps.jsx:305) and in the WATCH strip (Dashboard.jsx:203).
- `alerts[].body` *(string)* — Full descriptive sentence with CHF amounts / dates. Rendered as .bd (comps.jsx:306). Not parsed — display only.
- `alerts[].actions` *(string[])* — Ordered button labels. First is styled primary (comps.jsx:308-309: i===0 -> ' p'). Recognized labels drive the action router (comps.jsx:282-299): VIEW -> GET /alerts/{id}/target; DISMISS -> POST /alerts/{id}/dismiss; SNOOZE -> POST /alerts/{id}/snooze; MARK PAID -> POST /recurring/{relatedRecurring}/mark-paid (or POST /alerts/{id}/apply if no relatedRecurring); RAISE CAP / LOWER CAP / APPLY / anything-else -> POST /alerts/{id}/apply. Optional (defaults to []).
- `alerts[].relatedRecurring` *(string (recurring name))* — Optional. Only read when an action label is 'MARK PAID' (comps.jsx:294). If present, MARK PAID hits /recurring/{relatedRecurring}/mark-paid (URL-encoded); if absent it falls back to /alerts/{id}/apply. The value is a recurring item NAME (ADR-008), e.g. 'Activ Fitness'.

**Consumed by** — `Dashboard.jsx:36 (useGet('/alerts'))`, `Dashboard.jsx:50 (alertList = Array.isArray(alerts.data) ? alerts.data : (alerts.data && alerts.data.alerts) || [])`, `Dashboard.jsx:80 (watchItems = alertList.slice(0,3))`, `Dashboard.jsx:203 (a.tone, a.tag, a.head in WATCH strip)`, `Dashboard.jsx:291 (alertList.length count badge)`, `Dashboard.jsx:298 (alertList.slice(0,3).map -> AlertItem, key a.id || i)`, `comps.jsx:279 (a.tone -> icon)`, `comps.jsx:280 (id = a.id)`, `comps.jsx:294 (a.relatedRecurring -> MARK PAID branch)`, `comps.jsx:302 (className 'alert-i ' + a.tone)`, `comps.jsx:305 (a.tag, a.head)`, `comps.jsx:306 (a.body)`, `comps.jsx:308 ((a.actions || []).map -> action buttons)`

**Notes** — Response wrapping: Dashboard.jsx:50 reads `Array.isArray(alerts.data) ? alerts.data : (alerts.data && alerts.data.alerts) || []` — a bare array OR {alerts:[...]} both work; only `alerts` is checked as a wrapper key (no `items`). Each alert needs id+tone+tag+head+body+actions; relatedRecurring is the only conditional field (MARK PAID alerts). The tone enum is alert|warn|llm only — the draft's 'info' is not read by the frontend. Despite the backend todo mentioning 'source', the frontend never reads a `source` field — tone already encodes source ('llm'). No query params. Dashboard shows only .slice(0,3) in both the WATCH strip and the NEEDS ATTENTION rail but uses the full alertList.length for the count badge.

---

### `POST /alerts/{id}/apply`

Apply the alert's carried suggestion (e.g. raise/lower cap, move to savings); fire-and-forget then reload.

> backend todo: `planning: apply an alert suggestion (e.g. cap change + move to savings)`  ·  confidence: **low**

**Path params**

- `id` — The alert's stable slug id from /alerts (alert.id), e.g. 'transport-cut-2026-06'. ADR-008 stable term, not a UUID.

**Request body**

```json
{}
```

**Response** — frontend ignores the body (fire-and-forget; the page just calls `reload()`). Any 2xx = success.

**Consumed by** — `comps.jsx:296 (MARK PAID fallback when no relatedRecurring: api.post(`/alerts/${id}/apply`, {}))`, `comps.jsx:299 (default/APPLY/RAISE CAP/LOWER CAP: api.post(`/alerts/${id}/apply`, {}))`, `comps.jsx:281 (after = (r) => { onChanged(); return r; } -> alerts.reload, Dashboard.jsx:298)`

**Notes** — Frontend sends an EMPTY JSON object body `{}` (comps.jsx:296,299) — no fields. This single endpoint backs the buttons APPLY, RAISE CAP, LOWER CAP, and any unrecognized action label (the button TEXT is the only hint of intent and is NOT sent in the body), plus MARK PAID when the alert has no relatedRecurring. Response body is IGNORED — the handler only chains `.then(after)` which calls onChanged()/alerts.reload() to refetch the list. Toast/501 handling is by apiCall based on status. responseJson '' (fire-and-forget). Backend must distinguish which suggestion to apply purely from the alert id's server-side state.

---

### `POST /alerts/{id}/dismiss`

Dismiss (remove) an alert; fire-and-forget then reload the list.

> backend todo: `planning: dismiss an alert`  ·  confidence: **low**

**Path params**

- `id` — The alert's stable slug id (alert.id), e.g. 'dining-over-2026-06'. ADR-008 stable term, not a UUID.

**Response** — frontend ignores the body (fire-and-forget; the page just calls `reload()`). Any 2xx = success.

**Consumed by** — `comps.jsx:291 (api.post(`/alerts/${id}/dismiss`).then(after))`, `comps.jsx:281 (after -> onChanged -> alerts.reload)`

**Notes** — Called as api.post(path) with NO body argument (comps.jsx:291), so apiCall sends no body. Response IGNORED — only `.then(after)` -> alerts.reload(). requestJson and responseJson '' (fire-and-forget). Toast driven by status.

---

### `POST /alerts/{id}/snooze`

Snooze an alert (hide it for some period); fire-and-forget then reload.

> backend todo: `planning: snooze an alert`  ·  confidence: **low**

**Path params**

- `id` — The alert's stable slug id (alert.id), e.g. 'activ-fitness-missing-2026-06'. ADR-008 stable term, not a UUID.

**Request body**

```json
{}
```

**Response** — frontend ignores the body (fire-and-forget; the page just calls `reload()`). Any 2xx = success.

**Consumed by** — `comps.jsx:292 (api.post(`/alerts/${id}/snooze`, {}).then(after))`, `comps.jsx:281 (after -> onChanged -> alerts.reload)`

**Notes** — Frontend sends an EMPTY JSON object body `{}` (comps.jsx:292) — no snooze-duration field is ever sent; the snooze window is entirely a backend decision. Response IGNORED — only `.then(after)` -> alerts.reload(). responseJson '' (fire-and-forget).

---

### `GET /alerts/{id}/target`

Resolve an alert's deep-link filter target so the VIEW button can navigate to the relevant filtered view.

> backend todo: `planning: resolve an alert's deep-link filter target`  ·  confidence: **high**

**Path params**

- `id` — The alert's stable slug id (alert.id), e.g. 'groceries-onpace-2026-06'. ADR-008 stable term, not a UUID.

**Response `200`**

```json
{
  "category": "Groceries"
}
```

**Response fields**

- `category` *(string (category NAME))* — The ONLY field the frontend reads (comps.jsx:287). When present, the VIEW button navigates to /transactions?category=<category> (URL-encoded). A human category NAME per ADR-008, e.g. 'Groceries'. If absent/empty, VIEW resolves but performs no navigation (the `if (t.category)` guard fails).

**Consumed by** — `comps.jsx:285 (api.get(`/alerts/${id}/target`).then((r) => ...))`, `comps.jsx:286 (t = r.data || {})`, `comps.jsx:287 (if (t.category) navigate('/transactions?category=' + encodeURIComponent(t.category)))`

**Notes** — Triggered only by the VIEW action label (comps.jsx:284). Response read as `r.data || {}` and only `t.category` is consumed; any other returned fields are ignored by the current app. The category value is passed straight into the Transactions page's `category` query param, so it must match the category-NAME vocabulary the transactions filter expects (ADR-008 stable name, not an id).

---

## recurring

_Source: `backend/bin/phosk_api/src/routes//home/ovsiankina/Documents/phoskonomia/backend/bin/phosk_api/src/routes/recurring.rs`_

Dashboard-summary view of standing/recurring charges (subscriptions, telco, gym, insurance). Powers the Dashboard's "NEXT" due dock, the "RECURRING · CLEAN" panel with a monthly total, and the "MARK PAID" action on a missing-charge alert. This is the dashboard/budget-summary read model over standing charges; the full lifecycle UI lives under the separate /subscriptions domain.

### `GET /recurring`

List standing recurring charges plus an aggregate monthly total for the dashboard.

> backend todo: `recurring: list standing charges (dashboard summary, monthlyTotal)`  ·  confidence: **high**

**Response `200`**

```json
{
  "recurring": [
    {
      "id": "swisscom-mobile",
      "name": "Swisscom Mobile",
      "amount": 69.90,
      "cycle": "MONTHLY",
      "status": "due",
      "next": "JUN 20",
      "daysUntil": 2,
      "src": "user"
    },
    {
      "id": "spotify-premium",
      "name": "Spotify Premium",
      "amount": 12.95,
      "cycle": "MONTHLY",
      "status": "soon",
      "next": "JUN 24",
      "daysUntil": 6,
      "src": "llm"
    },
    {
      "id": "fitnesspark",
      "name": "Fitnesspark",
      "amount": 109.00,
      "cycle": "MONTHLY",
      "status": "ok",
      "next": "JUL 01",
      "daysUntil": 13,
      "src": "user"
    }
  ],
  "monthlyTotal": 191.85
}
```

**Response fields**

- `recurring` *(array<object>)* — The list of standing charges. Dashboard.jsx:48 reads the top level either as a bare array OR as this `recurring` key; either wrapping works. Items render in RecRow and feed the nextDue sort.
- `monthlyTotal` *(number)* — Aggregate monthly cost in CHF, formatted as 'CHF {n}/MO' in the RECURRING panel header (Dashboard.jsx:49,301). Optional — if null/absent the header shows '—'. Only read off the OBJECT form of the response (a bare array carries no monthlyTotal).
- `recurring[].id` *(string)* — Stable slug id (ADR-008, not a UUID). Used only as a React key (Dashboard.jsx:140,308; falls back to name/index). Never displayed.
- `recurring[].name` *(string)* — Human charge name, e.g. 'Swisscom Mobile' (Dashboard.jsx:142, comps.jsx:260).
- `recurring[].amount` *(number)* — Charge amount in CHF (decimal, not minor units), rendered 'CHF {chf(amount)}' i.e. 2 decimals (Dashboard.jsx:143, comps.jsx:264).
- `recurring[].cycle` *(string)* — Billing cadence label shown verbatim in RecRow, e.g. 'MONTHLY' / 'YEARLY' (comps.jsx:261). Display-only text, never parsed.
- `recurring[].status` *(string)* — One of 'due' | 'soon' | 'ok'. comps.jsx:255 only branches on 'due' (-> 'alert' tone + '⚠ DUE' badge at :265) and 'soon' (-> 'warn'); any other value falls through to 'ok'.
- `recurring[].next` *(string)* — Pre-formatted next-due date label shown verbatim, e.g. 'JUN 20'. App does NO date math (Dashboard.jsx:145, comps.jsx:265).
- `recurring[].daysUntil` *(number)* — Whole days until next charge. Drives the hero NEXT dock: items with daysUntil != null are sorted ascending and sliced to 3 (Dashboard.jsx:58-61); rendered 'TODAY' if <=0 else 'N DAY(S)' (Dashboard.jsx:145). Optional per item; items lacking it are excluded from nextDue but still appear in the RecRow list.
- `recurring[].src` *(string)* — Provenance; 'llm' -> 'AUTO' badge, anything else -> 'USER' (comps.jsx:267). Indicates AI-detected vs user-entered.

**Consumed by** — `Dashboard.jsx:35 (useGet('/recurring'))`, `Dashboard.jsx:48 (recList = Array.isArray(recurring.data) ? recurring.data : (recurring.data && recurring.data.recurring) || [])`, `Dashboard.jsx:49 (monthlyTotal = recurring.data && recurring.data.monthlyTotal)`, `Dashboard.jsx:58-61 (nextDue: filter r.daysUntil != null, sort asc by daysUntil, slice 3)`, `Dashboard.jsx:140 (key = n.id || n.name)`, `Dashboard.jsx:142 (n.name)`, `Dashboard.jsx:143 (n.amount -> 'CHF '+chf(n.amount))`, `Dashboard.jsx:145 (n.next, n.daysUntil -> 'DUE {next} · TODAY/N DAY(S)')`, `Dashboard.jsx:301 (monthlyTotal -> 'CHF {chf0(monthlyTotal)}/MO')`, `Dashboard.jsx:308 (recList.slice(0,4).map -> RecRow, key = r.id || i)`, `comps.jsx:255 (RecRow tone: r.status==='due'?'alert':r.status==='soon'?'warn':'ok')`, `comps.jsx:260 (r.name)`, `comps.jsx:261 (r.cycle)`, `comps.jsx:264 (r.amount -> 'CHF '+chf(r.amount))`, `comps.jsx:265 (r.next, r.status==='due'?'⚠ DUE':'NEXT '+r.next)`, `comps.jsx:267 (r.src==='llm'?'AUTO':'USER')`

**Notes** — Top-level wrapping: Dashboard.jsx:48 accepts EITHER a bare array `[...]` OR an object `{recurring:[...], monthlyTotal}`. To surface monthlyTotal you MUST return the object form (a bare array has no monthlyTotal -> 'RECURRING · CLEAN' header shows '—'). Recommended: object form. All money is CHF numbers (decimals, not minor units). `next` and `cycle` are display strings the frontend never parses. `daysUntil` is optional per item but required for an item to appear in the hero NEXT dock. `id` is a slug (ADR-008). Background load via useGet (quiet, no toast); on non-200 the panel renders an Awaiting/empty state.

---

### `POST /recurring/{name}/mark-paid`

Mark a standing charge as paid this cycle (resolves a 'missing recurring charge' alert).

> backend todo: `recurring: mark a standing charge paid`  ·  confidence: **high**

**Path params**

- `name` — The recurring charge identifier from the alert's `relatedRecurring` field, URL-encoded (encodeURIComponent). Per ADR-008 this is the human/slug charge name/id (e.g. 'swisscom-mobile' or 'Swisscom Mobile'), NOT a UUID.

**Response** — frontend ignores the body (fire-and-forget; the page just calls `reload()`). Any 2xx = success.

**Consumed by** — `comps.jsx:295 (api.post(`/recurring/${encodeURIComponent(a.relatedRecurring)}/mark-paid`))`, `comps.jsx:293-296 (AlertItem 'MARK PAID' action; only when alert.relatedRecurring is set, else falls back to api.post(`/alerts/${id}/apply`, {}))`, `comps.jsx:281,296 (.then(after) -> onChanged() re-fetches the alert list; response body not read)`

**Notes** — Fire-and-forget: api.post is called with NO request body (comps.jsx:295) -> no Content-Type, no body sent (api.js:51-54). The response body is never read — the handler chains .then(after) which calls onChanged() to reload the /alerts list (comps.jsx:281). This is a user-initiated (non-quiet) call so api.js toasts OK/501/ERR by res.status (api.js:64-68); on 501 it shows the route's `todo` tag. Return 200 with any/empty body. The path param comes straight from alert.relatedRecurring, URL-encoded.

---

### `POST /recurring/{name}/confirm`

Confirm an AI-detected recurring charge (promote a candidate to a tracked standing charge).

> backend todo: `recurring: confirm an AI-detected recurring charge`  ·  confidence: **low**

**Path params**

- `name` — Slug/name of the AI-detected recurring charge to confirm (ADR-008 human/stable id, not a UUID). No frontend call site exists, so the exact encoding cannot be confirmed.

**Response** — frontend ignores the body (fire-and-forget; the page just calls `reload()`). Any 2xx = success.

**Notes** — NO frontend consumer exists for this path anywhere in /frontend/app/src (grep confirmed). The app's AI-detect/confirm flow lives entirely under the /subscriptions domain (Subscriptions.jsx:477 onDetect, :322 onPrimary act('mark-paid')/act('pause')); it never calls /recurring/{name}/confirm. By analogy to the sibling /recurring/{name}/mark-paid (no body, fire-and-forget, response ignored, reload after) the most likely contract is: POST with no/empty body, response ignored, return 200. Shape is a genuine guess (hence low confidence).

---

## subscriptions

_Source: `backend/bin/phosk_api/src/routes//home/ovsiankina/Documents/phoskonomia/backend/bin/phosk_api/src/routes/subscriptions.rs`_

Standing/recurring charges management. Backs the Subscriptions page (frontend/app/src/pages/Subscriptions.jsx): a billing-sweep "impulse train" hero, a KPI roll-up band, a tunable grid/list of every subscription, a right-dock inspector with price history + AI guidance + lifecycle actions, and an AI "detect recurring charges" scan. Every derived figure (monthlyEquiv/annual/daysUntil/nextLabel/status) is READ off the backend objects, never recomputed by the frontend.

### `GET /subscriptions`

List subscriptions; backend owns ordering/grouping and all derived figures.

> backend todo: `recurring: list subscriptions (cadence/status/hist/next-due)`  ·  confidence: **high**

**Query params**

- `sort` *(string, optional)* — Sort key from the SORT control: one of "due" (default), "amount", "name". Backend owns the ordering; frontend never re-sorts.
- `group` *(string, optional)* — Set to "cadence" when the GROUP BY CADENCE toggle is on, otherwise omitted entirely (qs() drops undefined/null/empty-string). Frontend still re-splits into monthly/yearly sections itself off s.cadence, so this param is advisory.
- `amounts` *(string, optional)* — Display mode from the Amounts control: "monthly" (per-charge, default) or "annual". Does not change the returned numeric fields (amount/monthlyEquiv/annual are all read regardless); it only hints which figure the UI emphasizes.

**Response `200`**

```json
{"subscriptions":[{"id":"spotify","name":"SPOTIFY","glyph":"S","category":"SUBSCRIPTIONS","status":"ok","statusLabel":"ACTIVE","cadence":"monthly","amount":12.95,"monthlyEquiv":12.95,"annual":155.4,"daysUntil":17,"nextLabel":"5 JUL","day":5,"since":"MAR 2018","source":"llm","hist":[11.95,11.95,12.95,12.95,12.95,12.95]},{"id":"activ","name":"ACTIV FITNESS","glyph":"A","category":"HEALTH","status":"due","statusLabel":"NOT SEEN","cadence":"monthly","amount":89.0,"monthlyEquiv":89.0,"annual":1068.0,"daysUntil":null,"nextLabel":null,"day":5,"since":"FEB 2024","source":"llm","hist":[89,89,89,89,89,89]},{"id":"serafe","name":"SERAFE","glyph":"R","category":"UTILITIES","status":"ok","statusLabel":"ACTIVE","cadence":"yearly","amount":335.0,"monthlyEquiv":27.92,"annual":335.0,"daysUntil":null,"nextLabel":"MAR 27","day":null,"since":"2019","source":"user","hist":[320,320,335]}]}
```

**Response fields**

- `id` *(string)* — Stable slug id (ADR-008: human/slug, not UUID), e.g. "spotify". Used as React key, data-key for grouping, the inspector selection, and the {id} path param.
- `name` *(string)* — Display name, typically the caps shop/service name, e.g. "SPOTIFY".
- `glyph` *(string)* — 1-2 char monogram shown in the card/row tile, e.g. "S", "Nf".
- `category` *(string)* — Budget category NAME the charge ties to, e.g. "SUBSCRIPTIONS" (read as s.category; ADR-008 name/slug, not UUID).
- `status` *(string)* — Lifecycle/attention state. subStatus() branches on: "due" (NOT SEEN / overdue, coral), "soon" (DUE SOON, warn), "watch" (REVIEW, warn). Any other value (including "ok"/"paused") falls through to the active/ok branch (ACTIVE, blue). "paused" only changes behavior in the inspector (RESUME button), not in the list.
- `statusLabel` *(string)* — Optional human label override for the status pill. Falls back to NOT SEEN/DUE SOON/REVIEW/ACTIVE by status when absent.
- `cadence` *(string)* — "monthly" or "yearly". Drives "/MO" vs "/YR" unit, the cycle-meter shape, yearly vs monthly next-charge labeling, and the group split.
- `amount` *(number)* — Per-charge amount in CHF, e.g. 12.95. Formatted via chf(); whole numbers render with 0 decimals, else 2.
- `monthlyEquiv` *(number)* — Backend-computed monthly-equivalent CHF (yearly/12). Read as s.monthlyEquiv for the card's secondary line (annual mode). Optional — em-dash when null.
- `annual` *(number)* — Backend-computed annualized CHF (monthly*12 or the yearly amount). Read as s.annual for the secondary line and the annual-amount display mode. Optional — em-dash when null.
- `daysUntil` *(number)* — Days until next charge (monthly only; backend-computed, NOT recomputed). Drives "IN Nd", warn tone when <=4, and the cycle-meter fill fraction. Optional/null for yearly or when unknown.
- `nextLabel` *(string)* — Pre-formatted next-charge label, e.g. "22 JUN" or (yearly) "MAR 27". Read directly; em-dash when missing.
- `day` *(number)* — Charge day-of-month for monthly subs (shown after "MONTHLY · "). Optional/null for yearly.
- `since` *(string)* — When tracking began, e.g. "MAR 2018" or "2019". Display only; em-dash when missing.
- `source` *(string)* — Provenance. "llm" => AUTO badge + blue dot (AI-detected); any other value (e.g. "user") => USER badge + ok dot. Also emitted as the data-src attribute on the wrapper.
- `hist` *(number[])* — Price history oldest->newest (monthly: last ~6 charges; yearly: ~3 years), e.g. [11.95,11.95,12.95,...]. In the list it is rendered ONLY as a Spark sparkline (needs length>1); when length<=1 the card shows an em-dash.

**Consumed by** — `Subscriptions.jsx:449 (useGet('/subscriptions', {sort, group, amounts}))`, `Subscriptions.jsx:454 (subs = Array.isArray(listData) ? listData : (listData.subscriptions || listData.items || []))`, `Subscriptions.jsx:455 (listOk = listStatus===200 && subs.length>0)`, `Subscriptions.jsx:483-490 (s.cadence split into monthly/yearly groups when grouping)`, `Subscriptions.jsx:495,503 (key=s.id, data-src=s.source)`, `Subscriptions.jsx:201-253 SubCard (s.id, s.glyph, s.name, s.category, s.source, s.amount, s.monthlyEquiv, s.annual, s.cadence, s.daysUntil, s.nextLabel, s.day, s.since, s.hist, s.status, s.statusLabel)`, `Subscriptions.jsx:256-275 SubRow (s.id, s.glyph, s.name, s.status, s.statusLabel, s.cadence, s.daysUntil, s.nextLabel, s.amount, s.annual, s.source)`, `Subscriptions.jsx:24-30 subStatus (s.status, s.statusLabel)`, `Subscriptions.jsx:182-196 CycleMeter (s.cadence, s.daysUntil, s.status)`, `Subscriptions.jsx:247 (Spark sparkline reads s.hist; needs length>1)`

**Notes** — RESPONSE WRAPPING IS FLEXIBLE: the frontend accepts a bare array OR {subscriptions:[...]} OR {items:[...]} (Subscriptions.jsx:454), defaulting to [] otherwise. Recommend {subscriptions:[...]}. listOk requires status 200 AND a non-empty array — an empty list renders the Awaiting state, so return real data when seeded. amount/hist always present (numeric); monthlyEquiv/annual/daysUntil/nextLabel/day/since/statusLabel are optional (frontend em-dashes when null), but ADR convention is the backend computes the derived figures. The frontend trusts the backend's order for the given `sort` and does not re-sort. priceRose is NOT read in the list (only in the inspector via SubHistBars) — it is documented on GET /subscriptions/{id}, not here; the backend may still return it on list items but the list UI ignores it.

---

### `POST /subscriptions`

Create a subscription (no frontend caller exists yet).

> backend todo: `recurring: create subscription`  ·  confidence: **low**

**Response** — frontend ignores the body (fire-and-forget; the page just calls `reload()`). Any 2xx = success.

**Notes** — NOT CONSUMED by the current app — there is no create form or api.post('/subscriptions', ...) call anywhere in frontend/app/src/. Shape is a genuine guess; when a create UI is added it would presumably POST a subset of the list-item shape (name, category, cadence, amount, day) and the page would call reloadAll() (fire-and-forget). No request/response shape can be grounded in current frontend code.

---

### `GET /subscriptions/stats`

Roll-up KPIs for the header summary line and the 4 KPI tiles.

> backend todo: `recurring: subscription roll-up (monthly/annual/next30/flagged)`  ·  confidence: **high**

**Response `200`**

```json
{"count":11,"monthly":561.6,"annual":6739.2,"autoCount":8,"next30":{"count":4,"total":260.85,"items":[{"name":"SUNRISE","daysUntil":3},{"name":"LINKEDIN","daysUntil":9}]},"flagged":{"count":3,"note":"Flagged by GEMMA4 for review"}}
```

**Response fields**

- `count` *(number)* — Number of active standing charges. Shown in summary line, MONTHLY RECURRING ksub, and the STANDING CHARGES section header. Em-dash when null.
- `monthly` *(number)* — Total monthly run-rate CHF (sum of monthlyEquiv). Coral figure in the summary line + MONTHLY RECURRING tile.
- `annual` *(number)* — Total annualized CHF. Summary line + ANNUALIZED tile.
- `autoCount` *(number)* — How many are AI/auto-detected (source==llm). Optional; appended as "· N auto-detected by GEMMA4" only when present (!= null).
- `next30` *(object)* — NEXT 30 DAYS tile object. Frontend keeps it only if typeof===object, else coerces to {}.
- `next30.count` *(number)* — Count of charges in the next 30 days; rendered as "N CHARGES". Optional (em-dash when null).
- `next30.total` *(number)* — Sum CHF of those charges. Optional; em-dash when null.
- `next30.items` *(array)* — Upcoming charges; only items[0] is read for the "Next · NAME in Nd" caption. Must be an array to be read.
- `next30.items[].name` *(string)* — Subscription name of the next upcoming charge, e.g. "SUNRISE".
- `next30.items[].daysUntil` *(number)* — Days until that charge; rendered "in Nd". Optional.
- `flagged` *(object)* — NEEDS ATTENTION tile object. Frontend keeps it only if typeof===object, else coerces to {}.
- `flagged.count` *(number)* — Number of subs flagged (due/watch) for review; big coral number. Em-dash when null.
- `flagged.note` *(string)* — Optional caption; falls back to "Flagged by GEMMA4 for review" when stats are 200, else "Awaiting backend".

**Consumed by** — `Subscriptions.jsx:458 (useGet('/subscriptions/stats'))`, `Subscriptions.jsx:459-462 (S=statsData; statsOk=status===200&&statsData; next30/flagged coerced to {} if not objects)`, `Subscriptions.jsx:533-534 (S.count, S.monthly, S.annual in the summary line)`, `Subscriptions.jsx:561-562 (S.monthly, S.count, S.autoCount in MONTHLY RECURRING tile)`, `Subscriptions.jsx:566 (S.annual in ANNUALIZED tile)`, `Subscriptions.jsx:570-573 (next30.count, next30.total, next30.items[0].name, next30.items[0].daysUntil)`, `Subscriptions.jsx:578-579 (flagged.count, flagged.note)`, `Subscriptions.jsx:589 (S.count in section header)`, `Subscriptions.jsx:472 (also the harmless detail-fetch fallback when nothing is selected — response ignored in that role)`

**Notes** — All top-level fields optional individually (each guarded with != null and em-dash fallback). next30 and flagged are kept only if they are objects (non-objects coerced to {}). statsOk = res.status===200 && statsData truthy. This same endpoint doubles as the placeholder GET for the inspector when no sub is selected (Subscriptions.jsx:472) — in that role its body is ignored.

---

### `GET /subscriptions/billing-sweep`

Billing-sweep timeline: cycle window + per-charge impulses + footer roll-up for the hero chart.

> backend todo: `recurring: billing-sweep timeline (impulse train)`  ·  confidence: **high**

**Response `200`**

```json
{"cycle":{"day":18,"days":30,"asOf":"18 JUN"},"impulses":[{"id":"zvv","name":"ZVV ABO","amount":85.0,"day":1,"status":"paid"},{"id":"activ","name":"ACTIV FITNESS","amount":89.0,"day":5,"status":"due"},{"id":"swisscom","name":"SWISSCOM","amount":79.0,"day":17,"status":"paid"},{"id":"sunrise","name":"SUNRISE","amount":45.0,"day":22,"status":"soon"}],"footer":{"paidThisCycle":328,"stillDue":134,"next":{"name":"SUNRISE","nextLabel":"22 JUN","amount":45.0},"note":"ACTIV FITNESS not seen on its usual day 5"}}
```

**Response fields**

- `cycle` *(object)* — Current cycle window for axis math. Defaults to {} if absent.
- `cycle.day` *(number)* — Today's day-of-cycle (TODAY marker position). Defaults to 0, which hides the marker.
- `cycle.days` *(number)* — Cycle length in days (axis scale). Defaults to 30.
- `cycle.asOf` *(string)* — Optional as-of date label appended to the TODAY marker, e.g. "18 JUN".
- `impulses` *(array)* — One entry per charge plotted on the train. Must be a non-empty array (and status 200) for the chart to render; else the Awaiting state shows.
- `impulses[].id` *(string)* — Sub slug id; clicking the impulse selects it (must match the list/detail sub id). Also the React key.
- `impulses[].name` *(string)* — Sub name shown in the hover <title>.
- `impulses[].amount` *(number)* — Charge amount CHF; drives impulse height and the small numeric label.
- `impulses[].day` *(number)* — Day-of-cycle the impulse fires (x position).
- `impulses[].status` *(string)* — Per-impulse tone: "due" (coral hollow dashed), "soon"/"watch" (warn), "paid" (dim violet), any other value => upcoming indigo.
- `footer` *(object)* — Roll-up shown beneath the chart. Defaults to {}.
- `footer.paidThisCycle` *(number)* — CHF paid so far this cycle. Em-dash when null.
- `footer.stillDue` *(number)* — CHF still due this cycle (warn tone). Em-dash when null.
- `footer.next` *(object)* — The next-charge summary object, or null/absent (renders em-dash).
- `footer.next.name` *(string)* — Name of the next charge, e.g. "SUNRISE".
- `footer.next.nextLabel` *(string)* — Pre-formatted label for the next charge, e.g. "22 JUN". Optional (blank when missing).
- `footer.next.amount` *(number)* — CHF of the next charge. Optional.
- `footer.note` *(string)* — Optional small note with a blue dot, e.g. a cycle annotation.

**Consumed by** — `Subscriptions.jsx:465 (useGet('/subscriptions/billing-sweep'))`, `Subscriptions.jsx:40 (cycle = sweep.cycle || {})`, `Subscriptions.jsx:41 (impulses = Array.isArray(sweep.impulses) ? sweep.impulses : [])`, `Subscriptions.jsx:42 (footer = sweep.footer || {})`, `Subscriptions.jsx:43 (ok = res.status===200 && impulses.length>0)`, `Subscriptions.jsx:45 (cycle.days || 30, cycle.day || 0)`, `Subscriptions.jsx:106 (cycle.asOf appended to TODAY marker)`, `Subscriptions.jsx:49,116,126 (impulse.amount)`, `Subscriptions.jsx:54,116 (impulse.day)`, `Subscriptions.jsx:69-74,111 (impulse.status -> tone)`, `Subscriptions.jsx:115-116 (impulse.id click-selects + key, impulse.name in hover title)`, `Subscriptions.jsx:136-141 (footer.paidThisCycle, footer.stillDue, footer.next{name,nextLabel,amount}, footer.note)`

**Notes** — Chart only renders when res.status===200 AND impulses.length>0; otherwise the Awaiting (blue) state. cycle.day/days have safe defaults (0/30); footer fields all optional with em-dash fallbacks. impulse.id should match the list/detail sub id so clicking an impulse selects the same record (selectSub toggles by id).

---

### `POST /subscriptions/detect`

AI scan of transaction history to detect recurring charges, then the frontend reloads everything.

> backend todo: `ai: detect recurring charges from transaction history`  ·  confidence: **high**

**Request body**

```json
{"lookbackMonths":6}
```

**Response** — frontend ignores the body (fire-and-forget; the page just calls `reload()`). Any 2xx = success.

**Consumed by** — `Subscriptions.jsx:478 (api.post('/subscriptions/detect', {lookbackMonths:6}).then(r => if r.ok||status===501 reloadAll()))`, `Subscriptions.jsx:552 (⌁ DETECT button)`

**Notes** — REQUEST: always sends exactly {lookbackMonths:6} (hardcoded). RESPONSE BODY IGNORED — on r.ok (or even status 501) the page just calls reloadAll() (reloadList/reloadStats/reloadSweep/reloadDetail). Fire-and-forget; newly-detected subs surface via the reloaded list/sweep (likely as candidate:true records). responseJson intentionally empty.

---

### `GET /subscriptions/{id}`

Subscription detail for the right-dock inspector: all card fields plus note, recent charges, AI guidance, candidate flag.

> backend todo: `recurring: subscription detail + AI guidance`  ·  confidence: **high**

**Path params**

- `id` — Stable slug id of the selected subscription (ADR-008), e.g. "linkedin". URL-encoded by the frontend (encodeURIComponent).

**Response `200`**

```json
{"id":"linkedin","name":"LINKEDIN","category":"LEISURE","note":"Career Premium · unused 61 days. Cancel?","status":"watch","cadence":"monthly","amount":39.9,"monthlyEquiv":39.9,"annual":478.8,"nextLabel":"28 JUN","day":28,"since":"APR 2025","source":"llm","hist":[39.9,39.9,39.9,39.9,39.9,39.9],"priceRose":false,"histAxisLabel":"6 CHARGES","recent":[{"id":"c-202605","date":"28 MAY","note":"auto-detected","amount":39.9},{"id":"c-202604","date":"28 APR","note":"auto-detected","amount":39.9}],"guidance":{"text":"Unused 61 days — GEMMA4 suggests cancelling to save CHF 479/yr.","severity":"coral"},"candidate":false}
```

**Response fields**

- `id` *(string)* — Sub slug id; compared to the selected id to confirm the payload matches before rendering. If omitted the frontend still accepts it (detail.id==null bypasses the match guard).
- `name` *(string)* — Display name, header title.
- `category` *(string)* — Category name, shown in the header kls line. ADR-008 name/slug, not UUID.
- `note` *(string)* — Free-text note shown under the header; also the fallback for AI guidance text and per-charge note. Optional.
- `status` *(string)* — Same enum as list (due/soon/watch/paused/ok...). Drives the primary action: "due" => MARK PAID, else PAUSE; "paused" => RESUME button instead of CANCEL; due/watch default the guidance severity to coral.
- `cadence` *(string)* — "monthly" or "yearly"; drives YEARLY/MONTHLY labeling, hist axis default, and month vs day display.
- `amount` *(number)* — Per-charge CHF; big figure + "Per charge" stat.
- `monthlyEquiv` *(number)* — Monthly-equivalent CHF ("Monthly" stat). Optional/em-dash.
- `annual` *(number)* — Annualized CHF ("Annualized" stat + "annualized CHF ..." in the delta line). Optional.
- `nextLabel` *(string)* — Next-charge label ("Next charge" stat), e.g. "28 JUN". Em-dash when missing.
- `day` *(number)* — Monthly charge day (shown as "MONTHLY · D"). Optional.
- `month` *(string)* — Yearly charge month (shown as "YEARLY · MMM"), e.g. "MAR". Optional.
- `since` *(string)* — Tracked-since label ("Tracked since" stat). Em-dash when missing.
- `source` *(string)* — "llm" vs other; controls the default per-charge note text ("auto-detected" vs "confirmed").
- `hist` *(number[])* — Price history oldest->newest for the inspector bar chart (SubHistBars).
- `priceRose` *(boolean)* — Whether price rose; drives the last-bar color and the "PRICE ROSE"/"FLAT" axis label. Optional — when null the chart falls back to comparing last vs first hist value.
- `histAxisLabel` *(string)* — Axis caption for the chart; defaults to "3 YEARS" (yearly) or "6 CHARGES" (monthly) when absent.
- `recent` *(array)* — Recent charges, newest-first, for the "Recent charges" list. Frontend reads `recent` first, then falls back to `charges`.
- `recent[].id` *(string)* — Optional charge id (used as React key; falls back to array index).
- `recent[].date` *(string)* — Charge date label, e.g. "28 MAY" or "MAR 2025".
- `recent[].note` *(string)* — Optional per-charge note; falls back to "auto-detected"/"confirmed" by s.source.
- `recent[].amount` *(number)* — Charge amount CHF; rendered "CHF x.xx". Optional (blank when null).
- `charges` *(array)* — Alternate name for recent charges — read only if `recent` is not an array. Same item shape as recent[].
- `guidance` *(object)* — AI guidance object. Optional.
- `guidance.text` *(string)* — Guidance message shown in the footer. Falls back to s.note when absent.
- `guidance.severity` *(string)* — "coral" => emphasized coral bold. Otherwise defaults to coral when status is due/watch, else plain.
- `candidate` *(boolean)* — True for an AI-detected suggestion not yet confirmed: swaps the action row to CONFIRM/DISMISS instead of the primary/PAUSE/CANCEL/RESUME buttons.

**Consumed by** — `Subscriptions.jsx:471-472 (useGet('/subscriptions/'+encodeURIComponent(sel)), deps [sel])`, `Subscriptions.jsx:296 (ok = res.status===200 && detail && (detail.id==null || detail.id===sel))`, `Subscriptions.jsx:310-316 (s=detail; mo=s.monthlyEquiv; yr=s.annual; s.cadence; recent=s.recent||s.charges; guidance=s.guidance.text||s.note; sev=s.guidance.severity; axisLabel=s.histAxisLabel)`, `Subscriptions.jsx:329-331 (s.category, s.name, s.note)`, `Subscriptions.jsx:336-337 (s.amount, s.cadence, yr)`, `Subscriptions.jsx:341-342 SubHistBars (s.hist, s.cadence, s.priceRose)`, `Subscriptions.jsx:346-351 (s.amount, mo, yr, s.nextLabel, s.month/s.day, s.since)`, `Subscriptions.jsx:357-362 (recent[].id, recent[].date, recent[].note, recent[].amount; s.source)`, `Subscriptions.jsx:321-324,374-377,369-370 (s.status drives MARK PAID vs PAUSE and RESUME vs CANCEL; s.candidate -> CONFIRM/DISMISS)`

**Notes** — Inspector renders only when res.status===200 AND (detail.id is null OR equals the selected id) — guards a stale fetch. Superset of the list-item shape plus note/month/recent|charges/guidance/histAxisLabel/candidate/priceRose. recent vs charges: frontend prefers `recent`, falls back to `charges` (return either; recent recommended). guidance optional; if absent, guidance text falls back to note and severity to coral-when-due/watch.

---

### `PATCH /subscriptions/{id}`

Edit a subscription (no frontend caller exists yet).

> backend todo: `recurring: edit subscription`  ·  confidence: **low**

**Path params**

- `id` — Sub slug id to edit.

**Response** — frontend ignores the body (fire-and-forget; the page just calls `reload()`). Any 2xx = success.

**Notes** — NOT CONSUMED by the current app — there is no api.patch('/subscriptions/{id}', ...) anywhere in frontend/app/src/. The inspector exposes only lifecycle actions (pause/resume/cancel/mark-paid/confirm/dismiss/snooze), not field edits. Request/response shape is a genuine guess; presumably a partial update of editable list-item fields with the page calling reloadAll() afterward.

---

### `POST /subscriptions/{id}/pause`

Pause a subscription; frontend reloads list/stats/sweep/detail.

> backend todo: `recurring: pause subscription`  ·  confidence: **medium**

**Path params**

- `id` — Sub slug id; from the inspector's detail.id (encodeURIComponent).

**Response** — frontend ignores the body (fire-and-forget; the page just calls `reload()`). Any 2xx = success.

**Consumed by** — `Subscriptions.jsx:319 (act = verb => api.post('/subscriptions/'+id+'/'+verb, body).then(after))`, `Subscriptions.jsx:322,374 (PAUSE primary button when status!=='due')`, `Subscriptions.jsx:318 (after: if r.ok||status===501 -> onChanged -> reloadAll)`

**Notes** — No request body (act('pause') called with no body => apiCall sends no Content-Type/body). RESPONSE BODY IGNORED — after() only checks r.ok || r.status===501 then calls onChanged()/reloadAll() to resync. Fire-and-forget. responseJson empty by design.

---

### `POST /subscriptions/{id}/resume`

Resume a paused subscription; frontend reloads.

> backend todo: `recurring: resume subscription`  ·  confidence: **medium**

**Path params**

- `id` — Sub slug id from detail.id.

**Response** — frontend ignores the body (fire-and-forget; the page just calls `reload()`). Any 2xx = success.

**Consumed by** — `Subscriptions.jsx:376 (RESUME button, shown only when detail.status==='paused')`, `Subscriptions.jsx:319 (act('resume'))`

**Notes** — No request body. Only offered when the detail.status is "paused". RESPONSE BODY IGNORED — reloadAll() runs on r.ok||status===501. Fire-and-forget.

---

### `POST /subscriptions/{id}/cancel`

Cancel a subscription; frontend reloads.

> backend todo: `recurring: cancel subscription`  ·  confidence: **medium**

**Path params**

- `id` — Sub slug id from detail.id.

**Response** — frontend ignores the body (fire-and-forget; the page just calls `reload()`). Any 2xx = success.

**Consumed by** — `Subscriptions.jsx:377 (CANCEL button, shown for non-candidate, non-paused subs)`, `Subscriptions.jsx:319 (act('cancel'))`

**Notes** — No request body. RESPONSE BODY IGNORED — reloadAll() on r.ok||status===501. Fire-and-forget.

---

### `POST /subscriptions/{id}/mark-paid`

Mark this cycle's charge as paid/seen; frontend reloads.

> backend todo: `recurring: mark subscription charge paid`  ·  confidence: **medium**

**Path params**

- `id` — Sub slug id from detail.id.

**Response** — frontend ignores the body (fire-and-forget; the page just calls `reload()`). Any 2xx = success.

**Consumed by** — `Subscriptions.jsx:321-322,374 (primary button labeled MARK PAID when detail.status==='due')`, `Subscriptions.jsx:319 (act('mark-paid'))`

**Notes** — No request body (act('mark-paid') called with no body). Used to clear a "NOT SEEN" (due) state. RESPONSE BODY IGNORED — reloadAll() on r.ok||status===501. Fire-and-forget.

---

### `GET /subscriptions/{id}/charges`

List a subscription's charges (no dedicated frontend caller — the inspector reads charges off the detail object instead).

> backend todo: `recurring: list subscription charges`  ·  confidence: **low**

**Path params**

- `id` — Sub slug id.

**Response `200`**

```json
{"charges":[{"id":"c-202605","date":"5 MAY","note":"confirmed","amount":12.95},{"id":"c-202604","date":"5 APR","note":"confirmed","amount":12.95}]}
```

**Response fields**

- `id` *(string)* — Charge id (would map to the recent[].id React key).
- `date` *(string)* — Charge date label, e.g. "28 MAY".
- `note` *(string)* — Optional per-charge note.
- `amount` *(number)* — Charge amount CHF.

**Notes** — NOT directly consumed: there is no useGet/api call to /subscriptions/{id}/charges in frontend/app/src/. The inspector instead reads `recent`/`charges` embedded in the GET /subscriptions/{id} detail response (Subscriptions.jsx:313). Field shape is INFERRED from that embedded array (id/date/note/amount). If implemented standalone, mirror that item shape; wrapping (bare array vs {charges:[...]}) is a guess.

---

### `POST /subscriptions/{id}/charges`

Record a charge/payment for a subscription (no frontend caller exists yet).

> backend todo: `recurring: record a subscription charge/payment`  ·  confidence: **low**

**Path params**

- `id` — Sub slug id.

**Response** — frontend ignores the body (fire-and-forget; the page just calls `reload()`). Any 2xx = success.

**Notes** — NOT CONSUMED — no api.post('/subscriptions/{id}/charges', ...) in frontend/app/src/. The closest user action is mark-paid (a separate endpoint). Request/response shape is a guess; a recorded charge would presumably match the recent[] item shape {date, amount, note}.

---

### `POST /subscriptions/{id}/confirm`

Confirm an AI-detected candidate subscription; frontend reloads.

> backend todo: `recurring: confirm AI-detected subscription`  ·  confidence: **medium**

**Path params**

- `id` — Sub slug id of the candidate (from detail.id).

**Response** — frontend ignores the body (fire-and-forget; the page just calls `reload()`). Any 2xx = success.

**Consumed by** — `Subscriptions.jsx:369 (CONFIRM button, shown only when detail.candidate is true)`, `Subscriptions.jsx:319 (act('confirm'))`

**Notes** — No request body. Only rendered for candidate (AI-suggested) subscriptions. RESPONSE BODY IGNORED — reloadAll() on r.ok||status===501. Fire-and-forget.

---

### `POST /subscriptions/{id}/dismiss`

Dismiss an AI subscription suggestion; frontend reloads.

> backend todo: `recurring: dismiss AI subscription suggestion`  ·  confidence: **medium**

**Path params**

- `id` — Sub slug id of the candidate (from detail.id).

**Response** — frontend ignores the body (fire-and-forget; the page just calls `reload()`). Any 2xx = success.

**Consumed by** — `Subscriptions.jsx:370 (DISMISS button, shown only when detail.candidate is true)`, `Subscriptions.jsx:319 (act('dismiss'))`

**Notes** — No request body. Only rendered for candidate subscriptions. RESPONSE BODY IGNORED — reloadAll() on r.ok||status===501. Fire-and-forget.

---

### `POST /subscriptions/{id}/snooze`

Snooze a subscription's alert; frontend reloads.

> backend todo: `recurring: snooze subscription alert`  ·  confidence: **medium**

**Path params**

- `id` — Sub slug id from detail.id.

**Request body**

```json
{}
```

**Response** — frontend ignores the body (fire-and-forget; the page just calls `reload()`). Any 2xx = success.

**Consumed by** — `Subscriptions.jsx:383 (SNOOZE button)`, `Subscriptions.jsx:319 (act('snooze', {}))`

**Notes** — Called as act('snooze', {}) — sends an EMPTY JSON object body (Content-Type: application/json, body "{}"), unlike the other lifecycle verbs which send no body at all. No fields are populated by the frontend. RESPONSE BODY IGNORED — reloadAll() on r.ok||status===501. Fire-and-forget.

---

## Item signals

_Source: `backend/bin/phosk_api/src/routes//home/ovsiankina/Documents/phoskonomia/backend/bin/phosk_api/src/routes/signals.rs`_

AI-maintained item-level micro-signals (e.g. "toothpaste", "energy drinks") tracked distinctly from categories: list tracked signals + AI candidates, inspect a signal's trend/series/recent occurrences, surface top movers, track/dismiss candidates, and set a soft spending cap. Consumed by Dashboard (SignalStrip/SignalPanel), Analytics (item-signal matrix + movers leaderboard), Transactions (per-pill SignalSheet), and the AI feed in the global shell.

### `GET /signals`

List the tracked item-signals (the AI-maintained micro-categories).

> backend todo: `ledger/signals: list tracked item-signals (+ include candidates)`  ·  confidence: **high**

**Response `200`**

```json
{"signals":[{"id":"toothpaste","label":"Toothpaste","parent":"Groceries","since":"Feb 2026","deltaPct":12,"cycleQty":3,"unit":"pcs","cycleSpend":14.85,"txns":2,"series":[8.9,9.5,10.2,11.0,12.4,11.8,13.2,12.0,13.9,14.1,13.2,14.85]},{"id":"energy-drinks","label":"Energy drinks","parent":"Groceries","since":"Mar 2026","deltaPct":-18,"cycleQty":6,"unit":"cans","cycleSpend":11.40,"txns":3,"series":[18.0,16.5,17.2,15.0,14.1,13.8,15.2,14.0,13.1,12.5,11.9,11.40]}]}
```

**Response fields**

- `signals` *(array (or bare top-level array, or items[]))* — Tracked item-signals. App unwraps Array.isArray(data)?data:(data.signals||data.items). Dashboard.jsx:51, Analytics.jsx:359-361.
- `signals[].id` *(string (slug))* — Stable human id used as React key and the {id} path param for detail/cap (ADR-008 slug, e.g. "toothpaste"). SignalCard.onSelect(s.id) shell.jsx:248, ItemSignalRow Analytics.jsx:158,500.
- `signals[].label` *(string)* — Display name (Pilowlava). ItemSignalRow Analytics.jsx:164, SignalCard shell.jsx:249. Also the 'az' sort key (Analytics.jsx:369).
- `signals[].parent` *(string (category name))* — Parent category this signal rolls up under, e.g. "Groceries". Shown as 'parent · since {since}' in the matrix row. ItemSignalRow Analytics.jsx:165. (NOT read by the dashboard SignalCard.)
- `signals[].since` *(string)* — Human 'tracked since' label, e.g. "Feb 2026". ItemSignalRow Analytics.jsx:165.
- `signals[].deltaPct` *(number (signed, percent))* — Cycle-over-cycle delta percent; sign drives up/down arrow, abs() shown. Default 0 if absent. ItemSignalRow Analytics.jsx:156-157, SignalCard shell.jsx:245-246, momentum sort Analytics.jsx:370.
- `signals[].cycleQty` *(number)* — Quantity this cycle. ItemSignalRow Analytics.jsx:170, SignalCard shell.jsx:254.
- `signals[].unit` *(string)* — Unit label, e.g. "pcs"/"cans". Rendered next to cycleQty. ItemSignalRow Analytics.jsx:170, SignalCard shell.jsx:254.
- `signals[].cycleSpend` *(number (CHF))* — CHF spent this cycle; formatted via chf(). 'spend' sort key. ItemSignalRow Analytics.jsx:171, SignalCard shell.jsx:254, sort Analytics.jsx:368.
- `signals[].txns` *(number)* — Receipt/transaction count this cycle. ItemSignalRow Analytics.jsx:171 ('{txns} txns'). (NOT read by the dashboard SignalCard.)
- `signals[].series` *(number[] (~12 points))* — Sparkline trend (12-mo). Passed to SigSpark; falls back to [] if absent. ItemSignalRow Analytics.jsx:167, SignalCard shell.jsx:252.

**Consumed by** — `Dashboard.jsx:37 (useGet('/signals'))`, `Dashboard.jsx:51 (Array.isArray(signals.data)?signals.data:(signals.data.signals||signals.data.items))`, `shell.jsx:244-256 SignalCard reads s.id,s.label,s.deltaPct,s.candidate,s.series,s.cycleQty,s.unit,s.cycleSpend (dashboard strip)`, `Analytics.jsx:340 (useGet('/signals'))`, `Analytics.jsx:359-361 (same defensive unwrap)`, `Analytics.jsx:368-370 (client sort by cycleSpend / label / deltaPct)`, `Analytics.jsx:154-176 ItemSignalRow reads s.id,label,parent,since,deltaPct,cycleQty,unit,cycleSpend,txns,series (called at :499)`, `shell.jsx:268 empty-state references /signals`

**Notes** — List may be returned as a BARE array OR wrapped as {signals:[...]} OR {items:[...]} — frontend accepts all three (Dashboard.jsx:51, Analytics.jsx:359-361). Every numeric field defaults safely (deltaPct||0, series||[]). Sorting is client-side (Analytics.jsx:366-372). Status must be 200 for the matrix to render; Analytics gates the matrix on signalsGet.status===200 (Analytics.jsx:496), otherwise shows <Awaiting/>. The dashboard SignalCard reads a SUBSET (id,label,deltaPct,candidate,series,cycleQty,unit,cycleSpend); parent/since/txns are only read by the Analytics matrix row.

---

### `POST /signals`

Track a new item-signal (approve an AI candidate, or by name).

> backend todo: `ledger/signals: track a new signal (from candidate or name)`  ·  confidence: **medium**

**Request body**

```json
{"candidateId":"oat-milk","label":"Oat milk"}
```

**Response** — frontend ignores the body (fire-and-forget; the page just calls `reload()`). Any 2xx = success.

**Consumed by** — `Analytics.jsx:393 (api.post('/signals', { candidateId: id }) then reload signals+candidates)`, `shell.jsx:227 SignalPanel 'TRACK SIGNAL' (api.post('/signals', { candidateId: sig.id, label: sig.label }) then onChanged via after())`

**Notes** — Body shape varies by call site: Analytics sends only {candidateId} (Analytics.jsx:393); SignalPanel sends {candidateId, label} (shell.jsx:227) — so candidateId is the required key (a candidate slug, ADR-008) and label is an optional human name. Response body is IGNORED — fire-and-forget: callers just .then(reload of /signals and /signals/candidates). Backend may return anything (201/200); only the HTTP status matters for the toast (api.js:64-68).

---

### `GET /signals/candidates`

List AI-proposed candidate signals not yet tracked.

> backend todo: `ai: list signal candidates the AI proposes to track`  ·  confidence: **high**

**Response `200`**

```json
{"candidates":[{"id":"oat-milk","label":"Oat milk","desc":"appearing on 4 of your last 6 Migros receipts","series":[0,0,2.1,2.1,4.2,3.8,4.2,6.3,6.3,8.4,8.4,10.5]}]}
```

**Response fields**

- `candidates` *(array (or bare array, or items[]))* — AI candidate signals. App unwraps Array.isArray(data)?data:(data.candidates||data.items). Dashboard.jsx:52, Analytics.jsx:362-364.
- `candidates[].id` *(string (slug))* — Candidate slug; React key and the {id} for track/dismiss + POST /signals candidateId. Dashboard.jsx:54, Analytics.jsx:503, shell.jsx:227-228.
- `candidates[].label` *(string)* — Display name of the proposed signal. ItemSignalRow Analytics.jsx:164, SignalPanel shell.jsx:225, SignalCard shell.jsx:249.
- `candidates[].desc` *(string)* — Why the AI proposes it (subtitle); shown instead of 'parent · since' for candidates. ItemSignalRow Analytics.jsx:165, SignalCard shell.jsx:254, SignalPanel shell.jsx:225 ('The AI noticed {label} {desc}').
- `candidates[].series` *(number[])* — Sparkline preview for the candidate. SignalPanel candidate chart shell.jsx:222, ItemSignalRow Analytics.jsx:167, SignalCard shell.jsx:252. Falls back to [].

**Consumed by** — `Dashboard.jsx:38 (useGet('/signals/candidates'))`, `Dashboard.jsx:52 (Array.isArray?data:(data.candidates||data.items))`, `Dashboard.jsx:54 (candList.map(c => ({...c, candidate:true})))`, `Analytics.jsx:341 (useGet('/signals/candidates'))`, `Analytics.jsx:362-364 (same defensive unwrap)`, `Analytics.jsx:502-503 ItemSignalRow renders candidate via {...s, candidate:true}: reads s.id, s.label, s.desc, s.series`, `shell.jsx:220-231 SignalPanel candidate branch reads sig.label, sig.desc, sig.series, sig.id`

**Notes** — Same triple-shape tolerance as /signals: bare array OR {candidates:[...]} OR {items:[...]} (Dashboard.jsx:52, Analytics.jsx:362-364). Candidates do NOT carry deltaPct/cycleQty/cycleSpend/txns/parent/since — those branches are gated on !candidate; candidate rows render delta as the literal 'NEW' (Analytics.jsx:157, shell.jsx:246). The page flags each with {candidate:true} client-side (Dashboard.jsx:54, Analytics.jsx:503); the API need not send that flag. Visibility in the matrix is gated by the 'showCand' tweak (Analytics.jsx:496,502).

---

### `GET /signals/movers`

Top riser / faller signal-movers leaderboard for Analytics.

> backend todo: `insights: signal movers (top riser/faller)`  ·  confidence: **high**

**Response `200`**

```json
{"riser":{"label":"Toothpaste","deltaPct":34,"series":[8.9,9.5,10.2,11.0,12.4,11.8,13.2,12.0,13.9,14.1,13.2,14.85],"cycleQty":3,"unit":"pcs","cycleSpend":14.85,"parent":"Groceries"},"faller":{"label":"Energy drinks","deltaPct":-22,"series":[18.0,16.5,17.2,15.0,14.1,13.8,15.2,14.0,13.1,12.5,11.9,11.40],"cycleQty":6,"unit":"cans","cycleSpend":11.40,"parent":"Groceries"},"all":[]}
```

**Response fields**

- `riser` *(object)* — null | Fastest-rising signal. Drives the 'FASTEST RISER' MoverCard and the KPI '{label} ↑{deltaPct}% leads'. Analytics.jsx:414,478,511.
- `faller` *(object)* — null | Fastest-falling signal. Drives the 'FASTEST FALLER' MoverCard. Analytics.jsx:513-514.
- `all` *(array (optional))* — Read defensively (Array.isArray(moversData.all)?...:[]) at Analytics.jsx:356 into moversAll but NOT rendered anywhere — dead read. Safe to omit or send [].
- `riser/faller.label` *(string)* — Signal display name. MoverCard shell-style Analytics.jsx:188, KPI Analytics.jsx:478.
- `riser/faller.deltaPct` *(number (signed))* — Percent change; sign chooses +/− prefix and up/down styling. MoverCard Analytics.jsx:186, KPI Analytics.jsx:478. Default 0.
- `riser/faller.series` *(number[])* — Sparkline trend for the mover card (SigSpark w=232). MoverCard Analytics.jsx:189. Falls back to [].
- `riser/faller.cycleQty` *(number)* — Quantity this cycle, shown in the card subtitle. MoverCard Analytics.jsx:190.
- `riser/faller.unit` *(string)* — Unit label in the subtitle. MoverCard Analytics.jsx:190.
- `riser/faller.cycleSpend` *(number (CHF))* — CHF spent this cycle (chf-formatted) in the subtitle. MoverCard Analytics.jsx:190.
- `riser/faller.parent` *(string)* — Parent category shown at end of the subtitle. MoverCard Analytics.jsx:190.

**Consumed by** — `Analytics.jsx:338 (useGet('/signals/movers'))`, `Analytics.jsx:355-356 (moversData = data||{}; moversAll = Array.isArray(data.all)?data.all:[])`, `Analytics.jsx:414 (riser = moversData.riser||null)`, `Analytics.jsx:478 (riser.label, riser.deltaPct in the ITEM-SIGNALS KPI)`, `Analytics.jsx:510-515 (moversData.riser / moversData.faller -> MoverCard, gated on movers.status===200)`, `Analytics.jsx:180-193 MoverCard reads sig.label, sig.deltaPct, sig.series, sig.cycleQty, sig.unit, sig.cycleSpend, sig.parent`

**Notes** — Response is an OBJECT (not a list): {riser, faller, all?}. Cards render only when movers.status===200 AND the respective key is present; otherwise an <Awaiting/> placeholder shows (Analytics.jsx:510-515). riser/faller each have the SAME shape as a mover record (label/deltaPct/series/cycleQty/unit/cycleSpend/parent). 'all' is read into moversAll (Analytics.jsx:356) but never rendered — safe to omit or send [].

---

### `GET /signals/{id}`

Detail for one tracked signal: stats, 12-mo series, recent occurrences.

> backend todo: `ledger/signals: signal detail (series, recent lines)`  ·  confidence: **high**

**Path params**

- `id` — The signal slug (ADR-008 stable human id, e.g. "toothpaste"), URL-encoded by the caller via encodeURIComponent (Dashboard.jsx:75, Analytics.jsx:344). In Transactions the SignalSheet passes the raw sigId UNencoded (Transactions.jsx:353), sourced from line.signal_id.

**Response `200`**

```json
{"id":"toothpaste","label":"Toothpaste","parent":"Groceries","desc":"single-purchase oral care across Migros & Coop","deltaPct":12,"series":[8.9,9.5,10.2,11.0,12.4,11.8,13.2,12.0,13.9,14.1,13.2,14.85],"cycleQty":3,"unit":"pcs","cycleSpend":14.85,"avgUnit":4.95,"txns":2,"since":"Feb 2026","conf":0.91,"recent":[{"date":"14 Jun","note":"Elmex Sensitive","shop":"Migros","qty":2,"price":4.95},{"date":"03 Jun","note":"Candida White","shop":"Coop","qty":1,"price":4.95}]}
```

**Response fields**

- `id` *(string (slug))* — Signal slug. Echoed; matches the requested {id}. Not directly re-rendered in the detail panel but expected as the record identity.
- `label` *(string)* — Display name in the panel header. shell.jsx:184.
- `parent` *(string)* — Parent category shown in '⌁ ITEM-SIGNAL · {parent}' header and the footer 'Tracked distinctly from {parent}'. shell.jsx:183,236.
- `desc` *(string)* — Subtitle under the header. shell.jsx:185.
- `deltaPct` *(number (signed))* — null | Cycle delta; if null the panel shows 'NEW', else up/down arrow + abs%. shell.jsx:177-178.
- `series` *(number[])* — 12-mo trend for the panel chart (SigSpark). shell.jsx:196.
- `cycleQty` *(number)* — This-cycle quantity ('{cycleQty} {unit}'). shell.jsx:193,200.
- `unit` *(string)* — Unit label. shell.jsx:193,200.
- `cycleSpend` *(number (CHF))* — This-cycle spend, chf-formatted, coral. shell.jsx:201.
- `avgUnit` *(number (CHF))* — Average CHF per unit ('Avg / unit'). shell.jsx:202.
- `txns` *(number)* — Receipt count ('Receipts'). shell.jsx:203.
- `since` *(string)* — Tracked-since label. shell.jsx:204.
- `conf` *(number (0..1))* — null | AI confidence; rendered as Math.round(conf*100)+'%', '—' if null. shell.jsx:205.
- `recent` *(array)* — Recent occurrences list (falls back to []). shell.jsx:179,209.
- `recent[].date` *(string)* — Occurrence date label, e.g. '14 Jun'. shell.jsx:211.
- `recent[].note` *(string)* — Item/line note, e.g. 'Elmex Sensitive'. shell.jsx:212.
- `recent[].shop` *(string (shop name))* — Shop name, e.g. 'Migros'/'Coop' (ADR-008 name, not id). shell.jsx:213.
- `recent[].qty` *(number)* — Quantity on that occurrence; multiplied by price for the displayed CHF. shell.jsx:214 (qty*price).
- `recent[].price` *(number (CHF))* — Unit price on that occurrence; CHF shown is chf(qty*price). shell.jsx:214.
- `candidate` *(boolean (optional))* — If true the panel renders the candidate variant (TRACK/DISMISS buttons) instead of stats. Detail of a tracked signal is normally false/absent. shell.jsx:189,220.

**Consumed by** — `Dashboard.jsx:75 (useGet(sel ? '/signals/'+encodeURIComponent(sel) : '/signals', undefined, [sel]))`, `Dashboard.jsx:76 (sigObj = sel && ok200(status) ? data : null) -> SignalPanel`, `Analytics.jsx:344 (useGet(sel ? `/signals/${encodeURIComponent(sel)}` : '/signals', undefined, [sel]))`, `Analytics.jsx:384 (selSig = sel && status===200 && data && !Array.isArray(data) ? data : null)`, `Transactions.jsx:353 (SignalSheet useGet(`/signals/${sigId}`)) -> SignalPanel`, `shell.jsx:165-240 SignalPanel reads parent,label,desc,deltaPct,series,cycleQty,unit,cycleSpend,avgUnit,txns,since,conf,recent[],candidate`

**Notes** — Response is a single OBJECT (NOT an array) — Analytics explicitly requires !Array.isArray(data) (Analytics.jsx:384) and Dashboard requires ok200 status (Dashboard.jsx:76). Must be served at 200 or the panel stays empty. When no signal is selected the hook points at '/signals' instead (deps:[sel]); the detail object is only consumed when sel is set. deltaPct and conf may be null (panel handles via 'NEW' / '—'); recent may be omitted ([] fallback). qty*price is computed client-side, so qty and price must both be present per occurrence.

---

### `DELETE /signals/{id}`

Untrack / pause a tracked signal.

> backend todo: `ledger/signals: untrack / pause a signal`  ·  confidence: **low**

**Path params**

- `id` — Signal slug (ADR-008 stable human id). Same id used for GET /signals/{id}.

**Response** — frontend ignores the body (fire-and-forget; the page just calls `reload()`). Any 2xx = success.

**Notes** — Declared in the route file (.delete on /signals/{id}, signals.rs:29) but NO caller exists anywhere under frontend/app/src/ — no api.del or DELETE to '/signals' found in the codebase. No request body; response would be ignored. Shape is a pure guess; implement as idempotent untrack/pause returning 200/204.

---

### `POST /signals/{id}/cap`

Set a soft spending cap (nudge threshold) on a signal.

> backend todo: `planning: set a soft cap on a signal (nudge threshold)`  ·  confidence: **high**

**Path params**

- `id` — Signal slug. Sourced from suggestedCap.signalId (from GET /analytics/insights/movers), URL-encoded (Analytics.jsx:407). ADR-008 stable id.

**Request body**

```json
{"amount":18.00,"period":"cycle"}
```

**Response** — frontend ignores the body (fire-and-forget; the page just calls `reload()`). Any 2xx = success.

**Consumed by** — `Analytics.jsx:405-409 (applyCap: api.post(`/signals/${encodeURIComponent(suggestedCap.signalId)}/cap`, {amount: suggestedCap.amount, period:'cycle'}) then reload signals+insights+selDetail)`

**Notes** — Request body is {amount:number(CHF), period:string}. 'period' is hardcoded to the literal 'cycle' (Analytics.jsx:408); 'amount' comes from the AI's suggestedCap.amount. Response is IGNORED — fire-and-forget: .then() only re-fetches /signals, /analytics/insights/movers, and the selected detail (Analytics.jsx:409). Only HTTP status drives the toast.

---

### `POST /signals/candidates/{id}/track`

Track (approve) a candidate signal from the AI feed.

> backend todo: `ai: track a candidate signal (approve suggestion)`  ·  confidence: **high**

**Path params**

- `id` — Candidate slug. From the AI feed item: f.candidateId || f.sig || f.id (shell.jsx:129). ADR-008 stable candidate id (e.g. "oat-milk"). NOT URL-encoded.

**Response** — frontend ignores the body (fire-and-forget; the page just calls `reload()`). Any 2xx = success.

**Consumed by** — `shell.jsx:129 (AiPanel feed action: api.post('/signals/candidates/' + (f.candidateId||f.sig||f.id) + '/track').then(reloadFeed))`

**Notes** — No request body (api.post called with no second arg → no JSON body sent; api.js:76). Response is IGNORED — fire-and-forget: .then(reloadFeed) re-fetches /ai/feed; the page also locally dismisses the feed item (dismissFeed) and calls onTrack to open the signal (shell.jsx:130). Feed-driven analogue of POST /signals {candidateId}; both approve a candidate.

---

### `POST /signals/candidates/{id}/dismiss`

Dismiss (reject) a candidate signal the AI proposed.

> backend todo: `ai: dismiss a candidate signal`  ·  confidence: **high**

**Path params**

- `id` — Candidate slug. From sig.id of the candidate being viewed in SignalPanel (shell.jsx:228). ADR-008 stable candidate id. NOT URL-encoded.

**Response** — frontend ignores the body (fire-and-forget; the page just calls `reload()`). Any 2xx = success.

**Consumed by** — `shell.jsx:228 (SignalPanel candidate branch: api.post('/signals/candidates/' + sig.id + '/dismiss').then(after))`

**Notes** — No request body (api.js:76). Response is IGNORED — fire-and-forget: .then(after) invokes onChanged() (shell.jsx:176) which re-fetches the signal/candidate lists. Only HTTP status matters.

---

## debts

_Source: `backend/bin/phosk_api/src/routes//home/ovsiankina/Documents/phoskonomia/backend/bin/phosk_api/src/routes/debts.rs`_

Institutional debts: the /debts page (frontend/app/src/pages/Debts.jsx) renders outstanding balances as a decaying-waveform ledger — a payoff-trajectory hero, KPI roll-ups, a grid/list of debt cards with payoff-progress meters, and a right-dock inspector with a balance-decay chart, derived amortization stats, recent payments and AI guidance. All amortization (monthsToPayoff, interestRemaining, annualInterest, paidOffPct, decay series, trajectory, payoff targets) is computed by the backend; the frontend only formats. Mutations (set strategy, record extra payment, adjust plan, refinance) are fire-and-forget: the app ignores their response bodies and calls reload() to resync. NOTE: the personal-IOU endpoints (/personal-ious*) used by this same page belong to a DIFFERENT route file and are excluded here.

### `GET /debts`

List every institutional debt as a flat array of fully-derived debt records (balance, apr, payoff metrics, decay spark).

> backend todo: `debts: list institutional debts (+ derived payoff/interest)`  ·  confidence: **high**

**Response `200`**

```json
[{"id":"card","name":"CORNÈRCARD","lender":"REVOLVING CREDIT","glyph":"C","type":"CARD","balance":4350,"orig":6200,"apr":0.149,"monthly":220,"day":25,"term":null,"src":"llm","status":"high","statusLabel":"HIGH INTEREST","paidOffPct":0.298,"monthsToPayoff":24,"nextLabel":"25 JUN","hist":[3980,4120,4010,4280,4180,4350],"groupLabel":"REVOLVING CREDIT","note":"Revolving balance at 14.9% — by far the costliest franc you carry.","since":"2021","interestRemaining":712,"annualInterest":648},{"id":"leasing","name":"AUTO LEASING","lender":"AMAG · MULTILEASE","glyph":"⊙","type":"LEASE","balance":18600,"orig":32000,"apr":0.039,"monthly":540,"day":1,"term":60,"src":"user","status":"ok","statusLabel":"ON TRACK","paidOffPct":0.419,"monthsToPayoff":36,"nextLabel":"1 JUL","hist":[21840,21300,20760,20040,19320,18600],"groupLabel":"LEASES & LOANS","note":"60-month vehicle lease. Charged on the 1st, on schedule.","since":"JUL 2023","interestRemaining":1290,"annualInterest":725}]
```

**Response fields**

- `id` *(string (slug))* — Stable human/slug id (ADR-008), e.g. "card". Used as React key, selection id, and path param for /debts/{id}*.
- `name` *(string)* — Display name, e.g. "CORNÈRCARD".
- `lender` *(string)* — Lender/institution label, e.g. "REVOLVING CREDIT".
- `glyph` *(string)* — Single-char glyph shown in the card/row avatar, e.g. "C".
- `type` *(string)* — Debt type tag: LEASE | LOAN | CARD | TAX | BNPL | MEDICAL. "CARD" => treated as REVOLVING (no payoff month). Drives fallback group bucket via TYPE_LABEL.
- `balance` *(number (CHF))* — Current outstanding amount. Rendered as the OWED amount and 'outstanding'.
- `orig` *(number (CHF))* — Opening/peak borrowed amount; denominator for the 'OF' paid-down text. Optional — coalesced to 0.
- `apr` *(number (decimal 0..1))* — null | Nominal annual rate as a fraction (0.149 = 14.9%). null renders '—'. >0.08 toggles REFINANCE vs ADJUST PLAN button in the inspector.
- `monthly` *(number (CHF))* — Scheduled monthly payment; prefilled into the PAY EXTRA and ADJUST PLAN prompts.
- `day` *(number)* — Payment day-of-month; prefilled into the ADJUST PLAN prompt (d.day).
- `term` *(number)* — null | Term in months; null = revolving. Prefilled into the ADJUST PLAN prompt (blank when null).
- `src` *(string)* — Provenance: "user" | "llm". "llm" => blue dot (auto-detected) and counted in stats.autoCount. Emitted as data-src attribute.
- `status` *(string)* — "high" | "due" | "watch" | "ok". Maps to tone/label (high=HIGH INTEREST coral, due=DUE SOON warn, watch=REVIEW warn, else ON TRACK blue) and to the next/warning line.
- `statusLabel` *(string (optional))* — Backend-provided status label; overrides the local debtStatus() label when present (DebtCard line 238, DebtRow line 290).
- `paidOffPct` *(number (0..1))* — Backend-derived paid-down fraction; drives the payoff meter width and the 'NN% PAID OFF' text. Clamped 0..1; coalesced to 0.
- `monthsToPayoff` *(number)* — null | Months until paid off. >=600 (or null) renders '—'/REVOLVING. Drives 'payoff' sort and the PAYOFF month label (via monthLabel).
- `nextLabel` *(string)* — Next-payment label, e.g. "25 JUN". Shown on the NEXT/DUE line. Optional — '—' fallback.
- `hist` *(number[])* — Last ~6 monthly balances oldest->newest; drives the Spark in the card (only rendered if length>1).
- `groupLabel` *(string (optional))* — Backend bucket label used when 'Group by type' is on; falls back to a TYPE_LABEL[type] map then 'OTHER'.
- `note` *(string)* — Free-text note; shown in inspector header and used as AI-guidance fallback when /debts/{id} has no guidance.
- `since` *(string)* — Opened-since label, e.g. "2021" or "JUL 2023"; shown in inspector stats. Optional — '—' fallback.
- `interestRemaining` *(number)* — null | Total remaining interest over the debt's life. Infinity or <0 renders '∞'; null renders '—'. Inspector 'Interest left'.
- `annualInterest` *(number)* — null | Interest run-rate per year (balance*apr). null renders '—'. Inspector 'Interest / yr'.

**Consumed by** — `Debts.jsx:578 (useGet('/debts'))`, `Debts.jsx:579 (Array.isArray(debtsGet.data) ? debtsGet.data : [])`, `Debts.jsx:618-621 (sort by d.apr / d.balance / d.monthsToPayoff / d.name)`, `Debts.jsx:632 (d.groupLabel, d.type)`, `Debts.jsx:639 (debts.find(d => d.id === sel))`, `Debts.jsx:645,653 (key={d.id}, data-src={d.src})`, `Debts.jsx:227-279 DebtCard (d.paidOffPct, d.monthsToPayoff, d.id, d.hist, d.type, d.glyph, d.name, d.lender, d.src, d.status, d.statusLabel, d.balance, d.apr, d.orig, d.nextLabel, d.monthly)`, `Debts.jsx:283-297 DebtRow (d.glyph, d.name, d.statusLabel, d.apr, d.nextLabel, d.balance, d.monthly, d.id)`, `Debts.jsx:315-381 DebtInspector reads list record: d.status, d.monthsToPayoff, d.interestRemaining, d.paidOffPct, d.note, d.apr, d.type, d.lender, d.name, d.balance, d.monthly, d.annualInterest, d.since, d.day, d.term`

**Notes** — Frontend REQUIRES a bare JSON array (Array.isArray check at line 579; a wrapper object yields []). It is rendered only when status===200 AND length>0 (debtsReady, line 660), else an Awaiting empty-state shows. Every numeric field is optional in practice — the app coalesces missing balance/orig/monthly to 0, apr/monthsToPayoff/interestRemaining/annualInterest null -> '—'. statusLabel/groupLabel are optional overrides. The DebtInspector reads decay/guidance from the SEPARATE /debts/{id} call, not from this list record.

---

### `POST /debts`

Create a new debt. No create UI is wired in the current frontend, so request/response shape is unconstrained by the app.

> backend todo: `debts: create debt`  ·  confidence: **low**

**Response** — frontend ignores the body (fire-and-forget; the page just calls `reload()`). Any 2xx = success.

**Notes** — Route exists but the React app has NO call site that POSTs to /debts (no 'add debt' control). Shape is a genuine guess; a sensible create body would mirror the writable subset of the list record (name, lender, type, balance, orig, apr, monthly, day, term). Response likely the created debt record (same shape as GET /debts items) but the frontend would not read it today.

---

### `GET /debts/stats`

Portfolio roll-ups for the header sub-line, the 4 KPI tiles, the trajectory footer, and the avalanche/snowball target ids.

> backend todo: `debts: portfolio stats (totalOwed/weightedApr/debt-free horizon/targets)`  ·  confidence: **high**

**Response `200`**

```json
{"count":6,"totalOwed":38370,"totalOrig":63780,"totalMonthly":2065,"totalInterestYr":1247,"weightedApr":0.063,"autoCount":2,"paidOffTotalPct":0.398,"horizon":40,"debtFreeLabel":"OCT 29","avalancheTarget":"card","snowballTarget":"klarna"}
```

**Response fields**

- `count` *(number)* — Number of open debts. Header sub-line, MONTHLY OUTFLOW tile, and OPEN BALANCES section count.
- `totalOwed` *(number (CHF))* — Sum of outstanding balances. Header, TOTAL OWED KPI, and trajectory footer 'PAID DOWN' math (totalOrig-totalOwed).
- `totalOrig` *(number (CHF))* — Sum of original/borrowed amounts. Denominator of paid-down text and trajectory footer 'of CHF …'.
- `totalMonthly` *(number (CHF))* — Sum of scheduled monthly payments. Header '/mo' and MONTHLY OUTFLOW KPI.
- `totalInterestYr` *(number (CHF))* — Annual interest run-rate across all debts. INTEREST KPI tile.
- `weightedApr` *(number (decimal 0..1))* — Balance-weighted average APR as a fraction. Rendered as (x*100).toFixed(1)+'%' in the INTEREST KPI and trajectory footer 'AVG RATE'. null -> '—'.
- `autoCount` *(number)* — Count of debts auto-detected by the LLM (src==='llm'). MONTHLY OUTFLOW KPI 'NN auto-detected by GEMMA4'.
- `paidOffTotalPct` *(number (0..1))* — Portfolio paid-down fraction. Rendered as Math.round(v*100)+'%' in the TOTAL OWED KPI sub-line (pct100, line 710).
- `horizon` *(number (months))* — Global debt-free horizon in months. 'DEBT-FREE IN NN MO' and 'NN months at the current pace'. null -> '—'.
- `debtFreeLabel` *(string)* — Projected debt-free month label, e.g. "JUL 28". DEBT-FREE KPI, header, trajectory hud/endpoint. '—' fallback.
- `avalancheTarget` *(string (debt id))* — null | Id of the highest-APR debt to target under avalanche. Marks the ◎ TARGET card; shown in INTEREST KPI and trajectory footer. Compared to debt.id.
- `snowballTarget` *(string (debt id))* — null | Id of the smallest-balance debt to target under snowball. Marks the ◎ TARGET card; shown in trajectory footer when strategy==='snowball'.

**Consumed by** — `Debts.jsx:581-582 (useGet('/debts/stats'); S = statsGet.data || {})`, `Debts.jsx:613-614 (S.avalancheTarget / S.snowballTarget => strategy target id)`, `Debts.jsx:695-696 (S.count, S.totalOwed, S.totalMonthly, S.debtFreeLabel)`, `Debts.jsx:709-710 (S.totalOwed, S.paidOffTotalPct, S.totalOrig)`, `Debts.jsx:714-715 (S.totalMonthly, S.count, S.autoCount)`, `Debts.jsx:719-720 (S.totalInterestYr, S.weightedApr, S.avalancheTarget)`, `Debts.jsx:724-725 (S.debtFreeLabel, S.horizon)`, `Debts.jsx:741 (S.count for OPEN BALANCES count)`, `Debts.jsx:168-174 PayoffTrajectory footer (S.totalOrig, S.totalOwed, S.weightedApr, S.horizon, S.snowballTarget, S.avalancheTarget)`

**Notes** — Read as S = statsGet.data || {}; every field is individually optional and the UI shows '—'/em-dash when missing (fmtCount/fmtChf/pct100 helpers). avalancheTarget/snowballTarget are debt SLUG IDS (ADR-008), compared against debt.id to mark the target card. weightedApr/paidOffTotalPct are fractions (0..1). debtFreeLabel is a free-form month label string. IOU/personal-ious roll-ups come from /personal-ious/stats, NOT here.

---

### `GET /debts/trajectory`

Combined balance-decay trajectory (history + projection to zero) powering the hero SVG, re-fetched whenever the strategy changes.

> backend todo: `debts: combined payoff trajectory (history + projection)`  ·  confidence: **high**

**Query params**

- `strategy` *(string, required)* — Payoff strategy driving the projection: "avalanche" | "snowball" | "none". Always sent (defaults to 'avalanche').

**Response `200`**

```json
{"points":[{"m":-5,"total":45200},{"m":-4,"total":43980},{"m":-3,"total":42460},{"m":-2,"total":40310},{"m":-1,"total":39220},{"m":0,"total":38370},{"m":6,"total":31100},{"m":12,"total":24050},{"m":24,"total":11200},{"m":40,"total":0}],"xTicks":[0,12,24,40],"debtFreeLabel":"OCT 29"}
```

**Response fields**

- `points` *(Array<{m:number,total:number}>)* — Combined balance series. m = integer month-offset (negative=history, 0=today/NOW, positive=projection); total = combined CHF balance at that month. Must contain a point with m===0 (today marker). History points are p.m<=0, projection p.m>=0.
- `xTicks` *(number[] (optional))* — Month-offsets to label on the x-axis. If absent/empty the app derives [0, 12, 24, lastMonth].
- `debtFreeLabel` *(string (optional))* — Debt-free month label for the hud and endpoint marker; falls back to stats.debtFreeLabel then '—'.

**Consumed by** — `Debts.jsx:587 (useGet('/debts/trajectory', { strategy: trajStrategy }))`, `Debts.jsx:585-586 (trajStrategy = 'snowball' | 'none' | 'avalanche')`, `Debts.jsx:606 (trajGet.reload() after PUT /debts/strategy)`, `Debts.jsx:62 (traj.points array)`, `Debts.jsx:72 (traj.debtFreeLabel)`, `Debts.jsx:75-87 (points[].m, points[].total for the SVG path math)`, `Debts.jsx:95-97 (traj.xTicks array)`, `Debts.jsx:100 (points.find(p => p.m === 0))`

**Notes** — Rendered only when res.status===200 AND points.length>0 (line 64), else an Awaiting state shows. The hero needs at least one m<=0 (history) and one m>=0 (projection) point and ideally a point at m===0 for the TODAY marker. Month labels for ticks are computed client-side from cycle anchor + offset, so 'm' must be a true month offset, not a date. strategy is a required query param (qs() drops it only if empty; trajStrategy is never empty). xTicks/debtFreeLabel optional.

---

### `PUT /debts/strategy`

Persist the selected payoff strategy (avalanche/snowball/none). Fire-and-forget; app reloads trajectory + stats after.

> backend todo: `debts: set payoff strategy (avalanche/snowball/none)`  ·  confidence: **high**

**Request body**

```json
{"strategy":"avalanche"}
```

**Response** — frontend ignores the body (fire-and-forget; the page just calls `reload()`). Any 2xx = success.

**Consumed by** — `Debts.jsx:604-607 (onStrategy: api.put('/debts/strategy', { strategy: strat }).then(() => { trajGet.reload(); statsGet.reload(); }))`, `Debts.jsx:542 (TweakRadio onChange => onStrategy(v))`

**Notes** — Request body is exactly { strategy: "avalanche" | "snowball" | "none" }. Response body is IGNORED — the app's .then() only re-fetches /debts/trajectory and /debts/stats (so the persisted strategy must change those endpoints' output). Persisting strategy presumably also affects which target ids /debts/stats returns and the trajectory projection.

---

### `GET /debts/{id}`

Per-debt detail for the inspector: decay series for the balance chart plus AI payoff guidance text.

> backend todo: `debts: debt detail (decay series, payoff guidance)`  ·  confidence: **high**

**Path params**

- `id` — Debt slug id (ADR-008), e.g. "card". From the selected debt's id.

**Response `200`**

```json
{"decaySeries":{"hist":[3980,4120,4010,4280,4180,4350],"forward":[4350,4180,4005,3825,3640,3450,3250],"todayIndex":5},"guidance":"At 14.9% this is your costliest franc. Redirect the CHF 135 freed when Klarna clears here to save ~CHF 280 in interest."}
```

**Response fields**

- `decaySeries` *({hist:number[], forward:number[], todayIndex:number})* — Balance-decay chart data for the inspector. hist = past balances, forward = projected balances; the two are concatenated. todayIndex marks the join (defaults to hist.length-1 if absent). Needs >=2 total points or 'No decay series.' shows.
- `decaySeries.hist` *(number[])* — Historical balances (CHF), oldest->today. Used solid; rising history (last>first) colors the line coral.
- `decaySeries.forward` *(number[])* — Projected balances (CHF) from today forward; drawn dashed.
- `decaySeries.todayIndex` *(number (optional))* — Index in hist+forward marking today; defaults to hist.length-1 when omitted (line 203).
- `guidance` *(string)* — {text:string} | AI payoff guidance shown in the inspector footer. Accepts a plain string or an object with a .text field. Falls back to the list record's note when absent.

**Consumed by** — `Debts.jsx:590 (useGet(`/debts/${id}`, undefined, [sel]))`, `Debts.jsx:319 (decay = detail.decaySeries)`, `Debts.jsx:323-325 (detail.guidance — string OR {text})`, `Debts.jsx:196-219 DecayLine (decay.hist[], decay.forward[], decay.todayIndex)`, `Debts.jsx:369-371 (detailRes.status branch -> DecayLine or Awaiting)`

**Notes** — The inspector MERGES this detail over the list record: it reads ONLY decaySeries and guidance from this response; all the scalar stats (balance, monthly, apr, interestRemaining, annualInterest, monthsToPayoff, paidOffPct, since) come from the matching item in GET /debts (selDebt). So this endpoint can return just { decaySeries, guidance }. guidance is polymorphic (string OR {text}); decaySeries.todayIndex optional. If status!==200 the chart shows Awaiting and guidance falls back to the record's note. The comment at line 301 mentions {decaySeries, stats, guidance} but the app never reads a 'stats' key off the detail.

---

### `PATCH /debts/{id}`

Edit a debt's core fields. No edit-debt UI is wired, so request/response shape is unconstrained by the app.

> backend todo: `debts: edit debt`  ·  confidence: **low**

**Response** — frontend ignores the body (fire-and-forget; the page just calls `reload()`). Any 2xx = success.

**Notes** — Route exists but there is NO frontend call site that PATCHes /debts/{id} (the only PATCH the app issues is /debts/{id}/plan, line 351). Edits to balance/apr/etc. are not surfaced in the current UI. Shape is a guess; would mirror the writable subset of the list record. Response would be ignored today.

---

### `DELETE /debts/{id}`

Delete a debt. No delete UI is wired, so no app consumer.

> backend todo: `debts: delete debt`  ·  confidence: **low**

**Response** — frontend ignores the body (fire-and-forget; the page just calls `reload()`). Any 2xx = success.

**Notes** — Route exists but there is NO frontend call site that DELETEs /debts/{id} (no delete control in Debts.jsx; api.del is never invoked). Path param id is the debt slug. Response would be ignored; on success the app would presumably reload the list, but no such code path exists yet.

---

### `GET /debts/{id}/payments`

Recent payment history for the selected debt, shown in the inspector's 'Recent payments' list.

> backend todo: `debts: payment history`  ·  confidence: **high**

**Path params**

- `id` — Debt slug id (ADR-008), e.g. "card". From the selected debt's id.

**Response `200`**

```json
[{"id":"pay-2026-05","date":"25 MAY","note":"− CHF 220 scheduled","amount":220,"balance":4350},{"id":"pay-2026-04","date":"25 APR","amount":220,"balance":4180},{"id":"pay-2026-03","date":"25 MAR","amount":300,"balance":4280}]
```

**Response fields**

- `id` *(string (optional))* — Payment id; used as React key (falls back to array index).
- `date` *(string)* — Payment date label, e.g. "25 MAY". Shown as the row date; falls back to .label then '—'.
- `label` *(string (optional))* — Alternative date label used when .date is absent.
- `note` *(string (optional))* — Row description; when absent the app synthesizes '− CHF {amount} paid' from .amount.
- `amount` *(number (CHF, optional))* — Payment amount; used to build the default note when .note is absent.
- `balance` *(number (CHF, optional))* — Resulting balance after the payment; shown right-aligned as 'CHF …' when present.

**Consumed by** — `Debts.jsx:592 (useGet(`/debts/${id}/payments`, undefined, [sel]))`, `Debts.jsx:321-322 (recent = Array.isArray(payments) ? payments : (payments && (payments.payments || payments.items)) || [])`, `Debts.jsx:386-396 (paymentsRes.status branch; recent.slice(0,4).map(r => r.id, r.date||r.label, r.note||r.amount, r.balance))`

**Notes** — Frontend accepts EITHER a bare array OR an object wrapper {payments:[...]} OR {items:[...]} (line 321-322). Only the first 4 rows are shown. Each row needs at minimum a date/label and one of {note, amount}; balance is optional. If status!==200 an Awaiting state replaces the list. If empty -> 'No recent payments.'

---

### `POST /debts/{id}/payments`

Record an extra payment toward a debt. Fire-and-forget; app reloads all debt data after.

> backend todo: `debts: record an extra payment`  ·  confidence: **high**

**Request body**

```json
{"amount":300}
```

**Response** — frontend ignores the body (fire-and-forget; the page just calls `reload()`). Any 2xx = success.

**Consumed by** — `Debts.jsx:329-335 (onPayExtra: prompts for CHF amount, then api.post(`/debts/${id}/payments`, { amount }).then(() => onAction()))`, `Debts.jsx:400 (PAY EXTRA button)`, `Debts.jsx:600 (onAction = reloadDebtAll: reloads /debts, /debts/stats, /debts/trajectory, /debts/{id}, /debts/{id}/payments)`

**Notes** — Request body is exactly { amount: <positive number CHF> } (validated client-side: isFinite && >0). Response body is IGNORED — the app's .then() calls reloadDebtAll() to resync list/stats/trajectory/detail/payments. The new payment must therefore surface via subsequent GET /debts/{id}/payments and recomputed balances/decay.

---

### `PATCH /debts/{id}/plan`

Adjust a debt's payment plan (monthly amount, payment day, term). Fire-and-forget; app reloads all debt data after.

> backend todo: `debts: adjust payment plan (monthly/day/term)`  ·  confidence: **high**

**Request body**

```json
{"monthly":600,"day":28,"term":12}
```

**Response** — frontend ignores the body (fire-and-forget; the page just calls `reload()`). Any 2xx = success.

**Consumed by** — `Debts.jsx:339-352 (onAdjustPlan: prompts for monthly, day, term; body = { monthly:Number, day:Number, term: ''?null:Number }; api.patch(`/debts/${id}/plan`, body).then(() => onAction()))`, `Debts.jsx:403 (ADJUST PLAN button — shown when apr<=0.08)`, `Debts.jsx:600 (onAction = reloadDebtAll)`

**Notes** — Request body = { monthly: number (CHF), day: number (day-of-month), term: number|null } where term is null for revolving (empty prompt). All three are always sent. Response body is IGNORED — .then() calls reloadDebtAll(). The plan change must alter subsequent /debts and /debts/{id} payoff metrics (monthsToPayoff, decaySeries, nextLabel). Button shown only when the debt is NOT high-APR (apr<=0.08 or apr null); high-APR debts get REFINANCE instead.

---

### `POST /debts/{id}/refinance`

Refinance a high-APR debt (new apr/lender). Fire-and-forget; app sends an empty body and reloads all debt data.

> backend todo: `debts: refinance (new apr/lender)`  ·  confidence: **medium**

**Request body**

```json
{}
```

**Response** — frontend ignores the body (fire-and-forget; the page just calls `reload()`). Any 2xx = success.

**Consumed by** — `Debts.jsx:336-338 (onRefinance: api.post(`/debts/${id}/refinance`, {}).then(() => onAction()))`, `Debts.jsx:402 (REFINANCE button — shown when apr!=null && apr>0.08)`, `Debts.jsx:600 (onAction = reloadDebtAll)`

**Notes** — The frontend sends an EMPTY object body {} — it supplies no new apr/lender despite the todo label, so the backend must decide refinance terms server-side (or this is a stub the app just triggers). Response body is IGNORED — .then() calls reloadDebtAll() to resync. Button shown only when apr != null && apr>0.08 (the 'refinance' flag, line 327), i.e. for high-rate debts.

---

## Personal IOUs

_Source: `backend/bin/phosk_api/src/routes//home/ovsiankina/Documents/phoskonomia/backend/bin/phosk_api/src/routes/personal_ious.rs`_

Informal person-to-person IOU ledger (no interest, kept separate from real debts). The Debts page (/home/ovsiankina/Documents/phoskonomia/frontend/app/src/pages/Debts.jsx) renders a two-sided net-position beam plus two columns (OWED TO YOU / YOU OWE) of PersonCards, with per-card REMIND / SETTLE UP / MARK SETTLED actions. Sole consumer is Debts.jsx; data fetched via useGet, actions via api.post. Base URL http://127.0.0.1:3000/api/v1 (lib/api.js).

### `GET /personal-ious`

List all personal IOUs (both directions) — drives the IOU ledger columns and PersonCards.

> backend todo: `debts: list personal IOUs (dir in/out, repaid %)`  ·  confidence: **high**

**Response `200`**

```json
[
  { "id": "papa",   "person": "PAPA",     "initials": "Pa", "dir": "in",  "amount": 800, "since": "MAR 2026", "reason": "Bridge loan toward the flat deposit." },
  { "id": "marco",  "person": "MARCO B.", "initials": "MB", "dir": "in",  "amount": 340, "since": "MAY 2026", "reason": "Laax ski cabin — fronted the whole weekend." },
  { "id": "nadia",  "person": "NADIA",    "initials": "Na", "dir": "in",  "amount": 200, "of": 500, "repaidPct": 0.6, "since": "FEB 2026", "reason": "Used camera — paying you back monthly." },
  { "id": "jonas",  "person": "JONAS",    "initials": "Jo", "dir": "in",  "amount": 75,  "since": "JUN 2026", "reason": "Openair festival ticket." },
  { "id": "lena",   "person": "LENA K.",  "initials": "LK", "dir": "out", "amount": 120, "since": "JUN 2026", "reason": "Hallenstadion concert — she booked both." },
  { "id": "sophie", "person": "SOPHIE",   "initials": "So", "dir": "out", "amount": 45,  "since": "JUN 2026", "reason": "Dinner at Kreuz, split unevenly." }
]
```

**Response fields**

- `id` *(string (slug))* — Stable human/slug id (ADR-008), e.g. 'papa', 'marco'. Used as React key and as the {id} path param for remind/settle/settle-up actions (URL-encoded).
- `person` *(string)* — Display name of the counterparty, e.g. 'MARCO B.'. Rendered as the card name (Debts.jsx:475).
- `initials` *(string)* — Short avatar initials, e.g. 'MB'. Rendered in the avatar bubble (Debts.jsx:473).
- `dir` *(string enum ('in')* — 'out') | Direction. 'in' = they owe you (OWED TO YOU column, blue); 'out' = you owe them (YOU OWE column). App filters the array by this exact string (Debts.jsx:667-668) and picks the action verb from it (Debts.jsx:464,492).
- `amount` *(number (CHF, current outstanding))* — Outstanding amount. Rendered via chf(p.amount,0) (Debts.jsx:479). For partial IOUs this is the remaining balance (paired with 'of' as the original; meta is 'CHF {of-amount} OF {of}').
- `since` *(string (label))* — Human 'since' label rendered verbatim ('SINCE '+p.since at Debts.jsx:490), e.g. 'MAR 2026'. NOT a parsed date — free-form display string.
- `reason` *(string)* — Free-text note describing the IOU, rendered as-is (Debts.jsx:482).
- `repaidPct` *(number (optional))* — Repaid fraction. App tolerates either 0–1 OR 0–100: pct = round(repaidPct * (repaidPct<=1 ? 100 : 1)) (Debts.jsx:466). When null/absent the progress bar is hidden (Debts.jsx:483). Backend-derived; app never recomputes it.
- `of` *(number (optional))* — Original amount for a partially-settled IOU. When present the meta shows 'CHF {of - amount} OF {of}' (Debts.jsx:486). Omit for full/un-partial IOUs.

**Consumed by** — `Debts.jsx:594 (useGet('/personal-ious'))`, `Debts.jsx:595 (Array.isArray(iousGet.data) ? iousGet.data : [])`, `Debts.jsx:667-668 (filter p.dir === 'in' / 'out')`, `Debts.jsx:464 (p.dir)`, `Debts.jsx:466 (p.repaidPct)`, `Debts.jsx:468 (p.id)`, `Debts.jsx:473 (p.initials)`, `Debts.jsx:475 (p.person)`, `Debts.jsx:479,486 (p.amount)`, `Debts.jsx:482 (p.reason)`, `Debts.jsx:486 (p.of)`, `Debts.jsx:490 (p.since)`, `Debts.jsx:775,779 (key={p.id})`

**Notes** — Response is a BARE JSON array (Array.isArray check at :595 — NOT wrapped in {items}/{ious}). Each element is one IOU. Only 'in'/'out' literals are recognized for dir. repaidPct and of are optional (progress bar only renders when repaidPct != null). 'since' is a display string, not ISO. amount/of are plain numbers in CHF (no minor-units). The countIn/countOut totals normally come from /personal-ious/stats; the array length (iouIn.length / iouOut.length) is the fallback when stats counts are absent (Debts.jsx:774,778).

---

### `POST /personal-ious`

Create a new personal IOU.

> backend todo: `debts: create personal IOU`  ·  confidence: **low**

**Request body**

```json
{ "person": "MARCO B.", "initials": "MB", "dir": "in", "amount": 340, "since": "MAY 2026", "reason": "Laax ski cabin — fronted the whole weekend." }
```

**Response** — frontend ignores the body (fire-and-forget; the page just calls `reload()`). Any 2xx = success.

**Notes** — NO consumer exists in the React app — there is no create-IOU form anywhere in Debts.jsx (the only consumer file). Shape is inferred from the GET /personal-ious record shape (person, initials, dir, amount, since, reason; optionally of/repaidPct). Server presumably assigns id (slug). Frontend would not read the response (pattern elsewhere is reload() after mutation), so responseJson left empty. Low confidence — existence known only from the route file (.post on /personal-ious).

---

### `GET /personal-ious/stats`

Aggregate IOU stats: totals owed-to-you / you-owe, net position, and per-direction counts.

> backend todo: `debts: IOU stats (owedToYou/youOwe/net)`  ·  confidence: **high**

**Response `200`**

```json
{ "owedToYou": 1415, "youOwe": 165, "net": 1250, "countIn": 4, "countOut": 2 }
```

**Response fields**

- `owedToYou` *(number (CHF))* — Total others owe you (sum of dir==='in' amounts). Number()'d at :418; rendered in the blue beam label and OWED-TO-YOU column header (:774).
- `youOwe` *(number (CHF))* — Total you owe others (sum of dir==='out' amounts). Number()'d at :419; rendered in the coral beam label and YOU-OWE column header (:778).
- `net` *(number (CHF, optional))* — Net position = owedToYou - youOwe. Drives the net needle/label in NetBeam. OPTIONAL: if null/absent the app recomputes owedToYou - youOwe (:420).
- `countIn` *(number (optional))* — Count of inbound IOUs (dir==='in'). Used for the column header count and for total open count (countIn+countOut at :762,768). Falls back to iouIn.length when null (:774).
- `countOut` *(number (optional))* — Count of outbound IOUs (dir==='out'). Used for the column header count and total open count. Falls back to iouOut.length when null (:778).

**Consumed by** — `Debts.jsx:597 (useGet('/personal-ious/stats'))`, `Debts.jsx:598 (iouStatsGet.data || {})`, `Debts.jsx:418 (S.owedToYou)`, `Debts.jsx:419 (S.youOwe)`, `Debts.jsx:420 (S.net != null ? Number(S.net) : owedToYou - youOwe)`, `Debts.jsx:762,768 (iouStats.countIn + iouStats.countOut)`, `Debts.jsx:774 (iouStats.countIn, iouStats.owedToYou)`, `Debts.jsx:778 (iouStats.countOut, iouStats.youOwe)`

**Notes** — Returns a JSON OBJECT (read as iouStatsGet.data || {}). owedToYou/youOwe are required for correct rendering; net, countIn, countOut are all optional (app has explicit != null fallbacks: net→owedToYou-youOwe, counts→array .length). The header comment at Debts.jsx:414-415 also lists 'maxSingle' but the app NEVER reads maxSingle (grep confirms it appears only in that comment) — do not rely on it being consumed. All amounts plain CHF numbers.

---

### `PATCH /personal-ious/{id}`

Edit an existing personal IOU.

> backend todo: `debts: edit personal IOU`  ·  confidence: **low**

**Path params**

- `id` — Slug id of the IOU (ADR-008 stable term, e.g. 'marco'), URL-encoded.

**Request body**

```json
{ "amount": 300, "reason": "Laax ski cabin — partially settled." }
```

**Response** — frontend ignores the body (fire-and-forget; the page just calls `reload()`). Any 2xx = success.

**Notes** — NO consumer in the React app — Debts.jsx never PATCHes /personal-ious/{id}. Body shape inferred as a partial subset of the IOU record (person/initials/dir/amount/since/reason/of/repaidPct). Frontend would reload() rather than read the response. Low confidence.

---

### `DELETE /personal-ious/{id}`

Delete a personal IOU.

> backend todo: `debts: delete personal IOU`  ·  confidence: **low**

**Path params**

- `id` — Slug id of the IOU to delete (e.g. 'sophie'), URL-encoded.

**Response** — frontend ignores the body (fire-and-forget; the page just calls `reload()`). Any 2xx = success.

**Notes** — NO consumer in the React app — there is no delete-IOU control in Debts.jsx. api.del sends no body and the app would ignore any response. Low confidence; existence known only from the route file (.delete on /personal-ious/{id}).

---

### `POST /personal-ious/{id}/payments`

Record a payment against a personal IOU (partial settlement).

> backend todo: `debts: record an IOU payment`  ·  confidence: **low**

**Path params**

- `id` — Slug id of the IOU receiving the payment (e.g. 'nadia'), URL-encoded.

**Request body**

```json
{ "amount": 100 }
```

**Response** — frontend ignores the body (fire-and-forget; the page just calls `reload()`). Any 2xx = success.

**Notes** — NO direct consumer for the IOU variant — grep confirms the app's only /payments POST is for the DEBTS domain (Debts.jsx:334 POST /debts/{id}/payments {amount}), NOT /personal-ious. Body shape { amount } is inferred by analogy with that debts payment call and the partial-IOU model (amount + 'of' original). Fire-and-forget — response ignored. Low confidence.

---

### `POST /personal-ious/{id}/settle`

Mark a personal IOU fully settled (the MARK SETTLED button on every PersonCard).

> backend todo: `debts: mark IOU settled`  ·  confidence: **high**

**Path params**

- `id` — Slug id of the IOU to settle (e.g. 'marco'), URL-encoded. Comes from p.id of the card.

**Request body**

```json
{}
```

**Response** — frontend ignores the body (fire-and-forget; the page just calls `reload()`). Any 2xx = success.

**Consumed by** — `Debts.jsx:493 (button MARK SETTLED → act('settle'))`, `Debts.jsx:467-468 (api.post(`/personal-ious/${encodeURIComponent(p.id)}/${verb}`, {}))`, `Debts.jsx:601,775,779 (onAction=reloadIou → iousGet.reload()+iouStatsGet.reload())`

**Notes** — App sends an EMPTY JSON object body ({}) — no payload (Debts.jsx:468). Response is IGNORED: the .then() only calls onAction()=reloadIou(), which re-fetches /personal-ious and /personal-ious/stats. So responseJson is intentionally empty (fire-and-forget). Available on BOTH columns (the 'MARK SETTLED' button renders for every PersonCard regardless of dir, Debts.jsx:493).

---

### `POST /personal-ious/{id}/settle-up`

Settle up an IOU you owe (the SETTLE UP button shown on YOU-OWE / dir==='out' cards).

> backend todo: `debts: settle up an IOU`  ·  confidence: **high**

**Path params**

- `id` — Slug id of the outbound IOU being settled up (e.g. 'lena'), URL-encoded.

**Request body**

```json
{}
```

**Response** — frontend ignores the body (fire-and-forget; the page just calls `reload()`). Any 2xx = success.

**Consumed by** — `Debts.jsx:492 (button label/verb SETTLE UP when !inbound → act('settle-up'))`, `Debts.jsx:467-468 (api.post(`/personal-ious/${encodeURIComponent(p.id)}/settle-up`, {}))`, `Debts.jsx:601 (onAction=reloadIou)`

**Notes** — Empty {} body (Debts.jsx:468). The 'SETTLE UP' label/verb is used only on outbound cards: inbound = (p.dir === 'in'), and the verb is chosen by `inbound ? 'remind' : 'settle-up'` at :492 — so settle-up fires for dir==='out'. Response ignored; .then() → reloadIou() re-fetches list + stats. Fire-and-forget, responseJson empty.

---

### `POST /personal-ious/{id}/remind`

Send a reminder for an inbound IOU (the REMIND button shown on OWED-TO-YOU / dir==='in' cards).

> backend todo: `debts: send an IOU reminder`  ·  confidence: **high**

**Path params**

- `id` — Slug id of the inbound IOU to send a reminder for (e.g. 'papa'), URL-encoded.

**Request body**

```json
{}
```

**Response** — frontend ignores the body (fire-and-forget; the page just calls `reload()`). Any 2xx = success.

**Consumed by** — `Debts.jsx:492 (button label/verb REMIND when inbound → act('remind'))`, `Debts.jsx:467-468 (api.post(`/personal-ious/${encodeURIComponent(p.id)}/remind`, {}))`, `Debts.jsx:601 (onAction=reloadIou)`

**Notes** — Empty {} body (Debts.jsx:468). 'REMIND' is the verb only on inbound cards (dir==='in'); chosen by `inbound ? 'remind' : 'settle-up'` at :492. Response ignored; .then() → reloadIou() re-fetches /personal-ious + /personal-ious/stats. Fire-and-forget, responseJson empty.

---

## analytics

_Source: `backend/bin/phosk_api/src/routes//home/ovsiankina/Documents/phoskonomia/backend/bin/phosk_api/src/routes/analytics.rs`_

Retrospective / cross-cycle analytics for the Analytics page (/analytics route, frontend/app/src/pages/Analytics.jsx): a 12-cycle spend/savings trend oscilloscope and its rolled-up stats, per-category momentum small-multiples (now vs 3-cycle avg), a weekday discretionary spending-rhythm heatmap, and an AI (GEMMA4) movers narrative with a suggested soft-cap action. All five endpoints are GET with no query params and no request body; each is fetched independently via useGet and falls back to an <Awaiting/> empty state until the backend serves a real 200. IDs/refs are human/stable terms per ADR-008 (category NAME, signal slug id like "coffee"), never UUIDs. (Note: the page also calls /cycle/current, /signals*, /signals/{id}/cap, etc., but those belong to other domains, not analytics.rs.)

### `GET /analytics/spend-history`

12-cycle (oldest->current) monthly spend/savings history powering the SpendTrend hero scope.

> backend todo: `insights: 12-cycle spend/savings history`  ·  confidence: **high**

**Response `200`**

```json
{
  "points": [
    { "m": "JUL", "yr": "'25", "spend": 3820, "budget": 4200, "rate": 0.251, "projected": false, "over": false },
    { "m": "AUG", "yr": "'25", "spend": 4015, "budget": 4200, "rate": 0.213, "projected": false, "over": false },
    { "m": "SEP", "yr": "'25", "spend": 3650, "budget": 4200, "rate": 0.284, "projected": false, "over": false },
    { "m": "OCT", "yr": "'25", "spend": 3990, "budget": 4200, "rate": 0.218, "projected": false, "over": false },
    { "m": "NOV", "yr": "'25", "spend": 4280, "budget": 4200, "rate": 0.161, "projected": false, "over": true },
    { "m": "DEC", "yr": "'25", "spend": 4610, "budget": 4200, "rate": 0.096, "projected": false, "over": true },
    { "m": "JAN", "yr": "'26", "spend": 3470, "budget": 4200, "rate": 0.32, "projected": false, "over": false },
    { "m": "FEB", "yr": "'26", "spend": 3580, "budget": 4200, "rate": 0.298, "projected": false, "over": false },
    { "m": "MAR", "yr": "'26", "spend": 3910, "budget": 4200, "rate": 0.233, "projected": false, "over": false },
    { "m": "APR", "yr": "'26", "spend": 4050, "budget": 4200, "rate": 0.206, "projected": false, "over": false },
    { "m": "MAY", "yr": "'26", "spend": 3980, "budget": 4200, "rate": 0.22, "projected": false, "over": false },
    { "m": "JUN", "yr": "'26", "spend": 4120, "budget": 4200, "rate": 0.192, "projected": true, "over": false }
  ]
}
```

**Response fields**

- `points` *(array<object>)* — 12-cycle history, oldest -> current. Read as (spendHistory.data && spendHistory.data.points) || []; the page slices the tail to 6/12 for the window tweak. Empty/non-array tolerated (falls back to []), but the hero only renders when points.length>0.
- `points[].m` *(string)* — Month abbreviation, e.g. "JUN". Used as the x-axis tick label and (with yr) in the header range.
- `points[].yr` *(string)* — Short year tag, e.g. "'26". Concatenated to m only in the header range string (first.m+first.yr -> cur.m+cur.yr).
- `points[].spend` *(number)* — CHF total spend for the cycle. Drives bar height and the connecting trace in spend mode. Falls back to 0 if missing (d.spend || 0).
- `points[].budget` *(number)* — CHF budget for the cycle. Only points[0].budget is read -> dashed budget reference line (else falls back to 0/max). Send on every point for consistency.
- `points[].rate` *(number)* — Savings rate (0..1, saved/income) for the cycle. Drives the curve in savings ('rate') mode; gridlines render as percent. Falls back to 0 (d.rate || 0).
- `points[].projected` *(boolean)* — True for the current/run-rate cycle (JUN '26): bar drawn hollow/dashed, x-label dimmed. Optional; falsy treated as a closed cycle.
- `points[].over` *(boolean)* — True if spend exceeded budget: bar tinted hotter red (spend mode only). Optional; falsy = under budget.

**Consumed by** — `Analytics.jsx:333 (useGet('/analytics/spend-history'))`, `Analytics.jsx:350 (histPoints = (spendHistory.data && spendHistory.data.points) || [])`, `Analytics.jsx:352 (trendPoints = client-side slice to last 6/12 for trendWindow tweak)`, `Analytics.jsx:484 (hero renders only when status===200 && histPoints.length>0)`, `Analytics.jsx:37 (budgetLine = (data[0] && data[0].budget) || 0 -> dashed reference line)`, `Analytics.jsx:39-41 (d.rate, d.spend per point)`, `Analytics.jsx:64 (first.m, first.yr, cur.m, cur.yr in header range)`, `Analytics.jsx:100-102 (d.projected, d.over per bar)`, `Analytics.jsx:132 (d.m -> x-axis label)`

**Notes** — GET, no params, no body. Response MUST be an object with a `points` array (app reads spendHistory.data.points specifically; a bare array would NOT work here). Per-point shape mirrors the mock in frontend/.claude-design-export/analytics-data.jsx (income 5100, budgetLine 4200, JUL'25->JUN'26, JUN projected). `rate` should equal (income-spend)/income; `over` = spend>budget. The page recomputes nothing server-side: it only slices the array and reads these fields, so include every listed field on every point (yr/projected/over optional but expected). When status!==200 or points is empty the hero shows <Awaiting/>.

---

### `GET /analytics/spend-history/stats`

Rolled-up stats over the spend history: avg/peak/low, current & previous cycle, savings rate, vs-avg/vs-prev deltas. Feeds the KPI band and trend footer.

> backend todo: `insights: spend-history stats (avg/peak/low/vs-avg/vs-prev)`  ·  confidence: **high**

**Response `200`**

```json
{
  "months": 12,
  "avg": 3914,
  "avgRate": 0.232,
  "totalSaved": 14185,
  "curVsAvgPct": 5,
  "curVsPrevPct": 4,
  "cur": { "m": "JUN", "yr": "'26", "spend": 4120, "budget": 4200 },
  "prev": { "m": "MAY", "yr": "'26", "spend": 3980 },
  "peak": { "m": "DEC", "yr": "'25", "spend": 4610 },
  "low": { "m": "JAN", "yr": "'26", "spend": 3470 }
}
```

**Response fields**

- `months` *(number)* — Cycle count on record. Shown as "<months> cycles on record" (line 439) and "saved over <months> cyc" (line 471). '—' if null.
- `avg` *(number)* — CHF average spend over closed cycles (6-mo avg label). Shown in KPI (line 462) + trend footer (line 137); chf(_,0); '—' if null.
- `avgRate` *(number)* — Average savings rate (0..1). Rendered Math.round(avgRate*100) as a percent in the SAVINGS RATE KPI (line 470). '—' if null.
- `totalSaved` *(number)* — CHF total cashflow saved across all cycles. Shown in the savings-rate KPI sub only when non-null (line 471).
- `curVsAvgPct` *(number)* — Signed integer percent: current cycle vs 6-mo avg. Drives up/down arrow + sign in KPI (lines 455-456) and trend footer (lines 140-143). Block hidden when null.
- `curVsPrevPct` *(number)* — Signed integer percent: current vs previous cycle. Shown in trend footer note only when non-null (uses prev.m), line 143.
- `cur` *(object)* — Current cycle summary. Reads cur.spend (this-cycle run-rate, the headline number), cur.budget (6-mo-avg KPI sub), and cur.m/cur.yr (SpendTrend header via fallback).
- `cur.spend` *(number)* — CHF current-cycle spend / run-rate. Primary headline number (curSpend = S.cur ? S.cur.spend : null); '—' if cur missing.
- `cur.budget` *(number)* — CHF current-cycle budget. Shown as "budget CHF …" (line 464); '—' if null.
- `cur.m` *(string)* — Current cycle month abbrev (SpendTrend header range end, via cur = S.cur || data[last]).
- `cur.yr` *(string)* — Current cycle short year (SpendTrend header range end).
- `prev` *(object)* — Previous cycle. Only prev.m is read (label in the vs-prev footer note, line 143).
- `prev.m` *(string)* — Previous cycle month abbrev.
- `peak` *(object)* — Highest-spend cycle. Reads peak.m + peak.spend in KPI sub (line 465) and peak.spend/.m/.yr in footer (line 138).
- `peak.spend` *(number)* — CHF peak spend (warn-toned in footer; '—' if null).
- `peak.m` *(string)* — Peak cycle month abbrev.
- `peak.yr` *(string)* — Peak cycle short year (footer only).
- `low` *(object)* — Leanest cycle. Reads low.spend, low.m, low.yr (footer, line 139).
- `low.spend` *(number)* — CHF lowest spend (ok-toned; '—' if null).
- `low.m` *(string)* — Leanest cycle month abbrev.
- `low.yr` *(string)* — Leanest cycle short year.

**Consumed by** — `Analytics.jsx:334 (useGet('/analytics/spend-history/stats'))`, `Analytics.jsx:353 (S = histStats.data || {})`, `Analytics.jsx:413 (curSpend = S.cur ? S.cur.spend : null)`, `Analytics.jsx:439 (S.months), 456 (S.curVsAvgPct), 462 (S.avg), 464-465 (S.cur.budget, S.peak.m/.spend), 470-471 (S.avgRate, S.totalSaved, S.months)`, `Analytics.jsx:55,137-143 (SpendTrend footer: S.cur fallback, S.avg, S.peak.spend/.m/.yr, S.low.spend/.m/.yr, S.curVsAvgPct, S.curVsPrevPct, S.prev.m)`

**Notes** — GET, no params, no body. Response is a flat object (S = histStats.data || {}). Every field is individually null-guarded in the app: missing scalars render '—', and the curVsAvgPct/curVsPrevPct footer blocks are conditionally hidden when null. cur/prev/peak/low are nested objects; if cur is absent SpendTrend falls back to the last point of spend-history, but the KPI band reads S.cur directly so include it. Values mirror frontend/.claude-design-export/analytics-data.jsx histStats (avg over closed cycles, peak=DEC'25 4610, low=JAN'26 3470, cur=JUN'26 4120 budget 4200, prev=MAY'26 3980). curVsAvgPct/curVsPrevPct are rounded signed integers (percent points).

---

### `GET /analytics/category-momentum`

Per-category momentum cards: current cycle spend vs trailing 3-cycle average, with a 12-point sparkline series.

> backend todo: `insights: per-category momentum (now vs 3-cycle avg)`  ·  confidence: **high**

**Response `200`**

```json
[
  { "name": "GROCERIES", "now": 612, "deltaPct": 4, "fixed": false, "series": [588, 595, 601, 590, 607, 599, 610, 603, 615, 608, 619, 612] },
  { "name": "DINING & CAFÉS", "now": 284, "deltaPct": 34, "fixed": false, "series": [212, 198, 221, 207, 233, 219, 241, 228, 252, 239, 268, 284] },
  { "name": "HOUSING", "now": 1450, "deltaPct": 0, "fixed": true, "series": [1450, 1450, 1450, 1450, 1450, 1450, 1450, 1450, 1450, 1450, 1450, 1450] },
  { "name": "TRANSPORT", "now": 96, "deltaPct": -26, "fixed": false, "series": [130, 124, 118, 122, 112, 116, 108, 110, 104, 100, 98, 96] }
]
```

**Response fields**

- `[]` *(array<object>)* — BARE TOP-LEVEL ARRAY of category momentum records. App does Array.isArray(momentum.data) ? momentum.data : [] — anything not an array yields []. Do NOT wrap in {items|categories|...}.
- `[].name` *(string (category NAME))* — Category display name, e.g. "GROCERIES" (ADR-008 stable name, not a UUID). Used as React key, A-Z sort key, and card heading.
- `[].now` *(number)* — CHF current-cycle spend for the category. Shown as the card's big number (chf(now,0)) and used as the 'spend' sort key. Falls back to 0.
- `[].deltaPct` *(number)* — Signed integer percent vs trailing 3-cycle avg. Sign drives ↑/↓; abs>=15 = 'hot'; default momentum sort uses abs(deltaPct). Falls back to 0; ignored/overridden when fixed (shows FIXED badge).
- `[].fixed` *(boolean)* — True for fixed/recurring categories (e.g. HOUSING): card shows 'FIXED' badge, indigo spark, and sorts to top of momentum order. Falls back to false.
- `[].series` *(array<number>)* — ~12-point per-cycle spend series for the sparkline; last point should equal `now`. Empty/missing falls back to [0,0].

**Consumed by** — `Analytics.jsx:335 (useGet('/analytics/category-momentum'))`, `Analytics.jsx:375 (arr = Array.isArray(momentum.data) ? momentum.data : [])`, `Analytics.jsx:376-378 (sort by now / name / fixed+abs(deltaPct))`, `Analytics.jsx:545-548 (renders when status===200 && cats.length>0; key={c.name})`, `Analytics.jsx:199-213 (MomentumCard: c.deltaPct, c.fixed, c.name, c.series, c.now)`

**Notes** — GET, no params, no body. CRITICAL: response is a BARE ARRAY at the top level (not object-wrapped). Fields read by MomentumCard are exactly name, now, deltaPct, series, fixed. The source comment at Analytics.jsx:197 also lists `budget` and `priorAvg` but neither is read by the rendered card — they are unused (omit unless cheap; harmless if present). `series` length is treated as the sparkline width (12 in the mock); pin series[last]=now. Mirrors frontend/.claude-design-export/analytics-data.jsx catTrends (TREND multipliers per category; deltaPct = round((now-priorAvg)/priorAvg*100)).

---

### `GET /analytics/rhythm/weekday`

Weekday discretionary-spend rhythm heatmap: average CHF per weekday plus rollup stats (peak day, max, weekend share).

> backend todo: `insights: weekday spending rhythm (discretionary)`  ·  confidence: **high**

**Response `200`**

```json
{
  "weekday": [
    { "d": "MON", "v": 64 },
    { "d": "TUE", "v": 48 },
    { "d": "WED", "v": 72 },
    { "d": "THU", "v": 58 },
    { "d": "FRI", "v": 118 },
    { "d": "SAT", "v": 142 },
    { "d": "SUN", "v": 39 }
  ],
  "stats": { "peak": { "d": "SAT", "v": 142 }, "max": 142, "weekendShare": 56 }
}
```

**Response fields**

- `weekday` *(array<object>)* — Per-weekday bars (MON..SUN). Render gated on rhythm.data && Array.isArray(rhythm.data.weekday) && length>0, else <Awaiting/>. Object-wrapped (rhythm.data.weekday), not a bare array.
- `weekday[].d` *(string)* — Weekday label, e.g. "SAT". Used as React key, bar label, and matched against the derived peak day to flag the peak column.
- `weekday[].v` *(number)* — Average CHF discretionary spend for that weekday. Bar height is v/max; shown verbatim as "CHF <v>" (NO chf() formatting applied — send already-rounded integers).
- `stats` *(object)* — Rollup stats object (rhythm.data.stats). Optional — the heatmap recomputes max/peak from weekday[] if stats fields are missing.
- `stats.max` *(number)* — Max weekday v, used as the bar-scale denominator. If null, app falls back to Math.max(1, ...weekday[].v).
- `stats.peak` *(object)* — Peak weekday. Only stats.peak.d is read (to mark the peak column). If absent, app derives the peak from weekday[].
- `stats.peak.d` *(string)* — Weekday label of the peak day (e.g. "SAT").
- `stats.weekendShare` *(number)* — Integer percent of discretionary spend landing FRI-SUN. Shown as "<n>% LANDS FRI–SUN"; renders '—' if null (not recomputed by the app).

**Consumed by** — `Analytics.jsx:336 (useGet('/analytics/rhythm/weekday'))`, `Analytics.jsx:563-564 (renders when status===200 && rhythm.data && Array.isArray(rhythm.data.weekday) && weekday.length>0; passes weekday + stats)`, `Analytics.jsx:222-247 (RhythmHeatmap: weekday[].d, weekday[].v; stats.max, stats.peak.d, stats.weekendShare)`

**Notes** — GET, no params, no body. Response is an OBJECT with `weekday` (array) and `stats` (object) — app reads rhythm.data.weekday and rhythm.data.stats; a bare array would NOT render. `v` is printed raw ("CHF " + x.v) with no chf() formatting, so send already-rounded integers (mock: MON 64..SUN 39). stats is defensive: max/peak are recomputed from weekday[] when missing, but weekendShare is shown directly ('—' if null). Excludes fixed costs (rent/insurance) by design. Mirrors frontend/.claude-design-export weekday/weekdayStats (peak SAT 142, weekendShare 56%).

---

### `GET /analytics/insights/movers`

AI (GEMMA4) narrative about the cycle's spending movers, plus an optional suggested soft-cap action the user can apply with one click.

> backend todo: `ai: movers narrative + suggested cap (GEMMA4)`  ·  confidence: **high**

**Response `200`**

```json
{
  "model": "GEMMA4",
  "text": "COFFEE is your fastest riser this cycle — up 28% to CHF 86.40 across 11 visits, mostly weekday mornings. DINING & CAFÉS as a whole is heating (+34% vs its 3-cycle average), while TRANSPORT keeps cooling. A soft cap on COFFEE would absorb most of the drift without touching groceries.",
  "suggestedCap": {
    "signalId": "coffee",
    "amount": 70,
    "projectedSavings": 16
  }
}
```

**Response fields**

- `model` *(string)* — LLM tag shown in the panel header. Optional — falls back to literal 'GEMMA4' when missing (ins.model || 'GEMMA4').
- `text` *(string)* — Narrative paragraph. The whole insight block is gated on status===200 && ins.text; if absent the panel shows <Awaiting label="GEMMA4 READ"/>.
- `suggestedCap` *(object)* — null | Optional suggested soft cap. Rendered as an apply button only when suggestedCap && suggestedCap.signalId are truthy. Omit/null = no button.
- `suggestedCap.signalId` *(string (signal slug id))* — Target item-signal id, e.g. "coffee" (ADR-008 slug, not a UUID). Shown in the button ("CAP coffee AT CHF …") and posted to POST /signals/{signalId}/cap.
- `suggestedCap.amount` *(number)* — CHF cap amount. Shown in the button (chf(_,0)) and sent as `amount` in the cap POST body.
- `suggestedCap.projectedSavings` *(number)* — Optional CHF projected savings if the cap is applied. Appended to the button ("· +CHF <n> SAVED", chf(_,0)) only when non-null. NOT sent in the POST.

**Consumed by** — `Analytics.jsx:337 (useGet('/analytics/insights/movers'))`, `Analytics.jsx:403 (ins = insights.data || {})`, `Analytics.jsx:404 (suggestedCap = ins.suggestedCap || null)`, `Analytics.jsx:517 (ins.model || 'GEMMA4' header)`, `Analytics.jsx:518-521 (ins.text -> narrative, render gated on status===200 && ins.text)`, `Analytics.jsx:522-527 (suggestedCap.signalId/.amount/.projectedSavings -> CAP button label)`, `Analytics.jsx:405-409 (applyCap -> POST /signals/{signalId}/cap {amount, period:'cycle'} — separate signals-domain call)`

**Notes** — GET, no params, no body. Response is a flat object (ins = insights.data || {}). Only model, text, suggestedCap{signalId, amount, projectedSavings} are read. suggestedCap is optional and the button only appears when suggestedCap.signalId is present. Applying the cap is a SEPARATE call to POST /signals/{signalId}/cap with body {amount: suggestedCap.amount, period: 'cycle'} (period is hardcoded by the app, not taken from this response) — that endpoint lives in the signals domain, not analytics. Example values borrowed from the COFFEE item-signal in frontend/.claude-design-export/signals-data.jsx (id 'coffee', cycleSpend 86.40, deltaPct +28, txns 11).

---

## ai

_Source: `backend/bin/phosk_api/src/routes//home/ovsiankina/Documents/phoskonomia/backend/bin/phosk_api/src/routes/ai.rs`_

The AI-assistant domain: the left-rail assistant panel's activity feed, chat (history + send), the model/engine online status shown in the panel header and the Config ASSISTANT tile, plus a global low-confidence reprocess trigger. Only /ai/feed, /ai/chat (GET+POST), and /ai/status are actually consumed by the React app today; /ai/feed/{id}/dismiss is called fire-and-forget; /ai/reprocess (global) has no consumer yet.

### `GET /ai/feed`

Activity feed for the assistant panel: auto-maintenance items (categorize/reprocess/suggest/detect) the AI emits, each optionally with action buttons.

> backend todo: `ai: activity feed (categorize/reprocess/suggest/detect)`  ·  confidence: **high**

**Response `200`**

```json
{"feed":[{"id":"feed-2026-06-18-001","kind":"categorize","text":"Categorized 14 items from Migros receipt into Groceries","conf":0.94,"time":"2m ago","actions":["OK"]},{"id":"feed-2026-06-18-002","kind":"reprocess","text":"Re-reading 3 low-confidence lines on Coop receipt","conf":0.61,"state":"running","time":"just now"},{"id":"feed-2026-06-18-003","kind":"suggest","text":"Noticed recurring Denner toothpaste — track as an item-signal?","conf":0.82,"time":"18m ago","cand":true,"candidateId":"toothpaste","actions":["TRACK","DISMISS"]}]}
```

**Response fields**

- `feed` *(array (or top-level array, or items[]))* — List of feed items. App accepts a bare array, or an object wrapping it under `feed` or `items` (shell.jsx:46). Empty/missing -> [] (empty-state text shown).
- `feed[].id` *(string)* — Stable feed-item id; used as React key and in the dismiss URL (/ai/feed/{id}/dismiss). Falls back to array index when null/absent (f.id != null ? f.id : i). ADR-008: slug/human id, not UUID.
- `feed[].kind` *(string)* — Item category driving icon + CSS class. Known values: 'categorize' (✓), 'reprocess' (⟳), 'suggest' (⌁); any other value or absent -> ∿ icon (note: className becomes 'fitem undefined' if absent, so send a kind). Rendered as className 'fitem <kind>'.
- `feed[].text` *(string)* — Human-readable description of what the AI did/noticed. Rendered verbatim (shell.jsx:117).
- `feed[].conf` *(number (0..1))* — null | Confidence; shown as 'CONF NN%' (Math.round(conf*100)). Optional — hidden when null/absent (f.conf != null guard, shell.jsx:119).
- `feed[].state` *(string)* — null | When strictly === 'running', shows a 'RUNNING…' badge (colored var(--indigo-neon)). Any other value/absent -> no badge (shell.jsx:120).
- `feed[].time` *(string)* — Relative/absolute timestamp label rendered verbatim (e.g. '2m ago'). App does no date math (shell.jsx:121).
- `feed[].actions` *(string[])* — null | Button labels. Truthiness-gated (shell.jsx:123). First button (index 0) is styled 'p' when cand truthy, else 'coral'; non-first buttons get no modifier (shell.jsx:126). Optional — no buttons block if falsy.
- `feed[].cand` *(boolean)* — null | If truthy AND the clicked button is index 0, POSTs /signals/candidates/{candidateId|sig|id}/track instead of dismiss (shell.jsx:128-130). Any other button index falls through to dismiss.
- `feed[].candidateId` *(string)* — null | Signal-candidate id used (first in the || chain) in the track URL and passed to onTrack. Falls back to `sig` then `id`. ADR-008: item slug (e.g. 'toothpaste').
- `feed[].sig` *(string)* — null | Alternative signal id, used in the track URL/onTrack if candidateId absent (shell.jsx:129-130).

**Consumed by** — `shell.jsx:42 (useGet('/ai/feed'))`, `shell.jsx:46 (feed = Array.isArray(feedData) ? feedData : (feedData && (feedData.feed || feedData.items)) || [])`, `shell.jsx:87 (icon by f.kind: categorize/reprocess/suggest/else)`, `shell.jsx:88 (dismissed filter on f.id != null ? f.id : i)`, `shell.jsx:112 (key = f.id != null ? f.id : i)`, `shell.jsx:114 (className 'fitem ' + f.kind)`, `shell.jsx:115 (icon(f.kind))`, `shell.jsx:117 (f.text)`, `shell.jsx:119 (f.conf != null -> Math.round(f.conf*100) + '%')`, `shell.jsx:120 (f.state === 'running' -> RUNNING…)`, `shell.jsx:121 (f.time)`, `shell.jsx:123-136 (f.actions[] buttons; f.cand, f.candidateId || f.sig || f.id for track)`

**Notes** — Response wrapping is flexible: a bare array, or {feed:[...]}, or {items:[...]} (shell.jsx:46; null/undefined wrapper -> []). For non-cand items (or any non-index-0 button), the action button just POSTs /ai/feed/{id}/dismiss (with id falling back to the array index) then reloads. After any action click the item is also hidden locally via dismissFeed(key). conf/state/actions/cand/candidateId/sig are all optional. The list is the only thing read — no count/cursor fields are consumed.

---

### `POST /ai/feed/{id}/dismiss`

Dismiss a single feed item; the panel optimistically hides it locally and then reloads the feed.

> backend todo: `ai: dismiss a feed item`  ·  confidence: **low**

**Path params**

- `id` — The feed item id (feed[].id from GET /ai/feed). Slug/stable id per ADR-008; falls back to the item's array index if the item had no id.

**Response** — frontend ignores the body (fire-and-forget; the page just calls `reload()`). Any 2xx = success.

**Consumed by** — `shell.jsx:132 (api.post('/ai/feed/' + (f.id != null ? f.id : i) + '/dismiss').then(reloadFeed))`, `shell.jsx:134 (dismissFeed(key) optimistic local hide, key = f.id != null ? f.id : i)`

**Notes** — Fire-and-forget. api.post called with no body arg -> apiCall sends no Content-Type and no payload (api.js:48-54). Response body is completely ignored — the app only chains .then(reloadFeed) to re-fetch /ai/feed, and hides the item locally regardless of result. Any 2xx is sufficient. The app may send the array index as {id} when a feed item lacked an id, so the backend should tolerate non-canonical ids gracefully.

---

### `GET /ai/chat`

Chat message history used to seed the assistant transcript when the panel mounts.

> backend todo: `ai: chat message history`  ·  confidence: **high**

**Response `200`**

```json
{"messages":[{"role":"user","text":"How much did I spend on groceries this cycle?"},{"role":"assistant","text":"You've spent CHF 312.40 on Groceries in the June 2026 cycle, mostly at Migros and Coop."},{"role":"user","text":"track toothpaste"},{"role":"assistant","text":"Tracking 'toothpaste' as an item-signal from now on."}]}
```

**Response fields**

- `messages` *(array (or top-level array, or history[]))* — Ordered chat transcript. App accepts a bare array, or {messages:[...]}, or {history:[...]} (shell.jsx:60). Missing -> transcript stays empty.
- `messages[].role` *(string)* — Sender role. 'user' -> rendered as 'YOU' (who='usr'); anything else -> assistant ('sys', labeled with model name). App also accepts who:'usr' or from:'user' as equivalents (shell.jsx:62).
- `messages[].text` *(string)* — Message content rendered verbatim. App falls back to `reply` then `content` then '' if `text` is null/absent (shell.jsx:63).

**Consumed by** — `shell.jsx:43 (useGet('/ai/chat'))`, `shell.jsx:60 (raw = Array.isArray(chatData) ? chatData : (chatData.messages || chatData.history || []))`, `shell.jsx:62 (who from m.role==='user' || m.who==='usr' || m.from==='user')`, `shell.jsx:63 (text from m.text != null ? m.text : (m.reply || m.content || ''))`

**Notes** — Role detection is tri-key: m.role==='user' OR m.who==='usr' OR m.from==='user' marks a user message; everything else is treated as assistant/system. Text is read as m.text != null ? m.text : (m.reply || m.content || ''). Wrapping accepted as bare array, {messages:[...]}, or {history:[...]}. No timestamps/ids are read. Prefer {role,text} as the canonical shape.

---

### `POST /ai/chat`

Send a chat message to the local model (GEMMA4 via Ollama); the reply is appended to the transcript. May emit track/cap intents server-side.

> backend todo: `ai: send chat message (local GEMMA4, may emit track/cap intents)`  ·  confidence: **high**

**Request body**

```json
{"text":"track toothpaste"}
```

**Response `200`**

```json
{"reply":"Tracking 'toothpaste' as an item-signal from now on — I'll follow it on its own axis."}
```

**Response fields**

- `reply` *(string)* — Assistant's reply text, appended to the transcript. Read first in the || chain (res.data.reply, shell.jsx:78).
- `text` *(string)* — Alternative reply field, used only if `reply` is falsy (res.data.text). Either field works.
- `todo` *(string)* — Only read on a 501 response: the backend's not-implemented todo tag, shown inline in the bubble (shell.jsx:76). Not part of a real 200 response.

**Consumed by** — `shell.jsx:73 (api.post('/ai/chat', { text: q }))`, `shell.jsx:76 (res.data.todo shown on 501)`, `shell.jsx:78 (reply = (res.data && (res.data.reply || res.data.text)) || fallback)`

**Notes** — Request body is exactly { text: <string> } — the trimmed user input (shell.jsx:69,73). On res.ok the app reads res.data.reply || res.data.text and renders it (empty/missing body -> '(backend replied with empty body)', shell.jsx:78). On 501 it reads res.data.todo (shell.jsx:76). On status 0 it shows 'Backend unreachable'. Other non-ok -> 'Backend error {status}'. No streaming; a single JSON reply is expected. 'track/cap intents' are a server-side side effect, not reflected in any extra field the frontend reads.

---

### `GET /ai/status`

Assistant status: online flag plus model/engine/location, shown in the AI panel header rail and the Config ASSISTANT tile.

> backend todo: `ai: assistant status (model/engine/online/watched signals)`  ·  confidence: **high**

**Response `200`**

```json
{"online":true,"model":"GEMMA4","engine":"OLLAMA","location":"LOCAL"}
```

**Response fields**

- `online` *(boolean)* — Whether the local model is reachable. Coerced via !!aiStatus.online; drives the green(--ok)/idle(--ink-3) pulse dot in the panel rail and the head Dot tone (shell.jsx:47,95,100). Default false when status absent.
- `model` *(string)* — Model name, e.g. 'GEMMA4'. Shown in the rail vlabel and header 'mdl' line, and used as the assistant's display name in chat bubbles. App default 'GEMMA4' (shell.jsx:48,94,103,148). Config uses it for the ASSISTANT tile (Config.jsx:177).
- `engine` *(string)* — Inference engine, e.g. 'OLLAMA'. Shown in the header 'mdl' line. App default 'OLLAMA' (shell.jsx:49,103). Config uses it for the ASSISTANT tile (Config.jsx:176).
- `location` *(string)* — Where the model runs, e.g. 'LOCAL'. Shown in the header 'mdl' line. App default 'LOCAL' (shell.jsx:50,103). Not read by Config.

**Consumed by** — `shell.jsx:44 (useGet('/ai/status'))`, `shell.jsx:47 (online = aiStatus ? !!aiStatus.online : false)`, `shell.jsx:48 (model = (aiStatus && aiStatus.model) || 'GEMMA4')`, `shell.jsx:49 (engine = (aiStatus && aiStatus.engine) || 'OLLAMA')`, `shell.jsx:50 (location = (aiStatus && aiStatus.location) || 'LOCAL')`, `Config.jsx:160 (useGet('/ai/status'))`, `Config.jsx:173 (ai = aiStatusGet.data when status === 200)`, `Config.jsx:176 (engine = ... || (ai && ai.engine) || null)`, `Config.jsx:177 (model = ... || (ai && ai.model) || null)`

**Notes** — Flat object (not wrapped). shell.jsx defaults each field when missing (model='GEMMA4', engine='OLLAMA', location='LOCAL', online=false), so all fields are optional but recommended. Config.jsx only reads engine+model and only when HTTP status is exactly 200 (else null; engine/model fall back to summary/account sources first, then to the '—' empty state). The todo mentions 'watched signals' but the frontend reads NO such field — do not add it on the contract's account.

---

### `POST /ai/reprocess`

Global trigger to reprocess all low-confidence items across the dataset (not scoped to one transaction).

> backend todo: `ai: reprocess low-confidence items (global scope)`  ·  confidence: **low**

**Response** — frontend ignores the body (fire-and-forget; the page just calls `reload()`). Any 2xx = success.

**Notes** — NO frontend consumer found in /frontend/app/src/. The only reprocess calls in the app are per-transaction: api.post('/transactions/{id}/reprocess', {}) in Transactions.jsx:104 and :214 (a different domain, transactions.rs). The global /ai/reprocess is unused, so request and response shapes are unknown from the frontend. If/when wired, expect a fire-and-forget POST (likely empty or {} body) followed by a feed/transactions reload, mirroring the per-transaction pattern. Shape is a genuine guess until a consumer exists.

---

## Settings & account

_Source: `backend/bin/phosk_api/src/routes//home/ovsiankina/Documents/phoskonomia/backend/bin/phosk_api/src/routes/settings_account.rs`_

UI preferences (the cross-surface tweak store backing the Config page), a config summary roll-up, the account profile, and local AI-engine selection. The only live frontend consumer is pages/Config.jsx (it reads preferences, summary, and engine/model off /account). Preferences are persisted backend-side as a flat key/value mirror of the localStorage "phosk.cfg" tweak store; localStorage is authoritative on each device and backend prefs are merged UNDER it (defaults < backend < localStorage). The account holder/iban fields and all AI-engine selection routes exist in the route file but have NO frontend consumer today.

### `GET /settings/preferences`

Fetch the device's stored UI tweak/preference overrides (flat key->value, mirroring the Config-page tweak set).

> backend todo: `settings: get UI preferences`  ·  confidence: **high**

**Response `200`**

```json
{"aiOpen":true,"topDateFmt":"{label} · DAY {day}/{days}","drillMode":"inspector","showSparks":true,"envLayout":"cards","sort":"order","showProj":true,"subView":"cards","subSort":"due","subAmounts":"monthly","subGroup":false,"subHlAuto":false,"subInsp":"dock","debtView":"cards","debtSort":"balance","debtStrategy":"avalanche","debtProjection":true,"debtGroup":false,"debtHlAuto":false,"debtInsp":"dock","iouShow":true,"trendWindow":"12","trendMode":"spend","sigSort":"momentum","showCand":true,"showMomentum":true,"momentumSort":"momentum","showRhythm":true,"sigInsp":"dock"}
```

**Response fields**

- `aiOpen` *(boolean)* — Assistant dock open by default on every surface.
- `topDateFmt` *(string)* — Top-bar date-format template; tokens {label} {day} {days} {asOf}. Default '{label} · DAY {day}/{days}'.
- `drillMode` *(string)* — Transactions item-signal panel placement: 'inspector' | 'drawer' | 'sheet'.
- `showSparks` *(boolean)* — Transactions: inline sparkline on tracked-item tags.
- `envLayout` *(string)* — Budgets envelope layout: 'cards' | 'rows'.
- `sort` *(string)* — Budgets sort order: 'order' | 'used' | 'over'.
- `showProj` *(boolean)* — Budgets: projected end-of-cycle markers.
- `subView` *(string)* — Subscriptions layout: 'cards' | 'rows'.
- `subSort` *(string)* — Subscriptions sort: 'due' | 'amount' | 'name'.
- `subAmounts` *(string)* — Subscriptions amount mode: 'monthly' | 'annual'.
- `subGroup` *(boolean)* — Subscriptions: group by cadence.
- `subHlAuto` *(boolean)* — Subscriptions: highlight AI auto-detected charges.
- `subInsp` *(string)* — Subscriptions inspector placement: 'dock' | 'drawer'.
- `debtView` *(string)* — Debts layout: 'cards' | 'rows'.
- `debtSort` *(string)* — Debts sort: 'balance' | 'apr' | 'payoff' | 'name'.
- `debtStrategy` *(string)* — Payoff strategy overlay: 'avalanche' | 'snowball' | 'none'.
- `debtProjection` *(boolean)* — Debts: projected trajectory overlay.
- `debtGroup` *(boolean)* — Debts: group by type.
- `debtHlAuto` *(boolean)* — Debts: highlight auto-detected.
- `debtInsp` *(string)* — Debts inspector placement: 'dock' | 'drawer'.
- `iouShow` *(boolean)* — Debts: show the IOU ledger.
- `trendWindow` *(string)* — Analytics spend-trend window: '12' | '6' (string, not number).
- `trendMode` *(string)* — Analytics trend series: 'spend' | 'rate'.
- `sigSort` *(string)* — Analytics item-signal sort: 'momentum' | 'spend' | 'az'.
- `showCand` *(boolean)* — Analytics: show AI candidate signal.
- `showMomentum` *(boolean)* — Analytics: show category-momentum section.
- `momentumSort` *(string)* — Analytics momentum order: 'momentum' | 'spend' | 'az'.
- `showRhythm` *(boolean)* — Analytics: show weekday spending rhythm.
- `sigInsp` *(string)* — Analytics inspector placement: 'dock' | 'drawer'.

**Consumed by** — `Config.jsx:150 (useGet)`, `Config.jsx:152 (mergeBackend on status===200 && data)`, `Config.jsx:34-39 (pickKnown filters to CFG_DEFAULTS keys)`, `Config.jsx:54-58 (merged UNDER localStorage)`

**Notes** — Response is a FLAT object (NOT wrapped). The app runs it through pickKnown() (Config.jsx:34-39), which keeps ONLY keys present in CFG_DEFAULTS (the 30 keys above) and silently ignores extras — so returning a subset, the full set, or extra keys are all safe. All values may be absent: missing keys fall back to CFG_DEFAULTS, then to localStorage which always wins (defaults < backend < localStorage, Config.jsx:54-58). Booleans are real JSON booleans; the segment/select keys are the exact string literals shown in the CfgSeg/CfgSelect option lists; trendWindow is a STRING ('12'/'6'). Only applied when status===200 and data is truthy (Config.jsx:152); a 501 leaves CFG_DEFAULTS standing. No frontend reads holder/iban/engine/model from THIS endpoint.

---

### `PATCH /settings/preferences`

Persist one or more changed UI preferences (partial flat object of just the edited keys).

> backend todo: `settings: update preferences`  ·  confidence: **high**

**Request body**

```json
{"subInsp":"drawer","debtInsp":"drawer","sigInsp":"drawer"}
```

**Response** — frontend ignores the body (fire-and-forget; the page just calls `reload()`). Any 2xx = success.

**Consumed by** — `Config.jsx:67 (api.patch in set())`, `Config.jsx:60-68 (edits = single {key:val} or a multi-key object)`, `Config.jsx:182 (setInspAll sends 3 keys at once)`

**Notes** — Request body is the `edits` object — a PARTIAL flat map of only the changed preference keys (same key space as the GET response). Often a single key (e.g. {"aiOpen":false} or {"topDateFmt":"..."}); the 'Inspector placement' control sends three keys together {subInsp,debtInsp,sigInsp} (Config.jsx:182). FIRE-AND-FORGET: the app does not await or read the response (Config.jsx:65-67 comment 'localStorage already holds the authoritative live value'); only the api.js toast reacts to status (501/ok/err). responseJson intentionally empty — any 2xx body is ignored.

---

### `DELETE /settings/preferences`

Reset all UI preferences back to defaults (clear the device's stored overrides).

> backend todo: `settings: reset preferences`  ·  confidence: **high**

**Response** — frontend ignores the body (fire-and-forget; the page just calls `reload()`). Any 2xx = success.

**Consumed by** — `Config.jsx:73 (api.del in reset())`, `Config.jsx:69-74 (reset())`, `Config.jsx:184/213 (onReset / 'Reset all' button)`

**Notes** — No request body (api.del sends none — api.js:79). FIRE-AND-FORGET: triggered by the 'Reset all' button (Config.jsx:213); the app simultaneously clears localStorage and sets state to CFG_DEFAULTS locally, then calls api.del without awaiting or reading the response (Config.jsx:69-73). Only the api.js toast reacts to status. responseJson intentionally empty.

---

### `GET /settings/preferences/defaults`

(Backend stub) Canonical preference defaults — NOT consumed by the current frontend.

> backend todo: `settings: preference defaults`  ·  confidence: **low**

**Response** — frontend ignores the body (fire-and-forget; the page just calls `reload()`). Any 2xx = success.

**Notes** — NO frontend consumer exists anywhere in frontend/app/src (verified by grep on 'preferences/defaults'). The React app hardcodes its own CFG_DEFAULTS (Config.jsx:15-31) and never fetches this route, so the contract is unconstrained by the frontend. If implemented to match intent, it should return the same flat key->value shape as GET /settings/preferences but containing the full canonical default set (the 30 keys in CFG_DEFAULTS). Shape is a guess — frontend reads nothing here.

---

### `GET /settings/summary`

Config-page status roll-up: total/changed preference counts plus current AI engine & model.

> backend todo: `settings: config summary (counts/engine/model/storedOnDevice)`  ·  confidence: **high**

**Response `200`**

```json
{"totalPreferences":30,"changedCount":3,"engine":"OLLAMA","model":"GEMMA4"}
```

**Response fields**

- `totalPreferences` *(number)* — Count of tunable preferences; rendered as the PREFERENCES ribbon value and in the header summary. Read via != null check (Config.jsx:169).
- `changedCount` *(number)* — How many preferences differ from defaults; rendered in the CHANGED ribbon (coral if >0). Read via != null check (Config.jsx:170).
- `engine` *(string)* — Active local AI engine label (e.g. 'OLLAMA'); shown in the ASSISTANT ribbon. First source in the engine fallback chain (Config.jsx:176).
- `model` *(string)* — Active model label (e.g. 'GEMMA4'); shown in the ASSISTANT ribbon. First source in the model fallback chain (Config.jsx:177).

**Consumed by** — `Config.jsx:157 (useGet)`, `Config.jsx:168 (gated on status===200 && data)`, `Config.jsx:169 summary.totalPreferences`, `Config.jsx:170 summary.changedCount`, `Config.jsx:176 summary.engine`, `Config.jsx:177 summary.model`, `Config.jsx:208/228 render total`, `Config.jsx:233 renders changed`, `Config.jsx:239 renders engine·model`

**Notes** — Flat object, not wrapped. Only read when status===200 && data truthy (Config.jsx:168). Each field is OPTIONAL: totalPreferences/changedCount use a `!= null` guard and fall back to LOCALLY computed counts (localTotal = Object.keys(CFG_DEFAULTS).length = 30; localChanged = keys differing from defaults). engine/model are the FIRST link in a fallback chain summary -> /account -> /ai/status -> null (Config.jsx:176-177), so they may be omitted. The todo mentions 'storedOnDevice' but the frontend NEVER reads it (the 'stored on this device' text at Config.jsx:209 is a static string — verified by grep, zero 'storedOnDevice' matches). Counts are real numbers; engine/model are uppercase string labels matching the app's shell.jsx fallbacks (OLLAMA/GEMMA4).

---

### `GET /account`

Account profile — frontend only reads AI engine & model from it (as a fallback for the Config ASSISTANT tile).

> backend todo: `settings: account profile (holder/iban/model/engine)`  ·  confidence: **high**

**Response `200`**

```json
{"engine":"OLLAMA","model":"GEMMA4"}
```

**Response fields**

- `engine` *(string)* — AI engine label; READ as the 2nd fallback for the ASSISTANT tile engine (after /settings/summary, before /ai/status) — Config.jsx:176.
- `model` *(string)* — AI model label; READ as the 2nd fallback for the ASSISTANT tile model — Config.jsx:177.
- `holder` *(string)* — SPECULATIVE (unread by frontend). Account holder name. Present in the route's todo and PATCH /account but grep for 'holder' across frontend/app/src returns ZERO matches.
- `iban` *(string)* — SPECULATIVE (unread by frontend). Account IBAN (CHF). Present in the todo and PATCH /account but grep for 'iban' returns ZERO matches.

**Consumed by** — `Config.jsx:159 (useGet)`, `Config.jsx:172 (acct gated on status===200 && data)`, `Config.jsx:176 acct.engine`, `Config.jsx:177 acct.model`, `Config.jsx:239 renders engine·model in ASSISTANT ribbon`

**Notes** — Flat object, not wrapped. Only `engine` and `model` are actually CONSUMED today (Config.jsx:176-177), both optional via `acct && acct.engine` short-circuit and a 3-way fallback chain (summary -> account -> ai/status -> null). holder/iban are in the backend todo (route line 28) and the PATCH /account counterpart (route line 29) but NO app code reads them (verified by grep — zero matches), so they are speculative and not reflected in responseJson (which shows only the read fields). Engine/model labels match app conventions (OLLAMA/GEMMA4, see shell.jsx:48-49). Read only when status===200 && data truthy. Endpoint confidence stays high because the consumed fields (engine/model) are solidly grounded.

---

### `PATCH /account`

(Backend stub) Edit the account profile (holder/iban) — NOT consumed by the current frontend.

> backend todo: `settings: edit account (holder/iban)`  ·  confidence: **low**

**Response** — frontend ignores the body (fire-and-forget; the page just calls `reload()`). Any 2xx = success.

**Notes** — PRESENT in the route file (settings_account.rs:29, .patch on /account) but MISSED by the draft. NO frontend consumer — there is no account-editing form anywhere in frontend/app/src (verified: grep for api.patch/api.put/api.post on 'account' returns zero matches). The Config page only READS engine/model from GET /account; it never writes the profile. If implemented to intent the body would be a partial {holder, iban} (string fields), but the shape is unconstrained by the frontend — pure guess.

---

### `GET /account/ai/engines`

(Backend stub) List of available local AI engines/models — NOT consumed by the current frontend.

> backend todo: `settings: list available local AI engines/models`  ·  confidence: **low**

**Response** — frontend ignores the body (fire-and-forget; the page just calls `reload()`). Any 2xx = success.

**Notes** — NO frontend consumer (verified by grep for 'ai/engines' across frontend/app/src — zero matches). The Config page surfaces engine/model only as read-only labels from /settings/summary, /account, and /ai/status; it offers no engine PICKER UI. Contract is unconstrained by the frontend. If implemented to intent it would likely return a list of {engine,model} options (e.g. an array or {engines:[...]}), but that is a pure guess — frontend reads nothing here.

---

### `PUT /account/ai/engine`

(Backend stub) Select the active AI engine/model — NOT consumed by the current frontend.

> backend todo: `settings: select AI engine/model`  ·  confidence: **low**

**Response** — frontend ignores the body (fire-and-forget; the page just calls `reload()`). Any 2xx = success.

**Notes** — NO frontend consumer (verified by grep for 'ai/engine' across frontend/app/src — the only matches are local variable reads `ai.engine`/`acct.engine`/`summary.engine`, never this PUT path). No engine-selection control exists in Config.jsx or anywhere in the app, so neither the request body nor the response shape is exercised by the frontend. If implemented to intent the body would likely be {engine, model} (string labels per ADR-008, e.g. {"engine":"OLLAMA","model":"GEMMA4"}), but this is a guess. responseJson empty — frontend reads nothing.

---

## Shell

_Source: `backend/bin/phosk_api/src/routes//home/ovsiankina/Documents/phoskonomia/backend/bin/phosk_api/src/routes/shell.rs`_

Shell / nav / UI-config endpoints: the app chrome's navigation page list and the per-surface UI preferences ("UI config subset" — topDateFmt etc.) that drive every page's tweaks. NOTE: neither declared path is consumed at its literal route by the React app today. /nav/pages corresponds to the hardcoded PHOSK_PAGES nav array (comps.jsx:11-19) which is never fetched. /config corresponds conceptually to the UI-preferences store the Config page actually drives via the /settings/preferences family (GET Config.jsx:150, PATCH Config.jsx:67, DELETE Config.jsx:73) — the literal "/config" is ONLY the react-router route (main.jsx:56; nav `to` at comps.jsx:18), confirmed by grep to never appear as an api call. The route file defines exactly two routes: GET /nav/pages, and GET+PATCH /config (no DELETE). Shapes below are grounded in PHOSK_PAGES and in CFG_DEFAULTS / useCfg in Config.jsx, since those define what the frontend would read/write if these routes were wired.

### `GET /nav/pages`

Navigation pages for the top-bar/mega-menu chrome (key/abbr/glyph/href/to/desc, plus a backend-only `available`).

> backend todo: `shell: navigation pages (key/abbr/glyph/href/available)`  ·  confidence: **low**

**Response `200`**

```json
[{"key":"DASHBOARD","abbr":"DASH","glyph":"◳","href":"Phoskonomia%20Dashboard.html","to":"/dashboard","desc":"Spend trace, savings & alerts"},{"key":"TRANSACTIONS","abbr":"TXN","glyph":"⊟","href":"Phoskonomia%20Transactions.html","to":"/transactions","desc":"Every receipt, itemized"},{"key":"BUDGETS","abbr":"BUDG","glyph":"▦","href":"Phoskonomia%20Budgets.html","to":"/budgets","desc":"Envelopes & monthly caps"},{"key":"SUBSCRIPTIONS","abbr":"SUBS","glyph":"⊠","href":"Phoskonomia%20Subscriptions.html","to":"/subscriptions","desc":"Standing recurring charges"},{"key":"DEBTS","abbr":"DEBT","glyph":"∿","href":"Phoskonomia%20Debts.html","to":"/debts","desc":"Balances, payoff & IOUs"},{"key":"ANALYTICS","abbr":"ANLY","glyph":"⌁","href":"Phoskonomia%20Analytics.html","to":"/analytics","desc":"Trends & item-signals"},{"key":"CONFIG","abbr":"CFG","glyph":"⊙","href":"Phoskonomia%20Config.html","to":"/config","desc":"Preferences for every surface"}]
```

**Response fields**

- `key` *(string)* — Page id / display name in expanded nav, uppercase (e.g. "DASHBOARD", "CONFIG"). Used as React key (comps.jsx:127/135), as the nav label when navMode!='abbr' (comps.jsx:137), and matched against the active page (comps.jsx:136,166,179).
- `abbr` *(string)* — Short label shown when the bar is cramped (navMode=='abbr'), e.g. "DASH", "CFG" (comps.jsx:130,137).
- `glyph` *(string)* — Single-char glyph shown in the hover-card and mega-menu, e.g. "◳", "⊙" (comps.jsx:161,182).
- `href` *(string)* — Legacy static-mockup HTML filename (e.g. "Phoskonomia%20Config.html"); present in the const but NEVER read by the React app — carried, dead. Speculative for the API.
- `to` *(string)* — react-router path (e.g. "/config"). When truthy the page is a live <Link> (comps.jsx:136,188); when falsy the mega-menu renders a disabled "SOON" card (comps.jsx:184,189).
- `desc` *(string)* — One-line description shown in the hover-card (comps.jsx:164) and mega-menu (comps.jsx:183), e.g. "Preferences for every surface".
- `available` *(boolean)* — From the backend todo label only. The app has NO `available` field today; it infers availability purely from `to` (truthy => live link, falsy => SOON). Speculative/unread by the UI — if served, a backend should drive SOON via `to` rather than this. Kept per todo, downgraded.

**Consumed by** — `comps.jsx:11`, `comps.jsx:127`, `comps.jsx:130`, `comps.jsx:137`, `comps.jsx:161`, `comps.jsx:163`, `comps.jsx:164`, `comps.jsx:183`, `comps.jsx:184`, `comps.jsx:188`

**Notes** — NOT WIRED: grep confirms the frontend never fetches /nav/pages. The nav list is the hardcoded const PHOSK_PAGES (comps.jsx:11-19), so this contract is reverse-engineered from that array's exact field names (key, abbr, glyph, href, to, desc). Response is a BARE ARRAY of page objects (PHOSK_PAGES is `.map`-ed directly; no wrapper). Order matters — rendered in array order across the top bar and mega grid. `to` is the only routing field the React app uses; `href` is dead baggage (never read). The app derives 'SOON/disabled' from a falsy `to`, NOT from `available`; the todo's `available` field does not exist in app code (speculative). ids follow ADR-008: `key` and `to` are slugs/names, no UUIDs. confidence=low: frontend consumes the local const, not this endpoint.

---

### `GET /config`

UI config subset — the per-surface preference store (topDateFmt and the other CFG_DEFAULTS keys) layered under localStorage on each page.

> backend todo: `settings: UI config subset (topDateFmt, ...)`  ·  confidence: **medium**

**Response `200`**

```json
{"topDateFmt":"{label} · DAY {day}/{days}","aiOpen":true,"drillMode":"inspector","showSparks":true,"envLayout":"cards","sort":"order","showProj":true,"subView":"cards","subSort":"due","subAmounts":"monthly","subGroup":false,"subHlAuto":false,"subInsp":"dock","debtView":"cards","debtSort":"balance","debtStrategy":"avalanche","debtProjection":true,"debtGroup":false,"debtHlAuto":false,"debtInsp":"dock","iouShow":true,"trendWindow":"12","trendMode":"spend","sigSort":"momentum","showCand":true,"showMomentum":true,"momentumSort":"momentum","showRhythm":true,"sigInsp":"dock"}
```

**Response fields**

- `topDateFmt` *(string)* — Top-bar date format template with {label} {day} {days} {asOf} tokens; default "{label} · DAY {day}/{days}". Read by TopBar (comps.jsx:41-42,50) and the Config page (Config.jsx:261).
- `aiOpen` *(boolean)* — Assistant dock open by default. Default true. Read in Config (Config.jsx:238,251) AND on every page via the shared store as `tw.aiOpen` (e.g. Transactions.jsx:330,365; Budgets.jsx:354,374; Subscriptions.jsx:424,433; Debts.jsx:553,562; Analytics.jsx:309,318).
- `drillMode` *(string)* — Transactions signal-panel placement: "inspector"|"drawer"|"sheet". Default "inspector". Config.jsx:270; consumed in Transactions via the shared store.
- `showSparks` *(boolean)* — Transactions: inline sparkline on tracked-item pills. Default true. Config.jsx:275.
- `envLayout` *(string)* — Budgets envelope layout: "cards"|"rows". Default "cards". Config.jsx:283.
- `sort` *(string)* — Budgets sort order: "order"|"used"|"over". Default "order". Config.jsx:288.
- `showProj` *(boolean)* — Budgets projection markers. Default true. Config.jsx:293.
- `subView` *(string)* — Subscriptions layout: "cards"|"rows". Default "cards". Config.jsx:301.
- `subSort` *(string)* — Subscriptions sort: "due"|"amount"|"name". Default "due". Config.jsx:306.
- `subAmounts` *(string)* — Subscriptions amount mode: "monthly"|"annual". Default "monthly". Config.jsx:311.
- `subGroup` *(boolean)* — Subscriptions group by cadence. Default false. Config.jsx:316.
- `subHlAuto` *(boolean)* — Subscriptions highlight auto-detected. Default false. Config.jsx:319.
- `subInsp` *(string)* — Subscriptions inspector placement: "dock"|"drawer". Default "dock". Unified with debtInsp/sigInsp by the General > Inspector control (Config.jsx:180-182).
- `debtView` *(string)* — Debts layout: "cards"|"rows". Default "cards". Config.jsx:327.
- `debtSort` *(string)* — Debts sort: "balance"|"apr"|"payoff"|"name". Default "balance". Config.jsx:332.
- `debtStrategy` *(string)* — Debts payoff strategy overlay: "avalanche"|"snowball"|"none". Default "avalanche". Config.jsx:338.
- `debtProjection` *(boolean)* — Debts projected trajectory. Default true. Config.jsx:343.
- `debtGroup` *(boolean)* — Debts group by type. Default false. Config.jsx:346.
- `debtHlAuto` *(boolean)* — Debts highlight auto-detected. Default false. Config.jsx:349.
- `debtInsp` *(string)* — Debts inspector placement: "dock"|"drawer". Default "dock". Unified via Config.jsx:180-182.
- `iouShow` *(boolean)* — Debts show IOU ledger. Default true. Config.jsx:352.
- `trendWindow` *(string)* — Analytics spend-trend window in cycles: "12"|"6". Default "12". Config.jsx:360.
- `trendMode` *(string)* — Analytics trend series: "spend"|"rate". Default "spend". Config.jsx:365.
- `sigSort` *(string)* — Analytics item-signal sort: "momentum"|"spend"|"az". Default "momentum". Config.jsx:370.
- `showCand` *(boolean)* — Analytics show candidate signal. Default true. Config.jsx:375.
- `showMomentum` *(boolean)* — Analytics category-momentum section. Default true. Config.jsx:378.
- `momentumSort` *(string)* — Analytics momentum order: "momentum"|"spend"|"az". Default "momentum". Config.jsx:381.
- `showRhythm` *(boolean)* — Analytics weekday rhythm section. Default true. Config.jsx:386.
- `sigInsp` *(string)* — Analytics inspector placement: "dock"|"drawer". Default "dock". Unified via Config.jsx:180-182.

**Consumed by** — `Config.jsx:34`, `Config.jsx:57`, `Config.jsx:150`, `Config.jsx:151`, `Config.jsx:152`, `comps.jsx:41`, `comps.jsx:42`, `comps.jsx:46`, `tweaks.jsx:194`, `Transactions.jsx:330`, `Budgets.jsx:354`, `Subscriptions.jsx:424`, `Debts.jsx:553`, `Analytics.jsx:309`

**Notes** — Path-naming mismatch CONFIRMED by grep: the app drives this store via GET /settings/preferences (Config.jsx:150 useGet), NOT a literal GET /config — "/config" appears in app code only as a react-router route (main.jsx:56) and the nav `to` field (comps.jsx:18). Treat /config GET as an alias of the UI-preferences subset. RESPONSE is a flat object keyed by preference name; the frontend filters it through pickKnown() against CFG_DEFAULTS (Config.jsx:34-39,57) so only keys present in CFG_DEFAULTS are adopted and UNKNOWN keys are ignored (extras are safe). ALL fields optional — any missing key falls back to its CFG_DEFAULTS default; an empty {} or a 501 means defaults stand. Backend prefs are merged UNDER localStorage (defaults < backend < local, Config.jsx:54-58), so a live local override always wins. Keys propagate to every page via the shared `phosk.cfg` localStorage store + the "phoskcfg" event (tweaks.jsx:191-227, useTweaks), so e.g. aiOpen/drillMode/subView are read off this object on their respective pages even though they're set on Config. Field names+types are EXACT (verbatim from CFG_DEFAULTS, Config.jsx:15-31). confidence=medium: shapes exact, but the literal path is inferred from the todo since the app calls /settings/preferences.

---

### `PATCH /config`

Save a UI config subset — persist a partial set of preference edits (fire-and-forget; localStorage is authoritative).

> backend todo: `settings: save UI config`  ·  confidence: **medium**

**Request body**

```json
{"topDateFmt":"{label} · DAY {day}/{days}"}
```

**Response** — frontend ignores the body (fire-and-forget; the page just calls `reload()`). Any 2xx = success.

**Consumed by** — `Config.jsx:60`, `Config.jsx:67`, `Config.jsx:251`, `Config.jsx:262`

**Notes** — Path-naming mismatch CONFIRMED by grep: the app PATCHes /settings/preferences (Config.jsx:67), not a literal /config; treat /config PATCH as the same 'save UI config' operation. REQUEST BODY is a PARTIAL edits object — only the changed keys, not the full config. The toggle/select handlers call set(key, val) which normalizes to `{ [key]: val }` (Config.jsx:60-61) and sends that single-key object (e.g. {"aiOpen":false}, {"debtStrategy":"snowball"}, or the topDateFmt example shown); the General > Inspector control sends a MULTI-key edit {"subInsp":"drawer","debtInsp":"drawer","sigInsp":"drawer"} (Config.jsx:182). Keys/values are the same vocabulary as the GET response. RESPONSE IS IGNORED — fire-and-forget: set() does not read the result, the toast only surfaces 501/errors, and localStorage already holds the authoritative live value (Config.jsx:62-67). Hence responseJson="". NOTE the route file has NO DELETE on /config, yet the app's reset() calls DELETE /settings/preferences (Config.jsx:73) — a reset/clear-all endpoint is missing from this route module. confidence=medium: request shape exact, literal path inferred from todo.

---

## exports

_Source: `backend/bin/phosk_api/src/routes//home/ovsiankina/Documents/phoskonomia/backend/bin/phosk_api/src/routes/exports.rs`_

CSV file-download exports for the three list-bearing domains (transactions, budget, subscriptions). These are plain text/csv downloads, NOT JSON. VERIFIED CENTRAL FINDING (confirmed by exhaustive grep of frontend/app/src/): the React app NEVER references any of these three paths.

### `GET /exports/transactions.csv`

Download the transaction ledger as CSV, applying the same filters/sort the /transactions list uses.

> backend todo: `export: transactions CSV (mirrors list filters)`  ·  confidence: **low**

**Query params**

- `period` *(string, optional)* — Time window selector mirrored from the Transactions list. The list maps its `horizon` UI state (TODAY/7D/MONTH/QUARTER/YEAR/ALL) through HZ_PERIOD and sends the result under the key `period` (Transactions.jsx:394); empty maps to ALL and is dropped by qs(). Inferred for this export from the sibling GET /transactions call; not verified for this endpoint.
- `shop` *(string, optional)* — Filter by shop NAME (ADR-008 stable term, e.g. "Migros"). Sent as `shop: shop || undefined` (Transactions.jsx:395); omitted when empty.
- `category` *(string, optional)* — Filter by category NAME (e.g. "Groceries"). Sent as `category: cat || undefined` (Transactions.jsx:396); omitted when empty.
- `sort` *(string, optional)* — Sort order mirrored from the list control: "date" | "amount" | "shop" (Transactions.jsx:170-174,397).
- `q` *(string, optional)* — Free-text (debounced) search query; sent as `q: qParam || undefined` (Transactions.jsx:398), omitted when empty.

**Response** — frontend ignores the body (fire-and-forget; the page just calls `reload()`). Any 2xx = success.

**Response fields**

- `(csv header) date` *(string)* — Transaction date display string, e.g. "12 Jun". App reads t.date (Transactions.jsx:129/137).
- `(csv header) shop` *(string)* — Shop NAME, e.g. Migros. App reads t.shop (Transactions.jsx:139).
- `(csv header) category` *(string)* — Category NAME, e.g. Groceries. App reads t.category (Transactions.jsx:141).
- `(csv header) amount` *(decimal CHF)* — Transaction total in CHF. App reads t.amount and renders chf(t.amount) (Transactions.jsx:150).
- `(csv header) id` *(string slug)* — Stable transaction id used by the app as a row key (t.id, Transactions.jsx:457); slug-style per ADR-008, not a UUID.

**Notes** — NOT JSON — Content-Type should be text/csv (likely Content-Disposition: attachment; filename=transactions.csv). The frontend never calls this path, so the response is unconstrained by frontend code; columns above are inferred from the fields the app reads on GET /transactions rows (Transactions.jsx:129/139/141/150/457 → t.date, t.shop, t.category, t.amount, t.id). CORRECTION: the draft listed the query param as `horizon`; the actual key sent on the sibling list is `period` (= HZ_PERIOD[horizon]) — `horizon` is only local UI state. The todo 'mirrors list filters' implies the accepted params equal the GET /transactions filter set (period/shop/category/sort/q). Empty/undefined params are dropped client-side by qs(). responseJson left empty: CSV, not JSON; frontend reads no body.

---

### `GET /exports/budget.csv`

Download the budget snapshot as CSV — one row per envelope (category) plus alert/recurring context.

> backend todo: `export: budget CSV (envelopes/alerts/recurring)`  ·  confidence: **low**

**Response** — frontend ignores the body (fire-and-forget; the page just calls `reload()`). Any 2xx = success.

**Response fields**

- `(csv header) name` *(string)* — Envelope/category NAME, e.g. Groceries. App reads c.name (Budgets.jsx:85,118).
- `(csv header) budget` *(decimal CHF)* — Envelope cap. App reads c.budget via capOf() (Budgets.jsx:387) and in the used-fraction sort (Budgets.jsx:416,418).
- `(csv header) spent` *(decimal CHF)* — Amount spent in this envelope. App reads c.spent (Budgets.jsx:23,38,74,114).
- `(csv header) proj` *(decimal CHF)* — Projected end-of-cycle spend. App reads c.proj (Budgets.jsx:23,39,75).
- `(csv header) remaining` *(decimal CHF)* — Remaining = cap - spent. App reads c.remaining when present, else recomputes (Budgets.jsx:77).
- `(csv header) usedPct` *(number 0..1)* — Fraction of cap used. App reads c.usedPct when present, else derives spent/budget (Budgets.jsx:416).
- `(csv header) fixed` *(boolean)* — Whether the envelope is a fixed/locked cap. App reads c.fixed (Budgets.jsx:24,86,105,118,123).

**Notes** — NOT JSON — text/csv download. No query params consumed anywhere: the Budgets page calls /categories, /budget/totals, /budget/allocation all with no params (Budgets.jsx:365-367), so this export likely takes none. The frontend never calls /exports/budget.csv; columns are inferred from the category objects the app reads in EnvCard/EnvRow (c.name/c.budget/c.spent/c.proj/c.remaining/c.usedPct/c.fixed). Note c.budget IS read (as the cap, via capOf at line 387 and the sort at 416). The todo 'envelopes/alerts/recurring' hints the CSV may also include alert/recurring rows, but the app reads alerts/recurring on OTHER pages and never ties them to this export, so any such columns are pure guess. responseJson empty: CSV, not JSON.

---

### `GET /exports/subscriptions.csv`

Download the subscriptions list as CSV — one row per recurring charge.

> backend todo: `export: subscriptions CSV`  ·  confidence: **low**

**Response** — frontend ignores the body (fire-and-forget; the page just calls `reload()`). Any 2xx = success.

**Response fields**

- `(csv header) name` *(string)* — Subscription NAME, e.g. Spotify, Salt. App reads s.name (Subscriptions.jsx:220,267).
- `(csv header) amount` *(decimal CHF)* — Charge amount in CHF. App reads s.amount and renders chf(...) (Subscriptions.jsx:206,262; also impulse amount 49/116).
- `(csv header) cadence` *(string enum)* — Billing cadence: "monthly" | "yearly". App reads s.cadence (Subscriptions.jsx:33,205,259,484-485).
- `(csv header) day` *(number)* — Day-of-cycle the charge lands on. App reads s.day (Subscriptions.jsx:239; also impulse day 54/61).
- `(csv header) status` *(string enum)* — Status: ok|due|soon|watch|paid. App reads s.status (Subscriptions.jsx:25,234,270; impulse toneOf 70-73 incl. "paid").
- `(csv header) source` *(string)* — Origin of the entry; used as data-src and to derive AUTO/USER (source==="llm"). App reads s.source (Subscriptions.jsx:212,263,495,503).
- `(csv header) id` *(string slug)* — Stable subscription id used as row key / selection key (s.id, Subscriptions.jsx:215,265,496,503); slug-style per ADR-008.

**Notes** — NOT JSON — text/csv download. The Subscriptions list endpoint GET /subscriptions takes sort/group/amounts query params (Subscriptions.jsx:449-453), but the CSV export route exposes no such params in any frontend call (nothing references this path), so whether it mirrors those is unknown — queryParams left empty. Columns inferred from the subscription objects the app reads on GET /subscriptions rows (s.name/s.amount/s.cadence/s.day/s.status/s.source/s.id). responseJson empty: CSV, not JSON; the frontend reads no body. Confidence low because nothing in frontend/app/src/ ever requests this endpoint.

---


## Coverage

Documented **101** endpoints across **15** domains.

Verification pass made 75 corrections to the first drafts:

- **Cycle & dashboard insights**: spend-series: todayIndex is passed to PhoskChart as today= but the chart destructures `today` and never uses it (today marker derives from last cumulative point) — marked speculative, kept the field since the prop is wired, and clarified the per-field confidence
- **Cycle & dashboard insights**: spend-series: clarified that `daily` is only rendered when showBars=true, which is false on the dashboard, so it is effectively unread on this page — marked speculative
- **Cycle & dashboard insights**: spend-series: reordered fields to lead with the three solidly-consumed arrays (cumulative, pace, lastCycleCumulative) and removed todayIndex from the example responseJson since it is unread
- **Cycle & dashboard insights**: spend-series: added prims.jsx line refs and dropped the speculative todayIndex value from the example body
- **Cycle & dashboard insights**: totals: added a note that this route is a bare ni! stub with no header comment/test pinning the keys (unlike /cycle/current) — field shapes inferred from frontend only; confidence stays high since all fields are directly read
- **Cycle & dashboard insights**: totals: added chf0/chf decimal precision detail to perDayToStayOnBudget and spent descriptions
- **Cycle & dashboard insights**: cycle/current: corrected/expanded cross-page consumer line refs (Subscriptions.jsx:442,444,534; Debts.jsx:106,575-576; Analytics.jsx:330-331) and aligned field descriptions with the actual route impl (day=day-of-month, daysLeft=days-day, asOf=day+month abbr)
- **Cycle & dashboard insights**: top-shops: added (CHF) unit to total and maxTotal types and noted shop NAME is per ADR-008 (not UUID)
- **Transactions & receipt lines**: Fixed consumer citation: t.category renders at Transactions.jsx:141 (the draft said :142 — line 142 is the item-count span, not category)
- **Transactions & receipt lines**: category queryParam note: clarified that the Dashboard alert deep-link navigates to /transactions?category=<name> as a react-router URL (comps.jsx:287) but the Transactions page's useGet does NOT read that URL param back — its `cat` state still starts '' — so the draft's implication that 'the same param is reused as a URL query on the page' was misleading
- **Transactions & receipt lines**: flag field: added explicit note that the Transactions page does NOT read t.flag (Dashboard/TxnTape only) for clarity
- **Transactions & receipt lines**: Tightened/added precise line-number anchors throughout (qs filter behavior at api.js:43, groupByDay at 339-347, OCR-header at 229, Awaiting-gate at 106/216, etc.) — no field, type, method, path, request-body or query-param errors were found; the draft was substantively accurate
- **Categories & budget envelopes**: Added missing field `spark` (number[]) to GET /categories — read by Dashboard.jsx:178/182 for the hero CHANNELS sparkline (Array.isArray(c.spark)); it is in the backendTodo but the draft omitted it.
- **Categories & budget envelopes**: Fixed `usedPct` type/desc on GET /categories: flagged the cross-page UNIT CONFLICT — Budgets.jsx:416 expects a 0..1 fraction while Dashboard.jsx:186 does Math.round(c.usedPct)+'%' expecting 0..100. Draft labeled it '0..1+ fraction' which only matches one consumer.
- **Categories & budget envelopes**: Added the Dashboard CHANNELS strip consumers (Dashboard.jsx:176-188 reading c.name/c.budget/c.spent/c.spark/c.usedPct) to GET /categories consumers — the draft only listed CatRows and missed the hero channels block.
- **Categories & budget envelopes**: Updated GET /categories responseJson so each item carries `spark` and a 0..100 `usedPct` consistent with the Dashboard render (and trimmed the example to fewer rows).
- **Categories & budget envelopes**: Clarified `proj`/`spark` scope (Budgets-only vs Dashboard-only) and noted CatRows interpolates c.items unconditionally (renders 'undefined items' if absent).
- **Categories & budget envelopes**: Fixed the GET /categories/{name} path-param example encoding (Café -> %C3%89, not a literal É) to match encodeURIComponent output.
- **alerts**: Removed 'info' from the alerts[].tone enum — the frontend never matches 'info'; comps.jsx:279 and Dashboard.jsx:81-82 only branch on 'alert'/'warn'/'llm' with everything else hitting a default indigo/⚠ branch. Documented tone as alert|warn|llm + default.
- **alerts**: Dropped 'source' from the GET /alerts summary (kept only in backendTodo) since the frontend reads no source field — tone encodes source.
- **alerts**: Clarified the `after` callback signature in apply consumers: it is (r) => { onChanged(); return r; } (comps.jsx:281), not a no-arg function.
- **alerts**: Verified all consumer line numbers, methods, paths, path params, empty {} request bodies, and the single t.category read against comps.jsx and Dashboard.jsx — all other draft claims were accurate and confidence labels honest.
- **recurring**: Tightened the garbled comps.jsx:255 consumer string to the actual nested ternary (r.status==='due'?'alert':r.status==='soon'?'warn':'ok')
- **recurring**: Clarified status field desc: code only branches on 'due' and 'soon'; everything else falls through to 'ok' (no explicit 'others -> ok' check)
- **recurring**: Noted amount renders via chf() = 2 decimals (vs chf0 whole-CHF used only for monthlyTotal), and that all amounts are decimal CHF not minor units
- **recurring**: Clarified monthlyTotal is read only off the object form of the response (consistency with notes)
- **recurring**: Confirmed via api.js:51-54 that mark-paid sends no Content-Type/body, and via api.js:64-68 that it is a non-quiet (toasting) call
- **recurring**: Confirmed /recurring/{name}/confirm has zero frontend consumers via repo-wide grep; kept low confidence and corrected the analogy reference to the actual Subscriptions.jsx call sites (:477 detect, :322 act)
- **subscriptions**: Removed priceRose from GET /subscriptions (list) fields[] and responseJson: it is NOT read by the list consumers (SubCard/SubRow only read s.hist via Spark); priceRose is only read in the inspector's SubHistBars, so it stays documented on GET /subscriptions/{id} only. Added a note explaining this.
- **subscriptions**: Fixed the activ list example: changed nextLabel from the literal "—" (which is the frontend em-dash fallback, never a backend value) to null, matching how the backend actually signals a missing label.
- **Item signals**: GET /signals: fixed the SignalCard consumer citation from the wrong 'Dashboard.jsx:269 / shell.jsx:269' to the actual SignalCard definition shell.jsx:244-256 (Dashboard.jsx:269 is an unrelated category-budgets panel header; SignalStrip's map is shell.jsx:269)
- **Item signals**: GET /signals: corrected ItemSignalRow citation to its real definition range Analytics.jsx:154-176 (call site :499-500) instead of bare ':500'
- **Item signals**: GET /signals: noted explicitly that parent/since/txns are read ONLY by the Analytics matrix row (ItemSignalRow), NOT by the dashboard SignalCard, and re-cited deltaPct/cycleQty/unit/cycleSpend with both ItemSignalRow and SignalCard line numbers
- **Item signals**: GET /signals/movers: downgraded the 'all' field description to a dead read — it is read into moversAll (Analytics.jsx:356) but never rendered anywhere
- **Item signals**: GET /signals/{id}: added that Transactions' SignalSheet passes sigId UNencoded (Transactions.jsx:353) sourced from line.signal_id, unlike Dashboard/Analytics which encodeURIComponent
- **Item signals**: POST /signals/{id}/cap: corrected consumer line range to Analytics.jsx:405-409 (applyCap fn body) from ':402-409'
- **Item signals**: track/dismiss/cap: added api.js:76 reference confirming api.post with no body sends no JSON, and clarified track/dismiss path ids are not URL-encoded by callers
- **debts**: GET /debts/trajectory responseJson had a stray trailing comma after the closing brace (invalid JSON) — removed it
- **debts**: GET /debts/trajectory notes: line ref for the points.length>0 render gate corrected to line 64
- **debts**: Refinance condition tightened to apr != null && apr > 0.08 (matches line 327; was 'apr>0.08' alone) in REFINANCE consumer/notes and the ADJUST PLAN field note (apr<=0.08 OR apr null)
- **debts**: Tightened a few consumer line spans to match the file (DebtCard 227-279, DebtRow 283-297, DecayLine 196-219); payments recent-extraction snippet aligned to the actual line 321-322 expression
- **analytics**: spend-history consumer: corrected 'histPoints = spendHistory.data.points || []' to the actual null-guarded '(spendHistory.data && spendHistory.data.points) || []' (line 350) and the projected/over line range to 100-102
- **analytics**: spend-history field budget: corrected fallback from 'stats/max' to '0/max' (budgetLine = (data[0] && data[0].budget) || 0; there is no stats fallback for the budget line)
- **analytics**: stats consumers: corrected the KPI-band line citations (439 months, 456 curVsAvgPct, 462 avg, 464-465 cur.budget/peak, 470-471 avgRate/totalSaved) which the draft had clumped inaccurately
- **analytics**: stats field peak: noted peak.m+peak.spend are also read in the KPI sub at line 465 (draft only attributed peak to the footer)
- **analytics**: stats field cur.spend: corrected description to match actual guarded read 'S.cur ? S.cur.spend : null'
- **analytics**: category-momentum sort: clarified default momentum sort uses abs(deltaPct) and fixed is overridden, matching line 378
- **analytics**: rhythm consumer: corrected line 563 gate to include the leading 'rhythm.data &&' guard before Array.isArray
- **analytics**: rhythm weekday[].v: emphasized it is rendered RAW with no chf() formatting (already implied in notes; tightened the field desc)
- **analytics**: Grounding-path references: normalized '.claude-design-export/...' to the actual 'frontend/.claude-design-export/...' path verified via find
- **analytics**: insights/movers: noted projectedSavings is chf(_,0)-formatted and explicitly NOT sent in the POST body; added txns 11 to the signals-data grounding note
- **ai**: Tightened shell.jsx line citations to match the actual file (feed filter/key at :88/:112 use 'f.id != null ? f.id : i'; actions block spans :123-136; chat reply read is the guarded (res.data && (res.data.reply||res.data.text)) at :78; chat text fallback is m.text != null ? m.text : ...)
- **ai**: Corrected feed[] wrapper guard to '(feedData && (feedData.feed || feedData.items)) || []' (draft omitted the null guard)
- **ai**: Noted feed[].kind absent -> className 'fitem undefined' (icon falls to ∿) so a kind should always be sent
- **ai**: Clarified feed[].cand only diverts to /track when the clicked button is index 0; other indices fall through to dismiss
- **ai**: Corrected RUNNING badge color reference to var(--indigo-neon) (shell.jsx:120)
- **ai**: Added shell.jsx:100 (head Dot tone) to the online field consumers and clarified pulse-dot colors (--ok / --ink-3)
- **ai**: Clarified actions first-button styling: 'p' when cand truthy else 'coral'; reordered desc to match code
- **ai**: Noted Config engine/model fall back through summary/account sources before /ai/status (Config.jsx:176-177)
- **Settings & account**: Added missing endpoint PATCH /account (settings_account.rs:29, 'edit account holder/iban') — present in the route file but omitted by the draft; documented as low-confidence with no frontend consumer (no account-edit form exists, verified by grep).
- **Settings & account**: GET /account: trimmed responseJson to only the actually-read fields {engine, model}; removed the holder/iban placeholder example values from the response since no frontend code reads them.
- **Settings & account**: GET /account: marked holder/iban field descriptions explicitly SPECULATIVE (unread by frontend, zero grep matches) rather than just 'included for completeness'.
- **Settings & account**: Tightened consumer line citations to match the actual file (e.g. preferences merge gated at Config.jsx:152; reset button at Config.jsx:184/213; storedOnDevice static-string note pinned to Config.jsx:209).
- **Shell**: nav/pages: removed the fabricated `available:true` from every object in responseJson — the PHOSK_PAGES const (comps.jsx:12-18) has no `available` key; kept the field in fields[] but marked it speculative/unread per its already-low confidence
- **Shell**: nav/pages: corrected `href` desc — it is not just 'not used for routing', it is NEVER read at all by the React app (dead baggage), flagged speculative for the API
- **Shell**: config GET: confirmed via grep that literal /config is only a react-router route (main.jsx:56) + nav `to` (comps.jsx:18) and never an api call; tightened the purpose/notes to cite the exact /settings/preferences call sites (GET 150, PATCH 67, DELETE 73)
- **Shell**: config GET: enriched consumers[] and per-field descs to ground keys (aiOpen, drillMode, subView, etc.) in their real cross-page read sites via the shared phosk.cfg store (tweaks.jsx useTweaks; Transactions/Budgets/Subscriptions/Debts/Analytics), since the draft listed them as Config-only without proof they're read
- **Shell**: config GET: added exact line refs (Config.jsx:15-31 for CFG_DEFAULTS, 180-182 for inspector unification, 54-58 for merge layering) and fixed consumer line numbers
- **Shell**: PATCH /config: added consumer Config.jsx:251 (aiOpen set call) and noted the route module is MISSING a DELETE endpoint that the app's reset() actually calls (DELETE /settings/preferences, Config.jsx:73)
- **Shell**: purpose/notes: clarified the route file defines exactly GET /nav/pages and GET+PATCH /config (no DELETE), matching shell.rs exactly
- **Shell**: verified all field names/types/defaults are verbatim-correct against CFG_DEFAULTS and PHOSK_PAGES; verified ADR-008 compliance (key/to/preference-key are slugs/names, no UUIDs); confidence labels (low for never-fetched nav, medium for path-inferred config) are honest
- **exports**: transactions.csv: fixed queryParam name `horizon` -> `period` (the list sends period=HZ_PERIOD[horizon] under key `period`, Transactions.jsx:394; `horizon` is only local UI state)
- **exports**: Re-grounded every cited line number against the actual files and tightened field descriptions (e.g. t.date is a display string like '12 Jun', not a 2026-06-12 ISO date); confirmed all draft fields are genuinely read
- **exports**: Clarified budget.csv: c.budget IS read (as the cap via capOf at Budgets.jsx:387 and the sort at :416), correcting the draft's vaguer line refs
- **exports**: Verified central no-consumer finding by exhaustive grep (no /exports, .csv, Blob, createObjectURL, href, window.location, or api.base URL-building anywhere in frontend/app/src/); confidence labels (all low) confirmed honest

