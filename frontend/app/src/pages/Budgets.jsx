import React from 'react'
import { chf } from '../data/phosk.js'
import { api, useGet } from '../lib/api.js'
import { ScannerBg, Dot } from '../components/prims.jsx'
import { TopBar } from '../components/comps.jsx'
import { AiPanel } from '../components/shell.jsx'
import { Awaiting } from '../components/states.jsx'
import { useTweaks, TweaksPanel, TweakSection, TweakRadio, TweakToggle } from '../lib/tweaks.jsx'

/* Phoskonomia — Budgets page. Envelope console: tune category caps (channel
   thresholds) against the monthly budget, watch projection, inspect any channel.
   Reuses the shared shell (AI panel left, inspector dock right).

   Wired to phosk_api: every envelope, total, allocation segment and inspector
   detail is FETCHED from the backend; cap steppers PATCH /categories/{name}.
   No mock data — each section renders <Awaiting/> until its endpoint goes green. */
const { useState: useStateBP, useMemo: useMemoBP, useEffect: useEffectBP } = React;

/* ---- pure status mapper: category record {budget,spent,proj,fixed} → {key,label,tone}.
   Reads only fields the backend supplies; recomputes nothing the API already derives. */
function budgetStatus(c) {
  if (!c) return { key: "none", label: "—", tone: "blue" };
  const budget = c.budget || 0, spent = c.spent || 0, proj = c.proj != null ? c.proj : spent;
  if (c.fixed) return { key: "fixed", label: "FIXED", tone: "blue" };
  if (budget === 0) return { key: "none", label: "NO CAP", tone: "blue" };
  if (spent === 0) return { key: "unused", label: "UNUSED", tone: "blue" };
  const p = spent / budget, pj = proj / budget;
  if (p > 1) return { key: "over", label: Math.round((p - 1) * 100) + "% OVER", tone: "coral" };
  if (pj > 1) return { key: "willexceed", label: "ON PACE OVER", tone: "coral" };
  if (p >= 0.85) return { key: "tight", label: "TIGHT", tone: "blue" };
  return { key: "ontrack", label: "ON TRACK", tone: "blue" };
}

const num = (v) => (v == null || !isFinite(Number(v)) ? null : Number(v));

/* ---- the channel level meter: signal vs threshold(cap) vs projection ---- */
function EnvMeter({ c, cap, showProj = true, h = 9 }) {
  const spent = c.spent || 0;
  const proj = c.proj != null ? c.proj : spent;
  const capV = cap || spent || 1;
  const domain = Math.max(capV, proj, spent) * 1.06 || 1;
  const pc = (v) => (v / domain) * 100;
  const over = spent > capV && capV > 0;
  const st = budgetStatus({ ...c, budget: cap });
  const baseCol = st.key === "over" ? "var(--neon)" : st.key === "willexceed" || st.key === "tight" ? "var(--warn)" : "var(--indigo)";
  const capPart = Math.min(spent, capV);
  return (
    <div className="env-meter" style={{ height: h }}>
      <div className="env-sig" style={{ width: pc(capPart) + "%", background: baseCol, boxShadow: `0 0 7px ${baseCol}` }} />
      {over && <div className="env-sig over" style={{ left: pc(capV) + "%", width: pc(spent - capV) + "%" }} />}
      {cap > 0 && <div className="env-thresh" style={{ left: pc(capV) + "%" }} />}
      {showProj && cap > 0 && proj > spent &&
        <div className={"env-proj" + (proj > capV ? " hot" : "")} style={{ left: pc(proj) + "%" }} />}
    </div>
  );
}

/* ---- cap stepper — tune the threshold; live, mechanical ---- */
function CapStepper({ value, onStep, disabled }) {
  if (disabled) return <span className="cap-fixed">FIXED CHARGE</span>;
  return (
    <div className="cap-step" onClick={(e) => e.stopPropagation()}>
      <span className="cl">CAP</span>
      <button className="cs" onClick={() => onStep(-10)} title="Lower cap CHF 10">−</button>
      <span className="cv">CHF {chf(value, 0)}</span>
      <button className="cs" onClick={() => onStep(+10)} title="Raise cap CHF 10">+</button>
    </div>
  );
}

/* ---- ENVELOPE CARD — the primary unit ---- */
function EnvCard({ c, cap, daysLeft, active, onSelect, onStep, showProj }) {
  const st = budgetStatus({ ...c, budget: cap });
  const spent = c.spent || 0;
  const proj = c.proj != null ? c.proj : spent;
  const p = cap > 0 ? spent / cap : 0;
  const remaining = c.remaining != null ? c.remaining : cap - spent;
  const perDay = remaining > 0 && daysLeft > 0 ? remaining / daysLeft : 0;
  const items = c.items || 0;
  return (
    <div className={"env osc-bkt " + st.tone + (active ? " on" : "")} role="button" tabIndex={0}
      onClick={() => onSelect(c.name)} onKeyDown={(e) => { if (e.key === "Enter" || e.key === " ") { e.preventDefault(); onSelect(c.name); } }}>
      <span className="osc-leg">{st.label}</span>
      <div className="env-h">
        <span className="env-nm">{c.name}</span>
        <span className={"env-tag" + (c.fixed ? " fix" : "")}>{c.fixed ? "FIXED" : "VARIABLE"}</span>
      </div>
      <div className="env-amt">
        <span className="sp">CHF {chf(spent, 0)}</span>
        <span className="cap">/ {cap === 0 ? "—" : chf(cap, 0)}</span>
        <span className={"pct " + (st.key === "over" ? "alert" : st.key === "willexceed" || st.key === "tight" ? "warn" : "ok")}>
          {cap > 0 ? Math.round(p * 100) + "%" : "—"}
        </span>
      </div>
      <EnvMeter c={c} cap={cap} showProj={showProj} />
      <div className="env-meta">
        <span><i>{remaining >= 0 ? "LEFT" : "OVER"}</i> CHF {chf(Math.abs(remaining), 0)}</span>
        {showProj && !c.fixed && cap > 0 &&
          <span><i>PROJ</i> <b className={proj > cap ? "coral" : ""}>CHF {chf(proj, 0)}</b></span>}
        {!c.fixed && remaining > 0 && <span><i>/DAY</i> CHF {chf(perDay, 0)}</span>}
        {c.fixed && c.next && <span><i>NEXT</i> {c.next}</span>}
      </div>
      <div className="env-foot">
        <span className="env-items">{items} {items === 1 ? "ENTRY" : "ENTRIES"}</span>
        <CapStepper value={cap} disabled={c.fixed} onStep={(d) => onStep(c.name, d)} />
      </div>
    </div>
  );
}

/* ---- compact ROW variant ---- */
function EnvRow({ c, cap, active, onSelect, onStep, showProj }) {
  const st = budgetStatus({ ...c, budget: cap });
  const spent = c.spent || 0;
  const p = cap > 0 ? spent / cap : 0;
  return (
    <div className={"envrow" + (active ? " on" : "")} onClick={() => onSelect(c.name)}>
      <span className={"er-nm" + (c.fixed ? " fix" : "")}>{c.name}</span>
      <span className={"er-stat " + st.tone}>{st.label}</span>
      <div className="er-meter"><EnvMeter c={c} cap={cap} showProj={showProj} h={7} /></div>
      <span className="er-amt">CHF <b>{chf(spent, 0)}</b> <i>/ {cap === 0 ? "—" : chf(cap, 0)}</i></span>
      <span className={"er-pct " + (st.key === "over" ? "alert" : st.key === "willexceed" || st.key === "tight" ? "warn" : "ok")}>{cap > 0 ? Math.round(p * 100) + "%" : "—"}</span>
      <CapStepper value={cap} disabled={c.fixed} onStep={(d) => onStep(c.name, d)} />
    </div>
  );
}

/* ---- ALLOCATION CONSOLE — channel-mix bar vs the monthly budget threshold.
   Segments + GEMMA4 advice come straight from /budget/allocation. ---- */
/* The channel-mix bar. Layout (segment widths, budget marker) is presentation;
   the figures (allocated / over-/unallocated) and the GEMMA4 advice are read
   straight off the backend (/budget/totals + /budget/allocation.aiAdvice) — never
   recomputed or fabricated. */
function AllocationBar({ alloc, totals, budget, res, loading }) {
  const segments = (alloc && Array.isArray(alloc.segments)) ? alloc.segments : [];
  const advice = (alloc && alloc.aiAdvice) || null;
  const budgetV = num(budget);
  if (!alloc || (res && res.status !== 200) || segments.length === 0) {
    return <Awaiting label="ALLOCATION" res={res} loading={loading} tone="blue" />;
  }
  const T = totals || {};
  const allocated = num(T.allocated);        // backend-derived (— if absent)
  const overAllocated = num(T.overAllocated);
  const unallocated = num(T.unallocated);
  // geometry only: a denominator to draw the bars/marker against (presentation,
  // not a displayed metric). Prefer fetched allocated; else sum the segment caps.
  const capSum = segments.reduce((s, seg) => s + (seg.cap || 0), 0);
  const domain = Math.max(allocated != null ? allocated : capSum, budgetV || 0) * 1.02 || 1;
  const isOver = overAllocated != null ? overAllocated > 0
    : (unallocated != null ? false : null);
  const shades = ["rgba(143,125,255,.42)", "rgba(106,95,192,.5)", "rgba(120,104,210,.4)", "rgba(90,72,191,.5)"];
  return (
    <div className="alloc osc-bkt blue">
      <span className="osc-leg">ALLOCATION</span>
      <div className="alloc-h">
        <span className="hud">CHANNEL MIX · CAPS vs MONTHLY BUDGET</span>
        <span className={"alloc-flag " + (isOver ? "over" : "ok")}>
          {isOver === null ? "—"
            : isOver ? "▲ CHF " + chf(overAllocated, 0) + " OVER-ALLOCATED"
            : "✓ CHF " + chf(unallocated != null ? unallocated : 0, 0) + " UNALLOCATED"}
        </span>
      </div>
      <div className="alloc-track">
        {segments.map((seg, i) => {
          const cap = seg.cap || 0;
          if (cap <= 0) return null;
          const w = seg.share != null ? seg.share * 100 : (cap / domain) * 100;
          return (
            <span key={seg.name || i} className={"alloc-seg" + (seg.fixed ? " fix" : "")} style={{ width: w + "%", background: seg.fixed ? "rgba(143,125,255,.22)" : shades[i % shades.length] }}
              title={(seg.name || "") + " · CHF " + chf(cap, 0)}>
              {w > 9 && <span className="alloc-lbl">{String(seg.name || "").split(" ")[0]}</span>}
            </span>
          );
        })}
        {budgetV != null &&
          <div className="alloc-thresh" style={{ left: (budgetV / domain) * 100 + "%" }}>
            <span className="alloc-thresh-lbl">BUDGET · CHF {chf(budgetV, 0)}</span>
          </div>}
      </div>
      <div className="alloc-foot">
        <span><i>MONTHLY BUDGET</i> CHF {budgetV != null ? chf(budgetV, 0) : "—"}</span>
        <span><i>ALLOCATED</i> <b className={isOver ? "coral" : "blue"}>{allocated != null ? "CHF " + chf(allocated, 0) : "—"}</b></span>
        <span><i>ENVELOPES</i> {segments.filter((s) => (s.cap || 0) > 0).length}</span>
        <span className="spacer" />
        {advice && advice.text &&
          <span className="alloc-ai"><Dot tone="blue" size={6} /> {(advice.model || "GEMMA4")} · {advice.text}</span>}
      </div>
    </div>
  );
}

/* ---- six-cycle history mini-chart for the inspector (bars vs cap line).
   Bars are the fetched hist[] plus the fetched projection. ---- */
function HistBars({ hist = [], proj, cap, w = 300, h = 110 }) {
  const series = Array.isArray(hist) ? hist : [];
  const data = proj != null ? [...series, proj] : series;
  if (data.length === 0) return null;
  const labels = ["DEC", "JAN", "FEB", "MAR", "APR", "MAY", "JUN", "JUL", "AUG", "SEP", "OCT", "NOV"];
  const capV = cap || 0;
  const max = Math.max(capV, ...data) * 1.12 || 1;
  const padB = 16, padT = 8;
  const bw = (w / data.length) * 0.56;
  const y = (v) => h - padB - (v / max) * (h - padT - padB);
  return (
    <svg width="100%" height={h} viewBox={`0 0 ${w} ${h}`} preserveAspectRatio="none" style={{ display: "block" }}>
      <line x1="0" y1={y(capV)} x2={w} y2={y(capV)} stroke="var(--neon-dim)" strokeWidth="1" strokeDasharray="4 4" opacity=".8" />
      <text x={w} y={y(capV) - 4} textAnchor="end" fill="var(--neon-dim)" fontSize="8" fontFamily="var(--font-body)" letterSpacing=".1em">CAP</text>
      {data.map((v, i) => {
        const x = (i + 0.5) * (w / data.length);
        const isProj = proj != null && i === data.length - 1;
        const col = v > capV ? "var(--neon)" : isProj ? "rgba(143,125,255,.5)" : "rgba(132,116,222,.55)";
        return (
          <g key={i}>
            <rect x={x - bw / 2} y={y(v)} width={bw} height={h - padB - y(v)} fill={col}
              stroke={isProj ? "var(--indigo-neon)" : "none"} strokeDasharray={isProj ? "3 2" : "0"}
              style={v > capV ? { filter: "drop-shadow(0 0 4px var(--neon))" } : null} />
            <text x={x} y={h - 4} textAnchor="middle" fill={isProj ? "var(--indigo-neon)" : "var(--ink-3)"} fontSize="7.5" fontFamily="var(--font-body)" letterSpacing=".08em">{labels[i] || ""}</text>
          </g>
        );
      })}
    </svg>
  );
}

/* ---- BUDGET INSPECTOR (right dock) — category-focused, replaces signal dock.
   Wrapper: renders the empty state (no hooks) until a channel is selected, then
   mounts the body that self-fetches /categories/{name} + /transactions. ---- */
function BudgetInspector({ cat, cap, daysLeft, cycleLabel, onClose, onStep, variant }) {
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
  return <BudgetInspectorBody cat={cat} cap={cap} daysLeft={daysLeft} cycleLabel={cycleLabel} onClose={onClose} onStep={onStep} variant={variant} />;
}

function BudgetInspectorBody({ cat, cap, daysLeft, cycleLabel, onClose, onStep, variant }) {
  const enc = encodeURIComponent(cat.name);
  const { data: detail, status: detStatus, res: detRes } = useGet(`/categories/${enc}`, undefined, [cat.name]);
  const { data: txnData } = useGet(`/categories/${enc}/transactions`, undefined, [cat.name]);
  const name = cat.name;

  const d = detail || {};
  // Prefer detail-endpoint fields; fall back to the list record fields.
  const spent = cat.spent || 0;
  const proj = d.projectedSpend != null ? d.projectedSpend : (cat.proj != null ? cat.proj : spent);
  const hist = Array.isArray(cat.hist) ? cat.hist : [];
  const st = budgetStatus({ ...cat, budget: cap, proj });
  const p = cap > 0 ? spent / cap : 0;
  const remaining = cat.remaining != null ? cat.remaining : cap - spent;
  const histAvg = d.histAvg != null ? d.histAvg : null; // backend-derived; "—" when absent
  const guidance = d.guidance || cat.note || "";
  const txns = Array.isArray(txnData) ? txnData : (txnData && (txnData.transactions || txnData.rows)) || [];
  const detailEmpty = name && detStatus != null && detStatus !== 200;

  return (
    <aside className={"sig-panel bud-insp" + (variant ? " " + variant : "")}>
      <div className="sig-head">
        <div className="kls">⊞ BUDGET CHANNEL · {cat.fixed ? "FIXED" : "VARIABLE"}</div>
        <div className="nm">{cat.name}</div>
        <div className="ds">{guidance}</div>
        {onClose && <span className="x" onClick={onClose} title="Close">✕</span>}
      </div>

      <div className="sig-delta">
        <span className={"big " + (st.key === "over" ? "up" : "down")} style={st.tone === "blue" && st.key !== "over" ? { color: "var(--ink)" } : null}>
          {cap > 0 ? Math.round(p * 100) + "%" : "—"}
        </span>
        <span className="vs">of cap used · {daysLeft} days left</span>
      </div>

      {hist.length > 0 ? (
        <div className="sig-chart">
          <HistBars hist={hist} proj={proj} cap={cap} />
          <div className="axis"><span>{hist.length} CYCLES</span><span>PROJECTED · {cycleLabel || ""}</span></div>
        </div>
      ) : detailEmpty ? (
        <Awaiting label="HISTORY" res={detRes} loading={false} tone="blue" style={{ margin: "10px 0" }} />
      ) : null}

      <div className="sig-stats">
        <div className="st"><div className="k">Cap</div><div className="v">CHF {chf(cap, 0)}</div></div>
        <div className="st"><div className="k">Spent</div><div className="v coral">CHF {chf(spent, 0)}</div></div>
        <div className="st"><div className="k">{remaining >= 0 ? "Remaining" : "Over by"}</div><div className="v" style={{ color: remaining < 0 ? "var(--neon)" : "var(--ink)" }}>CHF {chf(Math.abs(remaining), 0)}</div></div>
        <div className="st"><div className="k">Projected</div><div className="v" style={{ color: proj > cap ? "var(--neon)" : "var(--ink)" }}>CHF {chf(proj, 0)}</div></div>
        <div className="st"><div className="k">Entries</div><div className="v">{cat.items != null ? cat.items : "—"}</div></div>
        <div className="st"><div className="k">{hist.length}-cyc avg</div><div className="v" style={{ fontSize: 16 }}>{histAvg != null ? "CHF " + chf(histAvg, 0) : "—"}</div></div>
      </div>

      {txns.length > 0 && (
        <div className="sig-recent">
          <div className="h">This cycle · {cat.name}</div>
          {txns.slice(0, 5).map((t, i) => (
            <div className="sig-occ" key={t.id || i}>
              <span className="dt">{t.date}</span>
              <span className="no">{t.shop}</span>
              <span className="pr">CHF {chf(t.amount)}</span>
            </div>
          ))}
        </div>
      )}

      {!cat.fixed && (
        <div className="insp-cap">
          <div className="h">Tune cap</div>
          <div className="insp-step" onClick={(e) => e.stopPropagation()}>
            <button className="cs" onClick={() => onStep(cat.name, -10)}>−</button>
            <span className="cv">CHF {chf(cap, 0)}</span>
            <button className="cs" onClick={() => onStep(cat.name, +10)}>+</button>
          </div>
        </div>
      )}

      <div className="sig-foot">
        <div className="tx">
          {st.key === "over" || st.key === "willexceed"
            ? <><b className="coral">⚠ {cat.name}</b> — {guidance} {d.overCapAmount != null && d.overCapAmount > 0 ? "Over cap by CHF " + chf(d.overCapAmount, 0) + ". " : ""}Raise the cap or trim spend before cycle close.</>
            : st.key === "fixed"
              ? <>Fixed charge. {guidance} Not tunable from here.</>
              : <>{guidance} The AI keeps this channel under watch and flags drift early.</>}
        </div>
      </div>
    </aside>
  );
}

const BUD_TWEAK_DEFAULTS = /*EDITMODE-BEGIN*/{
  "envLayout": "cards",
  "sort": "order",
  "showProj": true,
  "aiOpen": true
} /*EDITMODE-END*/;

function BudTweaks({ tw, setTweak, onAi }) {
  if (!TweaksPanel) return null;
  return (
    <TweaksPanel title="Tweaks">
      <TweakSection label="Envelope view" />
      <TweakRadio label="Layout" value={tw.envLayout}
      options={[{ value: "cards", label: "Cards" }, { value: "rows", label: "Rows" }]}
      onChange={(v) => setTweak("envLayout", v)} />
      <TweakRadio label="Sort" value={tw.sort}
      options={[{ value: "order", label: "Order" }, { value: "used", label: "Used" }, { value: "over", label: "Over" }]}
      onChange={(v) => setTweak("sort", v)} />
      <TweakSection label="Readout" />
      <TweakToggle label="Projection markers" value={tw.showProj}
      onChange={(v) => setTweak("showProj", v)} />
      <TweakSection label="Assistant" />
      <TweakToggle label="AI panel open" value={tw.aiOpen}
      onChange={(v) => {setTweak("aiOpen", v);onAi(!v);}} />
    </TweaksPanel>);

}

function BudgetPage() {
  const [tw, setTweak] = useTweaks(BUD_TWEAK_DEFAULTS);

  // ---- backend data ----
  const { data: cycle } = useGet('/cycle/current');
  const { data: catsData, status: catsStatus, loading: catsLoading, res: catsRes, reload: reloadCats } = useGet('/categories');
  const { data: totalsData, reload: reloadTotals } = useGet('/budget/totals');
  const { data: allocData, loading: allocLoading, res: allocRes, reload: reloadAlloc } = useGet('/budget/allocation');

  const C = cycle || {};
  const categories = Array.isArray(catsData) ? catsData : (catsData && (catsData.categories || catsData.items)) || [];
  const totals = totalsData || {};
  const catsReady = catsStatus === 200 && categories.length > 0;

  const [aiCollapsed, setAiCollapsed] = useStateBP(!tw.aiOpen);
  const [sel, setSel] = useStateBP(null);
  const [drawer, setDrawer] = useStateBP(false);
  const [narrow, setNarrow] = useStateBP(typeof window !== "undefined" && window.innerWidth < 1280);

  useEffectBP(() => {
    const on = () => setNarrow(window.innerWidth < 1280);
    on(); window.addEventListener("resize", on);
    return () => window.removeEventListener("resize", on);
  }, []);
  const dockable = !narrow;

  // ---- cap of an envelope = the budget the backend reports for it ----
  const capOf = (c) => (c && c.budget != null ? c.budget : 0);

  // ---- step a cap: PATCH /categories/{name} {delta}, then resync the affected reads ----
  const step = (name, d) =>
    api.patch(`/categories/${encodeURIComponent(name)}`, { delta: d })
      .then(() => { reloadCats(); reloadTotals(); reloadAlloc(); });

  const selectCat = (name) => { setSel(name); if (!dockable) setDrawer(true); };

  // ---- KPI figures: read straight off /budget/totals ----
  const budget = num(totals.budget);
  const allocated = num(totals.allocated);
  const spent = num(totals.spent);
  const projected = num(totals.projected);
  const remaining = num(totals.remaining);
  const overAlloc = num(totals.overAllocated);
  const unallocated = num(totals.unallocated);
  const envelopeCount = totals.envelopeCount != null ? totals.envelopeCount : (catsReady ? categories.length : null);
  const spentPct = (spent != null && budget) ? Math.round((spent / budget) * 100) : null;
  const projOver = (projected != null && budget != null) ? projected - budget : null;
  const daysLeft = C.daysLeft != null ? C.daysLeft : (C.days != null && C.day != null ? C.days - C.day : 0);

  // ---- sort the FETCHED category list per the tweak ----
  const ordered = useMemoBP(() => {
    const arr = [...categories];
    const rank = (c) => {
      const st = budgetStatus(c);
      return st.key === "over" ? 0 : st.key === "willexceed" ? 1 : 2;
    };
    const usedPct = (c) => (c.usedPct != null ? c.usedPct : (c.budget > 0 ? (c.spent || 0) / c.budget : 0));
    if (tw.sort === "used") arr.sort((a, b) => usedPct(b) - usedPct(a));
    else if (tw.sort === "over") arr.sort((a, b) => rank(a) - rank(b) || (b.proj || 0) / (b.budget || 1) - (a.proj || 0) / (a.budget || 1));
    return arr;
  }, [tw.sort, categories]);

  const selCat = sel ? categories.find((c) => c.name === sel) || null : null;
  const selCap = selCat ? capOf(selCat) : 0;
  const showDrawer = !dockable && drawer && selCat;
  const cycleLabel = C.label != null ? C.label : "";

  return (
    <div className="pk" style={{ height: "100vh", minHeight: 0 }}>
      {/* Budgets field — the molten signal sits LOWER-RIGHT, with a faint
           envelope mass upper-left and a light wire scaffold across the top. */}
      <ScannerBg className="pk-bg" seed={63} shapes={[
      { char: "4", cx: .8, cy: .77, scale: .44, style: "red", morph: "vein", live: true, fill: .5 },
      { char: "8", cx: .14, cy: .3, scale: .31, style: "faint", morph: "blob", live: false, fill: .5 },
      { char: "5", cx: .48, cy: .12, scale: .18, style: "wire", morph: "vein", live: false, fill: .36 },
      { char: "2", cx: .92, cy: .18, scale: .14, style: "wire", morph: "vein", live: false, fill: .3 }]
      } />

      <div className="app-shell swap">
        <AiPanel collapsed={aiCollapsed} onToggle={() => setAiCollapsed((c) => !c)} onTrack={() => {}} />

        <div className="app-main">
          <TopBar active="BUDGETS" />
          <div className="app-scroll" data-screen-label="BUDGETS">
            <div className="bud-wrap">

              <div className="bud-top">
                <div>
                  <div className="ttl">Budgets</div>
                  <div className="sum">
                    <b>{envelopeCount != null ? envelopeCount : "—"}</b> envelopes · <b>CHF {budget != null ? chf(budget, 0) : "—"}</b> monthly ·
                    <span className="coral"> {spentPct != null ? spentPct + "% spent" : "—"}</span>
                    {C.label ? <> · {C.label}</> : null}
                    {C.day != null && C.days != null ? <> · day {C.day}/{C.days}</> : null}
                  </div>
                </div>
                <div className="bud-controls">
                  <div className="modes">
                    <span className="mlbl">SORT</span>
                    <button className={"m" + (tw.sort === "order" ? " on" : "")} onClick={() => setTweak("sort", "order")}>ORDER</button>
                    <button className={"m" + (tw.sort === "used" ? " on" : "")} onClick={() => setTweak("sort", "used")}>USED</button>
                    <button className={"m" + (tw.sort === "over" ? " on" : "")} onClick={() => setTweak("sort", "over")}>OVER</button>
                  </div>
                  <div className="modes">
                    <button className={"m" + (tw.envLayout === "cards" ? " on" : "")} onClick={() => setTweak("envLayout", "cards")}>▦ CARDS</button>
                    <button className={"m" + (tw.envLayout === "rows" ? " on" : "")} onClick={() => setTweak("envLayout", "rows")}>≡ ROWS</button>
                  </div>
                </div>
              </div>

              {/* KPI band — values from /budget/totals; "—" until it goes green */}
              <div className="bud-kpis">
                <div className="bud-kpi">
                  <div className="lbl"><span>MONTHLY BUDGET</span><span>{C.label || ""}</span></div>
                  <div className="big"><span className="cur">CHF</span>{budget != null ? chf(budget, 0) : "—"}</div>
                  <div className="sub">{C.days != null ? C.days + "-day cycle · " : ""}{daysLeft} days left</div>
                </div>
                <div className="bud-kpi blue">
                  <div className="lbl"><span>ALLOCATED</span><span>{overAlloc != null && overAlloc > 0 ? "OVER" : "OK"}</span></div>
                  <div className="big"><span className="cur">CHF</span>{allocated != null ? chf(allocated, 0) : "—"}</div>
                  <div className="sub">{overAlloc != null && overAlloc > 0
                    ? "CHF " + chf(overAlloc, 0) + " over budget"
                    : unallocated != null ? "CHF " + chf(unallocated, 0) + " unallocated" : "—"}</div>
                </div>
                <div className="bud-kpi accent">
                  <div className="lbl"><span>SPENT</span><span>{spentPct != null ? spentPct + "%" : "—"}</span></div>
                  <div className="big"><span className="cur">CHF</span>{spent != null ? chf(spent, 0) : "—"}</div>
                  <div className="sub">{remaining != null ? "CHF " + chf(remaining, 0) + " left of monthly budget" : "—"}</div>
                </div>
                <div className="bud-kpi">
                  <div className="lbl"><span>PROJECTED</span><span>{C.endDate || ""}</span></div>
                  <div className="big" style={projOver != null && projOver > 0 ? { color: "var(--neon)", textShadow: "var(--glow-text)" } : null}><span className="cur">CHF</span>{projected != null ? chf(projected, 0) : "—"}</div>
                  <div className="sub" style={projOver != null && projOver > 0 ? { color: "var(--neon-hot)" } : null}>{projOver != null
                    ? (projOver > 0 ? "CHF " + chf(projOver, 0) + " over at current pace" : "CHF " + chf(-projOver, 0) + " under at current pace")
                    : "—"}</div>
                </div>
              </div>

              {/* allocation console */}
              <AllocationBar alloc={allocData} totals={totals} budget={budget} res={allocRes} loading={allocLoading} />

              <div className="bud-sec">
                <span className="lbl">⊞ ENVELOPES</span>
                <span className="ct">{catsReady ? categories.length : "—"}</span>
                <span className="rule" />
                <span className="meta">CAP = THRESHOLD · ━ SIGNAL · ┊ PROJECTION · CLICK TO INSPECT</span>
              </div>

              {!catsReady ? (
                <Awaiting label="ENVELOPES" res={catsRes} loading={catsLoading} tone="blue" />
              ) : tw.envLayout === "rows" ? (
                <div className="env-rows">
                  {ordered.map((c) =>
                    <EnvRow key={c.name} c={c} cap={capOf(c)} active={sel === c.name}
                      onSelect={selectCat} onStep={step} showProj={tw.showProj} />
                  )}
                </div>
              ) : (
                <div className="env-grid">
                  {ordered.map((c) =>
                    <EnvCard key={c.name} c={c} cap={capOf(c)} daysLeft={daysLeft} active={sel === c.name}
                      onSelect={selectCat} onStep={step} showProj={tw.showProj} />
                  )}
                </div>
              )}

            </div>
          </div>
        </div>

        {dockable && <BudgetInspector cat={selCat} cap={selCap} daysLeft={daysLeft} cycleLabel={cycleLabel} onStep={step} onClose={() => setSel(null)} />}
      </div>

      {showDrawer &&
      <div className="sig-drawer-back" onClick={() => setDrawer(false)}>
          <div className="sig-drawer" onClick={(e) => e.stopPropagation()}>
            <BudgetInspector cat={selCat} cap={selCap} daysLeft={daysLeft} cycleLabel={cycleLabel} onStep={step} onClose={() => setDrawer(false)} variant="drawer" />
          </div>
        </div>
      }

      <BudTweaks tw={tw} setTweak={setTweak} onAi={setAiCollapsed} />
    </div>);

}

export default BudgetPage;
