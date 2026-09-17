# Oscillocore — Canonical additions (from Phoskonomia)

Three patterns proven in the Phoskonomia app, written up so they can be folded
into the Oscillocore design system as first-class, canonical entries. Each entry
follows the same shape: **What it is → Recipe → Tokens → Markup → Rules →
Integration steps.**

1. [Frosted glass — side panels & over-scanner surfaces (`osc-glass`)](#1-frosted-glass--osc-glass)
2. [Text-legibility glass — frost behind text boxes](#2-text-legibility-glass)
3. [Dashboard layout — Console-hero + Terminal-grid](#3-dashboard-layout)

A shared note up front, because all three depend on it:

> **The scanner field is busy on purpose.** A near-black violet void crossed by
> a 39px indigo blueprint grid, with molten growth shapes simmering on top. The
> single hardest rule in the system is *text must never sit raw on a growth
> shape.* The first two entries are the canonical fixes; the third is the layout
> that arranges everything above the field.

---

## 1. Frosted glass — `osc-glass`

### What it is
The official surface for any panel that floats over the live scanner: the left
**Assistant** panel, the right **Signal** inspector, drawers, sheets, the
dashboard dock. A frosted slab that **blurs + darkens** whatever growth shape is
behind it, so phosphor text stays legible while the shape's colour still bleeds
through as a faint halo.

### Recipe
```css
.osc-glass {
  position: relative; z-index: 1;
  background: rgba(8,5,20,.34);                 /* translucent — MUST show through */
  -webkit-backdrop-filter: blur(9px) saturate(116%);
  backdrop-filter: blur(9px) saturate(116%);
  border: 2px solid rgba(86,72,191,.48);        /* blueprint indigo */
  border-radius: 0;                             /* corners are always sharp */
  box-shadow: inset 0 1px 0 rgba(152,152,205,.10), 0 8px 30px rgba(0,0,0,.42);
}
```
Two ingredients do all the work: **(a)** a nearly-transparent fill (if it were
opaque the blur would be invisible), and **(b)** `backdrop-filter: blur(9px)`.
That `9px` is the canonical radius — it's exactly what both side panels use.

### Modifiers
| Class | Effect | Use when |
|---|---|---|
| `.tint-coral` | warm coral edge + halo | a **red / solid** shape sits behind |
| `.tint-blue`  | cool indigo edge + halo | a **wire / faint** shape sits behind |
| `.dim`        | heavier blur (≈14px) + darker fill | dense **body-copy** regions |
| `.hair`       | hairline border instead of the 2px frame | the panel is large / edge-to-edge |

### Tokens (add to `colors_and_type.css`)
```css
:root {
  --blur-glass: 9px;        /* side panels, over-scanner content (canonical) */
  --blur-glass-soft: 6px;   /* large docks, where less frost reads cleaner   */
  --glass-fill: rgba(8,5,20,.34);
}
.osc-glass { backdrop-filter: blur(var(--blur-glass)) saturate(116%); }
```
(The system already declares a `--blur-glass` token — point the component at it.)

### Markup
```html
<aside class="sig-panel osc-glass hair tint-blue"> … inspector … </aside>
<div   class="osc-glass dim"> … body copy over the field … </div>
```

### Rules
- **Always pair the blur with a translucent fill.** An opaque fill wastes the
  compositing pass and kills the effect.
- **Match the tint to the shape behind it** (coral vs blue) so the halo reads
  intentional, not accidental.
- **Sharp corners only** (`border-radius: 0`) — soft consumer rounding is
  off-brand.
- Ship the `-webkit-` prefix.

### Integration steps
1. Replace the hand-written `background` + `backdrop-filter` on `.ai-panel` /
   `.sig-panel` with `class="… osc-glass hair"`; keep only panel-specific layout
   (width, which side the border is on, scroll).
2. Tokenise the radius (`--blur-glass`) so it's tunable system-wide.
3. From here on, **every** floating-over-scanner surface uses `osc-glass`
   (+ a tint) rather than re-deriving a blur: dock, drawer, sheet, toast, modal.

### Performance
`backdrop-filter` is GPU-cheap for a handful of large panels but **expensive on
dozens of small boxes**. Keep it on big surfaces; for many small text boxes see
the scoped guidance in §2.

---

## 2. Text-legibility glass

### What it is
The same frost as `osc-glass`, applied to the **content boxes that carry text
directly over the scanner** — the transaction rows and filter controls, and the
dashboard readout boxes (KPIs, stat boxes, alert/recurring items, the spend
scope, channel strip, signal band). Tested as an opt-in experiment, it clearly
won on legibility, so it is now **permanent and canonical**: a text box over the
field gets the side-panel blur.

### Recipe
```css
/* the SAME blur(9px) as the side panels — no saturate, to match them exactly */
.trow,
.txn-sel select,
.txn-search {
  -webkit-backdrop-filter: blur(9px);
  backdrop-filter: blur(9px);
}
```
These boxes already carry a translucent fill (e.g. `.trow` is `rgba(10,6,20,.4)`),
which is what lets the frost read. No fill change is needed — only the blur.

### Rule of thumb
> If a box contains running text **and** sits above the scanner canvas, frost it
> at `var(--blur-glass)`. If it's a bare label or a number with its own glow, it
> doesn't need frosting.

### Performance guardrail
Frosting many small boxes is the one place `backdrop-filter` can get costly.
Mitigations, in order of preference:
1. Frost the **container** (one glass panel) instead of N children, when layout
   allows — this is `osc-glass` doing the job for a whole region.
2. When rows must each frost (a long scrolling list, like here), keep the radius
   modest (`9px`), avoid stacking `saturate()`, and don't also animate the box.
3. Under `prefers-reduced-motion` / low-power, the frost is purely visual and may
   be dropped without affecting function.

### Integration steps
1. Promote the rule into the system as a documented pattern: "text-bearing boxes
   over the scanner are frosted at `--blur-glass`."
2. Where a region is a single scrollable list of text rows, prefer wrapping it in
   one `osc-glass` and dropping per-row blur — same look, far cheaper.

---

## 3. Dashboard layout — Console-hero + Terminal-grid

### What it is
The canonical Oscillocore **app dashboard**: a full-height instrument console on
the first screen (the "hero"), which scrolls to reveal a dense terminal-style
data grid below. Two persistent side panels flank everything — an **Assistant**
on the left, a **Signal inspector** on the right. One fixed scanner field sits
behind the whole thing.

### Frame (outermost → in)
```
.pk                         position-context, min-height:100vh, isolate
├─ canvas.pk-bg             FIXED, inset:0, z:0     ← the one live scanner
├─ .app-shell               flex row, height:100vh
│  ├─ aside.ai-panel        LEFT  · 340px (collapsed 50px) · osc-glass
│  ├─ .app-main             flex column, min-width:0
│  │  ├─ .phosk-top         sticky topbar, 62px tall
│  │  └─ .app-scroll        the ONLY scroll container (overflow-y:auto)
│  │     ├─ section.pk-hero.dash-c        ← CONSOLE HERO  (min-height: 100vh − 62)
│  │     ├─ .sig-strip                     ← item-signals band
│  │     ├─ .pk-seam                        ← "DETAIL · TERMINAL ▾" divider
│  │     └─ section.pk-terminal.dash-b     ← TERMINAL GRID
│  └─ aside.sig-panel        RIGHT · 344px · osc-glass (docked variant)
```

### Console hero — `.pk-hero` (grid: `322px 1fr`)
- **Dock** (`.c-dock.glass`, 322px): the headline readout (REMAINING), a savings
  dial, a SNAPSHOT stat list, and a bottom-pinned NEXT-charge readout box. Frosted
  at `--blur-glass-soft` (6px) — a large surface reads cleaner with less blur.
- **Main** (`.c-main`, rows `1fr auto auto`):
  - `.c-screen` — the oscilloscope "screen": a bordered scope holding the
    spend-trace chart with a corner HUD.
  - `.c-channels` — a 5-column strip of per-category gauges (spark + %).
  - `.c-watch` — a single-line ticker of anomalies.

### Terminal grid — `.pk-terminal` / `.pk-term-grid` (grid: `256px 1fr 320px`)
- **Left rail** — the numeric **readout boxes** (BUDGET / SPENT / REMAINING),
  plus RATES and TOP SHOPS. See *Readout-box conventions* below.
- **Mid** — list panels: category-budget matrix, recent transactions.
- **Right rail** (`background: rgba(8,5,18,.34)`) — alerts ("needs attention"),
  recurring, and the GEMMA4 insight box.

### Readout-box conventions (used throughout)
- **Corner brackets + legend are the signature.** Every framed readout box takes
  `osc-bkt` (thick top-left + bottom-right L-brackets) and an `osc-leg` Pilowlava
  legend cut into the top border (`.coral` / `.blue` accent). This is the
  everyday treatment.
- **Edge ruler ticks are RARE.** The graph-paper ruler (`osc-ticks`) is an
  occasional instrument accent (e.g. the OCR scan readout) — **never** uniform
  across boxes, never the default. (It is pseudo-element based, so it cannot
  coexist with `osc-bkt` on the same element.)
- **Sharp corners, glow-not-shadow, one coral moment per view** — per the core
  system.

### Responsive
- `≤1180px`: hero collapses to `280px 1fr`; terminal grid to `220px 1fr` with the
  right rail dropping full-width below.
- `<1240–1280px` (JS): the docked Signal inspector becomes a drawer/sheet instead
  of stealing a fixed column.

### Persistent side panels
- **Left — Assistant** (`.ai-panel`, `osc-glass`): collapsible to a 50px rail; a
  live auto-maintenance feed + chat thread.
- **Right — Signal inspector** (`.sig-panel`, `osc-glass`): docked at ≥1240px,
  else a right **drawer** or bottom **sheet** (both `osc-glass`, heavier fill
  `rgba(8,5,20,.97)` since they float over content, not just the field).

### Integration steps
1. Lift `.pk` / `.app-shell` / `.pk-hero` / `.pk-terminal` into the system as a
   named layout template ("App Dashboard").
2. Bind the two side panels to `osc-glass` (§1) and the over-scanner content to
   the text-legibility pattern (§2).
3. Keep the **single fixed scanner** + **single scroll container** invariants —
   they're what make the hero→terminal scroll and the letterboxed field work.
