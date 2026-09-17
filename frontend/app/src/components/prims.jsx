/* Phoskonomia — shared primitives. Dual-published: ES named exports + window net. */
import React from 'react'

const { useRef: useRefP, useEffect: useEffectP } = React;

/* ---- ScannerBg: live differential-growth scanner, sized to its artboard ---- */
function ScannerBg({ seed = 1, shapes, className = "phosk-bg", bg = true, grid = true, dish = true, parallax = false }) {
  const ref = useRefP(null);
  useEffectP(() => {
    let inst;
    const tryMount = () => {
      if (window.OscScanner && ref.current) {
        inst = window.OscScanner.mount(ref.current, {
          grid, dish, bg, parallax, seed,
          shapes: shapes || [{ char: "8", cx: .8, cy: .4, scale: .3, style: "faint", live: false, fill: .4 }]
        });
      } else {setTimeout(tryMount, 60);}
    };
    tryMount();
    return () => inst && inst.destroy && inst.destroy();
  }, []);
  return <canvas ref={ref} className={className} />;
}

/* ---- StatusDot ---- */
function Dot({ tone = "ok", size = 8 }) {
  const c = tone === "alert" ? "var(--neon)" : tone === "warn" ? "var(--warn)" : tone === "blue" ? "var(--indigo-neon)" : "var(--ok)";
  return <span style={{ width: size, height: size, borderRadius: 999, background: c, boxShadow: `0 0 8px ${c}`, flex: "0 0 auto", display: "inline-block" }} />;
}

/* ---- HudCell: framed Pilowlava glyph ---- */
function HudCell({ glyph, size = 46 }) {
  return <div className="phosk-cell" style={{ width: size, height: size }}><b style={{ fontSize: size * .64 }}>{glyph}</b></div>;
}

/* ---- Sparkline: tiny neon trace ---- */
function Spark({ data, w = 92, h = 26, tone = "neon" }) {
  const max = Math.max(...data),min = Math.min(...data),rng = max - min || 1;
  const pts = data.map((v, i) => `${(i / (data.length - 1) * w).toFixed(1)},${(h - 2 - (v - min) / rng * (h - 4)).toFixed(1)}`).join(" ");
  const stroke = tone === "indigo" ? "var(--indigo-neon)" : "var(--neon)";
  return (
    <svg width={w} height={h} viewBox={`0 0 ${w} ${h}`} style={{ display: "block" }}>
      <polyline points={pts} fill="none" stroke={stroke} strokeWidth="1.4" strokeLinejoin="round" strokeLinecap="round"
      style={{ filter: `drop-shadow(0 0 2px ${stroke})` }} />
    </svg>);

}

/* ---- CatBar: a category budget row's progress bar (indigo→amber→coral) ---- */
function pctTone(p) {return p > 1 ? "alert" : p >= 0.85 ? "warn" : "ok";}
function barColor(tone) {return tone === "alert" ? "var(--neon)" : tone === "warn" ? "var(--warn)" : "var(--indigo)";}

function CatBar({ spent, budget, h = 7, tone: toneOverride }) {
  const p = budget > 0 ? spent / budget : 0;
  const tone = toneOverride || pctTone(p);
  const col = barColor(tone);
  const over = tone === "alert" && p > 1;
  const fill = Math.min(p, 1) * 100;
  // overflow band width as % of the (capped) bar — encodes how far over, contained
  const band = over ? Math.max(12, Math.min((p - 1) * 100, 46)) : 0;
  return (
    <div className={"phosk-bar" + (over ? " over" : "")} style={{ height: h }}>
      <div className="phosk-bar-fill" style={{ width: `${fill}%`, background: col, boxShadow: `0 0 7px ${col}` }} />
      {over && <div className="phosk-bar-over" style={{ width: `${band}%` }} />}
    </div>);

}

/* ---- PhoskChart: spending-over-time. cumulative spend vs budget pace + daily bars.
   Fully prop-driven (no mock): the page passes cycle length, budget and the
   series from /cycle/current + /cycle/current/spend-series. Renders nothing
   meaningful (just the grid) until `cumulative` has points. ---- */
function PhoskChart({ width, height, days = 30, today = 0, budget = 0,
  daily = [], cumulative = [], pace = [], lastCumulative = [],
  showBars = true, showPace = true, showArea = true, showLast = false,
  padL = 8, padR = 8, padT = 14, padB = 18 }) {
  const W = width, H = height;
  const span = days > 1 ? days - 1 : 1;
  const x = (d) => padL + d / span * (W - padL - padR);
  const maxY = (budget || Math.max(1, ...cumulative)) * 1.04;
  const y = (v) => H - padB - v / maxY * (H - padT - padB);

  // cumulative line (today's point is last)
  const cum = cumulative || [];
  const hasCum = cum.length > 0;
  const cumPts = cum.map((v, i) => `${x(i).toFixed(1)},${y(v).toFixed(1)}`).join(" ");
  const lastX = hasCum ? x(cum.length - 1) : padL, lastY = hasCum ? y(cum[cum.length - 1]) : y(0);

  // budget pace
  const pacePts = (pace || []).map((v, i) => `${x(i).toFixed(1)},${y(v).toFixed(1)}`).join(" ");

  // prior-cycle cumulative for comparison (scope mode)
  const lastPts = (lastCumulative || []).map((v, i) => `${x(i).toFixed(1)},${y(v).toFixed(1)}`).join(" ");

  const areaPts = `${padL},${y(0)} ${cumPts} ${lastX},${y(0)}`;
  const barW = Math.max(2, (W - padL - padR) / Math.max(days, 1) * 0.42);

  return (
    <svg width={W} height={H} viewBox={`0 0 ${W} ${H}`} style={{ display: "block" }}>
      <defs>
        <linearGradient id="cumfill" x1="0" y1="0" x2="0" y2="1">
          <stop offset="0%" stopColor="rgba(255,94,77,.30)" />
          <stop offset="100%" stopColor="rgba(255,94,77,0)" />
        </linearGradient>
      </defs>
      {/* horizontal gridlines at 25/50/75/100% of budget */}
      {[0.25, 0.5, 0.75, 1].map((f, i) =>
      <g key={i}>
          <line x1={padL} y1={y(budget * f)} x2={W - padR} y2={y(budget * f)}
        stroke="rgba(106,95,192,.18)" strokeWidth="1" strokeDasharray={f === 1 ? "0" : "2 4"} />
          <text x={W - padR} y={y(budget * f) - 3} textAnchor="end"
        fill="var(--ink-3)" fontSize="8.5" fontFamily="var(--font-body)" letterSpacing=".06em">
            {f === 1 ? "BUDGET" : (budget * f / 1000).toFixed(1) + "k"}
          </text>
        </g>
      )}
      {/* daily bars */}
      {showBars && (daily || []).map((d, i) => {
        const bh = Math.min(d, 600) / maxY * (H - padT - padB);
        return <rect key={i} x={x(i) - barW / 2} y={H - padB - bh} width={barW} height={Math.max(0, bh)}
        fill={d > 300 ? "rgba(255,94,77,.30)" : "rgba(132,116,222,.32)"} />;
      })}
      {/* prior cycle */}
      {showLast && lastPts && <polyline points={lastPts} fill="none" stroke="rgba(143,125,255,.55)" strokeWidth="1.4" strokeDasharray="4 4" />}
      {/* budget pace */}
      {showPace && pacePts && <polyline points={pacePts} fill="none" stroke="var(--indigo-neon)" strokeWidth="1.2" strokeDasharray="5 5" opacity=".8" />}
      {/* cumulative area + line */}
      {hasCum && showArea && <polygon points={areaPts} fill="url(#cumfill)" />}
      {hasCum && <polyline points={cumPts} fill="none" stroke="var(--neon)" strokeWidth="2" strokeLinejoin="round"
      style={{ filter: "drop-shadow(0 0 3px var(--neon))" }} />}
      {/* today marker */}
      {hasCum && <line x1={lastX} y1={padT - 6} x2={lastX} y2={H - padB} stroke="rgba(255,59,46,.4)" strokeWidth="1" strokeDasharray="2 3" />}
      {hasCum && <circle cx={lastX} cy={lastY} r="3" fill="var(--neon-white)" style={{ filter: "drop-shadow(0 0 4px var(--neon))" }} />}
    </svg>);

}

/* ---- SavingsDial: arc gauge for savings progress (pure: props only) ---- */
function SavingsDial({ size = 132, saved = 0, target = 1, projected = 0 }) {
  const r = size / 2 - 12,cx = size / 2,cy = size / 2;
  const start = -220,end = 40; // degrees, sweep
  const sweep = end - start;
  const pSaved = Math.min(saved / target, 1);
  const pProj = Math.min(projected / target, 1);
  const pol = (deg, rad = r) => {
    const a = deg * Math.PI / 180;
    return [cx + rad * Math.cos(a), cy + rad * Math.sin(a)];
  };
  const arc = (p, rad = r) => {
    const a0 = start,a1 = start + sweep * p;
    const [x0, y0] = pol(a0, rad),[x1, y1] = pol(a1, rad);
    const large = a1 - a0 > 180 ? 1 : 0;
    return `M ${x0.toFixed(1)} ${y0.toFixed(1)} A ${rad} ${rad} 0 ${large} 1 ${x1.toFixed(1)} ${y1.toFixed(1)}`;
  };
  return (
    <svg width={size} height={size} viewBox={`0 0 ${size} ${size}`} style={{ display: "block" }}>
      <path d={arc(1)} fill="none" stroke="rgba(106,95,192,.25)" strokeWidth="7" strokeLinecap="round" />
      <path d={arc(pProj)} fill="none" stroke="var(--indigo-neon)" strokeWidth="3" strokeLinecap="round" strokeDasharray="2 3" opacity=".7" />
      <path d={arc(pSaved)} fill="none" stroke="var(--neon)" strokeWidth="7" strokeLinecap="round"
      style={{ filter: "drop-shadow(0 0 5px var(--neon))" }} />
      <text x={cx} y={cy - 2} textAnchor="middle" fill="var(--ink)" fontSize={size * .2} fontFamily="var(--font-display)">{Math.round(pSaved * 100)}%</text>
      <text x={cx} y={cy + size * .16} textAnchor="middle" fill="var(--ink-3)" fontSize="9" fontFamily="var(--font-body)" letterSpacing=".14em">OF TARGET</text>
    </svg>);

}

Object.assign(window, { ScannerBg, Dot, HudCell, Spark, CatBar, PhoskChart, SavingsDial, pctTone, barColor });

export { ScannerBg, Dot, HudCell, Spark, CatBar, PhoskChart, SavingsDial, pctTone, barColor };
