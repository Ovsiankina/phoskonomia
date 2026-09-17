import React from 'react'
import { chf } from '../data/phosk.js'
import { ScannerBg, Dot, Spark } from '../components/prims.jsx'
import { TopBar } from '../components/comps.jsx'
import { AiPanel } from '../components/shell.jsx'
import { Awaiting } from '../components/states.jsx'
import { api, useGet } from '../lib/api.js'
import { useTweaks, TweaksPanel, TweakSection, TweakRadio, TweakToggle, TweakSelect } from '../lib/tweaks.jsx'

/* ============================ PURE PRESENTATION HELPERS ==================== */
/* Phoskonomia — Debts. Outstanding balances read as a DECAYING WAVEFORM: each
   debt is a balance damping toward a zero baseline as it's paid down. The hero
   plots the combined balance envelope from history through today to the
   projected debt-free point. All amortization is derived by the backend — these
   helpers only FORMAT (status→tone, calendar labels), they never compute money. */

// status → tone/label used across the page (pure, on a fetched debt record).
function debtStatus(d) {
  const s = d ? d.status : null;
  if (s === "high")  return { key: "high",  label: "HIGH INTEREST", tone: "coral" };
  if (s === "due")   return { key: "due",   label: "DUE SOON",      tone: "warn" };
  if (s === "watch") return { key: "watch", label: "REVIEW",        tone: "warn" };
  return { key: "ok", label: "ON TRACK", tone: "blue" };
}

// month label N months ahead of the current cycle, anchored on the fetched
// cycle's month (cycle.startDate or cycle.label). Pure calendar formatting.
const MONTHS = ["JAN", "FEB", "MAR", "APR", "MAY", "JUN", "JUL", "AUG", "SEP", "OCT", "NOV", "DEC"];
function cycleAnchor(cycle) {
  // Prefer an ISO startDate; fall back to parsing a "MON YYYY"-ish label; else now.
  const c = cycle || {};
  if (c.startDate) {
    const dt = new Date(c.startDate);
    if (!isNaN(dt)) return { m: dt.getMonth(), y: dt.getFullYear() };
  }
  if (typeof c.label === "string") {
    const parts = c.label.toUpperCase().split(/[^A-Z0-9]+/).filter(Boolean);
    const mi = parts.findIndex((p) => MONTHS.indexOf(p) >= 0);
    if (mi >= 0) {
      const m = MONTHS.indexOf(parts[mi]);
      const yp = parts.find((p) => /^\d{4}$/.test(p));
      const y = yp ? Number(yp) : new Date().getFullYear();
      return { m, y };
    }
  }
  const now = new Date();
  return { m: now.getMonth(), y: now.getFullYear() };
}
function monthLabel(offset, cycle) {
  const a = cycleAnchor(cycle);
  let m = a.m + (offset | 0), y = a.y;
  while (m > 11) { m -= 12; y++; }
  while (m < 0) { m += 12; y--; }
  const yy = y % 100;
  return MONTHS[m] + " " + (yy < 10 ? "0" : "") + yy;
}

/* ============================ PAYOFF TRAJECTORY (hero) ===================== */
/* Source: GET /debts/trajectory { points[]{m,total}, xTicks[], debtFreeLabel }
   plus /debts/stats for the footer roll-ups. The SVG math is kept verbatim. */
function PayoffTrajectory({ showProjection = true, strategy = "none", traj, stats, cycle, res, loading }) {
  const points = (traj && Array.isArray(traj.points)) ? traj.points : [];
  const S = stats || {};
  if ((res && res.status !== 200) || !points.length) {
    return (
      <div className="traj osc-bkt blue">
        <span className="osc-leg">PAYOFF TRAJECTORY</span>
        <Awaiting label="COMBINED BALANCE DECAY" res={res} loading={loading} tone="blue" />
      </div>
    );
  }
  const debtFreeLabel = traj.debtFreeLabel || S.debtFreeLabel || "—";
  const W = 1000, H = 196, padL = 54, padR = 64, padT = 26, padB = 34;
  const base = H - padB;
  const mMin = points[0].m, mMax = points[points.length - 1].m;
  const span = (mMax - mMin) || 1;
  const maxY = Math.max(1, ...points.map((p) => p.total)) * 1.06;
  const x = (m) => padL + (m - mMin) / span * (W - padL - padR);
  const y = (v) => base - (v / maxY) * (base - padT);

  const histPts = points.filter((p) => p.m <= 0);
  const projPts = points.filter((p) => p.m >= 0);
  const toPts = (arr) => arr.map((p) => `${x(p.m).toFixed(1)},${y(p.total).toFixed(1)}`).join(" ");
  const histLine = toPts(histPts);
  const projLine = toPts(projPts);
  const histArea = histPts.length ? `${x(histPts[0].m)},${base} ${histLine} ${x(0)},${base}` : "";
  const projArea = projPts.length ? `${x(0)},${base} ${projLine} ${x(mMax)},${base}` : "";

  // y gridlines at round franc levels
  const step = maxY > 30000 ? 10000 : 5000;
  const lines = [];
  for (let v = step; v < maxY; v += step) lines.push(v);

  // x ticks: prefer backend xTicks, else today/+12/+24/debt-free
  const ticks = (Array.isArray(traj.xTicks) && traj.xTicks.length)
    ? traj.xTicks
    : (() => { const t = [0]; if (mMax >= 12) t.push(12); if (mMax >= 24) t.push(24); t.push(mMax); return t; })();

  const dfX = x(mMax), dfY = y(0);
  const todayPt = points.find((p) => p.m === 0);

  return (
    <div className="traj osc-bkt blue">
      <span className="osc-leg">PAYOFF TRAJECTORY</span>
      <div className="traj-h">
        <span className="hud">∿ COMBINED BALANCE DECAY · {(cycle && cycle.label) || "—"} → {debtFreeLabel}</span>
        <div className="traj-key">
          <span><i className="k owed" /> OWED</span>
          <span><i className="k proj" /> PROJECTED</span>
          <span><i className="k free" /> DEBT-FREE</span>
        </div>
      </div>

      <svg width="100%" height={H} viewBox={`0 0 ${W} ${H}`} preserveAspectRatio="none" className="traj-svg">
        <defs>
          <linearGradient id="trajfill" x1="0" y1="0" x2="0" y2="1">
            <stop offset="0%" stopColor="rgba(255,94,77,.30)" />
            <stop offset="100%" stopColor="rgba(255,94,77,0)" />
          </linearGradient>
          <linearGradient id="trajproj" x1="0" y1="0" x2="0" y2="1">
            <stop offset="0%" stopColor="rgba(143,125,255,.20)" />
            <stop offset="100%" stopColor="rgba(143,125,255,0)" />
          </linearGradient>
        </defs>

        {/* y gridlines */}
        {lines.map((v) => (
          <g key={v}>
            <line x1={padL} y1={y(v)} x2={W - padR} y2={y(v)} stroke="rgba(106,95,192,.16)" strokeWidth="1" strokeDasharray="2 4" />
            <text x={padL - 7} y={y(v) + 3} textAnchor="end" fill="var(--ink-3)" fontSize="8.5" fontFamily="var(--font-body)" letterSpacing=".04em">{(v / 1000)}k</text>
          </g>
        ))}
        {/* ground line */}
        <line x1={padL} y1={base} x2={W - padR} y2={base} stroke="rgba(106,95,192,.4)" strokeWidth="1" />

        {/* x ticks */}
        {ticks.map((m) => (
          <text key={m} x={x(m)} y={base + 17} textAnchor="middle" fill="var(--ink-3)" fontSize="8.5" fontFamily="var(--font-body)" letterSpacing=".1em">
            {m === 0 ? "NOW" : monthLabel(m, cycle)}
          </text>
        ))}

        {/* projection area + line (under history) */}
        {showProjection && projArea && <polygon points={projArea} fill="url(#trajproj)" />}
        {showProjection && projLine && <polyline points={projLine} fill="none" stroke="var(--indigo-neon)" strokeWidth="1.6" strokeDasharray="5 4" opacity=".85" />}

        {/* history area + line */}
        {histArea && <polygon points={histArea} fill="url(#trajfill)" />}
        {histLine && <polyline points={histLine} fill="none" stroke="var(--neon)" strokeWidth="2.2" strokeLinejoin="round"
          style={{ filter: "drop-shadow(0 0 3px var(--neon))" }} />}

        {/* today marker */}
        <line x1={x(0)} y1={padT - 8} x2={x(0)} y2={base + 6} stroke="rgba(255,59,46,.5)" strokeWidth="1.2" strokeDasharray="3 3" />
        <text x={x(0)} y={padT - 12} textAnchor="middle" fill="var(--neon-dim)" fontSize="8.5" fontFamily="var(--font-body)" letterSpacing=".12em">TODAY · {(cycle && cycle.asOf) || ""}</text>
        {todayPt && <circle cx={x(0)} cy={y(todayPt.total)} r="3.5" fill="var(--neon-white)" style={{ filter: "drop-shadow(0 0 4px var(--neon))" }} />}

        {/* debt-free endpoint */}
        {showProjection && (
          <g>
            <circle cx={dfX} cy={dfY} r="4" fill="var(--bg)" stroke="var(--ok)" strokeWidth="1.8" style={{ filter: "drop-shadow(0 0 5px var(--ok))" }} />
            <text x={dfX} y={dfY - 11} textAnchor="end" fill="var(--ok)" fontSize="9" fontFamily="var(--font-display)" letterSpacing=".04em">DEBT-FREE</text>
            <text x={dfX} y={dfY - 1} textAnchor="end" fill="var(--ink-3)" fontSize="8" fontFamily="var(--font-body)" letterSpacing=".06em">{debtFreeLabel}</text>
          </g>
        )}
      </svg>

      <div className="traj-foot">
        <span className="tf-stat"><i>PAID DOWN</i> <b>CHF {chf((S.totalOrig || 0) - (S.totalOwed || 0), 0)}</b> <em>of {chf(S.totalOrig || 0, 0)}</em></span>
        <span className="tf-stat"><i>AVG RATE</i> <b className="warn">{S.weightedApr != null ? (S.weightedApr * 100).toFixed(1) + "%" : "—"}</b></span>
        <span className="tf-stat"><i>DEBT-FREE IN</i> <b>{S.horizon != null ? S.horizon + " MO" : "—"}</b></span>
        <span className="tf-note">
          <Dot tone="alert" size={6} /> {strategy === "snowball"
            ? <>SNOWBALL · smallest first{S.snowballTarget ? <> → {String(S.snowballTarget)}</> : null}</>
            : <>AVALANCHE{S.avalancheTarget ? <> · {String(S.avalancheTarget)} costs the most</> : null} — target it first</>}
        </span>
      </div>
    </div>
  );
}

/* ============================ payoff-progress meter ======================== */
/* Reads debt.paidOffPct (0..1) from the backend — no local computation. */
function PayoffMeter({ d, tone }) {
  const p = Math.max(0, Math.min(1, Number(d && d.paidOffPct) || 0));
  const col = tone === "coral" ? "var(--neon)" : tone === "warn" ? "var(--warn)" : "var(--indigo-neon)";
  return (
    <div className="pay-meter">
      <div className="pay-fill" style={{ width: Math.max(2, p * 100) + "%", background: col, boxShadow: `0 0 6px ${col}` }} />
      <span className="pay-mk" style={{ left: p * 100 + "%" }} />
    </div>
  );
}

/* ============================ balance decay line (inspector) =============== */
/* Source: GET /debts/{id} decaySeries { hist[], forward[], todayIndex }. */
function DecayLine({ decay, w = 300, h = 104 }) {
  const hist = (decay && Array.isArray(decay.hist)) ? decay.hist : [];
  const forward = (decay && Array.isArray(decay.forward)) ? decay.forward : [];
  const series = [...hist, ...forward];
  if (series.length < 2) {
    return <div className="dim" style={{ padding: "20px 0", textAlign: "center", fontSize: 11 }}>No decay series.</div>;
  }
  const todayIdx = decay.todayIndex != null ? decay.todayIndex : Math.max(0, hist.length - 1);
  const maxY = Math.max(1, ...series) * 1.08;
  const padB = 16, padT = 8, padL = 4, padR = 4;
  const x = (i) => padL + i / (series.length - 1) * (w - padL - padR);
  const y = (v) => h - padB - (v / maxY) * (h - padT - padB);
  const histPts = series.slice(0, todayIdx + 1).map((v, i) => `${x(i).toFixed(1)},${y(v).toFixed(1)}`).join(" ");
  const projPts = series.slice(todayIdx).map((v, i) => `${x(i + todayIdx).toFixed(1)},${y(v).toFixed(1)}`).join(" ");
  const rose = hist.length > 1 && hist[hist.length - 1] > hist[0];
  const histCol = rose ? "var(--neon)" : "var(--indigo-neon)";
  return (
    <svg width="100%" height={h} viewBox={`0 0 ${w} ${h}`} preserveAspectRatio="none" style={{ display: "block" }}>
      <line x1={padL} y1={h - padB} x2={w - padR} y2={h - padB} stroke="rgba(106,95,192,.3)" strokeWidth="1" />
      <polyline points={projPts} fill="none" stroke="var(--indigo-neon)" strokeWidth="1.5" strokeDasharray="4 3" opacity=".8" />
      <polyline points={histPts} fill="none" stroke={histCol} strokeWidth="2" strokeLinejoin="round"
        style={{ filter: `drop-shadow(0 0 3px ${histCol})` }} />
      <line x1={x(todayIdx)} y1={padT - 4} x2={x(todayIdx)} y2={h - padB} stroke="rgba(255,59,46,.4)" strokeWidth="1" strokeDasharray="2 3" />
      <circle cx={x(todayIdx)} cy={y(series[todayIdx])} r="3" fill="var(--neon-white)" style={{ filter: "drop-shadow(0 0 3px var(--neon))" }} />
    </svg>
  );
}

/* ============================ DEBT CARD =================================== */
/* Reads derived fields off the fetched debt record (paidOffPct, monthsToPayoff,
   nextLabel, statusLabel). Spark uses debt.hist. */
function DebtCard({ d, active, onSelect, target, cycle }) {
  const st = debtStatus(d);
  const pct = Math.round((Number(d.paidOffPct) || 0) * 100);
  const months = d.monthsToPayoff;
  const isTarget = target && target === d.id;
  const hist = Array.isArray(d.hist) ? d.hist : [];
  const monthsLabel = months == null ? "—" : (months >= 600 ? "—" : months + " MO");
  const payoffWord = d.type === "CARD" ? "REVOLVING" : (months != null && months < 600 ? "PAYOFF " + monthLabel(months, cycle) : "PAYOFF —");
  return (
    <div className={"debt osc-bkt " + st.tone + (active ? " on" : "") + (isTarget ? " target" : "")} role="button" tabIndex={0}
      onClick={() => onSelect(d.id)} onKeyDown={(e) => { if (e.key === "Enter" || e.key === " ") { e.preventDefault(); onSelect(d.id); } }}>
      <span className="osc-leg">{isTarget ? "◎ TARGET" : (d.statusLabel || st.label)}</span>
      <div className="debt-h">
        <div className="debt-gl"><b>{d.glyph}</b></div>
        <div className="debt-id">
          <span className="debt-nm">{d.name}</span>
          <span className="debt-len"><Dot tone={d.src === "llm" ? "blue" : "ok"} size={5} />{d.lender}</span>
        </div>
        <span className="debt-type">{d.type}</span>
      </div>

      <div className="debt-amt">
        <span className="sp"><span className="cur">CHF</span>{chf(d.balance, 0)}</span>
        <span className="un">OWED</span>
        <span className="apr">{d.apr != null ? (d.apr * 100).toFixed(1) + "% APR" : "—"}</span>
      </div>

      <div className="debt-prog">
        <PayoffMeter d={d} tone={st.tone} />
        <div className="debt-progmeta">
          <span>{pct}% PAID OFF</span>
          <span>CHF {chf((d.orig || 0) - (d.balance || 0), 0)} OF {chf(d.orig || 0, 0)}</span>
        </div>
      </div>

      <div className="debt-next">
        {d.status === "high"
          ? <span className="nx alert">⚠ COSTLIEST RATE YOU CARRY</span>
          : d.status === "due"
            ? <span className="nx warn">⚠ DUE · {d.nextLabel || "—"}</span>
            : <span className="nx">NEXT · {d.nextLabel || "—"}</span>}
        <span className="pay">CHF {chf(d.monthly, 0)}/MO</span>
      </div>

      <div className="debt-foot">
        <span className="term">{payoffWord} · {monthsLabel}</span>
        <div className="decaytrack">
          <span className="pl">BALANCE</span>
          {hist.length > 1 && <Spark data={hist} w={70} h={20} tone={d.status === "high" ? "neon" : "indigo"} />}
        </div>
      </div>
    </div>
  );
}

/* ============================ compact ROW variant ========================= */
function DebtRow({ d, active, onSelect, target }) {
  const st = debtStatus(d);
  const isTarget = target && target === d.id;
  return (
    <div className={"debtrow" + (active ? " on" : "") + (isTarget ? " target" : "")} onClick={() => onSelect(d.id)}>
      <div className="dr-gl"><b>{d.glyph}</b></div>
      <span className="dr-nm">{d.name}</span>
      <span className={"dr-stat " + st.tone}>{isTarget ? "◎ TARGET" : (d.statusLabel || st.label)}</span>
      <div className="dr-meter"><PayoffMeter d={d} tone={st.tone} /></div>
      <span className="dr-apr">{d.apr != null ? (d.apr * 100).toFixed(1) + "%" : "—"}</span>
      <span className="dr-next">{d.nextLabel || "—"}</span>
      <span className="dr-amt">CHF <b>{chf(d.balance, 0)}</b></span>
      <span className="dr-pay">CHF {chf(d.monthly, 0)}<i>/mo</i></span>
    </div>
  );
}

/* ============================ INSPECTOR (right dock) ====================== */
/* Source: GET /debts/{id} { decaySeries, stats, guidance } over the list record.
   Actions: PAY EXTRA → POST /debts/{id}/payments {amount}; REFINANCE →
   POST /debts/{id}/refinance; ADJUST PLAN → PATCH /debts/{id}/plan. */
function DebtInspector({ d, detail, detailRes, detailLoading, payments, paymentsRes, paymentsLoading, onClose, variant, target, cycle, onAction }) {
  if (!d) {
    return (
      <aside className={"sig-panel debt-insp" + (variant ? " " + variant : "")}>
        <div className="sig-empty">
          <span className="mk">∿</span>
          <div className="tx">No debt selected.<br />Click any <b>balance</b> on the trajectory or a card to inspect its amortization, interest cost and AI payoff guidance.</div>
        </div>
      </aside>
    );
  }
  const st = debtStatus(d);
  const months = d.monthsToPayoff;
  const intRem = d.interestRemaining;
  const pct = Math.round((Number(d.paidOffPct) || 0) * 100);
  const decay = detail ? detail.decaySeries : null;
  // recent payments come from the dedicated GET /debts/{id}/payments endpoint.
  const recent = Array.isArray(payments) ? payments
    : (payments && (payments.payments || payments.items)) || [];
  const guidanceText = detail && detail.guidance
    ? (typeof detail.guidance === "string" ? detail.guidance : detail.guidance.text)
    : (d.note || "");

  const refinance = d.apr != null && d.apr > 0.08;

  const onPayExtra = () => {
    const raw = window.prompt("Extra payment toward " + d.name + " (CHF)", String(d.monthly || ""));
    if (raw == null) return;
    const amount = Number(raw);
    if (!isFinite(amount) || amount <= 0) return;
    api.post(`/debts/${encodeURIComponent(d.id)}/payments`, { amount }).then(() => onAction && onAction());
  };
  const onRefinance = () => {
    api.post(`/debts/${encodeURIComponent(d.id)}/refinance`, {}).then(() => onAction && onAction());
  };
  const onAdjustPlan = () => {
    const monthlyRaw = window.prompt("Monthly payment (CHF)", String(d.monthly || ""));
    if (monthlyRaw == null) return;
    const dayRaw = window.prompt("Payment day of month", String(d.day || ""));
    if (dayRaw == null) return;
    const termRaw = window.prompt("Term (months, blank for revolving)", d.term != null ? String(d.term) : "");
    if (termRaw == null) return;
    const body = {
      monthly: Number(monthlyRaw),
      day: Number(dayRaw),
      term: termRaw === "" ? null : Number(termRaw),
    };
    api.patch(`/debts/${encodeURIComponent(d.id)}/plan`, body).then(() => onAction && onAction());
  };

  return (
    <aside className={"sig-panel debt-insp" + (variant ? " " + variant : "")}>
      <div className="sig-head">
        <div className="kls">∿ DEBT · {d.type} · {d.lender}</div>
        <div className="nm">{d.name}</div>
        <div className="ds">{d.note}</div>
        {onClose && <span className="x" onClick={onClose} title="Close">✕</span>}
      </div>

      <div className="sig-delta">
        <span className="big up"><span style={{ fontSize: 16, color: "var(--ink-3)", marginRight: 5, verticalAlign: 4 }}>CHF</span>{chf(d.balance, 0)}</span>
        <span className="vs">outstanding · {pct}% paid off · {d.apr != null ? (d.apr * 100).toFixed(1) : "—"}% APR</span>
      </div>

      <div className="sig-chart">
        {detailRes && detailRes.status !== 200
          ? <Awaiting label="DECAY SERIES" res={detailRes} loading={detailLoading} tone="blue" />
          : <DecayLine decay={decay} />}
        <div className="axis"><span>6 MO BACK</span><span>{d.type === "CARD" ? "REVOLVING" : "PROJECTED →"}</span></div>
      </div>

      <div className="sig-stats">
        <div className="st"><div className="k">Outstanding</div><div className="v coral">CHF {chf(d.balance, 0)}</div></div>
        <div className="st"><div className="k">Monthly</div><div className="v">CHF {chf(d.monthly, 0)}</div></div>
        <div className="st"><div className="k">Interest / yr</div><div className="v">{d.annualInterest != null ? "CHF " + chf(d.annualInterest, 0) : "—"}</div></div>
        <div className="st"><div className="k">Interest left</div><div className="v" style={{ fontSize: 16 }}>{intRem == null ? "—" : (intRem === Infinity || intRem < 0 ? "∞" : "CHF " + chf(intRem, 0))}</div></div>
        <div className="st"><div className="k">Payoff</div><div className="v" style={{ fontSize: 16 }}>{months == null || months >= 600 ? "—" : monthLabel(months, cycle)}</div></div>
        <div className="st"><div className="k">Since</div><div className="v" style={{ fontSize: 16 }}>{d.since || "—"}</div></div>
      </div>

      <div className="sig-recent">
        <div className="h">Recent payments</div>
        {paymentsRes && paymentsRes.status !== 200
          ? <Awaiting label="PAYMENTS" res={paymentsRes} loading={paymentsLoading} tone="blue" />
          : recent.length === 0
            ? <div className="dim" style={{ fontSize: 11, padding: "6px 0" }}>No recent payments.</div>
            : recent.slice(0, 4).map((r, i) => (
              <div className="sig-occ" key={r.id || i}>
                <span className="dt">{r.date || r.label || "—"}</span>
                <span className="no" style={{ flex: 1 }}>{r.note || (r.amount != null ? "− CHF " + chf(r.amount, 0) + " paid" : "")}</span>
                <span className="pr">{r.balance != null ? "CHF " + chf(r.balance, 0) : ""}</span>
              </div>
            ))}
      </div>

      <div className="insp-acts">
        <button className="gbtn p" onClick={onPayExtra}>PAY EXTRA</button>
        {refinance
          ? <button className={"gbtn" + (d.status === "high" ? " coral" : "")} onClick={onRefinance}>REFINANCE</button>
          : <button className="gbtn" onClick={onAdjustPlan}>ADJUST PLAN</button>}
      </div>

      <div className="sig-foot">
        <div className="tx">{guidanceText}</div>
      </div>
    </aside>
  );
}

/* ============================ PERSONAL · IOU LEDGER ======================== */
/* Source: GET /personal-ious/stats { owedToYou, youOwe, net, countIn, countOut,
   maxSingle } + GET /personal-ious. A two-sided net-position beam. */
function NetBeam({ stats, count }) {
  const S = stats || {};
  const owedToYou = Number(S.owedToYou) || 0;
  const youOwe = Number(S.youOwe) || 0;
  const net = S.net != null ? Number(S.net) : owedToYou - youOwe;
  const W = 1000, H = 96, pad = 150, cx0 = W / 2, axisY = 52;
  const half = W / 2 - pad;
  const maxTotal = Math.max(owedToYou, youOwe, 1);
  const rx = cx0 + (owedToYou / maxTotal) * half;
  const lx = cx0 - (youOwe / maxTotal) * half;
  const netX = cx0 + (net / maxTotal) * half;
  const bh = 16;
  return (
    <div className="iou-beam osc-bkt">
      <span className="osc-leg">NET POSITION</span>
      <div className="beam-h">
        <span className="hud">⟷ PERSONAL · IOU LEDGER · {count != null ? count : "—"} OPEN</span>
        <span className={"beam-net " + (net >= 0 ? "pos" : "neg")}>
          NET {net >= 0 ? "+" : "−"}CHF {chf(Math.abs(net), 0)} {net >= 0 ? "IN YOUR FAVOUR" : "YOU'RE BEHIND"}
        </span>
      </div>
      <svg width="100%" height={H} viewBox={`0 0 ${W} ${H}`} preserveAspectRatio="none" className="beam-svg">
        {/* baseline */}
        <line x1={pad} y1={axisY} x2={W - pad} y2={axisY} stroke="rgba(106,95,192,.35)" strokeWidth="1" />
        {/* you-owe bar (left, coral) */}
        <rect x={lx} y={axisY - bh / 2} width={cx0 - lx} height={bh} fill="rgba(255,94,77,.5)" />
        <line x1={lx} y1={axisY - bh / 2 - 3} x2={lx} y2={axisY + bh / 2 + 3} stroke="var(--neon)" strokeWidth="1.5" style={{ filter: "drop-shadow(0 0 4px var(--neon))" }} />
        {/* owed-to-you bar (right, blue) */}
        <rect x={cx0} y={axisY - bh / 2} width={rx - cx0} height={bh} fill="rgba(143,125,255,.5)" />
        <line x1={rx} y1={axisY - bh / 2 - 3} x2={rx} y2={axisY + bh / 2 + 3} stroke="var(--indigo-neon)" strokeWidth="1.5" style={{ filter: "drop-shadow(0 0 4px var(--indigo-neon))" }} />
        {/* zero tick */}
        <line x1={cx0} y1={axisY - bh / 2 - 9} x2={cx0} y2={axisY + bh / 2 + 9} stroke="var(--neon-white)" strokeWidth="1.5" />
        <text x={cx0} y={axisY + bh / 2 + 22} textAnchor="middle" fill="var(--ink-3)" fontSize="8" fontFamily="var(--font-body)" letterSpacing=".16em">EVEN</text>
        {/* net needle */}
        <path d={`M ${netX} ${axisY - bh / 2 - 11} l -5 -7 l 10 0 z`} fill={net >= 0 ? "var(--indigo-neon)" : "var(--neon)"} style={{ filter: `drop-shadow(0 0 4px ${net >= 0 ? "var(--indigo-neon)" : "var(--neon)"})` }} />
        {/* end labels */}
        <text x={lx - 10} y={axisY + 4} textAnchor="end" fill="var(--neon-hot)" fontSize="14" fontFamily="var(--font-display)">{chf(youOwe, 0)}</text>
        <text x={lx - 10} y={axisY - 11} textAnchor="end" fill="var(--ink-3)" fontSize="7.5" fontFamily="var(--font-body)" letterSpacing=".14em">YOU OWE</text>
        <text x={rx + 10} y={axisY + 4} textAnchor="start" fill="var(--text-blue)" fontSize="14" fontFamily="var(--font-display)">{chf(owedToYou, 0)}</text>
        <text x={rx + 10} y={axisY - 11} textAnchor="start" fill="var(--ink-3)" fontSize="7.5" fontFamily="var(--font-body)" letterSpacing=".14em">OWED TO YOU</text>
      </svg>
    </div>
  );
}

/* PersonCard — REMIND → POST /personal-ious/{id}/remind ; SETTLE UP →
   /settle-up ; MARK SETTLED → /settle. Reads repaidPct + of off the record. */
function PersonCard({ p, onAction }) {
  const inbound = p.dir === "in";
  // backend-derived repaid fraction (tolerate 0–1 or 0–100); no client recompute
  const pct = p.repaidPct != null ? Math.round(Number(p.repaidPct) * (Number(p.repaidPct) <= 1 ? 100 : 1)) : null;
  const act = (verb) => () => {
    api.post(`/personal-ious/${encodeURIComponent(p.id)}/${verb}`, {}).then(() => onAction && onAction());
  };
  return (
    <div className={"person " + (inbound ? "in" : "out")}>
      <div className="person-h">
        <div className="person-av"><b>{p.initials}</b></div>
        <div className="person-id">
          <span className="person-nm">{p.person}</span>
          <span className={"person-dir " + (inbound ? "in" : "out")}>{inbound ? "← OWED TO YOU" : "YOU OWE →"}</span>
        </div>
        <div className="person-amt">
          <span className="cur">CHF</span>{chf(p.amount, 0)}
        </div>
      </div>
      <div className="person-reason">{p.reason}</div>
      {pct != null && (
        <div className="person-prog">
          <div className="pp-bar"><div className="pp-fill" style={{ width: pct + "%" }} /></div>
          <span className="pp-meta">{pct}% REPAID{p.of != null ? <> · CHF {chf(p.of - p.amount, 0)} OF {chf(p.of, 0)}</> : null}</span>
        </div>
      )}
      <div className="person-foot">
        <span className="since">SINCE {p.since}</span>
        <div className="person-acts">
          <button className="gbtn" onClick={act(inbound ? "remind" : "settle-up")}>{inbound ? "REMIND" : "SETTLE UP"}</button>
          <button className="gbtn p" onClick={act("settle")}>MARK SETTLED</button>
        </div>
      </div>
    </div>
  );
}

/* ============================ PAGE ======================================== */
/* Phoskonomia — Debts page. Outstanding balances as a decaying waveform. A
   payoff-trajectory hero up top, then a tunable grid/list of every debt with a
   payoff-progress meter, and a right-dock inspector. Strategy overlay
   (avalanche / snowball) marks which debt to target. Reuses the shared shell. */
const { useState: useStateDP, useMemo: useMemoDP, useEffect: useEffectDP } = React;

const DEBT_TWEAK_DEFAULTS = /*EDITMODE-BEGIN*/{
  "debtView": "cards",
  "debtSort": "balance",
  "debtStrategy": "avalanche",
  "debtProjection": true,
  "debtGroup": false,
  "debtHlAuto": false,
  "debtInsp": "dock",
  "iouShow": true,
  "aiOpen": true
} /*EDITMODE-END*/;

function DebtTweaks({ tw, setTweak, onAi, onStrategy }) {
  if (!TweaksPanel) return null;
  return (
    <TweaksPanel title="Tweaks">
      <TweakSection label="Ledger view" />
      <TweakRadio label="Layout" value={tw.debtView}
        options={[{ value: "cards", label: "Cards" }, { value: "rows", label: "Rows" }]}
        onChange={(v) => setTweak("debtView", v)} />
      <TweakSelect label="Sort" value={tw.debtSort}
        options={[
          { value: "balance", label: "Largest balance" },
          { value: "apr", label: "Highest rate" },
          { value: "payoff", label: "Soonest payoff" },
          { value: "name", label: "A–Z" },
        ]}
        onChange={(v) => setTweak("debtSort", v)} />
      <TweakToggle label="Group by type" value={tw.debtGroup}
        onChange={(v) => setTweak("debtGroup", v)} />
      <TweakToggle label="Highlight auto-detected" value={tw.debtHlAuto}
        onChange={(v) => setTweak("debtHlAuto", v)} />
      <TweakSection label="Payoff strategy" />
      <TweakRadio label="Target" value={tw.debtStrategy}
        options={[{ value: "avalanche", label: "Avalanche" }, { value: "snowball", label: "Snowball" }, { value: "none", label: "Off" }]}
        onChange={(v) => { setTweak("debtStrategy", v); onStrategy(v); }} />
      <TweakToggle label="Projected trajectory" value={tw.debtProjection}
        onChange={(v) => setTweak("debtProjection", v)} />
      <TweakSection label="Personal IOUs" />
      <TweakToggle label="Show IOU ledger" value={tw.iouShow}
        onChange={(v) => setTweak("iouShow", v)} />
      <TweakSection label="Detail" />
      <TweakRadio label="Inspector" value={tw.debtInsp}
        options={[{ value: "dock", label: "Dock" }, { value: "drawer", label: "Drawer" }]}
        onChange={(v) => setTweak("debtInsp", v)} />
      <TweakSection label="Assistant" />
      <TweakToggle label="AI panel open" value={tw.aiOpen}
        onChange={(v) => { setTweak("aiOpen", v); onAi(!v); }} />
    </TweaksPanel>
  );
}

function DebtsPage() {
  const [tw, setTweak] = useTweaks(DEBT_TWEAK_DEFAULTS);

  const [aiCollapsed, setAiCollapsed] = useStateDP(!tw.aiOpen);
  const [sel, setSel] = useStateDP(null);
  const [drawer, setDrawer] = useStateDP(false);
  const [narrow, setNarrow] = useStateDP(typeof window !== "undefined" && window.innerWidth < 1280);

  useEffectDP(() => {
    const on = () => setNarrow(window.innerWidth < 1280);
    on(); window.addEventListener("resize", on);
    return () => window.removeEventListener("resize", on);
  }, []);
  useEffectDP(() => { document.body.classList.toggle("hl-auto-debt", !!tw.debtHlAuto); }, [tw.debtHlAuto]);

  /* ---- data loads ---- */
  const cycleGet = useGet('/cycle/current');
  const cycle = cycleGet.data || {};

  const debtsGet = useGet('/debts');
  const debts = Array.isArray(debtsGet.data) ? debtsGet.data : [];

  const statsGet = useGet('/debts/stats');
  const S = statsGet.data || {};

  // trajectory re-fetches when the strategy tweak changes (params drive useGet).
  const trajStrategy = tw.debtStrategy === "snowball" ? "snowball"
    : tw.debtStrategy === "none" ? "none" : "avalanche";
  const trajGet = useGet('/debts/trajectory', { strategy: trajStrategy });

  // selected debt detail (decaySeries / stats / guidance). path changes → re-fetch.
  const detailGet = useGet(sel ? `/debts/${encodeURIComponent(sel)}` : '/debts/_none', undefined, [sel]);
  // recent payment history for the selected debt (separate endpoint per API.md)
  const paymentsGet = useGet(sel ? `/debts/${encodeURIComponent(sel)}/payments` : '/debts/_none', undefined, [sel]);

  const iousGet = useGet('/personal-ious');
  const ious = Array.isArray(iousGet.data) ? iousGet.data : [];

  const iouStatsGet = useGet('/personal-ious/stats');
  const iouStats = iouStatsGet.data || {};

  const reloadDebtAll = () => { debtsGet.reload(); statsGet.reload(); trajGet.reload(); if (sel) { detailGet.reload(); paymentsGet.reload(); } };
  const reloadIou = () => { iousGet.reload(); iouStatsGet.reload(); };

  // Strategy tweak ALSO persists to the backend, then re-fetch trajectory + stats.
  const onStrategy = (v) => {
    const strat = v === "snowball" ? "snowball" : v === "none" ? "none" : "avalanche";
    api.put('/debts/strategy', { strategy: strat }).then(() => { trajGet.reload(); statsGet.reload(); });
  };

  const dockable = !narrow && tw.debtInsp === "dock";
  const selectDebt = (id) => { setSel(id === sel ? null : id); if (id !== sel && !dockable) setDrawer(true); };

  // strategy target id comes from /debts/stats (avalancheTarget / snowballTarget).
  const target = tw.debtStrategy === "avalanche" ? (S.avalancheTarget || null)
    : tw.debtStrategy === "snowball" ? (S.snowballTarget || null) : null;

  const sorted = useMemoDP(() => {
    const arr = [...debts];
    if (tw.debtSort === "apr") arr.sort((a, b) => (b.apr || 0) - (a.apr || 0) || (b.balance || 0) - (a.balance || 0));
    else if (tw.debtSort === "name") arr.sort((a, b) => String(a.name).localeCompare(String(b.name)));
    else if (tw.debtSort === "payoff") arr.sort((a, b) => (a.monthsToPayoff ?? 1e9) - (b.monthsToPayoff ?? 1e9));
    else arr.sort((a, b) => (b.balance || 0) - (a.balance || 0));
    return arr;
  }, [tw.debtSort, debts]);

  // Prefer the backend's groupLabel; fall back to a small type→bucket map.
  const TYPE_LABEL = { LEASE: "LEASES & LOANS", LOAN: "LEASES & LOANS", CARD: "REVOLVING CREDIT", TAX: "OBLIGATIONS", BNPL: "OBLIGATIONS", MEDICAL: "OBLIGATIONS" };
  const groups = useMemoDP(() => {
    if (!tw.debtGroup) return [{ label: null, items: sorted }];
    const buckets = {};
    const order = [];
    sorted.forEach((d) => {
      const g = d.groupLabel || TYPE_LABEL[d.type] || "OTHER";
      if (!buckets[g]) { buckets[g] = []; order.push(g); }
      buckets[g].push(d);
    });
    return order.map((g) => ({ label: g + " · " + buckets[g].length, items: buckets[g] }));
  }, [sorted, tw.debtGroup]);

  const selDebt = sel ? debts.find((d) => d.id === sel) || null : null;
  const showDrawer = !dockable && drawer && selDebt;

  const renderItems = (items) => tw.debtView === "rows" ? (
    <div className="debt-rows">
      {items.map((d) => (
        <div key={d.id} data-src={d.src}>
          <DebtRow d={d} active={sel === d.id} onSelect={selectDebt} target={target} />
        </div>
      ))}
    </div>
  ) : (
    <div className="debt-grid">
      {items.map((d) => (
        <div key={d.id} data-src={d.src} style={{ display: "contents" }}>
          <DebtCard d={d} active={sel === d.id} onSelect={selectDebt} target={target} cycle={cycle} />
        </div>
      ))}
    </div>
  );

  const debtsReady = debtsGet.status === 200 && debts.length > 0;
  const iousReady = iousGet.status === 200 && ious.length > 0;
  const dash = <span style={{ color: "var(--ink-3)" }}>—</span>;
  const fmtCount = (v) => v != null ? v : dash;
  const fmtChf = (v, dp = 0) => v != null ? "CHF " + chf(v, dp) : dash;
  const pct100 = (v) => v != null ? Math.round(v * 100) + "%" : "—";

  const iouIn = ious.filter((p) => p.dir === "in");
  const iouOut = ious.filter((p) => p.dir === "out");

  return (
    <div className="pk" style={{ height: "100vh", minHeight: 0 }}>
      {/* Debts field — a DECAYING waveform. Signature: a SOLID coral principal
          mass upper-right (no other page leads with a solid), a molten growth
          tail sweeping down to lower-left, faint blue blobs deep in the corners. */}
      <ScannerBg className="pk-bg" seed={137} shapes={[
        { char: "8", cx: .82, cy: .27, scale: .47, style: "solid", morph: "blob", live: false, fill: .82 },
        { char: "3", cx: .29, cy: .74, scale: .54, style: "red", morph: "vein", live: true, fill: .5 },
        { char: "e", cx: .57, cy: .45, scale: .22, style: "wire", morph: "vein", live: false, fill: .4 },
        { char: "0", cx: .1, cy: .19, scale: .3, style: "faint", morph: "blob", live: false, fill: .5 },
        { char: "5", cx: .94, cy: .9, scale: .2, style: "faint", morph: "blob", live: false, fill: .4 }
      ]} />

      <div className="app-shell swap">
        <AiPanel collapsed={aiCollapsed} onToggle={() => setAiCollapsed((c) => !c)} onTrack={() => {}} />

        <div className="app-main">
          <TopBar active="DEBTS" />
          <div className="app-scroll" data-screen-label="DEBTS">
            <div className="debts-wrap">

              <div className="debts-top">
                <div>
                  <div className="ttl">Debts</div>
                  <div className="sum">
                    <b>{fmtCount(S.count)}</b> open balances · <span className="coral">{fmtChf(S.totalOwed)}</span> owed ·
                    <b> {fmtChf(S.totalMonthly)}</b>/mo · debt-free {S.debtFreeLabel || "—"}
                  </div>
                </div>
                <div className="modes">
                  <button className={"m" + (tw.debtView === "cards" ? " on" : "")} onClick={() => setTweak("debtView", "cards")}>▦ CARDS</button>
                  <button className={"m" + (tw.debtView === "rows" ? " on" : "")} onClick={() => setTweak("debtView", "rows")}>≡ ROWS</button>
                </div>
              </div>

              {/* KPI band — always framed; numerals fall back to em-dash. */}
              <div className="debts-kpis">
                <div className="debts-kpi accent">
                  <div className="lbl"><span>TOTAL OWED</span><span>OUTSTANDING</span></div>
                  <div className="big"><span className="cur">CHF</span>{S.totalOwed != null ? chf(S.totalOwed, 0) : "—"}</div>
                  <div className="ksub">{pct100(S.paidOffTotalPct)} paid down of {fmtChf(S.totalOrig)} borrowed</div>
                </div>
                <div className="debts-kpi blue">
                  <div className="lbl"><span>MONTHLY OUTFLOW</span><span>SCHEDULED</span></div>
                  <div className="big"><span className="cur">CHF</span>{S.totalMonthly != null ? chf(S.totalMonthly, 0) : "—"}</div>
                  <div className="ksub">{fmtCount(S.count)} payments · {fmtCount(S.autoCount)} auto-detected by GEMMA4</div>
                </div>
                <div className="debts-kpi">
                  <div className="lbl"><span>INTEREST</span><span>RUN-RATE / YR</span></div>
                  <div className="big" style={{ color: "var(--warn)" }}><span className="cur">CHF</span>{S.totalInterestYr != null ? chf(S.totalInterestYr, 0) : "—"}</div>
                  <div className="ksub">Avg {S.weightedApr != null ? (S.weightedApr * 100).toFixed(1) + "%" : "—"}{S.avalancheTarget ? <> · {String(S.avalancheTarget)} is the leak</> : null}</div>
                </div>
                <div className="debts-kpi">
                  <div className="lbl"><span>DEBT-FREE</span><span>PROJECTED</span></div>
                  <div className="big" style={{ color: "var(--ok)" }}>{S.debtFreeLabel || "—"}</div>
                  <div className="ksub">{S.horizon != null ? S.horizon + " months at the current pace" : "—"}</div>
                </div>
              </div>

              {/* payoff trajectory hero */}
              <PayoffTrajectory
                showProjection={tw.debtProjection}
                strategy={trajStrategy}
                traj={trajGet.data}
                stats={S}
                cycle={cycle}
                res={trajGet.res}
                loading={trajGet.loading} />

              <div className="debts-sec">
                <span className="lbl">∿ OPEN BALANCES</span>
                <span className="ct">{fmtCount(S.count != null ? S.count : (debtsReady ? debts.length : null))}</span>
                <span className="rule" />
                <span className="meta">
                  {target ? <>◎ {tw.debtStrategy === "snowball" ? "SNOWBALL" : "AVALANCHE"} TARGET MARKED · </> : null}
                  ▌ BAR = PAID OFF · CLICK TO INSPECT
                </span>
              </div>

              {debtsReady
                ? groups.map((g, i) => (
                  <React.Fragment key={g.label || i}>
                    {g.label && <div className="debts-group">{g.label}<span className="gr" /></div>}
                    {renderItems(g.items)}
                  </React.Fragment>
                ))
                : <Awaiting label="OPEN BALANCES" res={debtsGet.res} loading={debtsGet.loading} tone="coral" />}

              {tw.iouShow && (
                <div className="ious">
                  <div className="debts-sec ious-sec">
                    <span className="lbl">⟷ PERSONAL · IOU</span>
                    <span className="ct">{fmtCount(iouStats.countIn != null && iouStats.countOut != null ? iouStats.countIn + iouStats.countOut : (iousReady ? ious.length : null))}</span>
                    <span className="rule" />
                    <span className="meta">INFORMAL · NO INTEREST · KEPT OUT OF YOUR REAL DEBT</span>
                  </div>

                  {iouStatsGet.status === 200
                    ? <NetBeam stats={iouStats} count={iouStats.countIn != null && iouStats.countOut != null ? iouStats.countIn + iouStats.countOut : ious.length} />
                    : <div className="iou-beam osc-bkt"><span className="osc-leg">NET POSITION</span><Awaiting label="NET POSITION" res={iouStatsGet.res} loading={iouStatsGet.loading} tone="blue" /></div>}

                  {iousReady ? (
                    <div className="iou-cols">
                      <div className="iou-col">
                        <div className="iou-colh in">← OWED TO YOU<span className="n">{fmtCount(iouStats.countIn != null ? iouStats.countIn : iouIn.length)} · {fmtChf(iouStats.owedToYou)}</span></div>
                        {iouIn.map((p) => <PersonCard key={p.id} p={p} onAction={reloadIou} />)}
                      </div>
                      <div className="iou-col">
                        <div className="iou-colh out">YOU OWE →<span className="n">{fmtCount(iouStats.countOut != null ? iouStats.countOut : iouOut.length)} · {fmtChf(iouStats.youOwe)}</span></div>
                        {iouOut.map((p) => <PersonCard key={p.id} p={p} onAction={reloadIou} />)}
                      </div>
                    </div>
                  ) : (
                    <Awaiting label="PERSONAL IOUS" res={iousGet.res} loading={iousGet.loading} tone="blue" />
                  )}
                </div>
              )}

            </div>
          </div>
        </div>

        {dockable && (
          <DebtInspector
            d={selDebt}
            detail={detailGet.data}
            detailRes={selDebt ? detailGet.res : null}
            detailLoading={detailGet.loading}
            payments={paymentsGet.data}
            paymentsRes={selDebt ? paymentsGet.res : null}
            paymentsLoading={paymentsGet.loading}
            onClose={() => setSel(null)}
            target={target}
            cycle={cycle}
            onAction={reloadDebtAll} />
        )}
      </div>

      {showDrawer && (
        <div className="sig-drawer-back" onClick={() => setDrawer(false)}>
          <div className="sig-drawer" onClick={(e) => e.stopPropagation()}>
            <DebtInspector
              d={selDebt}
              detail={detailGet.data}
              detailRes={selDebt ? detailGet.res : null}
              detailLoading={detailGet.loading}
              payments={paymentsGet.data}
              paymentsRes={selDebt ? paymentsGet.res : null}
              paymentsLoading={paymentsGet.loading}
              onClose={() => setDrawer(false)}
              variant="drawer"
              target={target}
              cycle={cycle}
              onAction={reloadDebtAll} />
          </div>
        </div>
      )}

      <DebtTweaks tw={tw} setTweak={setTweak} onAi={setAiCollapsed} onStrategy={onStrategy} />
    </div>
  );
}

export default DebtsPage
