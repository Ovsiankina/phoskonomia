> # ⚠️ OBSOLETE — superseded by `frontend/dioxus-app/`. See [OBSOLETE.md](OBSOLETE.md). Do not extend; scheduled for deletion (task T05).
> The text below is historical and partly wrong (the app was later wired to a REST API that never worked).

# Phoskonomia — Web Frontend (prototype)

A real, buildable React app ported from the static `claude-design-export/` mockup. It reproduces
the **Oscillocore** CRT/oscilloscope design system across all seven product surfaces. This is a
**frontend-only prototype**: all data is mock data (`src/data/phosk.js`). The HTTP API layer that the
Rust backend will serve is **not wired up yet** — that comes later.

## Stack

- **Vite + React 18 + react-router-dom v6** (plain JSX, no TypeScript).
- No CDN scripts and no in-browser Babel (the mockup loaded React/Babel from `unpkg` and transpiled
  `.jsx` at runtime). Everything is bundled and version-pinned. `npm audit` is clean.

## Run

```bash
npm install
npm run dev       # dev server at http://localhost:3717 (pinned; strictPort)
npm run build     # production build -> dist/
npm run preview   # serve the production build at http://localhost:4173
```

## Routes

`/dashboard` · `/transactions` · `/budgets` · `/subscriptions` · `/debts` · `/analytics` · `/config`
(`/` and unknown paths redirect to `/dashboard`).

## Structure

```
src/
  main.jsx              # entry: imports all CSS (ordered) + scanner + shared modules, mounts the router
  App.jsx              # layout route: the permanent CRT overlays (osc-scan/osc-roll/osc-vig) + <Outlet/>
  data/phosk.js        # the PHOSK mock-data object (merges the old data.jsx + signals-data.jsx)
  lib/
    osc-scanner.js     # the differential-growth canvas engine (sets window.OscScanner) — copied verbatim
    tweaks.jsx         # useTweaks + Tweak* controls + the phosk.cfg config store ('phoskcfg' event)
  components/
    prims.jsx          # ScannerBg, Dot, HudCell, Spark, CatBar, PhoskChart, SavingsDial, pctTone, barColor
    comps.jsx          # PHOSK_PAGES, TopBar (router-ified nav), Kpi, CatRows, TxnTape, RecRow, AlertItem
    shell.jsx          # AiPanel, SignalPanel, SignalStrip, SignalCard, SigSpark
  pages/               # one self-contained file per route (page + its components + page-local data merged in)
  styles/              # design tokens, CRT skin, per-page CSS, and fonts/ (Pilowlava + VG5000)
```

Each page file is self-contained: it renders its own `TopBar`, background `ScannerBg`, side panels and
content, mirroring how the export shipped one HTML file per page.

## Conversion notes (how the port was done)

- The mockup shared components via `window.*` globals with a strict `<script>` load order. The port
  converts these to **ES modules with named exports**, but each shared module **also re-registers its
  exports on `window`** as a runtime safety net, and `main.jsx` side-effect-imports the shared layer
  before the first render. So a stray `window.Foo` reference still resolves. Prefer explicit imports in
  new code; the `window.*` bridge is a compatibility shim, not the intended long-term style.
- Cross-page `<a href="Phoskonomia X.html">` links became `<Link to="/...">`; `ReactDOM.createPortal`
  became an imported `createPortal`; per-page `createRoot` calls were removed in favor of the central
  router mount.

## Design system

`claude-design-export/` (and its `_ds/oscillocore-design-system-*/` bundle) remains the **source of
truth** for the Oscillocore look. Key enforced rules carried over: numbers use the **Pilowlava** font,
body/UI uses **VG5000**, colors/spacing come from CSS tokens (`var(--...)`, no raw hex/px), depth is
**glow not shadow**, the 39px grid must not fade, and every page keeps the top bar + side panels.

## Not done yet (intentional)

- No backend/API integration (mock data only).
- Per-page side-panel state resets on route change (each page mounts its own shell) — acceptable for a
  prototype; a shared persistent shell can be hoisted into `App.jsx` later.
