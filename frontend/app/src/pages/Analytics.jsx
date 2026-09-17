import React from 'react'
import { chf } from '../data/phosk.js'
import { ScannerBg, Dot, Spark } from '../components/prims.jsx'
import { TopBar } from '../components/comps.jsx'
import { SigSpark, AiPanel, SignalPanel } from '../components/shell.jsx'
import { Awaiting } from '../components/states.jsx'
import { api, useGet } from '../lib/api.js'
import { useTweaks, TweaksPanel, TweakSection, TweakRadio, TweakToggle, TweakSelect } from '../lib/tweaks.jsx'

/* =========================================================================
   ANALYTICS — wired to phosk_api.
   The retrospective read: a 12-cycle SPEND TREND oscilloscope, the
   ITEM-SIGNAL matrix, per-category MOMENTUM small-multiples, and a weekday
   SPENDING RHYTHM. All data is FETCHED (useGet); every endpoint returns 501
   until the backend goes green, so each surface falls back to <Awaiting/>.
   Presentation helpers below are pure — they read FETCHED records as props
   and never recompute roll-ups the backend already provides.
   ========================================================================= */

/* ============================ SPEND TREND (hero) ========================== */
/* A multi-cycle scope: monthly spend as molten bars under a connecting trace,
   a dashed budget reference, and the current cycle drawn hollow (projected).
   mode 'rate' swaps to the cashflow savings-rate curve.
   `points` come from /analytics/spend-history; `stats` from .../stats. The
   page slices `points` client-side for the trend-window tweak (6 vs 12). */
function SpendTrend({ points = [], stats = {}, mode = 'spend' }) {
  const data = Array.isArray(points) ? points : [];
  const S = stats || {};
  const W = 1000, H = 212, padL = 52, padR = 20, padT = 24, padB = 30;
  const base = H - padB;
  const n = data.length;
  const slot = n > 0 ? (W - padL - padR) / n : (W - padL - padR);
  const cx = (i) => padL + slot * (i + 0.5);

  const isRate = mode === 'rate';
  // Budget line: prefer first point's budget; fall back to stats / max spend.
  const budgetLine = (data[0] && data[0].budget) || 0;
  const maxY = isRate
    ? (Math.max(0.0001, ...data.map((d) => d.rate || 0)) * 1.25)
    : (Math.max(budgetLine, ...data.map((d) => d.spend || 0), 1) * 1.08);
  const val = (d) => (isRate ? (d.rate || 0) : (d.spend || 0));
  const y = (v) => base - (v / maxY) * (base - padT);

  // y gridlines
  const lines = isRate
    ? [0.1, 0.2, 0.3].map((v) => ({ v, lab: Math.round(v * 100) + '%' }))
    : [2000, 4000].map((v) => ({ v, lab: (v / 1000) + 'k' }));

  const linePts = data.map((d, i) => `${cx(i).toFixed(1)},${y(val(d)).toFixed(1)}`).join(' ');
  const areaPts = n > 0 ? `${cx(0)},${base} ${linePts} ${cx(n - 1)},${base}` : '';
  const barW = Math.min(slot * 0.5, 30);
  const budgetY = y(budgetLine);

  const first = data[0] || {};
  const cur = S.cur || data[data.length - 1] || {};
  const peak = S.peak || {};
  const low = S.low || {};
  const num = (v) => (v == null || !isFinite(Number(v)) ? '—' : chf(v, 0));

  return (
    <div className="atrend osc-bkt blue">
      <span className="osc-leg">{isRate ? 'SAVINGS-RATE TREND' : 'SPEND TREND'}</span>
      <div className="atrend-h">
        <span className="hud">⌁ {isRate ? 'CASHFLOW SAVED' : 'SPEND'} · LAST {n} CYCLES{n > 0 ? ` · ${first.m || ''}${first.yr || ''} → ${cur.m || ''}${cur.yr || ''}` : ''}</span>
        <div className="atrend-key">
          {!isRate && <span><i className="k spend" /> SPEND</span>}
          {!isRate && <span><i className="k bud" /> BUDGET {num(budgetLine)}</span>}
          {isRate && <span><i className="k rate" /> SAVED / INCOME</span>}
          <span><i className="k proj" /> CURRENT · PROJECTED</span>
        </div>
      </div>

      <svg width="100%" height={H} viewBox={`0 0 ${W} ${H}`} preserveAspectRatio="none" className="atrend-svg">
        <defs>
          <linearGradient id="atrendfill" x1="0" y1="0" x2="0" y2="1">
            <stop offset="0%" stopColor={isRate ? 'rgba(143,125,255,.26)' : 'rgba(255,94,77,.28)'} />
            <stop offset="100%" stopColor={isRate ? 'rgba(143,125,255,0)' : 'rgba(255,94,77,0)'} />
          </linearGradient>
        </defs>

        {/* gridlines */}
        {lines.map((g) => (
          <g key={g.v}>
            <line x1={padL} y1={y(g.v)} x2={W - padR} y2={y(g.v)} stroke="rgba(106,95,192,.16)" strokeWidth="1" strokeDasharray="2 4" />
            <text x={padL - 8} y={y(g.v) + 3} textAnchor="end" fill="var(--ink-3)" fontSize="8.5" fontFamily="var(--font-body)" letterSpacing=".04em">{g.lab}</text>
          </g>
        ))}
        <line x1={padL} y1={base} x2={W - padR} y2={base} stroke="rgba(106,95,192,.4)" strokeWidth="1" />

        {/* budget reference (spend mode only) */}
        {!isRate && budgetLine > 0 && (
          <g>
            <line x1={padL} y1={budgetY} x2={W - padR} y2={budgetY} stroke="var(--indigo-neon)" strokeWidth="1.2" strokeDasharray="5 5" opacity=".75" />
            <text x={W - padR} y={budgetY - 4} textAnchor="end" fill="var(--indigo-neon)" fontSize="8.5" fontFamily="var(--font-body)" letterSpacing=".1em">BUDGET</text>
          </g>
        )}

        {/* bars */}
        {data.map((d, i) => {
          const bx = cx(i) - barW / 2, by = y(val(d)), bh = base - by;
          const proj = d.projected;
          const over = !isRate && d.over;
          const col = isRate ? 'rgba(143,125,255,.42)' : over ? 'rgba(255,59,46,.5)' : 'rgba(255,94,77,.34)';
          if (proj) {
            return (
              <g key={i}>
                <rect x={bx} y={by} width={barW} height={Math.max(0, bh)} fill="none"
                  stroke={isRate ? 'var(--indigo-neon)' : 'var(--neon)'} strokeWidth="1.3" strokeDasharray="3 2" opacity=".9" />
                <rect x={bx} y={by} width={barW} height={Math.max(0, bh)} fill={isRate ? 'rgba(143,125,255,.1)' : 'rgba(255,94,77,.1)'} />
              </g>
            );
          }
          return <rect key={i} x={bx} y={by} width={barW} height={Math.max(0, bh)} fill={col} />;
        })}

        {/* connecting trace */}
        {n > 0 && <polygon points={areaPts} fill="url(#atrendfill)" />}
        {n > 0 && <polyline points={linePts} fill="none" stroke={isRate ? 'var(--indigo-neon)' : 'var(--neon)'} strokeWidth="2.2"
          strokeLinejoin="round" strokeLinecap="round" style={{ filter: `drop-shadow(0 0 3px ${isRate ? 'var(--indigo-neon)' : 'var(--neon)'})` }} />}

        {/* point markers + current */}
        {data.map((d, i) => {
          const last = i === n - 1;
          return <circle key={i} cx={cx(i)} cy={y(val(d))} r={last ? 4 : 2.4}
            fill={last ? 'var(--neon-white)' : (isRate ? 'var(--indigo-neon)' : 'var(--neon)')}
            style={last ? { filter: `drop-shadow(0 0 4px ${isRate ? 'var(--indigo-neon)' : 'var(--neon)'})` } : {}} />;
        })}

        {/* x labels */}
        {data.map((d, i) => (
          <text key={i} x={cx(i)} y={base + 16} textAnchor="middle"
            fill={d.projected ? 'var(--neon-dim)' : 'var(--ink-3)'} fontSize="8.5" fontFamily="var(--font-body)" letterSpacing=".06em">{d.m}</text>
        ))}
      </svg>

      <div className="atrend-foot">
        <span className="tf"><i>6-MO AVG</i> <b>CHF {num(S.avg)}</b></span>
        <span className="tf"><i>PEAK</i> <b className="warn">CHF {num(peak.spend)}</b> <em>{peak.m} {peak.yr}</em></span>
        <span className="tf"><i>LEANEST</i> <b style={{ color: 'var(--ok)' }}>CHF {num(low.spend)}</b> <em>{low.m} {low.yr}</em></span>
        {S.curVsAvgPct != null && (
          <span className="tf-note">
            <Dot tone={S.curVsAvgPct > 0 ? 'alert' : 'ok'} size={6} />
            THIS CYCLE TRACKING {S.curVsAvgPct > 0 ? '+' : '−'}{Math.abs(S.curVsAvgPct)}% vs 6-MO AVG{S.curVsPrevPct != null ? ` · ${S.curVsPrevPct > 0 ? '↑' : '↓'} ${Math.abs(S.curVsPrevPct)}% vs ${S.prev ? S.prev.m : ''}` : ''}
          </span>
        )}
      </div>
    </div>
  );
}

/* ============================ ITEM-SIGNAL MATRIX ROW ======================
   Pure row over a FETCHED signal record. Candidate rows call onTrack(id);
   tracked rows call onSelect(id). */
function ItemSignalRow({ sig, active, onSelect, onTrack, rank }) {
  const cand = !!sig.candidate;
  const up = (sig.deltaPct || 0) >= 0;
  const delta = cand ? 'NEW' : (up ? '↑' : '↓') + Math.abs(sig.deltaPct || 0) + '%';
  const click = () => { if (cand && onTrack) onTrack(sig.id); else onSelect(sig.id); };
  return (
    <button className={'isig' + (active ? ' on' : '') + (cand ? ' cand' : '')} onClick={click}>
      <span className="isig-rk">{cand ? '—' : rank}</span>
      <span className="isig-gl">⌁</span>
      <div className="isig-id">
        <span className="isig-nm">{sig.label}</span>
        <span className="isig-pa">{cand ? sig.desc : sig.parent + ' · since ' + sig.since}</span>
      </div>
      <div className="isig-spk"><SigSpark data={sig.series || []} w={150} h={38} /></div>
      <span className={'isig-dl ' + (cand ? 'cand' : up ? 'up' : 'down')}>{delta}</span>
      <div className="isig-num">
        <span className="v">{sig.cycleQty} <i>{sig.unit}</i></span>
        <span className="s">{cand ? 'this cycle' : 'CHF ' + chf(sig.cycleSpend) + ' · ' + sig.txns + ' txns'}</span>
      </div>
      <span className="isig-go">{cand ? 'TRACK ▸' : '▸'}</span>
    </button>
  );
}

/* ============================ SIGNAL MOVER CARD ===========================
   Pure card over a FETCHED mover record from /signals/movers. */
function MoverCard({ sig, kind }) {
  const up = kind === 'riser';
  return (
    <div className={'mover ' + (up ? 'up' : 'down')}>
      <div className="mover-h">
        <span className="lbl">{up ? '↑ FASTEST RISER' : '↓ FASTEST FALLER'}</span>
        <span className={'dl ' + (up ? 'up' : 'down')}>{(sig.deltaPct || 0) > 0 ? '+' : ''}{sig.deltaPct}%</span>
      </div>
      <div className="mover-nm">{sig.label}</div>
      <div className="mover-spk"><SigSpark data={sig.series || []} w={232} h={44} /></div>
      <div className="mover-sub">{sig.cycleQty} {sig.unit} · CHF {chf(sig.cycleSpend)} this cycle · {sig.parent}</div>
    </div>
  );
}

/* ============================ CATEGORY MOMENTUM ===========================
   Pure card over a FETCHED momentum record from /analytics/category-momentum:
   {name,now,budget,series[12],deltaPct,priorAvg,fixed}. */
function MomentumCard({ c }) {
  const up = (c.deltaPct || 0) >= 0;
  const strong = Math.abs(c.deltaPct || 0) >= 15;
  const tone = c.fixed ? 'flat' : up ? (strong ? 'hot' : 'up') : 'down';
  return (
    <div className={'momo ' + tone}>
      <div className="momo-h">
        <span className="cn">{c.name}</span>
        <span className={'dl ' + tone}>{c.fixed ? 'FIXED' : (up ? '↑' : '↓') + Math.abs(c.deltaPct || 0) + '%'}</span>
      </div>
      <div className="momo-spk">
        <Spark data={(c.series && c.series.length ? c.series : [0, 0])} w={150} h={30} tone={!c.fixed && up && strong ? 'neon' : 'indigo'} />
      </div>
      <div className="momo-f">
        <span className="now">CHF {chf(c.now, 0)}</span>
        <span className="vs">vs 3-cyc avg</span>
      </div>
    </div>
  );
}

/* ============================ SPENDING RHYTHM =============================
   Pure heatmap over FETCHED /analytics/rhythm/weekday:
   {weekday:[{d,v}], stats:{peak,max,total,avg,weekendShare}}. */
function RhythmHeatmap({ weekday = [], stats = {} }) {
  const wd = Array.isArray(weekday) ? weekday : [];
  const S = stats || {};
  const max = S.max != null ? S.max : Math.max(1, ...wd.map((x) => x.v || 0));
  const peakDay = S.peak && S.peak.d ? S.peak.d : (wd.reduce((a, x) => ((x.v || 0) > (a.v || 0) ? x : a), wd[0] || {}).d);
  return (
    <div className="rhythm osc-bkt coral">
      <span className="osc-leg">SPENDING RHYTHM</span>
      <div className="rhythm-h">
        <span className="hud">∿ DISCRETIONARY SPEND · AVG BY WEEKDAY</span>
        <span className="rhythm-meta">{S.weekendShare != null ? S.weekendShare + '%' : '—'} LANDS FRI–SUN · RENT &amp; INSURANCE EXCLUDED</span>
      </div>
      <div className="rhythm-grid">
        {wd.map((x) => {
          const p = max > 0 ? (x.v || 0) / max : 0;
          const peak = x.d === peakDay;
          return (
            <div className={'rcol' + (peak ? ' peak' : '')} key={x.d}>
              <span className="rv">CHF {x.v}</span>
              <div className="rbar-wrap">
                <div className="rbar" style={{ height: Math.max(6, p * 100) + '%' }} />
              </div>
              <span className="rd">{x.d}</span>
            </div>
          );
        })}
      </div>
    </div>
  );
}

/* =========================================================================
   PAGE COMPONENT
   ========================================================================= */
const { useState: useStateA, useMemo: useMemoA, useEffect: useEffectA } = React;

const AN_TWEAK_DEFAULTS = /*EDITMODE-BEGIN*/{
  "trendWindow": "12",
  "trendMode": "spend",
  "sigSort": "momentum",
  "showCand": true,
  "showMomentum": true,
  "momentumSort": "momentum",
  "showRhythm": true,
  "sigInsp": "dock",
  "aiOpen": true
} /*EDITMODE-END*/;

function AnalyticsTweaks({ tw, setTweak, onAi }) {
  if (!TweaksPanel) return null;
  return (
    <TweaksPanel title="Tweaks">
      <TweakSection label="Spend trend" />
      <TweakRadio label="Window" value={tw.trendWindow}
        options={[{ value: "12", label: "12 cyc" }, { value: "6", label: "6 cyc" }]}
        onChange={(v) => setTweak("trendWindow", v)} />
      <TweakRadio label="Series" value={tw.trendMode}
        options={[{ value: "spend", label: "Spend" }, { value: "rate", label: "Savings" }]}
        onChange={(v) => setTweak("trendMode", v)} />
      <TweakSection label="Item-signals" />
      <TweakSelect label="Sort" value={tw.sigSort}
        options={[
          { value: "momentum", label: "Momentum" },
          { value: "spend", label: "Spend" },
          { value: "az", label: "A–Z" },
        ]}
        onChange={(v) => setTweak("sigSort", v)} />
      <TweakToggle label="Show candidate signal" value={tw.showCand}
        onChange={(v) => setTweak("showCand", v)} />
      <TweakSection label="Category momentum" />
      <TweakToggle label="Show section" value={tw.showMomentum}
        onChange={(v) => setTweak("showMomentum", v)} />
      <TweakSelect label="Order" value={tw.momentumSort}
        options={[
          { value: "momentum", label: "Most movement" },
          { value: "spend", label: "Largest spend" },
          { value: "az", label: "A–Z" },
        ]}
        onChange={(v) => setTweak("momentumSort", v)} />
      <TweakSection label="Spending rhythm" />
      <TweakToggle label="Show weekday rhythm" value={tw.showRhythm}
        onChange={(v) => setTweak("showRhythm", v)} />
      <TweakSection label="Detail" />
      <TweakRadio label="Inspector" value={tw.sigInsp}
        options={[{ value: "dock", label: "Dock" }, { value: "drawer", label: "Drawer" }]}
        onChange={(v) => setTweak("sigInsp", v)} />
      <TweakSection label="Assistant" />
      <TweakToggle label="AI panel open" value={tw.aiOpen}
        onChange={(v) => { setTweak("aiOpen", v); onAi(!v); }} />
    </TweaksPanel>
  );
}

function AnalyticsPage() {
  const [tw, setTweak] = useTweaks(AN_TWEAK_DEFAULTS);

  const [aiCollapsed, setAiCollapsed] = useStateA(!tw.aiOpen);
  const [sel, setSel] = useStateA(null);
  const [drawer, setDrawer] = useStateA(false);
  const [narrow, setNarrow] = useStateA(typeof window !== "undefined" && window.innerWidth < 1280);

  useEffectA(() => {
    const on = () => setNarrow(window.innerWidth < 1280);
    on(); window.addEventListener("resize", on);
    return () => window.removeEventListener("resize", on);
  }, []);

  /* ---- DATA: every surface fetches its own endpoint ---- */
  const cycle = useGet('/cycle/current');
  const cycleLabel = (cycle.data && cycle.data.label) || 'THIS CYCLE';

  const spendHistory = useGet('/analytics/spend-history');
  const histStats = useGet('/analytics/spend-history/stats');
  const momentum = useGet('/analytics/category-momentum');
  const rhythm = useGet('/analytics/rhythm/weekday');
  const insights = useGet('/analytics/insights/movers');
  const movers = useGet('/signals/movers');

  const signalsGet = useGet('/signals');
  const candidatesGet = useGet('/signals/candidates');

  // Selected signal detail (fetched on demand; key in deps so it refetches).
  const selDetail = useGet(sel ? `/signals/${encodeURIComponent(sel)}` : '/signals', undefined, [sel]);

  const dockable = !narrow && tw.sigInsp === "dock";
  const selectSig = (id) => { setSel(id === sel ? null : id); if (id !== sel && !dockable) setDrawer(true); };

  /* ---- derive collections (defensive: never assume shape) ---- */
  const histPoints = (spendHistory.data && spendHistory.data.points) || [];
  const windowN = parseInt(tw.trendWindow, 10) || 12;
  const trendPoints = histPoints.slice(Math.max(0, histPoints.length - windowN));
  const S = histStats.data || {};

  const moversData = movers.data || {};
  const moversAll = Array.isArray(moversData.all) ? moversData.all : [];

  // item-signals (tracked) + optional candidate, sorted by the sigSort tweak.
  const trackedRaw = Array.isArray(signalsGet.data)
    ? signalsGet.data
    : (signalsGet.data && (signalsGet.data.signals || signalsGet.data.items)) || [];
  const candidatesRaw = Array.isArray(candidatesGet.data)
    ? candidatesGet.data
    : (candidatesGet.data && (candidatesGet.data.candidates || candidatesGet.data.items)) || [];

  const signals = useMemoA(() => {
    const arr = [...trackedRaw];
    if (tw.sigSort === "spend") arr.sort((a, b) => (b.cycleSpend || 0) - (a.cycleSpend || 0));
    else if (tw.sigSort === "az") arr.sort((a, b) => String(a.label).localeCompare(String(b.label)));
    else arr.sort((a, b) => (b.deltaPct || 0) - (a.deltaPct || 0));
    return arr;
  }, [trackedRaw, tw.sigSort]);

  const cats = useMemoA(() => {
    const arr = [...(Array.isArray(momentum.data) ? momentum.data : [])];
    if (tw.momentumSort === "spend") arr.sort((a, b) => (b.now || 0) - (a.now || 0));
    else if (tw.momentumSort === "az") arr.sort((a, b) => String(a.name).localeCompare(String(b.name)));
    else arr.sort((a, b) => (b.fixed ? -999 : Math.abs(b.deltaPct || 0)) - (a.fixed ? -999 : Math.abs(a.deltaPct || 0)));
    return arr;
  }, [momentum.data, tw.momentumSort]);

  // The signal inspector reads the on-demand detail fetch (only valid when a
  // signal is selected AND the detail endpoint actually served that id).
  const selSig = (sel && selDetail.status === 200 && selDetail.data && !Array.isArray(selDetail.data)) ? selDetail.data : null;
  const showDrawer = !dockable && drawer && sel;

  const sigCount = trackedRaw.length;
  const candCount = candidatesRaw.length;

  /* ---- ACTIONS ---- */
  // Track a candidate item-signal → POST /signals {candidateId} → reload lists.
  const trackCandidate = (id) => {
    api.post('/signals', { candidateId: id }).then(() => {
      signalsGet.reload();
      candidatesGet.reload();
    });
  };
  // Re-fetch the tracked+candidate lists (passed to SignalPanel as onChanged so
  // its in-built track/dismiss resync the matrix).
  const reloadSignals = () => { signalsGet.reload(); candidatesGet.reload(); if (sel) selDetail.reload(); };

  // Apply the GEMMA4-suggested soft cap → POST /signals/{id}/cap {amount,period}.
  const ins = insights.data || {};
  const suggestedCap = ins.suggestedCap || null;
  const applyCap = () => {
    if (!suggestedCap || !suggestedCap.signalId) return;
    api.post(`/signals/${encodeURIComponent(suggestedCap.signalId)}/cap`, {
      amount: suggestedCap.amount, period: 'cycle',
    }).then(() => { signalsGet.reload(); insights.reload(); if (sel) selDetail.reload(); });
  };

  // KPI helpers: em-dash when missing rather than NaN.
  const curSpend = S.cur ? S.cur.spend : null;
  const riser = moversData.riser || null;

  return (
    <div className="pk" style={{ height: "100vh", minHeight: 0 }}>
      {/* Analytics field — a wide READ-OUT sweep. */}
      <ScannerBg className="pk-bg" seed={211} shapes={[
        { char: "2", cx: .2, cy: .78, scale: .5, style: "red", morph: "vein", live: true, fill: .5 },
        { char: "8", cx: .88, cy: .26, scale: .44, style: "faint", morph: "blob", live: false, fill: .62 },
        { char: "e", cx: .52, cy: .5, scale: .24, style: "wire", morph: "vein", live: false, fill: .4 },
        { char: "5", cx: .07, cy: .2, scale: .26, style: "faint", morph: "blob", live: false, fill: .42 },
        { char: "3", cx: .7, cy: .9, scale: .18, style: "wire", morph: "vein", live: false, fill: .35 }
      ]} />

      <div className="app-shell swap">
        <AiPanel collapsed={aiCollapsed} onToggle={() => setAiCollapsed((c) => !c)} onTrack={selectSig} />

        <div className="app-main">
          <TopBar active="ANALYTICS" />
          <div className="app-scroll" data-screen-label="ANALYTICS">
            <div className="an-wrap">

              <div className="an-top">
                <div>
                  <div className="ttl">Analytics</div>
                  <div className="sum">
                    <b>{S.months != null ? S.months : '—'}</b> cycles on record · trending <span className="coral">CHF {curSpend != null ? chf(curSpend, 0) : '—'}</span> this cycle ·
                    <b> {sigCount}</b> item-signals tracked · AI-maintained
                  </div>
                </div>
                <div className="an-modes">
                  <button className={"m" + (tw.trendMode === "spend" ? " on" : "")} onClick={() => setTweak("trendMode", "spend")}>∿ SPEND</button>
                  <button className={"m" + (tw.trendMode === "rate" ? " on" : "")} onClick={() => setTweak("trendMode", "rate")}>⌁ SAVINGS</button>
                </div>
              </div>

              {/* KPI band */}
              <div className="an-kpis">
                <div className="akpi accent">
                  <div className="lbl"><span>THIS CYCLE</span><span>RUN-RATE</span></div>
                  <div className="big"><span className="cur">CHF</span>{curSpend != null ? chf(curSpend, 0) : '—'}</div>
                  <div className="sub">
                    {S.curVsAvgPct != null
                      ? <><span className={S.curVsAvgPct > 0 ? "up" : "dn"}>{S.curVsAvgPct > 0 ? "↑" : "↓"} {Math.abs(S.curVsAvgPct)}%</span> vs 6-mo avg</>
                      : <span className="dim">awaiting backend</span>}
                  </div>
                </div>
                <div className="akpi blue">
                  <div className="lbl"><span>6-MONTH AVG</span><span>SPEND</span></div>
                  <div className="big"><span className="cur">CHF</span>{S.avg != null ? chf(S.avg, 0) : '—'}</div>
                  <div className="sub">
                    budget CHF {S.cur && S.cur.budget != null ? chf(S.cur.budget, 0) : '—'}
                    {S.peak ? <> · peak {S.peak.m} {chf(S.peak.spend, 0)}</> : null}
                  </div>
                </div>
                <div className="akpi blue bluebig">
                  <div className="lbl"><span>SAVINGS RATE</span><span>CASHFLOW</span></div>
                  <div className="big">{S.avgRate != null ? Math.round(S.avgRate * 100) : '—'}<span className="cur" style={{ marginLeft: 2 }}>%</span></div>
                  <div className="sub">avg of income{S.totalSaved != null ? <> · CHF {chf(S.totalSaved, 0)} saved over {S.months} cyc</> : null}</div>
                </div>
                <div className="akpi">
                  <div className="lbl"><span>ITEM-SIGNALS</span><span>TRACKED</span></div>
                  <div className="big">{sigCount || '—'}</div>
                  <div className="sub">
                    {candCount > 0 ? `+${candCount} candidate · ` : ''}
                    {riser ? <>{riser.label} ↑{riser.deltaPct}% leads</> : 'awaiting movers'}
                  </div>
                </div>
              </div>

              {/* SPEND TREND hero */}
              {spendHistory.status === 200 && histPoints.length > 0
                ? <SpendTrend points={trendPoints} stats={S} mode={tw.trendMode} />
                : <Awaiting label="SPEND TREND" res={spendHistory.res} loading={spendHistory.loading} tone="blue" />}

              {/* ITEM-SIGNALS */}
              <div className="an-sec">
                <span className="lbl">⌁ ITEM-SIGNALS</span>
                <span className="ct">{sigCount}</span>
                <span className="rule" />
                <span className="meta">12-MO TREND · CLICK TO INSPECT</span>
              </div>

              {signalsGet.status === 200 && (signals.length > 0 || (tw.showCand && candidatesRaw.length > 0))
                ? (
                  <div className="isig-list">
                    {signals.map((s, i) => (
                      <ItemSignalRow key={s.id} sig={s} rank={i + 1} active={sel === s.id} onSelect={selectSig} onTrack={trackCandidate} />
                    ))}
                    {tw.showCand && candidatesRaw.map((s) => (
                      <ItemSignalRow key={'cand-' + s.id} sig={{ ...s, candidate: true }} active={sel === s.id} onSelect={selectSig} onTrack={trackCandidate} />
                    ))}
                  </div>
                )
                : <Awaiting label="ITEM-SIGNALS" res={signalsGet.res} loading={signalsGet.loading} tone="blue" />}

              <div className="an-movers">
                {movers.status === 200 && moversData.riser
                  ? <MoverCard sig={moversData.riser} kind="riser" />
                  : <Awaiting label="FASTEST RISER" res={movers.res} loading={movers.loading} tone="coral" />}
                {movers.status === 200 && moversData.faller
                  ? <MoverCard sig={moversData.faller} kind="faller" />
                  : <Awaiting label="FASTEST FALLER" res={movers.res} loading={movers.loading} tone="blue" />}
                <div className="an-insight">
                  <div className="ih"><Dot tone="blue" size={6} />{(ins.model || 'GEMMA4')} · READ</div>
                  {insights.status === 200 && ins.text
                    ? (
                      <>
                        <div className="q">{ins.text}</div>
                        {suggestedCap && suggestedCap.signalId && (
                          <div style={{ marginTop: 10 }}>
                            <button className="gbtn p" onClick={applyCap}>
                              CAP {suggestedCap.signalId} AT CHF {chf(suggestedCap.amount, 0)}
                              {suggestedCap.projectedSavings != null ? ` · +CHF ${chf(suggestedCap.projectedSavings, 0)} SAVED` : ''}
                            </button>
                          </div>
                        )}
                      </>
                    )
                    : <Awaiting label="GEMMA4 READ" res={insights.res} loading={insights.loading} tone="blue" />}
                </div>
              </div>

              {/* CATEGORY MOMENTUM */}
              {tw.showMomentum && (
                <>
                  <div className="an-sec">
                    <span className="lbl">▦ CATEGORY MOMENTUM</span>
                    <span className="ct">{cats.length}</span>
                    <span className="rule" />
                    <span className="meta">NOW vs 3-CYCLE AVG · ↑ HEATING · ↓ COOLING</span>
                  </div>
                  {momentum.status === 200 && cats.length > 0
                    ? (
                      <div className="momo-grid">
                        {cats.map((c) => <MomentumCard key={c.name} c={c} />)}
                      </div>
                    )
                    : <Awaiting label="CATEGORY MOMENTUM" res={momentum.res} loading={momentum.loading} tone="blue" />}
                </>
              )}

              {/* SPENDING RHYTHM */}
              {tw.showRhythm && (
                <>
                  <div className="an-sec">
                    <span className="lbl">∿ SPENDING RHYTHM</span>
                    <span className="rule" />
                    <span className="meta">WHEN IN THE WEEK IT GOES</span>
                  </div>
                  {rhythm.status === 200 && rhythm.data && Array.isArray(rhythm.data.weekday) && rhythm.data.weekday.length > 0
                    ? <RhythmHeatmap weekday={rhythm.data.weekday} stats={rhythm.data.stats} />
                    : <Awaiting label="SPENDING RHYTHM" res={rhythm.res} loading={rhythm.loading} tone="coral" />}
                </>
              )}

            </div>
          </div>
        </div>

        {dockable && <SignalPanel sig={selSig} onClose={() => setSel(null)} cycleLabel={cycleLabel} onChanged={reloadSignals} />}
      </div>

      {showDrawer && (
        <div className="sig-drawer-back" onClick={() => setDrawer(false)}>
          <div className="sig-drawer" onClick={(e) => e.stopPropagation()}>
            <SignalPanel sig={selSig} onClose={() => setDrawer(false)} variant="drawer" cycleLabel={cycleLabel} onChanged={reloadSignals} />
          </div>
        </div>
      )}

      <AnalyticsTweaks tw={tw} setTweak={setTweak} onAi={setAiCollapsed} />
    </div>
  );
}

export default AnalyticsPage
