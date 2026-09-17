# Oscillocore — Design System

> A CRT oscilloscope for your data. Molten neon signal on a blueprint grid,
> wrapped in a permanent scanline. This repository is the source of truth for
> the Oscillocore brand: type, color, the living scanner background, UI kits and
> slide templates.

---

## What Oscillocore is

**Oscillocore** is a creative-technical brand whose identity is a *signal scanner*:
a dark CRT field crossed by an indigo blueprint grid, with molten neon-red curves
that grow organically out of letterforms (differential growth seeded from glyph
outlines). The aesthetic sits at the intersection of **lab instrument** and
**1980s home-computer phosphor** — equal parts oscilloscope, demoscene, and
hacker-congress title card.

The visual language was reverse-engineered from the materials provided (see
*Sources* below). Two surfaces are represented in those materials:

1. **The Scanner / brand surface** — title cards, talk slides and motion pieces
   built on the differential-growth scanner. Reference stills show an
   "ILLEGAL INSTRUCTIONS" title card and a talk slide ("Reticulum: Unstoppable
   Networks for The People — markqvist — 38C3"). This is the marketing / motion /
   presentation face of the brand.
2. **Anomalies — the app surface** — a personal-finance / subscription "anomaly"
   tracker. The provided UI still shows an `ANOMALIES` ledger with a flagged item
   ("FITNESS ABO — usually charged on the 5th, but missing this month…") and two
   terse actions: `MARK PAID` / `SNOOZE`. The app reads the world as a signal and
   surfaces the *anomalies* — the spikes that don't fit the waveform.

Both surfaces share one engine: the scanner. Every page is a unique read-out.

> **Note on naming/abbreviation:** the brand favors abbreviations where they read
> naturally — `ABO` (Abonnement / subscription), `OSC`, `38C3`, `MARK PAID`.
> See *Content Fundamentals*.

---

## Sources (provided materials)

These are the inputs this system was built from. Stored here in case the reader
has access to the originals.

| Source | What it is |
|---|---|
| `pilowlava-growth(1).html` | **The canonical authority.** A working "Differential Growth Scanner" — the exact grid (39px), scanline, vignette, dish, HUD cells, rulers and molten curve. All foundations derive from this file. |
| `uploads/mpi2*-image.png` (×5) | Reference stills: title card, dense & clean scanner read-outs, and the heterogeneous multi-shape composition. Copied into `assets/brand-*.png`. |
| `WhatsApp Image …18.42.49.jpeg` | The **Anomalies** app UI still (`assets/ref-anomalies-ui.jpeg`). |
| `Pilowlava-*` (otf/woff/woff2) | Display typeface — *Pilowlava* by Velvetyne (OFL). In `fonts/`. |
| `vg5000-master.zip` | Body typeface — *VG5000* by Justin Bihan / Velvetyne (OFL). Extracted to `fonts/`. |

No code repository or Figma file was provided — the UI kits below are
high-fidelity reconstructions from the stills + the canonical scanner code.
**If a production codebase or Figma exists, attach it and these kits can be made
pixel-exact.**

---

## Index — what's in this folder

```
README.md                 ← you are here
SKILL.md                  ← Agent-Skills entry point (portable)
colors_and_type.css       ← all color + type tokens (CSS vars + semantic roles)

fonts/
  fonts.css               ← @font-face for Pilowlava + VG5000
  Pilowlava-Regular.*     ← display (woff2/woff/otf)
  Pilowlava-Atome.*       ← decorative molten cut (woff2/woff)
  VG5000-Regular.*        ← body (woff2/woff/otf)

assets/
  osc-scanner.js          ← the living differential-growth background (configurable)
  osc-crt.css             ← scanline + vignette + HUD chrome (always-on overlays)
  brand-*.png             ← brand scanner stills
  ref-anomalies-ui.jpeg   ← app reference

preview/                  ← Design System tab cards (type/color/spacing/components/brand)

ui_kits/
  anomalies/              ← the app surface (ledger / anomaly tracker)
  scanner/                ← the brand/site surface (hero, talk cards)

slides/                   ← talk / title-card slide templates (16:9)
```

---

## Content Fundamentals

How Oscillocore writes.

- **Voice:** terse, technical, a little conspiratorial. It speaks like an
  instrument reporting a reading, not a brand selling you something. Short
  declaratives. No exclamation marks, no hype.
- **Person:** mostly **impersonal / system voice** ("Subscription paused or
  forgotten?", "Anomaly detected"). When it addresses the user it's direct and
  dry ("you"), never chummy. Never "we're so excited".
- **Abbreviations are a feature.** Use them where they read naturally:
  `ABO` (subscription), `OSC`, `MARK PAID`, `SNOOZE`, `38C3`, `RX/TX`. Labels are
  clipped, not spelled out, as long as meaning survives.
- **Casing:** UI labels and tags are **UPPERCASE** and tracked out
  (`ANOMALIES`, `MARK PAID`, `FITNESS ABO`). Headlines in Pilowlava are
  mixed/title-case. Body copy in VG5000 is sentence case.
- **Numbers are heroes.** Amounts, dates, counts, frequencies get Pilowlava
  weight and neon. "charged on the **5th**", "€**38,00**".
- **No emoji.** Iconography is unicode glyphs / scanner marks (`⚠ ◷ ✓ → ↑ ↓ ⌁`),
  never emoji. Punctuation can do icon-work (`·`, `—`, `?`).
- **Tone examples (verbatim-style):**
  - Title card: `ILLEGAL INSTRUCTIONS`
  - Talk slide: `Reticulum: Unstoppable Networks for The People`
  - Anomaly: `Fitness Abo usually charged on the 5th, but missing this month.
    Subscription paused or forgotten?`
  - HUD: `「3」 nodes 4128 · 92% of target`
- **The question mark earns its place.** The app raises questions rather than
  asserting ("…paused or forgotten?"). It flags; the user decides.

---

## Visual Foundations

Everything below is encoded in `colors_and_type.css`, `assets/osc-scanner.js`
and `assets/osc-crt.css`.

### Color
- A **near-black violet void** (`#06040c`) is the page. There is no white page,
  ever. Panels lift only a hair (`#0b0718`, `#080514`).
- **One hero hue: neon coral** (`#ff5e4d` core, `#fc6a60` text), sampled directly
  from the reference stills — warm but **never** a hard orange or skin tone. The
  hard vermillion `#ff3b2e` returns only as the **deep** anchor (the most
  saturated bloom under the glow). The bright end may cool toward **pink**
  (`#ff5277`) / **magenta** (`#e0457f`, from the dish rim) — never warm toward
  orange. It's the *signal*: the growth curve, the primary action, the anomaly.
  **Used sparingly — one coral moment per view.**
- **Text is cool phosphor white** (`#eef1f7` → `#b7bcce` → `#7c8096`), with a
  faint indigo cast. It belongs to the blueprint — **zero warm/salmon tones**.
- **Two prominent text accents, deeper & saturated:** `--text-coral #f5564a`
  (headings, prominent red) and `--text-blue #8474de` (subheads, prominent blue).
  Headings are **never plain white** — they take coral or blue.
- **Indigo is structure, not emphasis** (`#3a2f7a` grid → `#5848bf` frame →
  `#6a5fc0` ticks). It builds the lab; it never shouts. The brighter `#9c93e0`
  is reserved for HUD labels (legibility). A **blue neon** (`#8f7dff`) is the
  cool counter-signal — wordmark accent + secondary scanner shapes.
- Semantic: **alert = the core coral**; amber `#ffb454` and a muted green
  `#5fd08a` appear rarely and low-saturation.

### Type
- **Two families, strict roles.** *Pilowlava* (molten, organic, Velvetyne) for
  **numbers, headers, tags, hero display only** — never body. *VG5000* (CRT
  pixel-grotesque) for **everything else**: UI, labels, data, paragraphs.
- Pilowlava is swap-only and wants size + air (≥ 40px). VG5000 sets tight,
  monospaced and even.
- HUD micro-labels: VG5000, 11px, UPPERCASE, `letter-spacing: .32em`, indigo,
  with a soft indigo glow.

### Backgrounds — *composed, intended, never duplicated*
This is the single most important rule. **Every surface runs a unique scanner
field — and the composition is deliberate, not decorative wallpaper.** The
background is generated, then *arranged*:
- A **39px indigo blueprint grid** (`rgba(58,47,122,.30)`) with a 2px frame
  (`rgba(86,72,191,.55)`) inset by 0.6 cells.
- The grid is **broken on purpose.** A flat 39px sheet edge-to-edge is the most
  common way this system is gotten *wrong*. Interrupt it: punch through with HUD
  **cells**, **rulers** (`.osc-ruler`), **readouts** (`.osc-readout`), ruled
  panels (`.osc-ruled`) and denser sub-grid blocks. The grid bends around content
  (see `pilowlava-growth` corner cells). Heterogeneity is the goal of the whole
  surface, not just the shapes.
- A **radial dish** glow behind the primary signal shape.
- **Differential-growth shapes** seeded from Pilowlava glyphs. Two independent
  axes, both of which must vary across a composition:
  - **Render style** — `solid` (opaque molten coral *mass*, **filled, no border**),
    `faint` (soft low-alpha indigo fill — deep background, **filled, no border**),
    `red` (molten glow *stroke* — the growth curve), `wire` (thin indigo neon
    *stroke* — blueprint). **Fill rule: a filled shape has NO border; an outline
    shape IS its border.** Never put a stroked rim on a filled mass.
  - **Morphology** (`morph:`) — the *silhouette*, set independently of style.
    Defaults favour **open, smooth** shapes; the dense maze is opt-in:
    `blob` (smooth rounded amoeba — default for **solid/faint**, fill these),
    `vein` (an **open winding line**, long lazy loops — default for **red/wire**),
    `mass` (dense packed body — legacy, only for a deliberately chunky fill),
    `coral` (the dense molten **maze**). **`coral` is opt-in and used once per
    view, very sparingly** — when several shapes are dense mazes the field reads
    "bacterial". The reference `mpi2t2ff` is a *smooth* solid coral mass + an
    *open* winding line + faint blue blobs, with no fingerprint maze at all.
- **Shapes live in negative space.** Place them where text is *not*. A growth
  shape behind body copy kills legibility — that is a composition bug, not a
  vibe. (`seed:` keeps each page stable on reload but distinct from its
  neighbours. Mix positions, scales, glyphs, styles, morphs and rotation.)
- **Parallax (motion surfaces):** set `parallax:true` so the shape layer drifts
  against the static grid + content on **scroll** for genuine depth (pass
  `{pointer:px}` to also react to the mouse — off by default). Use the
  transparent layer mode (`bg:false`) with a separate CSS grid below it, and give
  the canvas bleed (`.osc-shape-layer`: `inset:-6%`) so drift never shows an edge.

### Text over the field — *glass is the canonical fix*
When content **must** overlap the scanner, do not just hope it's legible and do
not flatten the background. Put the content on a **glass panel** — `.osc-glass`:
a frosted slab (`backdrop-filter: blur(var(--blur-glass))`, **9px** canonical)
over a translucent fill (`--glass-fill`, `rgba(8,5,20,.34)`) that blurs and
darkens whatever mass is behind it while the shape's colour still bleeds through
as a halo. Two ingredients do all the work: a **nearly-transparent fill** (an
opaque fill kills the effect) and the **blur**. Modifiers: `.dim` (≈14px blur +
darker fill) for body-copy regions, `.soft` (`--blur-glass-soft`, 6px) for large
docks, `.hair` for a hairline border, `.tint-coral`/`.tint-blue` to match the
shape behind, `.over-content` (heavier fill) for drawers/sheets that float over
content rather than just the field. Ship the `-webkit-` prefix. Ruled / readout /
cell panels also protect text; glass is the soft one.

**Text-legibility frost (`.osc-frost`).** A box that carries running text
*directly* over the scanner — transaction rows, filter controls, dashboard
readout boxes — gets the **same blur as the side panels** (`--blur-glass`, no
saturate, so they match exactly). The box keeps its own translucent fill; frost
only adds the blur. Rule of thumb: *running text over the canvas → frost; a bare
label or a glowing number → leave it.* Performance: `backdrop-filter` is cheap on
a few big panels, costly on dozens of small boxes — prefer frosting the
**container** (one `.osc-glass` for a region) over N children; the frost is
purely visual and drops under `prefers-reduced-motion` without affecting
function.

### The CRT skin — *always present*
- A **scanline veil** (`repeating-linear-gradient`, `mix-blend-mode: multiply`)
  over the entire viewport, `z-index` top, `pointer-events:none`.
- A **corner vignette** (radial, falloff to `rgba(2,1,8,.85)`).
- Optional slow flicker (disabled under `prefers-reduced-motion`).
- These two overlays (`.osc-scan`, `.osc-vig`) appear on **every** Oscillocore
  surface without exception.

### Motion & easing
- The hero scanner **simmers continuously** (differential growth never fully
  settles). Decorative shapes grow-then-freeze for cost.
- Rulers carry an **oscillating marker** (sine). HUD glyph cells flicker through
  random characters.
- UI transitions are **fast and mechanical** (~120ms), linear or ease-out. No
  bounce, no spring, no playful overshoot — this is instrumentation.
- Entrances: fade + 1–2px settle. Nothing slides far.

### Surfaces, borders, depth
- **Corners are sharp.** Frames and cells are `border-radius: 0`; inputs `2px`;
  buttons `3px`; pills only for status dots. Soft consumer rounding is off-brand.
- **Borders are 2px indigo** by default (the scanner stroke), or 1.5px neon for
  active/primary, hairlines (`rgba(106,95,192,.28)`) for dividers.
- **Depth = glow, not shadow.** Elevation reads as a neon bloom or an indigo
  halo radiating outward. Real drop-shadows appear only beneath floating panels
  (`0 8px 30px rgba(0,0,0,.6)`), and the side dock uses `-6px 0 24px`.
- **Glass:** floating docks and any text over the scanner use **`.osc-glass`**
  (`--glass-fill` `rgba(8,5,20,.34)` + `backdrop-filter: blur(var(--blur-glass))`,
  9px). It is the canonical fix for legibility over the field — see *Text over
  the field* above. Text-bearing boxes over the canvas frost with **`.osc-frost`**.

### Hover / press
- **Hover:** chips/buttons *lighten* their fill (`#150c33 → #241653`); primary
  *blooms* (glow grows). Links shift coral → brighter coral (never orange).
- **Press:** no shrink-to-zero gimmicks; a brief darken + glow snap. Mechanical.

### Imagery vibe
- All imagery is **the scanner itself** — warm neon on cold dark, heavy grain
  from the scanline, vignetted. Cool indigo structure, hot red subject. No
  photography, no gradients-for-decoration, no stock.

---

## Iconography

See `ICONOGRAPHY` section below.

## ICONOGRAPHY

- **No icon font, no emoji.** Oscillocore's "icons" are **unicode glyphs and
  scanner marks** set in VG5000 or Pilowlava: `⚠` (anomaly), `◷` (snoozed),
  `✓` (clear), `→ ↑ ↓ ↔` (flow/navigation), `·` `—` (separators), `⌁` `≈` `∿`
  (waveform marks), `▾ ▸` (disclosure). VG5000 ships a rich set of these (arrows,
  triangles, dingbats, astrological/CRT marks) — prefer them so icons share the
  text metrics and phosphor glow.
- **Structural "icons" are drawn by the system, not placed:** HUD corner cells
  (a framed Pilowlava glyph), oscillating rulers, the dish, the grid. These are
  generated by `osc-crt.css` / `osc-scanner.js`, never hand-authored SVG.
- **Numerals double as iconography.** A single molten Pilowlava digit in a corner
  cell is the most recognizable brand mark — see the `3 / 8 / e / 3` corners in
  the reference stills.
- **Color rule for marks:** alert/active marks take neon red + glow; inert marks
  take indigo or `ink-3`. A mark is never larger than its line's cap-height + glow.
- If a richer pictographic icon is unavoidable in a future surface, substitute a
  **thin-stroke, square-cut** set (e.g. Lucide at 1.5px) tinted to `--ink` /
  `--neon` — and **flag the substitution**. None was needed for the current kits.

---

## Using the system

```html
<link rel="stylesheet" href="fonts/fonts.css">
<link rel="stylesheet" href="colors_and_type.css">
<link rel="stylesheet" href="assets/osc-crt.css">

<!-- static grid below, drifting shapes above, content on glass -->
<div class="osc-gridbg osc-framed"></div>
<canvas id="bg" class="osc-shape-layer"></canvas>
<div class="osc-scan"></div>
<div class="osc-vig"></div>

<header class="osc-glass tint-coral" style="position:absolute;left:46px;top:44px;padding:26px 30px">
  <h1>Reticulum: Unstoppable Networks for The People</h1>
</header>

<script src="assets/osc-scanner.js"></script>
<script>
  OscScanner.mount(document.getElementById('bg'), {
    bg:false, dish:true, parallax:true, seed: 7 /* unique per page! */,
    shapes:[
      { char:'8', cx:.60,cy:.60, scale:.66, r:.42, style:'faint', morph:'blob', fill:.7 },  // deep blob
      { char:'3', cx:.83,cy:.34, scale:.40, r:.32, style:'solid', morph:'blob', fill:.85 }, // filled mass, no border
      { char:'e', cx:.40,cy:.85, scale:.26, r:.20, style:'wire',  morph:'vein', fill:.6 },  // open wire line
      { char:'2', cx:.60,cy:.52, scale:.22, r:.32, style:'red',   morph:'vein', live:true, fill:.6 }, // open molten curve
    ],
  });
</script>
```

Pick a **different `seed`, shape arrangement AND morph mix for every page** —
heterogeneity is the rule. Keep shapes in negative space; put any overlapping
text on `.osc-glass`.

---

## App Dashboard — the canonical app frame

The full Oscillocore app layout (`ui_kits/dashboard/`), reconstructed from the
Phoskonomia build. Reach for it whenever you need a data-dense product screen
rather than a poster or a single panel. Anatomy:

- **One fixed scanner field** — a single `canvas.pk-bg` at `inset:0, z:0`.
  Everything floats over it; never give regions their own competing fields.
  Park the dish + drifting shapes in **negative space** (right of the console),
  away from text.
- **`.app-shell`** — a `height:100vh` flex row holding three columns:
  - **Left · Assistant** (`.ai-panel`, 340px, collapsible to 50px) on
    `.osc-glass` — the live auto-maintenance feed + the local-LLM chat.
  - **Center · `.app-main`** — a sticky 62px `.phosk-top` topbar over the
    **single scroll container** (`.app-scroll`, the only `overflow-y:auto`).
  - **Right · Signal inspector** (`.sig-panel`, 344px) on `.osc-glass`; hides
    under 1320px.
- **Console hero** (`.pk-hero`, `grid: 322px 1fr`, min-height a full viewport):
  a left **dock** (big remaining figure, savings dial, snapshot, next-due) and
  the **main console** — a `.c-screen` spend-trace scope, a 5-up `.c-channels`
  sparkline strip, and a `.c-watch` ticker.
- **Signal strip** of `.osc-readout` boxes, a `.pk-seam` divider, then the
  **terminal grid** (`.pk-terminal`, `grid: 256px 1fr 320px`): readout/ rates/
  shops rail, a category-budget matrix + transaction list, and an
  attention/recurring/insight rail.

Surfaces follow the legibility rules above: side panels are `.osc-glass`,
text-bearing boxes over the field carry `.osc-frost`, and the readout boxes use
`.osc-readout` with corner brackets + a Pilowlava legend. The CRT skin
(`.osc-scan` + `.osc-vig`) sits on top of the whole shell.

---

## Caveats / open questions

- **No production code or Figma** was provided. UI kits are faithful
  reconstructions from stills + the canonical scanner; attach real source to make
  them exact.
- **Fonts are the genuine Velvetyne files** you supplied — no substitution was
  needed. (`osc-scanner.js` also lazy-loads a webfont mirror of Pilowlava purely
  so the canvas can read glyph outlines if the page font isn't ready.)
- The brand name "Oscillocore" and the two-surface split (Scanner / Anomalies)
  are inferred from the materials — confirm and correct.
