/* Phoskonomia — Budgets page components.
   The budget is read as an oscilloscope: each category is a CHANNEL, its cap a
   trigger THRESHOLD, spend the signal level, and projection a forward marker.
   Extends window.PHOSK with budget metadata (projection + cycle history). */
const { useState: useStateB, useMemo: useMemoB } = React;

/* ---- budget metadata, attached to each category by name ----
   proj = projected end-of-cycle spend · hist = last 6 full cycles of spend.
   Tuned to the dashboard's narrative (dining creeping over, transport slack…). */
(function () {
  const P = window.PHOSK;
  const META = {
    "GROCERIES":      { proj: 868, hist: [765, 742, 810, 788, 756, 802], note: "On pace ~CHF 70 over cap." },
    "DINING & CAFÉS": { proj: 592, hist: [338, 356, 372, 388, 401, 410], note: "Crept up 3 cycles running." },
    "HOUSING":        { proj: 1680, hist: [1680, 1680, 1680, 1680, 1680, 1680], note: "Fixed — rent, charged on the 1st." },
    "HEALTH":         { proj: 470, hist: [210, 498, 180, 322, 150, 290], note: "Lumpy — pharmacy + check-ups." },
    "TRANSPORT":      { proj: 238, hist: [262, 244, 212, 205, 188, 176], note: "Under cap 3 cycles. Trim to CHF 220?" },
    "SUBSCRIPTIONS":  { proj: 234, hist: [164, 176, 176, 188, 188, 189], note: "Sunrise (CHF 45) will push over." },
    "UTILITIES":      { proj: 236, hist: [228, 210, 245, 198, 221, 205], note: "Seasonal — within band." },
    "HOUSEHOLD":      { proj: 252, hist: [145, 98, 210, 176, 132, 226], note: "Galaxus lamp pushed this cycle." },
    "LEISURE":        { proj: 212, hist: [266, 188, 310, 224, 198, 170], note: "Comfortably inside cap." },
    "CLOTHING":       { proj: 58,  hist: [120, 0, 210, 45, 0, 88], note: "Barely touched this cycle." },
  };
  P.categories.forEach((c) => { Object.assign(c, META[c.name] || { proj: c.spent, hist: [c.spent, c.spent, c.spent, c.spent, c.spent, c.spent], note: "" }); });
  // status from spend + projection against cap
  P.budgetStatus = (c) => {
    if (c.fixed) return { key: "fixed", label: "FIXED", tone: "blue" };
    if (c.budget === 0) return { key: "none", label: "NO CAP", tone: "blue" };
    if (c.spent === 0) return { key: "unused", label: "UNUSED", tone: "blue" };
    const p = c.spent / c.budget, pj = c.proj / c.budget;
    if (p > 1) return { key: "over", label: Math.round((p - 1) * 100) + "% OVER", tone: "coral" };
    if (pj > 1) return { key: "willexceed", label: "ON PACE OVER", tone: "coral" };
    if (p >= 0.85) return { key: "tight", label: "TIGHT", tone: "blue" };
    return { key: "ontrack", label: "ON TRACK", tone: "blue" };
  };
})();

/* ---- the channel level meter: signal vs threshold(cap) vs projection ---- */
function EnvMeter({ c, showProj = true, h = 9 }) {
  const D = window.PHOSK;
  const cap = c.budget || c.spent || 1;
  const domain = Math.max(cap, c.proj, c.spent) * 1.06;
  const pc = (v) => (v / domain) * 100;
  const over = c.spent > cap && cap > 0;
  const st = D.budgetStatus(c);
  const baseCol = st.key === "over" ? "var(--neon)" : st.key === "willexceed" || st.key === "tight" ? "var(--warn)" : "var(--indigo)";
  // signal fill: indigo/amber up to the cap, coral for any portion past it
  const capPart = Math.min(c.spent, cap);
  return (
    <div className="env-meter" style={{ height: h }}>
      <div className="env-sig" style={{ width: pc(capPart) + "%", background: baseCol, boxShadow: `0 0 7px ${baseCol}` }} />
      {over && <div className="env-sig over" style={{ left: pc(cap) + "%", width: pc(c.spent - cap) + "%" }} />}
      {c.budget > 0 && <div className="env-thresh" style={{ left: pc(cap) + "%" }} />}
      {showProj && c.budget > 0 && c.proj > c.spent &&
        <div className={"env-proj" + (c.proj > cap ? " hot" : "")} style={{ left: pc(c.proj) + "%" }} />}
    </div>
  );
}

/* ---- cap stepper — tune the threshold; live, mechanical ---- */
function CapStepper({ value, onStep, disabled }) {
  const D = window.PHOSK;
  if (disabled) return <span className="cap-fixed">FIXED CHARGE</span>;
  return (
    <div className="cap-step" onClick={(e) => e.stopPropagation()}>
      <span className="cl">CAP</span>
      <button className="cs" onClick={() => onStep(-10)} title="Lower cap CHF 10">−</button>
      <span className="cv">CHF {D.chf(value, 0)}</span>
      <button className="cs" onClick={() => onStep(+10)} title="Raise cap CHF 10">+</button>
    </div>
  );
}

/* ---- ENVELOPE CARD — the primary unit ---- */
function EnvCard({ c, cap, active, onSelect, onStep, showProj }) {
  const D = window.PHOSK;
  const cc = { ...c, budget: cap };
  const st = D.budgetStatus(cc);
  const p = cap > 0 ? cc.spent / cap : 0;
  const remaining = cap - cc.spent;
  const daysLeft = D.cycle.days - D.cycle.day;
  const perDay = remaining > 0 && daysLeft > 0 ? remaining / daysLeft : 0;
  return (
    <div className={"env osc-bkt " + st.tone + (active ? " on" : "")} role="button" tabIndex={0}
      onClick={() => onSelect(c.name)} onKeyDown={(e) => { if (e.key === "Enter" || e.key === " ") { e.preventDefault(); onSelect(c.name); } }}>
      <span className="osc-leg">{st.label}</span>
      <div className="env-h">
        <span className="env-nm">{c.name}</span>
        <span className={"env-tag" + (c.fixed ? " fix" : "")}>{c.fixed ? "FIXED" : "VARIABLE"}</span>
      </div>
      <div className="env-amt">
        <span className="sp">CHF {D.chf(cc.spent, 0)}</span>
        <span className="cap">/ {cap === 0 ? "—" : D.chf(cap, 0)}</span>
        <span className={"pct " + (st.key === "over" ? "alert" : st.key === "willexceed" || st.key === "tight" ? "warn" : "ok")}>
          {cap > 0 ? Math.round(p * 100) + "%" : "—"}
        </span>
      </div>
      <EnvMeter c={cc} showProj={showProj} />
      <div className="env-meta">
        <span><i>{remaining >= 0 ? "LEFT" : "OVER"}</i> CHF {D.chf(Math.abs(remaining), 0)}</span>
        {showProj && !c.fixed && cap > 0 &&
          <span><i>PROJ</i> <b className={c.proj > cap ? "coral" : ""}>CHF {D.chf(c.proj, 0)}</b></span>}
        {!c.fixed && remaining > 0 && <span><i>/DAY</i> CHF {D.chf(perDay, 0)}</span>}
        {c.fixed && <span><i>NEXT</i> 1 JUL</span>}
      </div>
      <div className="env-foot">
        <span className="env-items">{c.items} {c.items === 1 ? "ENTRY" : "ENTRIES"}</span>
        <CapStepper value={cap} disabled={c.fixed} onStep={(d) => onStep(c.name, d)} />
      </div>
    </div>
  );
}

/* ---- compact ROW variant ---- */
function EnvRow({ c, cap, active, onSelect, onStep, showProj }) {
  const D = window.PHOSK;
  const cc = { ...c, budget: cap };
  const st = D.budgetStatus(cc);
  const p = cap > 0 ? cc.spent / cap : 0;
  const remaining = cap - cc.spent;
  return (
    <div className={"envrow" + (active ? " on" : "")} onClick={() => onSelect(c.name)}>
      <span className={"er-nm" + (c.fixed ? " fix" : "")}>{c.name}</span>
      <span className={"er-stat " + st.tone}>{st.label}</span>
      <div className="er-meter"><EnvMeter c={cc} showProj={showProj} h={7} /></div>
      <span className="er-amt">CHF <b>{D.chf(cc.spent, 0)}</b> <i>/ {cap === 0 ? "—" : D.chf(cap, 0)}</i></span>
      <span className={"er-pct " + (st.key === "over" ? "alert" : st.key === "willexceed" || st.key === "tight" ? "warn" : "ok")}>{cap > 0 ? Math.round(p * 100) + "%" : "—"}</span>
      <CapStepper value={cap} disabled={c.fixed} onStep={(d) => onStep(c.name, d)} />
    </div>
  );
}

/* ---- ALLOCATION CONSOLE — channel-mix bar vs the monthly budget threshold ---- */
function AllocationBar({ caps }) {
  const D = window.PHOSK;
  const cats = D.categories;
  const allocated = cats.reduce((s, c) => s + (caps[c.name] ?? c.budget), 0);
  const budget = D.totals.budget;
  const domain = Math.max(allocated, budget) * 1.02;
  const over = allocated - budget;
  const shades = ["rgba(143,125,255,.42)", "rgba(106,95,192,.5)", "rgba(120,104,210,.4)", "rgba(90,72,191,.5)"];
  let acc = 0;
  return (
    <div className="alloc osc-bkt blue">
      <span className="osc-leg">ALLOCATION</span>
      <div className="alloc-h">
        <span className="hud">CHANNEL MIX · CAPS vs MONTHLY BUDGET</span>
        <span className={"alloc-flag " + (over > 0 ? "over" : "ok")}>
          {over > 0 ? "▲ CHF " + D.chf(over, 0) + " OVER-ALLOCATED" : "✓ CHF " + D.chf(-over, 0) + " UNALLOCATED"}
        </span>
      </div>
      <div className="alloc-track">
        {cats.map((c, i) => {
          const cap = caps[c.name] ?? c.budget;
          if (cap <= 0) return null;
          const w = (cap / domain) * 100;
          const seg = (
            <span key={c.name} className={"alloc-seg" + (c.fixed ? " fix" : "")} style={{ width: w + "%", background: c.fixed ? "rgba(143,125,255,.22)" : shades[i % shades.length] }}
              title={c.name + " · CHF " + D.chf(cap, 0)}>
              {w > 9 && <span className="alloc-lbl">{c.name.split(" ")[0]}</span>}
            </span>
          );
          acc += cap;
          return seg;
        })}
        <div className="alloc-thresh" style={{ left: (budget / domain) * 100 + "%" }}>
          <span className="alloc-thresh-lbl">BUDGET · CHF {D.chf(budget, 0)}</span>
        </div>
      </div>
      <div className="alloc-foot">
        <span><i>MONTHLY BUDGET</i> CHF {D.chf(budget, 0)}</span>
        <span><i>ALLOCATED</i> <b className={over > 0 ? "coral" : "blue"}>CHF {D.chf(allocated, 0)}</b></span>
        <span><i>ENVELOPES</i> {cats.filter((c) => (caps[c.name] ?? c.budget) > 0).length}</span>
        <span className="spacer" />
        <span className="alloc-ai"><window.Dot tone="blue" size={6} /> {over > 0
          ? <>GEMMA4 · trim CHF {D.chf(over, 0)} — Transport has run under cap 3 cycles.</>
          : <>GEMMA4 · balanced. Caps sum within your monthly budget.</>}</span>
      </div>
    </div>
  );
}

/* ---- six-cycle history mini-chart for the inspector (bars vs cap line) ---- */
function HistBars({ c, cap, w = 300, h = 110 }) {
  const D = window.PHOSK;
  const data = [...c.hist, c.proj];
  const labels = ["DEC", "JAN", "FEB", "MAR", "APR", "MAY", "JUN"];
  const max = Math.max(cap, ...data) * 1.12;
  const padB = 16, padT = 8;
  const bw = (w / data.length) * 0.56;
  const y = (v) => h - padB - (v / max) * (h - padT - padB);
  return (
    <svg width="100%" height={h} viewBox={`0 0 ${w} ${h}`} preserveAspectRatio="none" style={{ display: "block" }}>
      {/* cap threshold */}
      <line x1="0" y1={y(cap)} x2={w} y2={y(cap)} stroke="var(--neon-dim)" strokeWidth="1" strokeDasharray="4 4" opacity=".8" />
      <text x={w} y={y(cap) - 4} textAnchor="end" fill="var(--neon-dim)" fontSize="8" fontFamily="var(--font-body)" letterSpacing=".1em">CAP</text>
      {data.map((v, i) => {
        const x = (i + 0.5) * (w / data.length);
        const proj = i === data.length - 1;
        const col = v > cap ? "var(--neon)" : proj ? "rgba(143,125,255,.5)" : "rgba(132,116,222,.55)";
        return (
          <g key={i}>
            <rect x={x - bw / 2} y={y(v)} width={bw} height={h - padB - y(v)} fill={col}
              stroke={proj ? "var(--indigo-neon)" : "none"} strokeDasharray={proj ? "3 2" : "0"}
              style={v > cap ? { filter: "drop-shadow(0 0 4px var(--neon))" } : null} />
            <text x={x} y={h - 4} textAnchor="middle" fill={proj ? "var(--indigo-neon)" : "var(--ink-3)"} fontSize="7.5" fontFamily="var(--font-body)" letterSpacing=".08em">{labels[i]}</text>
          </g>
        );
      })}
    </svg>
  );
}

/* ---- BUDGET INSPECTOR (right dock) — category-focused, replaces signal dock ---- */
function BudgetInspector({ cat, cap, onClose, onStep, variant }) {
  const D = window.PHOSK;
  if (!cat) {
    return (
      <aside className={"sig-panel bud-insp" + (variant ? " " + variant : "")}>
        <div className="sig-empty">
          <span className="mk">⊞</span>
          <div className="tx">No envelope selected.<br />Click any <b>budget channel</b> to inspect its cycle history, projection and AI cap guidance.</div>
        </div>
      </aside>
    );
  }
  const c = { ...cat, budget: cap };
  const st = D.budgetStatus(c);
  const p = cap > 0 ? c.spent / cap : 0;
  const remaining = cap - c.spent;
  const projOver = c.proj - cap;
  const txns = D.transactions.filter((t) => t.cat === cat.name);
  return (
    <aside className={"sig-panel bud-insp" + (variant ? " " + variant : "")}>
      <div className="sig-head">
        <div className="kls">⊞ BUDGET CHANNEL · {c.fixed ? "FIXED" : "VARIABLE"}</div>
        <div className="nm">{cat.name}</div>
        <div className="ds">{cat.note}</div>
        {onClose && <span className="x" onClick={onClose} title="Close">✕</span>}
      </div>

      <div className="sig-delta">
        <span className={"big " + (st.key === "over" ? "up" : "down")} style={st.tone === "blue" && st.key !== "over" ? { color: "var(--ink)" } : null}>
          {cap > 0 ? Math.round(p * 100) + "%" : "—"}
        </span>
        <span className="vs">of cap used · {D.cycle.days - D.cycle.day} days left</span>
      </div>

      <div className="sig-chart">
        <HistBars c={cat} cap={cap} />
        <div className="axis"><span>6 CYCLES</span><span>PROJECTED · {D.cycle.label}</span></div>
      </div>

      <div className="sig-stats">
        <div className="st"><div className="k">Cap</div><div className="v">CHF {D.chf(cap, 0)}</div></div>
        <div className="st"><div className="k">Spent</div><div className="v coral">CHF {D.chf(c.spent, 0)}</div></div>
        <div className="st"><div className="k">{remaining >= 0 ? "Remaining" : "Over by"}</div><div className="v" style={{ color: remaining < 0 ? "var(--neon)" : "var(--ink)" }}>CHF {D.chf(Math.abs(remaining), 0)}</div></div>
        <div className="st"><div className="k">Projected</div><div className="v" style={{ color: c.proj > cap ? "var(--neon)" : "var(--ink)" }}>CHF {D.chf(c.proj, 0)}</div></div>
        <div className="st"><div className="k">Entries</div><div className="v">{cat.items}</div></div>
        <div className="st"><div className="k">6-cyc avg</div><div className="v" style={{ fontSize: 16 }}>CHF {D.chf(cat.hist.reduce((s, v) => s + v, 0) / cat.hist.length, 0)}</div></div>
      </div>

      {txns.length > 0 && (
        <div className="sig-recent">
          <div className="h">This cycle · {cat.name}</div>
          {txns.slice(0, 5).map((t, i) => (
            <div className="sig-occ" key={i}>
              <span className="dt">{t.date}</span>
              <span className="no">{t.shop}</span>
              <span className="pr">CHF {D.chf(t.amount)}</span>
            </div>
          ))}
        </div>
      )}

      {!cat.fixed && (
        <div className="insp-cap">
          <div className="h">Tune cap</div>
          <div className="insp-step" onClick={(e) => e.stopPropagation()}>
            <button className="cs" onClick={() => onStep(cat.name, -10)}>−</button>
            <span className="cv">CHF {D.chf(cap, 0)}</span>
            <button className="cs" onClick={() => onStep(cat.name, +10)}>+</button>
          </div>
        </div>
      )}

      <div className="sig-foot">
        <div className="tx">
          {st.key === "over" || st.key === "willexceed"
            ? <><b className="coral">⚠ {cat.name}</b> — {cat.note} Raise the cap or trim spend before cycle close.</>
            : st.key === "fixed"
              ? <>Fixed charge. {cat.note} Not tunable from here.</>
              : <>{cat.note} The AI keeps this channel under watch and flags drift early.</>}
        </div>
      </div>
    </aside>
  );
}

Object.assign(window, { EnvMeter, CapStepper, EnvCard, EnvRow, AllocationBar, HistBars, BudgetInspector });
