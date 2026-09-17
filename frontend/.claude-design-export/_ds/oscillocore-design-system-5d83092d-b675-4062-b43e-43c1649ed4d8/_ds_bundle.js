/* @ds-bundle: {"format":3,"namespace":"OscillocoreDesignSystem_5d8309","components":[],"sourceHashes":{"assets/osc-scanner.js":"6b5af0aee628","ui_kits/anomalies/App.jsx":"7febbab02ad3","ui_kits/anomalies/components.jsx":"184b7e11794f","ui_kits/anomalies/data.jsx":"5311adf982aa","ui_kits/anomalies/screens.jsx":"ec9f0a24519b","ui_kits/dashboard/console.js":"677990b967c1","ui_kits/scanner/App.jsx":"c1fffa2068ca","ui_kits/scanner/components.jsx":"167fdbcf799c","ui_kits/scanner/screens.jsx":"16996e494780"},"inlinedExternals":[],"unexposedExports":[]} */

(() => {

const __ds_ns = (window.OscillocoreDesignSystem_5d8309 = window.OscillocoreDesignSystem_5d8309 || {});

const __ds_scope = {};

(__ds_ns.__errors = __ds_ns.__errors || []);

// assets/osc-scanner.js
try { (() => {
/* ============================================================================
   OSCILLOCORE · osc-scanner.js
   ----------------------------------------------------------------------------
   The brand's living background. A CRT oscilloscope that grows organic curves
   out of Pilowlava glyph outlines via differential growth.

   Ported from the canonical PILOWLAVA · GROWTH SCANNER. Generalised so every
   page gets a UNIQUE, heterogeneous background: seed it with a different glyph,
   different shape positions/scales, and a mix of render styles (molten-red
   stroke, indigo wireframe, faint indigo fill). Never duplicate page to page.

   USAGE
   -----
     <canvas id="bg"></canvas>
     <script src="osc-scanner.js"></script>
     <script>
       OscScanner.mount(document.getElementById('bg'), {
         grid: true, dish: true,                  // scanner chrome
         parallax: true,                          // shape layer drifts on SCROLL
         shapes: [
           { char:'3', cx:.50, cy:.52, scale:.42, style:'solid', morph:'blob', fill:.30 },
           { char:'8', cx:.18, cy:.30, scale:.22, style:'wire',  morph:'vein', fill:.50 },
           { char:'e', cx:.82, cy:.70, scale:.30, style:'red',   morph:'coral', live:true, fill:.66 },
         ],
         seed: 7,                                 // deterministic per page
       });
     </script>

   RENDER STYLES (how a shape is painted). RULE: a FILLED shape has NO border;
   an OUTLINE shape IS its border.
            'solid'= opaque coral MASS, FILLED, no stroke (the big signal)
            'faint'= soft filled indigo blob, very low alpha, FILLED, no stroke
            'red'  = molten neon-red glow STROKE (the differential-growth curve)
            'wire' = thin indigo neon outline STROKE (blueprint)

   MORPHOLOGIES (the SILHOUETTE — set independently of style via `morph`).
   Defaults favour OPEN/smooth shapes; the dense maze is opt-in:
            'blob' = smooth rounded amoeba (default for solid/faint) — fill these
            'vein' = open winding line (default for red/wire) — long lazy loops
            'mass' = dense packed fill (legacy) — only if you want a chunky body
            'coral'= dense molten MAZE — opt-in, USE ONCE per view, very sparingly

   PARALLAX: set parallax:true (scroll-driven by default; pass {scroll, ease} to
   tune, or {pointer:px} to also react to the mouse). Give the canvas bleed
   (inset:-6%; width:112%; height:112%) so drift never exposes an empty edge.
   BG LAYER: set bg:false to render a TRANSPARENT shapes-only layer (no void fill,
   no grid) you can stack over a separate CSS grid — useful for parallax depth.

   Set live:false to grow-then-freeze (cheap). live:true keeps simmering.
   ============================================================================ */
(function (global) {
  "use strict";

  const FONT_SOURCES = ["https://db.onlinewebfonts.com/t/051b066e5f93c7f3e525181ffa11ec5d.woff2", "https://db.onlinewebfonts.com/t/051b066e5f93c7f3e525181ffa11ec5d.woff"];
  let fontReady = null;
  function ensureFont() {
    if (fontReady) return fontReady;
    // Prefer a Pilowlava already installed on the page (via @font-face).
    fontReady = (async () => {
      try {
        if (document.fonts) {
          // Wait briefly for the page's local @font-face to finish — but never
          // block forever on it.
          await Promise.race([document.fonts.ready, new Promise(r => setTimeout(r, 1200))]);
          if (document.fonts.check('400 40px "Pilowlava"')) return true;
        }
      } catch (e) {}
      for (const u of FONT_SOURCES) {
        try {
          const ff = new FontFace("Pilowlava", `url("${u}")`);
          // Cap the network wait so a hung/blocked fetch can't stall the canvas.
          await Promise.race([ff.load(), new Promise((_, rej) => setTimeout(() => rej(new Error("font timeout")), 1500))]);
          document.fonts.add(ff);
          return true;
        } catch (e) {}
      }
      return false;
    })();
    return fontReady;
  }

  // ---- small seeded RNG so each page is deterministic but distinct ----
  function mulberry32(a) {
    return function () {
      a |= 0;
      a = a + 0x6D2B79F5 | 0;
      let t = Math.imul(a ^ a >>> 15, 1 | a);
      t = t + Math.imul(t ^ t >>> 7, 61 | t) ^ t;
      return ((t ^ t >>> 14) >>> 0) / 4294967296;
    };
  }

  /* ===================== GLYPH OUTLINE (marching boundary) ================ */
  const oc = document.createElement("canvas");
  const octx = oc.getContext("2d", {
    willReadFrequently: true
  });
  function glyphOutline(chr) {
    const S = 400,
      pad = 46;
    oc.width = S;
    oc.height = S;
    octx.fillStyle = "#000";
    octx.fillRect(0, 0, S, S);
    octx.fillStyle = "#fff";
    octx.textAlign = "center";
    octx.textBaseline = "middle";
    let size = S - pad * 2;
    octx.font = `400 ${size}px "Pilowlava","Arial Black",system-ui,sans-serif`;
    let w = octx.measureText(chr).width;
    if (w > S - pad * 2) {
      size *= (S - pad * 2) / w;
      octx.font = `400 ${size}px "Pilowlava","Arial Black",system-ui,sans-serif`;
    }
    octx.fillText(chr, S / 2, S / 2 + size * 0.04);
    const d = octx.getImageData(0, 0, S, S).data,
      bin = new Uint8Array(S * S);
    for (let i = 0, p = 0; i < S * S; i++, p += 4) bin[i] = d[p] > 120 ? 1 : 0;
    const label = new Int32Array(S * S);
    let best = 0,
      bestSz = 0,
      cur = 0;
    const st = [];
    for (let i = 0; i < S * S; i++) {
      if (bin[i] && !label[i]) {
        cur++;
        let sz = 0;
        st.length = 0;
        st.push(i);
        label[i] = cur;
        while (st.length) {
          const q = st.pop();
          sz++;
          const x = q % S,
            y = q / S | 0;
          if (x > 0 && bin[q - 1] && !label[q - 1]) {
            label[q - 1] = cur;
            st.push(q - 1);
          }
          if (x < S - 1 && bin[q + 1] && !label[q + 1]) {
            label[q + 1] = cur;
            st.push(q + 1);
          }
          if (y > 0 && bin[q - S] && !label[q - S]) {
            label[q - S] = cur;
            st.push(q - S);
          }
          if (y < S - 1 && bin[q + S] && !label[q + S]) {
            label[q + S] = cur;
            st.push(q + S);
          }
        }
        if (sz > bestSz) {
          bestSz = sz;
          best = cur;
        }
      }
    }
    if (!best) return null;
    const mask = new Uint8Array(S * S);
    for (let i = 0; i < S * S; i++) mask[i] = label[i] === best ? 1 : 0;
    const off = [[0, -1], [1, -1], [1, 0], [1, 1], [0, 1], [-1, 1], [-1, 0], [-1, -1]];
    const fg = (x, y) => x >= 0 && y >= 0 && x < S && y < S && mask[y * S + x] === 1;
    let sx = -1,
      sy = -1;
    for (let i = 0; i < S * S && sx < 0; i++) {
      if (mask[i]) {
        sx = i % S;
        sy = i / S | 0;
      }
    }
    const idxOf = (dx, dy) => {
      for (let k = 0; k < 8; k++) if (off[k][0] === dx && off[k][1] === dy) return k;
      return 0;
    };
    const pts = [];
    let px = sx,
      py = sy,
      bxx = sx - 1,
      byy = sy,
      guard = 0,
      max = S * S * 4;
    pts.push([px, py]);
    while (guard++ < max) {
      const dd = idxOf(bxx - px, byy - py);
      let found = -1;
      for (let k = 1; k <= 8; k++) {
        const di = (dd + k) % 8,
          nx = px + off[di][0],
          ny = py + off[di][1];
        if (fg(nx, ny)) {
          found = di;
          break;
        }
      }
      if (found < 0) break;
      const pbi = (found - 1 + 8) % 8;
      bxx = px + off[pbi][0];
      byy = py + off[pbi][1];
      px += off[found][0];
      py += off[found][1];
      if (px === sx && py === sy && pts.length > 4) break;
      pts.push([px, py]);
    }
    if (pts.length < 8) return null;
    let mnx = 1e9,
      mny = 1e9,
      mxx = -1e9,
      mxy = -1e9;
    for (const [a, b] of pts) {
      if (a < mnx) mnx = a;
      if (b < mny) mny = b;
      if (a > mxx) mxx = a;
      if (b > mxy) mxy = b;
    }
    const span = Math.max(mxx - mnx, mxy - mny) || 1,
      ox = (mnx + mxx) / 2,
      oy = (mny + mxy) / 2;
    return pts.map(([a, b]) => [(a - ox) / span, (b - oy) / span]);
  }
  function resample(norm, n) {
    const Q = norm.slice();
    Q.push(norm[0]);
    let len = 0;
    const cum = [0];
    for (let i = 1; i < Q.length; i++) {
      const dx = Q[i][0] - Q[i - 1][0],
        dy = Q[i][1] - Q[i - 1][1];
      len += Math.hypot(dx, dy);
      cum.push(len);
    }
    const out = [];
    let j = 0;
    for (let i = 0; i < n; i++) {
      const t = len * i / n;
      while (j < cum.length - 2 && cum[j + 1] < t) j++;
      const seg = cum[j + 1] - cum[j] || 1,
        f = (t - cum[j]) / seg;
      out.push([Q[j][0] + (Q[j + 1][0] - Q[j][0]) * f, Q[j][1] + (Q[j + 1][1] - Q[j][1]) * f]);
    }
    return out;
  }

  /* ===================== ONE GROWING SHAPE ================================= */
  const CAP = 9000;

  /* ---- Global growth throttle: only a few scanners simulate at once, so a
     page full of cards doesn't thunder on load. Grown shapes freeze and free
     their slot for the next one. This is the main defence against page lag. ---- */
  let ACTIVE_GROW = 0;
  const MAX_GROW = 2;
  const GROW_Q = [];
  function grantGrow() {
    while (ACTIVE_GROW < MAX_GROW && GROW_Q.length) {
      const s = GROW_Q.shift();
      s._queued = false;
      if (s.alive) {
        s._growing = true;
        ACTIVE_GROW++;
        s.start();
      }
    }
  }
  function releaseGrow(s) {
    if (s._growing) {
      s._growing = false;
      ACTIVE_GROW = Math.max(0, ACTIVE_GROW - 1);
      grantGrow();
    }
  }
  function enqueueGrow(s) {
    if (s._queued || s._growing) return;
    s._queued = true;
    s._grownDone = false;
    GROW_Q.push(s);
    grantGrow();
  }

  /* ---- GROWTH MORPHOLOGIES --------------------------------------------------
     The single most important heterogeneity lever. Differential growth can read
     as anything from a SMOOTH AMOEBA to a DENSE CORAL MAZE depending on how hard
     the perimeter is forced to fold inside its boundary. We expose three named
     profiles so a composition mixes silhouettes instead of repeating one texture:
        blob  — smooth, rounded mass. Few gentle lobes, no convolution. The big
               background masses + every FILLED shape should be a blob. (high
               align + spring, near-zero noise, low node target → can't fold).
       vein  — a meandering, looping line. Reads like circuitry / a single long
               tube. Best as a thin WIRE stroke.
       coral — the dense molten maze (the canonical scanner texture). Striking,
               but USE IT ONCE per view — it is what makes everything look
               "bacterial" when overused. Reserve it for a single signal stroke.
      `foldK` scales the node target: lower = fewer nodes = the boundary stays near
     its circumference = smooth. High noise + low align + high foldK = maze. ---- */
  const MORPHS = {
    blob: {
      mode: "perim",
      desired: 7.0,
      maxEdge: 13,
      minEdge: 4.0,
      sep: 18,
      spring: 0.28,
      align: 0.20,
      repulsion: 1.05,
      noise: 0.050,
      foldK: 1.30
    },
    vein: {
      mode: "perim",
      desired: 7.4,
      maxEdge: 14,
      minEdge: 4.2,
      sep: 20,
      spring: 0.28,
      align: 0.20,
      repulsion: 1.05,
      noise: 0.110,
      foldK: 2.10
    },
    mass: {
      mode: "area",
      desired: 6.8,
      maxEdge: 12,
      minEdge: 3.4,
      sep: 14,
      spring: 0.26,
      align: 0.15,
      repulsion: 1.12,
      noise: 0.065,
      foldK: 1.00
    },
    coral: {
      mode: "area",
      desired: 6.8,
      maxEdge: 12,
      minEdge: 3.4,
      sep: 17,
      spring: 0.26,
      align: 0.13,
      repulsion: 1.15,
      noise: 0.290,
      foldK: 1.00
    }
  };
  // Sensible default morphology per render style (override with spec.morph).
  // Defaults favour OPEN/smooth silhouettes; the dense maze ('coral') is opt-in.
  const STYLE_MORPH = {
    red: "vein",
    solid: "blob",
    faint: "blob",
    wire: "vein"
  };
  function Shape(spec, rng) {
    this.spec = spec;
    this.char = spec.char || "3";
    this.style = spec.style || "red"; // red | solid | wire | faint
    this.live = !!spec.live;
    this.fillFrac = spec.fill != null ? spec.fill : 0.6;
    this.ax = new Float32Array(CAP);
    this.ay = new Float32Array(CAP);
    this.bx = new Float32Array(CAP);
    this.by = new Float32Array(CAP);
    this.fx = new Float32Array(CAP);
    this.fy = new Float32Array(CAP);
    this.N = 0;
    this.frame = 0;
    this.noiseSeed = rng() * 1000;
    this.maxNodes = 1500;
    this.settled = false;
    this.warm = 0;
    this.grown = false;
    // morphology — decoupled from render style so silhouettes vary per shape
    this.morphName = spec.morph || STYLE_MORPH[this.style] || "coral";
    const m = MORPHS[this.morphName] || MORPHS.coral;
    // mild per-shape jitter so two shapes of the same morph still differ
    const j = (v, amt) => v * (1 - amt + rng() * amt * 2);
    this.foldK = m.foldK;
    this.mode = m.mode || "area";
    this.P = {
      desired: m.desired,
      maxEdge: m.maxEdge,
      minEdge: m.minEdge,
      sep: j(m.sep, 0.12),
      spring: m.spring,
      align: m.align,
      repulsion: m.repulsion,
      maxForce: 2.6,
      noise: j(m.noise, 0.20),
      stepsPerFrame: 3
    };
    this.rot = spec.rot != null ? spec.rot : rng() * Math.PI * 2; // glyph rotation → no two seeds align
    // Lower flow-noise frequency on smooth/open morphs → long lazy undulations
    // instead of high-frequency wrinkling (the "bacterial" texture).
    this.nScale = (this.mode === "perim" ? 0.007 : 0.016) + rng() * 0.006;
    this.nSpeed = 0.004;
    this.seedScale = spec.scale || 0.4;
    this.lwCore = spec.lw || (this.style === "red" ? 2.4 : this.style === "solid" ? 1.5 : 1.6);
    this.glow = this.style === "red" ? 15 : 8;
    this.grid = new Map();
    this.cell = 17;
  }
  Shape.prototype.center = function (W, H) {
    this.cx = (this.spec.cx != null ? this.spec.cx : 0.5) * W;
    this.cy = (this.spec.cy != null ? this.spec.cy : 0.5) * H;
    this.R = Math.min(W, H) * (this.spec.r != null ? this.spec.r : 0.4);
    // Two node-target models. 'perim' scales with CIRCUMFERENCE → a gentle
    // rounded loop (smooth blob / open winding vein) that never folds into a
    // maze, regardless of how big R is. 'area' packs the disk → the dense maze.
    const perim = 2 * Math.PI * this.R / this.P.desired;
    const area = Math.PI * this.R * this.R / (this.P.sep * this.P.desired);
    const base = this.mode === "perim" ? perim : area;
    this.maxNodes = Math.max(48, Math.min(1400, Math.round(base * this.fillFrac * this.foldK)));
  };
  Shape.prototype.seed = function () {
    const norm = glyphOutline(this.char) || glyphOutline("o");
    if (!norm) return;
    const s = resample(norm, 150),
      scale = this.R * this.seedScale;
    const cos = Math.cos(this.rot),
      sin = Math.sin(this.rot);
    this.N = s.length;
    for (let i = 0; i < this.N; i++) {
      const px = s[i][0] * scale,
        py = s[i][1] * scale; // rotate the seed glyph
      this.ax[i] = this.cx + px * cos - py * sin;
      this.ay[i] = this.cy + px * sin + py * cos;
    }
    this.frame = 0;
    this.settled = false;
    this.warm = 0;
    this.grown = false;
  };
  Shape.prototype.flow = function (x, y, t) {
    const s = this.nScale;
    const a = Math.sin(x * s + t) + Math.cos(y * (s * 1.11) - t * 0.7);
    const b = Math.cos(x * (s * 0.89) - t * 0.6) + Math.sin(y * (s * 1.22) + t * 0.9);
    const ang = (a + b) * 1.6;
    return [Math.cos(ang), Math.sin(ang)];
  };
  Shape.prototype.step = function () {
    const P = this.P,
      N = this.N;
    if (N < 3) return;
    const sep = Math.max(2, P.sep),
      sep2 = sep * sep,
      t = this.frame * this.nSpeed + this.noiseSeed;
    const ax = this.ax,
      ay = this.ay,
      bx = this.bx,
      by = this.by,
      fx = this.fx,
      fy = this.fy;
    this.grid.clear();
    this.cell = Math.max(2, P.sep);
    const key = (gx, gy) => gx * 100003 + gy,
      cell = this.cell,
      grid = this.grid;
    for (let i = 0; i < N; i++) {
      const gx = Math.floor(ax[i] / cell),
        gy = Math.floor(ay[i] / cell),
        k = key(gx, gy);
      let a = grid.get(k);
      if (!a) {
        a = [];
        grid.set(k, a);
      }
      a.push(i);
    }
    let sflx = 0,
      sfly = 0;
    for (let i = 0; i < N; i++) {
      const pi = (i - 1 + N) % N,
        ni = (i + 1) % N;
      let Fx = 0,
        Fy = 0,
        x = ax[i],
        y = ay[i];
      let dx = ax[pi] - x,
        dy = ay[pi] - y,
        dd = Math.hypot(dx, dy) || 1;
      Fx += dx * P.spring * (dd - P.desired) / dd;
      Fy += dy * P.spring * (dd - P.desired) / dd;
      dx = ax[ni] - x;
      dy = ay[ni] - y;
      dd = Math.hypot(dx, dy) || 1;
      Fx += dx * P.spring * (dd - P.desired) / dd;
      Fy += dy * P.spring * (dd - P.desired) / dd;
      Fx += ((ax[pi] + ax[ni]) * 0.5 - x) * P.align;
      Fy += ((ay[pi] + ay[ni]) * 0.5 - y) * P.align;
      const gx = Math.floor(x / cell),
        gy = Math.floor(y / cell);
      for (let ox = -1; ox <= 1; ox++) for (let oy = -1; oy <= 1; oy++) {
        const arr = grid.get(key(gx + ox, gy + oy));
        if (!arr) continue;
        for (let q = 0; q < arr.length; q++) {
          const j = arr[q];
          if (j === i || j === pi || j === ni) continue;
          const ddx = x - ax[j],
            ddy = y - ay[j],
            r2 = ddx * ddx + ddy * ddy;
          if (r2 < sep2 && r2 > 0.0001) {
            const r = Math.sqrt(r2),
              f = (1 - r / sep) * P.repulsion;
            Fx += ddx / r * f;
            Fy += ddy / r * f;
          }
        }
      }
      const fl = this.flow(x, y, t),
        nx = fl[0] * P.noise,
        ny = fl[1] * P.noise;
      Fx += nx;
      Fy += ny;
      sflx += nx;
      sfly += ny;
      const m = Math.hypot(Fx, Fy);
      if (m > P.maxForce) {
        Fx = Fx / m * P.maxForce;
        Fy = Fy / m * P.maxForce;
      }
      fx[i] = Fx;
      fy[i] = Fy;
    }
    const mflx = sflx / N,
      mfly = sfly / N;
    for (let i = 0; i < N; i++) {
      ax[i] += fx[i] - mflx;
      ay[i] += fy[i] - mfly;
    }
    for (let i = 0; i < N; i++) {
      const dx = ax[i] - this.cx,
        dy = ay[i] - this.cy,
        r = Math.hypot(dx, dy),
        lim = this.R - 2;
      if (r > lim) {
        const k = lim / r;
        ax[i] = this.cx + dx * k;
        ay[i] = this.cy + dy * k;
      }
    }
    const target = this.maxNodes,
      diff = target - N;
    let inj = null,
      drop = null;
    if (diff > 0) {
      inj = new Set();
      const bud = Math.max(1, Math.min(diff, Math.round(diff * 0.04) + 1));
      for (let k = 0; k < bud; k++) inj.add(Math.random() * N | 0);
    } else if (diff < -3) {
      drop = new Set();
      const bud = Math.min(-diff, Math.round(-diff * 0.05) + 1);
      for (let k = 0; k < bud; k++) drop.add(Math.random() * N | 0);
    }
    let M = 0;
    const me2 = P.minEdge * P.minEdge,
      xe2 = P.maxEdge * P.maxEdge;
    for (let i = 0; i < N; i++) {
      if (drop && drop.has(i) && M > 0 && N - i > 3) continue;
      if (M > 0 && P.minEdge > 0) {
        const dx = ax[i] - bx[M - 1],
          dy = ay[i] - by[M - 1];
        if (dx * dx + dy * dy < me2 && N - i > 3) continue;
      }
      bx[M] = ax[i];
      by[M] = ay[i];
      M++;
      const ni = (i + 1) % N,
        dx = ax[ni] - ax[i],
        dy = ay[ni] - ay[i],
        el2 = dx * dx + dy * dy;
      if (M < target - 1 && (el2 > xe2 || inj && inj.has(i))) {
        bx[M] = ax[i] + dx * 0.5;
        by[M] = ay[i] + dy * 0.5;
        M++;
      }
    }
    this.ax = bx;
    this.bx = ax;
    this.ay = by;
    this.by = ay;
    this.N = M;
    this.frame++;
    if (this.N >= this.maxNodes * 0.97) this.warm++;
    if (this.warm > 14) {
      this.grown = true;
      if (!this.live) this.settled = true;
    }
  };
  Shape.prototype.buildPath = function () {
    const ax = this.ax,
      ay = this.ay,
      N = this.N,
      p = new Path2D();
    if (N < 3) return p;
    p.moveTo((ax[N - 1] + ax[0]) * 0.5, (ay[N - 1] + ay[0]) * 0.5);
    for (let i = 0; i < N; i++) {
      const ni = (i + 1) % N;
      p.quadraticCurveTo(ax[i], ay[i], (ax[i] + ax[ni]) * 0.5, (ay[i] + ay[ni]) * 0.5);
    }
    p.closePath();
    return p;
  };
  Shape.prototype.draw = function (ctx) {
    const path = this.buildPath();
    ctx.save();
    ctx.lineJoin = "round";
    ctx.lineCap = "round";
    if (this.style === "solid") {
      // A SOLID molten MASS. With the smooth 'blob' morph the path is a clean
      // rounded loop, so we simply FILL it (a coral radial body + outer bloom).
      // RULE: a filled shape has NO border — no rim stroke.
      ctx.shadowColor = "rgba(255,60,40,.8)";
      ctx.shadowBlur = 40;
      ctx.fillStyle = "rgba(255,72,55,.20)";
      ctx.fill(path); // outer bloom
      ctx.shadowBlur = 0;
      const g = ctx.createRadialGradient(this.cx, this.cy, this.R * 0.05, this.cx, this.cy, this.R * 0.98);
      g.addColorStop(0, "rgba(255,98,80,.96)");
      g.addColorStop(.7, "rgba(247,70,56,.94)");
      g.addColorStop(1, "rgba(212,42,52,.92)");
      ctx.fillStyle = g;
      ctx.fill(path); // opaque coral body, no stroke
    } else if (this.style === "faint") {
      // filled deep-background blob — RULE: filled ⇒ no border.
      ctx.globalCompositeOperation = "lighter";
      ctx.fillStyle = "rgba(80,66,180,.12)";
      ctx.fill(path);
    } else if (this.style === "wire") {
      ctx.globalCompositeOperation = "lighter";
      ctx.shadowColor = "rgba(143,125,255,.8)";
      ctx.shadowBlur = this.glow;
      ctx.strokeStyle = "rgba(143,125,255,.7)";
      ctx.lineWidth = this.lwCore;
      ctx.stroke(path);
      ctx.shadowBlur = this.glow * 0.4;
      ctx.strokeStyle = "rgba(190,180,255,.85)";
      ctx.lineWidth = this.lwCore * 0.55;
      ctx.stroke(path);
    } else {
      // red — the signal
      const dense = this.N > 4500,
        blur = dense ? Math.min(this.glow, 10) : this.glow;
      ctx.globalCompositeOperation = "lighter";
      ctx.strokeStyle = "rgba(150,40,120,.045)";
      ctx.lineWidth = this.lwCore * 7.7;
      ctx.stroke(path);
      ctx.strokeStyle = "rgba(180,50,110,.05)";
      ctx.lineWidth = this.lwCore * 4.2;
      ctx.stroke(path);
      ctx.shadowColor = "rgba(255,50,30,.9)";
      ctx.shadowBlur = blur;
      ctx.strokeStyle = "rgba(255,70,48,.85)";
      ctx.lineWidth = this.lwCore;
      ctx.stroke(path);
      ctx.shadowBlur = blur * 0.4;
      ctx.strokeStyle = "rgba(255,120,80,.95)";
      ctx.lineWidth = this.lwCore * 0.6;
      ctx.stroke(path);
      if (!dense) {
        ctx.shadowBlur = 0;
        ctx.strokeStyle = "rgba(255,228,208,.85)";
        ctx.lineWidth = this.lwCore * 0.38;
        ctx.stroke(path);
      }
    }
    ctx.restore();
  };

  /* ===================== THE SCANNER (compositor) ========================= */
  function Scanner(canvas, opts) {
    this.opts = Object.assign({
      grid: true,
      dish: true,
      bg: true,
      parallax: false,
      seed: 1,
      shapes: []
    }, opts);
    this.cv = canvas;
    this.ctx = canvas.getContext("2d", {
      alpha: this.opts.bg === false
    });
    this.rng = mulberry32((this.opts.seed | 0) * 2654435761 >>> 0);
    this.shapes = (this.opts.shapes.length ? this.opts.shapes : [{
      char: "3",
      cx: .5,
      cy: .5,
      scale: .42,
      style: "red",
      live: true,
      fill: .66
    }]).map(s => new Shape(s, this.rng));
    this.W = 0;
    this.H = 0;
    this.DPR = 1;
    this.raf = 0;
    this.alive = true;
    this.visible = true;
    this.docHidden = false;
    this.tickCount = 0;
    this.last = 0;
    const ro = () => this.resize();
    this._onResize = () => {
      clearTimeout(this._rt);
      this._rt = setTimeout(ro, 200);
    };
    addEventListener("resize", this._onResize);
    // Pause when the document is hidden (background tab)
    this._onVis = () => {
      this.docHidden = document.hidden;
      if (!this.docHidden) this.start();else this.stop();
    };
    document.addEventListener("visibilitychange", this._onVis);
    this._initParallax();
    // Paint as soon as fonts resolve — but NEVER let a hung font fetch delay the
    // first frame past ~1.3s; fall back to the metric-compatible Arial Black
    // outline so the field is never blank.
    let firstResize = false;
    const kick = () => {
      if (firstResize || !this.alive) return;
      firstResize = true;
      this.resize();
    };
    ensureFont().then(kick);
    setTimeout(kick, 1300);
  }
  /* ---- Parallax: the SHAPE LAYER drifts against the grid/content for depth.
     opts.parallax = true | { scroll:factor, pointer:px, ease:0..1 }. By DEFAULT
     the layer drifts on SCROLL only (pointer:0); set pointer>0 to also react to
     the mouse. The canvas should bleed past the viewport (e.g. inset:-6%;
     width:112%) so the drift never exposes an empty edge. Disabled under
     prefers-reduced-motion. ---- */
  Scanner.prototype._initParallax = function () {
    if (!this.opts.parallax) return;
    try {
      if (matchMedia("(prefers-reduced-motion: reduce)").matches) return;
    } catch (e) {}
    const cfg = typeof this.opts.parallax === "object" ? this.opts.parallax : {};
    // Scroll-driven by default; pointer is opt-in (pointer:0 → no mouse drift).
    this.par = {
      pointer: cfg.pointer != null ? cfg.pointer : 0,
      scroll: cfg.scroll != null ? cfg.scroll : 0.12,
      ease: cfg.ease != null ? cfg.ease : 0.10
    };
    this.par.tx = 0;
    this.par.ty = 0;
    this.par.cx = 0;
    this.par.cy = 0;
    this.par.mx = 0;
    this.par.my = 0;
    this.cv.style.willChange = "transform";
    if (this.par.pointer > 0) {
      this._onPointer = e => {
        this.par.mx = (e.clientX / innerWidth - 0.5) * 2;
        this.par.my = (e.clientY / innerHeight - 0.5) * 2;
      };
      addEventListener("pointermove", this._onPointer, {
        passive: true
      });
    }
    const loop = () => {
      if (!this.alive) return;
      const p = this.par;
      p.tx = -p.mx * p.pointer;
      p.ty = -p.my * p.pointer - (scrollY || 0) * p.scroll;
      p.cx += (p.tx - p.cx) * p.ease;
      p.cy += (p.ty - p.cy) * p.ease;
      this.cv.style.transform = `translate3d(${p.cx.toFixed(2)}px, ${p.cy.toFixed(2)}px, 0)`;
      this._parRaf = requestAnimationFrame(loop);
    };
    this._parRaf = requestAnimationFrame(loop);
  };
  Scanner.prototype.resize = function () {
    this.DPR = Math.min(1.5, global.devicePixelRatio || 1);
    const r = this.cv.getBoundingClientRect();
    this.W = Math.max(1, r.width || this.cv.clientWidth || innerWidth);
    this.H = Math.max(1, r.height || this.cv.clientHeight || innerHeight);
    this.cv.width = this.W * this.DPR;
    this.cv.height = this.H * this.DPR;
    this.ctx.setTransform(this.DPR, 0, 0, this.DPR, 0, 0);
    for (const sh of this.shapes) {
      sh.center(this.W, this.H);
      sh.seed();
    }
    enqueueGrow(this);
  };
  Scanner.prototype.drawGrid = function () {
    const ctx = this.ctx,
      W = this.W,
      H = this.H,
      g = 39;
    if (this.opts.bg === false) {
      ctx.clearRect(0, 0, W, H);
      return;
    } // transparent shape layer
    ctx.fillStyle = "#06040c";
    ctx.fillRect(0, 0, W, H);
    if (!this.opts.grid) return;
    ctx.lineWidth = 1;
    ctx.strokeStyle = "rgba(58,47,122,.30)";
    ctx.beginPath();
    for (let x = 0; x <= W; x += g) {
      ctx.moveTo(x + 0.5, 0);
      ctx.lineTo(x + 0.5, H);
    }
    for (let y = 0; y <= H; y += g) {
      ctx.moveTo(0, y + 0.5);
      ctx.lineTo(W, y + 0.5);
    }
    ctx.stroke();
    ctx.strokeStyle = "rgba(86,72,191,.55)";
    ctx.lineWidth = 2;
    ctx.strokeRect(g * 0.6, g * 0.6, W - g * 1.2, H - g * 1.2);
  };
  Scanner.prototype.drawDish = function (sh) {
    if (!this.opts.dish || sh.style !== "red") return;
    const ctx = this.ctx,
      cx = sh.cx,
      cy = sh.cy,
      R = sh.R;
    const grd = ctx.createRadialGradient(cx, cy, R * 0.1, cx, cy, R * 1.05);
    grd.addColorStop(0, "rgba(60,10,40,.55)");
    grd.addColorStop(.55, "rgba(34,8,28,.40)");
    grd.addColorStop(1, "rgba(8,4,16,0)");
    ctx.fillStyle = grd;
    ctx.beginPath();
    ctx.arc(cx, cy, R * 1.05, 0, 7);
    ctx.fill();
    ctx.strokeStyle = "rgba(150,40,90,.35)";
    ctx.lineWidth = 2;
    ctx.beginPath();
    ctx.arc(cx, cy, R * 1.02, 0, 7);
    ctx.stroke();
  };
  Scanner.prototype.render = function () {
    this.drawGrid();
    const order = {
      faint: 0,
      solid: 1,
      wire: 2,
      red: 3
    };
    const ordered = this.shapes.slice().sort((a, b) => (order[a.style] || 0) - (order[b.style] || 0));
    for (const sh of ordered) {
      if (sh.style === "red") this.drawDish(sh);
      sh.draw(this.ctx);
    }
  };
  // Returns true if any shape advanced this frame (i.e. a redraw is warranted).
  Scanner.prototype.frameStep = function () {
    let moved = false;
    for (const sh of this.shapes) {
      if (!sh.grown) {
        for (let s = 0; s < sh.P.stepsPerFrame; s++) sh.step();
        moved = true;
      } else if (sh.live && this.tickCount < 900 && this.tickCount % 5 === 0) {
        sh.step();
        moved = true;
      } // bounded slow simmer (~30s)
    }
    return moved;
  };
  Scanner.prototype.allStatic = function () {
    return this.shapes.every(sh => sh.grown) && (this.tickCount >= 900 || this.shapes.every(sh => !sh.live));
  };
  Scanner.prototype.start = function () {
    if (this.raf || !this.alive || !this.visible || this.docHidden) return;
    if (!this._growing && !this._grownDone) return; // wait for a growth slot
    this.raf = requestAnimationFrame(t => this.tick(t));
  };
  Scanner.prototype.stop = function () {
    if (this.raf) {
      cancelAnimationFrame(this.raf);
      this.raf = 0;
    }
  };
  Scanner.prototype.tick = function (ts) {
    this.raf = 0;
    if (!this.alive || !this.visible || this.docHidden) return;
    // throttle to ~30fps
    if (ts - this.last < 30) {
      this.raf = requestAnimationFrame(t => this.tick(t));
      return;
    }
    this.last = ts;
    this.tickCount++;
    const moved = this.frameStep();
    if (moved) this.render();
    // Growth finished → free the global grow slot so the next card can start.
    if (!this._grownDone && this.shapes.every(sh => sh.grown)) {
      this._grownDone = true;
      releaseGrow(this);
    }
    // Once everything has grown and nothing simmers, the final frame is already
    // drawn — stop the loop entirely (canvas stays static, zero cost).
    if (!moved && this.allStatic()) return;
    this.raf = requestAnimationFrame(t => this.tick(t));
  };
  Scanner.prototype.destroy = function () {
    this.alive = false;
    this.stop();
    releaseGrow(this);
    if (this._parRaf) cancelAnimationFrame(this._parRaf);
    if (this._onPointer) removeEventListener("pointermove", this._onPointer);
    removeEventListener("resize", this._onResize);
    document.removeEventListener("visibilitychange", this._onVis);
  };
  global.OscScanner = {
    mount(canvas, opts) {
      return new Scanner(canvas, opts);
    },
    ensureFont
  };
})(window);
})(); } catch (e) { __ds_ns.__errors.push({ path: "assets/osc-scanner.js", error: String((e && e.message) || e) }); }

// ui_kits/anomalies/App.jsx
try { (() => {
/* Anomalies — app state machine. */
const {
  useState: useStateA
} = React;
function App() {
  const [view, setView] = useStateA("unlock"); // unlock | ledger | detail
  const [active, setActive] = useStateA(null);
  if (view === "unlock") return /*#__PURE__*/React.createElement(UnlockScreen, {
    onUnlock: () => setView("ledger")
  });
  if (view === "detail" && active) return /*#__PURE__*/React.createElement(AnomalyDetail, {
    item: active,
    onBack: () => setView("ledger"),
    onResolve: () => setView("ledger")
  });
  return /*#__PURE__*/React.createElement(Ledger, {
    onOpen: a => {
      setActive(a);
      setView("detail");
    }
  });
}
ReactDOM.createRoot(document.getElementById("root")).render(/*#__PURE__*/React.createElement(App, null));
})(); } catch (e) { __ds_ns.__errors.push({ path: "ui_kits/anomalies/App.jsx", error: String((e && e.message) || e) }); }

// ui_kits/anomalies/components.jsx
try { (() => {
/* Anomalies — shared primitives. Exported to window for cross-file use. */
const {
  useRef,
  useEffect,
  useState
} = React;

/* ---- ScannerBg: mounts the differential-growth scanner on a canvas ---- */
function ScannerBg({
  seed = 1,
  shapes
}) {
  const ref = useRef(null);
  useEffect(() => {
    let inst;
    const tryMount = () => {
      if (window.OscScanner && ref.current) {
        inst = window.OscScanner.mount(ref.current, {
          grid: true,
          dish: true,
          seed,
          shapes: shapes || [{
            char: "3",
            cx: .72,
            cy: .5,
            scale: .4,
            style: "red",
            live: true,
            fill: .58
          }]
        });
      } else {
        setTimeout(tryMount, 60);
      }
    };
    tryMount();
    return () => inst && inst.destroy && inst.destroy();
  }, []);
  return /*#__PURE__*/React.createElement("canvas", {
    ref: ref,
    className: "osc-bg"
  });
}

/* ---- Button ---- */
function Button({
  variant = "default",
  children,
  onClick,
  style
}) {
  return /*#__PURE__*/React.createElement("button", {
    className: "osc-btn " + variant,
    onClick: onClick,
    style: style
  }, children);
}

/* ---- Tag (uppercase Pilowlava) ---- */
function Tag({
  children,
  tone = "neon",
  style
}) {
  return /*#__PURE__*/React.createElement("span", {
    className: "osc-tagchip " + tone,
    style: style
  }, children);
}

/* ---- SectionHeader: wide indigo HUD label with a hairline rule ---- */
function SectionHeader({
  children,
  count
}) {
  return /*#__PURE__*/React.createElement("div", {
    className: "osc-sechead"
  }, /*#__PURE__*/React.createElement("span", {
    className: "lbl"
  }, children), count != null && /*#__PURE__*/React.createElement("span", {
    className: "ct"
  }, count), /*#__PURE__*/React.createElement("span", {
    className: "rule"
  }));
}

/* ---- HudCell: framed Pilowlava glyph ---- */
function HudCell({
  glyph,
  size = 64
}) {
  return /*#__PURE__*/React.createElement("div", {
    className: "osc-cell",
    style: {
      width: size,
      height: size
    }
  }, /*#__PURE__*/React.createElement("b", {
    style: {
      fontSize: size * .66
    }
  }, glyph));
}

/* ---- StatusDot ---- */
function StatusDot({
  tone
}) {
  const c = tone === "alert" ? "var(--neon)" : tone === "warn" ? "var(--warn)" : "var(--ok)";
  return /*#__PURE__*/React.createElement("span", {
    style: {
      width: 8,
      height: 8,
      borderRadius: 999,
      background: c,
      boxShadow: `0 0 8px ${c}`,
      flex: "0 0 auto"
    }
  });
}

/* ---- Waveform: a mini oscilloscope of the spending signal ---- */
function Waveform({
  data,
  height = 64,
  mark
}) {
  const w = 100,
    mid = height / 2;
  const pts = data.map((v, i) => {
    const x = i / (data.length - 1) * w;
    const y = mid - (v - .5) * height * 1.5;
    return `${x.toFixed(2)},${y.toFixed(2)}`;
  }).join(" ");
  return /*#__PURE__*/React.createElement("svg", {
    className: "osc-wave",
    viewBox: `0 0 ${w} ${height}`,
    preserveAspectRatio: "none"
  }, /*#__PURE__*/React.createElement("line", {
    x1: "0",
    y1: mid,
    x2: w,
    y2: mid,
    stroke: "rgba(106,95,192,.3)",
    strokeWidth: ".4"
  }), /*#__PURE__*/React.createElement("polyline", {
    points: pts,
    fill: "none",
    stroke: "var(--neon)",
    strokeWidth: "1",
    strokeLinejoin: "round",
    style: {
      filter: "drop-shadow(0 0 2px var(--neon))"
    }
  }), mark != null && (() => {
    const x = mark / (data.length - 1) * w;
    const v = data[mark];
    const y = mid - (v - .5) * height * 1.5;
    return /*#__PURE__*/React.createElement("g", null, /*#__PURE__*/React.createElement("line", {
      x1: x,
      y1: "0",
      x2: x,
      y2: height,
      stroke: "rgba(255,59,46,.35)",
      strokeWidth: ".5"
    }), /*#__PURE__*/React.createElement("circle", {
      cx: x,
      cy: y,
      r: "1.8",
      fill: "var(--neon-white)",
      style: {
        filter: "drop-shadow(0 0 3px var(--neon))"
      }
    }));
  })());
}
Object.assign(window, {
  ScannerBg,
  Button,
  Tag,
  SectionHeader,
  HudCell,
  StatusDot,
  Waveform
});
})(); } catch (e) { __ds_ns.__errors.push({ path: "ui_kits/anomalies/components.jsx", error: String((e && e.message) || e) }); }

// ui_kits/anomalies/data.jsx
try { (() => {
/* Anomalies — fake ledger data. German-flavored subscription tracker (ABO = Abonnement). */
window.OSC_DATA = {
  account: {
    holder: "M. QVIST",
    iban: "DE89 ⋯ 4042",
    balance: "2.418,55",
    delta: "−312,90",
    // 28-day spending "waveform" — normalised 0..1 amplitudes
    wave: [.42, .5, .38, .61, .44, .3, .52, .7, .48, .55, .4, .33, .62, .81, .5, .44, .58, .39, .47, .66, .93, .5, .41, .55, .6, .38, .49, .72]
  },
  anomalies: [{
    id: "a1",
    tag: "FITNESS ABO",
    amount: "38,00",
    cycle: "Monthly · 5th",
    kind: "missing",
    note: "Fitness Abo usually charged on the 5th, but missing this month. Subscription paused or forgotten?"
  }, {
    id: "a2",
    tag: "STREAM+ ABO",
    amount: "17,99",
    cycle: "Monthly · 12th",
    kind: "spike",
    note: "Charged €17,99 — €5,00 above the usual €12,99. Price change, or a second profile added?"
  }, {
    id: "a3",
    tag: "CLOUD STORE",
    amount: "9,99",
    cycle: "Monthly · 1st",
    kind: "double",
    note: "Two identical charges on the 1st. Looks like a duplicate — dispute one?"
  }],
  recurring: [{
    id: "r1",
    tag: "MIETE",
    amount: "1.180,00",
    cycle: "Monthly · 1st",
    status: "ok"
  }, {
    id: "r2",
    tag: "STROM ABO",
    amount: "64,00",
    cycle: "Monthly · 3rd",
    status: "ok"
  }, {
    id: "r3",
    tag: "BAHNCARD",
    amount: "59,90",
    cycle: "Yearly · Mar",
    status: "ok"
  }, {
    id: "r4",
    tag: "NEWS ABO",
    amount: "12,00",
    cycle: "Monthly · 8th",
    status: "ok"
  }, {
    id: "r5",
    tag: "VPN ABO",
    amount: "3,33",
    cycle: "Yearly · Sep",
    status: "ok"
  }]
};
})(); } catch (e) { __ds_ns.__errors.push({ path: "ui_kits/anomalies/data.jsx", error: String((e && e.message) || e) }); }

// ui_kits/anomalies/screens.jsx
try { (() => {
/* Anomalies — screens: Unlock, Ledger, Detail. */
const {
  useState: useStateS
} = React;

/* ============================ UNLOCK ============================ */
function UnlockScreen({
  onUnlock
}) {
  const [pin, setPin] = useStateS("");
  const press = d => {
    if (d === "⌫") return setPin(pin.slice(0, -1));
    const next = (pin + d).slice(0, 4);
    setPin(next);
    if (next.length === 4) setTimeout(onUnlock, 260);
  };
  return /*#__PURE__*/React.createElement("div", {
    className: "scr unlock"
  }, /*#__PURE__*/React.createElement(ScannerBg, {
    seed: 3,
    shapes: [{
      char: "8",
      cx: .5,
      cy: .42,
      scale: .34,
      style: "red",
      live: true,
      fill: .5
    }, {
      char: "3",
      cx: .2,
      cy: .75,
      scale: .2,
      style: "wire",
      live: false,
      fill: .3
    }]
  }), /*#__PURE__*/React.createElement("div", {
    className: "unlock-inner"
  }, /*#__PURE__*/React.createElement("div", {
    className: "osc-hud",
    style: {
      marginBottom: 10
    }
  }, "OSCILLOCORE \xB7 ANOMALIES"), /*#__PURE__*/React.createElement("div", {
    className: "wordmark"
  }, "Oscillo", /*#__PURE__*/React.createElement("span", null, "core")), /*#__PURE__*/React.createElement("div", {
    className: "osc-p",
    style: {
      margin: "8px 0 26px",
      color: "var(--ink-3)",
      fontSize: 12
    }
  }, "Enter access code to read the signal."), /*#__PURE__*/React.createElement("div", {
    className: "pin-dots"
  }, [0, 1, 2, 3].map(i => /*#__PURE__*/React.createElement("span", {
    key: i,
    className: "pd" + (pin.length > i ? " on" : "")
  }))), /*#__PURE__*/React.createElement("div", {
    className: "keypad"
  }, ["1", "2", "3", "4", "5", "6", "7", "8", "9", "", "0", "⌫"].map((k, i) => k === "" ? /*#__PURE__*/React.createElement("span", {
    key: i
  }) : /*#__PURE__*/React.createElement("button", {
    key: i,
    className: "key",
    onClick: () => press(k)
  }, k)))));
}

/* ============================ LEDGER ============================ */
function Ledger({
  onOpen
}) {
  const d = window.OSC_DATA;
  const [anoms, setAnoms] = useStateS(d.anomalies);
  const resolve = id => setAnoms(anoms.filter(a => a.id !== id));
  return /*#__PURE__*/React.createElement("div", {
    className: "scr ledger"
  }, /*#__PURE__*/React.createElement(ScannerBg, {
    seed: 7,
    shapes: [{
      char: "3",
      cx: .82,
      cy: .26,
      scale: .26,
      style: "faint",
      live: false,
      fill: .5
    }, {
      char: "e",
      cx: .12,
      cy: .82,
      scale: .22,
      style: "wire",
      live: false,
      fill: .3
    }]
  }), /*#__PURE__*/React.createElement("header", {
    className: "topbar"
  }, /*#__PURE__*/React.createElement("div", {
    className: "brand"
  }, /*#__PURE__*/React.createElement("span", {
    className: "osc-hud"
  }, "OSC"), /*#__PURE__*/React.createElement("b", null, "Anomalies")), /*#__PURE__*/React.createElement("nav", {
    className: "tabs"
  }, /*#__PURE__*/React.createElement("span", {
    className: "tab on"
  }, "Ledger"), /*#__PURE__*/React.createElement("span", {
    className: "tab"
  }, "Signal"), /*#__PURE__*/React.createElement("span", {
    className: "tab"
  }, "Rules")), /*#__PURE__*/React.createElement("div", {
    className: "acct"
  }, /*#__PURE__*/React.createElement("span", {
    className: "osc-hud"
  }, d.account.holder), /*#__PURE__*/React.createElement(HudCell, {
    glyph: "3",
    size: 40
  }))), /*#__PURE__*/React.createElement("div", {
    className: "ledger-body"
  }, /*#__PURE__*/React.createElement("section", {
    className: "balance"
  }, /*#__PURE__*/React.createElement("div", {
    className: "bal-left"
  }, /*#__PURE__*/React.createElement("div", {
    className: "osc-hud"
  }, "Balance \xB7 28-day signal"), /*#__PURE__*/React.createElement("div", {
    className: "bal-amt"
  }, "\u20AC", d.account.balance), /*#__PURE__*/React.createElement("div", {
    className: "bal-delta"
  }, d.account.delta, " ", /*#__PURE__*/React.createElement("span", null, "this cycle"))), /*#__PURE__*/React.createElement("div", {
    className: "bal-wave"
  }, /*#__PURE__*/React.createElement(Waveform, {
    data: d.account.wave,
    height: 70,
    mark: 20
  }), /*#__PURE__*/React.createElement("div", {
    className: "wave-cap"
  }, /*#__PURE__*/React.createElement("span", null, "day 1"), /*#__PURE__*/React.createElement("span", {
    className: "osc-tagchip neon",
    style: {
      fontSize: 9
    }
  }, "SPIKE \xB7 day 21"), /*#__PURE__*/React.createElement("span", null, "day 28")))), /*#__PURE__*/React.createElement(SectionHeader, {
    count: anoms.length
  }, "Anomalies"), /*#__PURE__*/React.createElement("div", {
    className: "anom-list"
  }, anoms.length === 0 && /*#__PURE__*/React.createElement("div", {
    className: "empty"
  }, "\u2301 No anomalies. The signal is clean."), anoms.map(a => /*#__PURE__*/React.createElement("article", {
    key: a.id,
    className: "anom",
    onClick: () => onOpen(a)
  }, /*#__PURE__*/React.createElement("span", {
    className: "warn"
  }, "\u26A0"), /*#__PURE__*/React.createElement("div", {
    className: "anom-main"
  }, /*#__PURE__*/React.createElement("div", {
    className: "anom-head"
  }, /*#__PURE__*/React.createElement("b", null, a.tag), /*#__PURE__*/React.createElement("span", {
    className: "amt"
  }, "\u20AC", a.amount)), /*#__PURE__*/React.createElement("div", {
    className: "anom-note"
  }, a.note), /*#__PURE__*/React.createElement("div", {
    className: "anom-meta"
  }, a.cycle, " \xB7 ", /*#__PURE__*/React.createElement("span", {
    className: "kind"
  }, a.kind))), /*#__PURE__*/React.createElement("div", {
    className: "anom-actions",
    onClick: e => e.stopPropagation()
  }, /*#__PURE__*/React.createElement(Button, {
    variant: "primary",
    onClick: () => resolve(a.id)
  }, "Mark Paid"), /*#__PURE__*/React.createElement(Button, {
    onClick: () => resolve(a.id)
  }, "Snooze"))))), /*#__PURE__*/React.createElement(SectionHeader, {
    count: d.recurring.length
  }, "Recurring \xB7 clean"), /*#__PURE__*/React.createElement("div", {
    className: "rec-grid"
  }, d.recurring.map(r => /*#__PURE__*/React.createElement("div", {
    key: r.id,
    className: "rec"
  }, /*#__PURE__*/React.createElement(StatusDot, {
    tone: "ok"
  }), /*#__PURE__*/React.createElement("div", {
    className: "rec-body"
  }, /*#__PURE__*/React.createElement("b", null, r.tag), /*#__PURE__*/React.createElement("span", null, r.cycle)), /*#__PURE__*/React.createElement("span", {
    className: "rec-amt"
  }, "\u20AC", r.amount))))));
}

/* ============================ DETAIL ============================ */
function AnomalyDetail({
  item,
  onBack,
  onResolve
}) {
  return /*#__PURE__*/React.createElement("div", {
    className: "scr detail"
  }, /*#__PURE__*/React.createElement(ScannerBg, {
    seed: item.id.charCodeAt(1) * 5,
    shapes: [{
      char: item.tag[0],
      cx: .7,
      cy: .5,
      scale: .4,
      style: "red",
      live: true,
      fill: .55
    }, {
      char: "8",
      cx: .16,
      cy: .3,
      scale: .2,
      style: "wire",
      live: false,
      fill: .3
    }]
  }), /*#__PURE__*/React.createElement("div", {
    className: "detail-inner"
  }, /*#__PURE__*/React.createElement("button", {
    className: "back",
    onClick: onBack
  }, "\u2190 Ledger"), /*#__PURE__*/React.createElement("div", {
    className: "detail-card"
  }, /*#__PURE__*/React.createElement("div", {
    className: "osc-hud"
  }, "Anomaly \xB7 ", item.kind), /*#__PURE__*/React.createElement("h1", {
    className: "detail-title"
  }, item.tag), /*#__PURE__*/React.createElement("div", {
    className: "detail-amt"
  }, "\u20AC", item.amount, " ", /*#__PURE__*/React.createElement("span", null, item.cycle)), /*#__PURE__*/React.createElement("p", {
    className: "detail-note"
  }, item.note), /*#__PURE__*/React.createElement("div", {
    className: "detail-rows"
  }, /*#__PURE__*/React.createElement("div", {
    className: "drow"
  }, /*#__PURE__*/React.createElement("span", null, "Expected"), /*#__PURE__*/React.createElement("b", null, "5th of month")), /*#__PURE__*/React.createElement("div", {
    className: "drow"
  }, /*#__PURE__*/React.createElement("span", null, "Last seen"), /*#__PURE__*/React.createElement("b", null, "2 cycles ago")), /*#__PURE__*/React.createElement("div", {
    className: "drow"
  }, /*#__PURE__*/React.createElement("span", null, "Confidence"), /*#__PURE__*/React.createElement("b", {
    className: "neon"
  }, "92%"))), /*#__PURE__*/React.createElement("div", {
    className: "detail-actions"
  }, /*#__PURE__*/React.createElement(Button, {
    variant: "primary",
    onClick: onResolve
  }, "Mark Paid"), /*#__PURE__*/React.createElement(Button, {
    onClick: onResolve
  }, "Snooze 30d"), /*#__PURE__*/React.createElement(Button, {
    variant: "ghost",
    onClick: onResolve
  }, "Dismiss")))));
}
Object.assign(window, {
  UnlockScreen,
  Ledger,
  AnomalyDetail
});
})(); } catch (e) { __ds_ns.__errors.push({ path: "ui_kits/anomalies/screens.jsx", error: String((e && e.message) || e) }); }

// ui_kits/dashboard/console.js
try { (() => {
/* ============================================================================
   OSCILLOCORE · App Dashboard — instrument behaviour
   Single fixed scanner field + the live readouts (dial, scope, sparks, ticker).
   ============================================================================ */
(function () {
  'use strict';

  // ---- read tokens from the cascade so the canvas matches the theme ----
  const css = getComputedStyle(document.documentElement);
  const tok = (n, f) => css.getPropertyValue(n).trim() || f;
  const NEON = tok('--neon', '#ff5e4d');
  const NEONHOT = tok('--neon-hot', '#ff6e62');
  const INDIGO = tok('--indigo-neon', '#8f7dff');
  const INK3 = tok('--ink-3', '#7c8096');
  const GRID = 'rgba(58,47,122,.32)';

  // ---- 1. the ONE fixed scanner field ----------------------------------
  // Full background (bg:true) with the dish + shapes parked in NEGATIVE space
  // (right of the scope, away from the dock text). Open/smooth morphs only.
  if (window.OscScanner) {
    OscScanner.mount(document.getElementById('bg'), {
      bg: true,
      dish: true,
      parallax: true,
      seed: 1906,
      shapes: [{
        char: '3',
        cx: .80,
        cy: .30,
        scale: .42,
        r: .34,
        style: 'faint',
        morph: 'blob',
        fill: .7
      }, {
        char: '8',
        cx: .92,
        cy: .74,
        scale: .30,
        r: .26,
        style: 'wire',
        morph: 'vein',
        fill: .55
      }, {
        char: 'e',
        cx: .66,
        cy: .58,
        scale: .22,
        r: .30,
        style: 'red',
        morph: 'vein',
        live: true,
        fill: .5
      }]
    });
  }
  const dpr = Math.min(window.devicePixelRatio || 1, 2);

  // ---- 2. savings dial -------------------------------------------------
  (function dial() {
    const c = document.getElementById('dial');
    if (!c) return;
    const ctx = c.getContext('2d');
    const w = c.width,
      h = c.height,
      cx = w / 2,
      cy = h / 2,
      R = w / 2 - 22;
    const pct = 0.62,
      start = -Math.PI * 0.5,
      end = start + Math.PI * 2 * pct;
    ctx.clearRect(0, 0, w, h);
    ctx.lineCap = 'round';
    // track
    ctx.lineWidth = 16;
    ctx.strokeStyle = 'rgba(86,72,191,.22)';
    ctx.beginPath();
    ctx.arc(cx, cy, R, 0, Math.PI * 2);
    ctx.stroke();
    // coral arc with glow
    ctx.shadowColor = NEON;
    ctx.shadowBlur = 18;
    ctx.lineWidth = 16;
    ctx.strokeStyle = NEON;
    ctx.beginPath();
    ctx.arc(cx, cy, R, start, end);
    ctx.stroke();
    // a small indigo tail past the head (the cool counter-signal)
    ctx.shadowColor = INDIGO;
    ctx.shadowBlur = 10;
    ctx.strokeStyle = INDIGO;
    ctx.lineWidth = 10;
    ctx.beginPath();
    ctx.arc(cx, cy, R, end, end + 0.28);
    ctx.stroke();
  })();

  // ---- 3. spend-trace scope -------------------------------------------
  function fit(c) {
    const r = c.getBoundingClientRect();
    c.width = Math.max(1, Math.round(r.width * dpr));
    c.height = Math.max(1, Math.round(r.height * dpr));
    const ctx = c.getContext('2d');
    ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
    return {
      ctx,
      w: r.width,
      h: r.height
    };
  }
  function drawScope() {
    const c = document.getElementById('scope');
    if (!c) return;
    const {
      ctx,
      w,
      h
    } = fit(c);
    ctx.clearRect(0, 0, w, h);
    const padL = 14,
      padR = 56,
      padT = 40,
      padB = 34;
    const x0 = padL,
      x1 = w - padR,
      y0 = padT,
      y1 = h - padB;
    const N = 19,
      days = 30,
      max = 4200;

    // faint internal grid (broken graph-paper, not edge-to-edge)
    ctx.strokeStyle = GRID;
    ctx.lineWidth = 1;
    for (let g = 0; g <= 3; g++) {
      const yy = y1 - (y1 - y0) * (g / 3);
      ctx.beginPath();
      ctx.moveTo(x0, yy);
      ctx.lineTo(x1, yy);
      ctx.stroke();
      ctx.fillStyle = INK3;
      ctx.font = '10px VG5000, monospace';
      ctx.textAlign = 'left';
      ctx.textBaseline = 'middle';
      if (g > 0) ctx.fillText((g * 1.1).toFixed(1) + 'k', x1 + 8, yy);
    }
    const X = i => x0 + (x1 - x0) * (i / (days - 1));
    const Y = v => y1 - (y1 - y0) * (v / max);

    // budget pace (straight dashed indigo to the cap)
    ctx.setLineDash([5, 5]);
    ctx.strokeStyle = 'rgba(143,125,255,.7)';
    ctx.lineWidth = 1.5;
    ctx.beginPath();
    ctx.moveTo(X(0), Y(0));
    ctx.lineTo(X(days - 1), Y(max));
    ctx.stroke();
    ctx.fillStyle = INDIGO;
    ctx.font = '10px VG5000, monospace';
    ctx.textAlign = 'right';
    ctx.fillText('BUDGET', x1 - 2, Y(max) - 8);

    // last cycle (faint dashed)
    ctx.strokeStyle = 'rgba(140,128,180,.4)';
    ctx.beginPath();
    for (let i = 0; i < days; i++) {
      const v = i / (days - 1) * 3960;
      (i ? ctx.lineTo : ctx.moveTo).call(ctx, X(i), Y(v));
    }
    ctx.stroke();
    ctx.setLineDash([]);

    // this cycle — molten coral trace (to day 19) + filled area
    const pts = [];
    let acc = 0;
    for (let i = 0; i < N; i++) {
      acc += 110 + Math.sin(i * 1.3) * 22 + i * 4;
      pts.push([X(i), Y(acc)]);
    }
    // area
    const grad = ctx.createLinearGradient(0, y0, 0, y1);
    grad.addColorStop(0, 'rgba(255,80,60,.28)');
    grad.addColorStop(1, 'rgba(255,59,46,.02)');
    ctx.beginPath();
    ctx.moveTo(pts[0][0], y1);
    pts.forEach(p => ctx.lineTo(p[0], p[1]));
    ctx.lineTo(pts[pts.length - 1][0], y1);
    ctx.closePath();
    ctx.fillStyle = grad;
    ctx.fill();
    // glowing line
    ctx.shadowColor = NEON;
    ctx.shadowBlur = 12;
    ctx.strokeStyle = NEON;
    ctx.lineWidth = 2;
    ctx.lineJoin = 'round';
    ctx.beginPath();
    pts.forEach((p, i) => (i ? ctx.lineTo : ctx.moveTo).call(ctx, p[0], p[1]));
    ctx.stroke();
    // head marker + dotted drop
    const head = pts[pts.length - 1];
    ctx.shadowBlur = 0;
    ctx.setLineDash([2, 4]);
    ctx.strokeStyle = 'rgba(255,94,77,.6)';
    ctx.beginPath();
    ctx.moveTo(head[0], head[1]);
    ctx.lineTo(head[0], y1);
    ctx.stroke();
    ctx.setLineDash([]);
    ctx.fillStyle = '#fff';
    ctx.beginPath();
    ctx.arc(head[0], head[1], 3.2, 0, Math.PI * 2);
    ctx.fill();
    ctx.shadowColor = NEON;
    ctx.shadowBlur = 14;
    ctx.strokeStyle = NEONHOT;
    ctx.lineWidth = 1.5;
    ctx.beginPath();
    ctx.arc(head[0], head[1], 4.5, 0, Math.PI * 2);
    ctx.stroke();
    ctx.shadowBlur = 0;
  }

  // ---- 4. category sparklines -----------------------------------------
  function drawSparks() {
    document.querySelectorAll('canvas.spark').forEach(c => {
      const {
        ctx,
        w,
        h
      } = fit(c);
      ctx.clearRect(0, 0, w, h);
      const pts = (c.dataset.pts || '').split(',').map(Number);
      if (!pts.length) return;
      const coral = c.dataset.coral === '1';
      const mn = Math.min(...pts),
        mx = Math.max(...pts),
        pad = 4;
      const X = i => pad + (w - pad * 2) * (i / (pts.length - 1));
      const Y = v => h - pad - (h - pad * 2) * ((v - mn) / (mx - mn || 1));
      ctx.shadowColor = coral ? NEON : INDIGO;
      ctx.shadowBlur = 7;
      ctx.strokeStyle = coral ? NEON : INDIGO;
      ctx.lineWidth = 1.6;
      ctx.lineJoin = 'round';
      ctx.beginPath();
      pts.forEach((v, i) => (i ? ctx.lineTo : ctx.moveTo).call(ctx, X(i), Y(v)));
      ctx.stroke();
    });
  }

  // ---- 5. watch ticker (gentle marquee) -------------------------------
  (function ticker() {
    const t = document.getElementById('ticker');
    if (!t) return;
    const base = t.innerHTML;
    t.innerHTML = base + ' <span class="dim">·</span> ' + base;
    let x = 0;
    function step() {
      x -= 0.4;
      if (-x > t.scrollWidth / 2) x = 0;
      t.style.transform = 'translateX(' + x + 'px)';
      requestAnimationFrame(step);
    }
    if (!window.matchMedia('(prefers-reduced-motion: reduce)').matches) requestAnimationFrame(step);
  })();

  // ---- 6. assistant collapse ------------------------------------------
  const btn = document.getElementById('aiCollapse'),
    panel = document.getElementById('aiPanel');
  if (btn && panel) btn.addEventListener('click', () => {
    panel.classList.toggle('collapsed');
    btn.textContent = panel.classList.contains('collapsed') ? '›' : '‹';
    requestAnimationFrame(() => {
      drawScope();
      drawSparks();
    });
  });

  // ---- redraw on resize ----
  function redraw() {
    drawScope();
    drawSparks();
  }
  redraw();
  let rt;
  window.addEventListener('resize', () => {
    clearTimeout(rt);
    rt = setTimeout(redraw, 120);
  });
  // fonts settling can change box sizes slightly — redraw once after load
  if (document.fonts && document.fonts.ready) document.fonts.ready.then(redraw);
  window.addEventListener('load', redraw);
})();
})(); } catch (e) { __ds_ns.__errors.push({ path: "ui_kits/dashboard/console.js", error: String((e && e.message) || e) }); }

// ui_kits/scanner/App.jsx
try { (() => {
const {
  useState: useStateApp
} = React;
function App() {
  const [route, setRoute] = useStateApp("home");
  const nav = r => setRoute(r === "index" ? "talks" : r);
  let page;
  if (route === "talks") page = /*#__PURE__*/React.createElement(Talks, null);else if (route === "scanner") page = /*#__PURE__*/React.createElement(ScannerPage, null);else page = /*#__PURE__*/React.createElement(Home, {
    onNav: nav
  });
  return /*#__PURE__*/React.createElement("div", {
    className: "site"
  }, /*#__PURE__*/React.createElement(Nav, {
    onNav: nav,
    route: route === "home" ? "index" : route
  }), page);
}
ReactDOM.createRoot(document.getElementById("root")).render(/*#__PURE__*/React.createElement(App, null));
})(); } catch (e) { __ds_ns.__errors.push({ path: "ui_kits/scanner/App.jsx", error: String((e && e.message) || e) }); }

// ui_kits/scanner/components.jsx
try { (() => {
/* Scanner brand-site kit — shared primitives. */
const {
  useRef,
  useEffect,
  useState
} = React;
function ScannerBg({
  seed = 1,
  shapes
}) {
  const ref = useRef(null);
  useEffect(() => {
    let inst;
    const t = () => {
      if (window.OscScanner && ref.current) {
        inst = window.OscScanner.mount(ref.current, {
          grid: true,
          dish: true,
          seed,
          shapes
        });
      } else setTimeout(t, 60);
    };
    t();
    return () => inst && inst.destroy && inst.destroy();
  }, []);
  return /*#__PURE__*/React.createElement("canvas", {
    ref: ref,
    className: "sbg"
  });
}
function Nav({
  onNav,
  route
}) {
  const items = [["index", "Index"], ["talks", "Talks"], ["scanner", "Scanner"]];
  return /*#__PURE__*/React.createElement("header", {
    className: "site-nav"
  }, /*#__PURE__*/React.createElement("div", {
    className: "logo",
    onClick: () => onNav("home")
  }, "Oscillo", /*#__PURE__*/React.createElement("span", null, "core")), /*#__PURE__*/React.createElement("nav", null, items.map(([k, l]) => /*#__PURE__*/React.createElement("button", {
    key: k,
    className: "nv" + (route === k ? " on" : ""),
    onClick: () => onNav(k)
  }, l)), /*#__PURE__*/React.createElement("span", {
    className: "osc-hud",
    style: {
      marginLeft: 14
    }
  }, "38C3")));
}

/* A framed talk/title card like the reference Reticulum slide */
function TalkCard({
  index,
  title,
  speaker,
  tag,
  onOpen
}) {
  return /*#__PURE__*/React.createElement("article", {
    className: "talk",
    onClick: onOpen
  }, /*#__PURE__*/React.createElement("div", {
    className: "talk-frame"
  }, /*#__PURE__*/React.createElement("span", {
    className: "talk-idx"
  }, index), /*#__PURE__*/React.createElement("h2", {
    className: "talk-title"
  }, title), /*#__PURE__*/React.createElement("div", {
    className: "talk-foot"
  }, /*#__PURE__*/React.createElement("span", {
    className: "talk-spk"
  }, speaker), /*#__PURE__*/React.createElement("span", {
    className: "talk-tag"
  }, tag)), /*#__PURE__*/React.createElement("span", {
    className: "corner-tick"
  })));
}
function Button({
  variant = "default",
  children,
  onClick
}) {
  return /*#__PURE__*/React.createElement("button", {
    className: "sbtn " + variant,
    onClick: onClick
  }, children);
}
Object.assign(window, {
  ScannerBg,
  Nav,
  TalkCard,
  Button
});
})(); } catch (e) { __ds_ns.__errors.push({ path: "ui_kits/scanner/components.jsx", error: String((e && e.message) || e) }); }

// ui_kits/scanner/screens.jsx
try { (() => {
/* Scanner brand-site — pages. */
const TALKS = [{
  idx: "01",
  title: "Reticulum: Unstoppable Networks for The People",
  speaker: "markqvist",
  tag: "38C3"
}, {
  idx: "02",
  title: "Illegal Instructions: Decoding the Undocumented",
  speaker: "osc/lab",
  tag: "TALK"
}, {
  idx: "03",
  title: "Differential Growth as a Signal Language",
  speaker: "osc/lab",
  tag: "DEMO"
}, {
  idx: "04",
  title: "Reading Anomalies in Everyday Ledgers",
  speaker: "m. qvist",
  tag: "FIELD"
}];
function Home({
  onNav
}) {
  return /*#__PURE__*/React.createElement("div", {
    className: "page home"
  }, /*#__PURE__*/React.createElement(ScannerBg, {
    seed: 9,
    shapes: [{
      char: "3",
      cx: .68,
      cy: .54,
      scale: .4,
      style: "red",
      live: true,
      fill: .62
    }, {
      char: "8",
      cx: .2,
      cy: .3,
      scale: .22,
      style: "wire",
      live: false,
      fill: .3
    }, {
      char: "e",
      cx: .42,
      cy: .82,
      scale: .26,
      style: "faint",
      live: false,
      fill: .5
    }]
  }), /*#__PURE__*/React.createElement("div", {
    className: "hero"
  }, /*#__PURE__*/React.createElement("div", {
    className: "osc-hud"
  }, "Oscillocore \xB7 differential growth scanner"), /*#__PURE__*/React.createElement("h1", {
    className: "hero-title"
  }, "Illegal", /*#__PURE__*/React.createElement("br", null), "Instructions"), /*#__PURE__*/React.createElement("p", {
    className: "hero-sub"
  }, "A CRT oscilloscope for your data. We grow signal out of letterforms and read the anomalies nobody else sees."), /*#__PURE__*/React.createElement("div", {
    className: "hero-cta"
  }, /*#__PURE__*/React.createElement(Button, {
    variant: "primary",
    onClick: () => onNav("scanner")
  }, "Open Scanner"), /*#__PURE__*/React.createElement(Button, {
    onClick: () => onNav("talks")
  }, "Browse Talks"))), /*#__PURE__*/React.createElement("div", {
    className: "home-talks"
  }, TALKS.slice(0, 2).map(t => /*#__PURE__*/React.createElement(TalkCard, {
    key: t.idx,
    index: t.idx,
    title: t.title,
    speaker: t.speaker,
    tag: t.tag,
    onOpen: () => onNav("talks")
  }))));
}
function Talks() {
  return /*#__PURE__*/React.createElement("div", {
    className: "page talks"
  }, /*#__PURE__*/React.createElement(ScannerBg, {
    seed: 4,
    shapes: [{
      char: "e",
      cx: .85,
      cy: .22,
      scale: .22,
      style: "faint",
      live: false,
      fill: .5
    }, {
      char: "3",
      cx: .1,
      cy: .8,
      scale: .2,
      style: "wire",
      live: false,
      fill: .3
    }]
  }), /*#__PURE__*/React.createElement("div", {
    className: "talks-inner"
  }, /*#__PURE__*/React.createElement("div", {
    className: "osc-hud"
  }, "Index \xB7 talks & field notes"), /*#__PURE__*/React.createElement("h1", {
    className: "page-h1"
  }, "Talks"), /*#__PURE__*/React.createElement("div", {
    className: "talks-grid"
  }, TALKS.map(t => /*#__PURE__*/React.createElement(TalkCard, {
    key: t.idx,
    index: t.idx,
    title: t.title,
    speaker: t.speaker,
    tag: t.tag,
    onOpen: () => {}
  })))));
}
function ScannerPage() {
  const [seedChar, setSeedChar] = useState("3");
  const [fill, setFill] = useState(62);
  const [k, setK] = useState(0); // remount key
  return /*#__PURE__*/React.createElement("div", {
    className: "page scanpage"
  }, /*#__PURE__*/React.createElement(ScannerBg, {
    key: k,
    seed: seedChar.charCodeAt(0) + fill,
    shapes: [{
      char: seedChar,
      cx: .5,
      cy: .5,
      scale: .42,
      style: "red",
      live: true,
      fill: fill / 100
    }]
  }), /*#__PURE__*/React.createElement("aside", {
    className: "dock"
  }, /*#__PURE__*/React.createElement("div", {
    className: "osc-hud",
    style: {
      marginBottom: 14
    }
  }, "Parameters"), /*#__PURE__*/React.createElement("label", {
    className: "dock-row"
  }, /*#__PURE__*/React.createElement("span", null, "seed glyph"), /*#__PURE__*/React.createElement("input", {
    className: "seed-in",
    maxLength: 1,
    value: seedChar,
    onChange: e => {
      setSeedChar(e.target.value || "3");
      setK(k + 1);
    }
  })), /*#__PURE__*/React.createElement("label", {
    className: "dock-row"
  }, /*#__PURE__*/React.createElement("span", null, "fill ", fill, "%"), /*#__PURE__*/React.createElement("input", {
    type: "range",
    min: "10",
    max: "95",
    value: fill,
    onChange: e => {
      setFill(+e.target.value);
      setK(k + 1);
    }
  })), /*#__PURE__*/React.createElement(Button, {
    onClick: () => setK(k + 1)
  }, "Regrow"), /*#__PURE__*/React.createElement("div", {
    className: "readout"
  }, "\u300C", seedChar, "\u300D growing \xB7 ", fill, "% target")));
}
Object.assign(window, {
  Home,
  Talks,
  ScannerPage
});
})(); } catch (e) { __ds_ns.__errors.push({ path: "ui_kits/scanner/screens.jsx", error: String((e && e.message) || e) }); }

})();
