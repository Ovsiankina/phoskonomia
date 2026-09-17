/* Phoskonomia — Subscriptions page (merged, self-contained).
   Standing/recurring charges as a periodic impulse train. Billing sweep up top,
   then a tunable grid/list of every subscription, with a right-dock inspector.

   DATA SOURCE: real backend (phosk_api) via useGet — no mock data. Every section
   renders <Awaiting/> until its endpoint goes green; every control hits api.* then
   reloads the affected fetch. Presentation-only helpers (subStatus, cadUnit)
   stay; every derived figure (monthlyEquiv/annual/daysUntil/nextLabel/stats…) is
   READ off the fetched API objects per API.md, never recomputed. */
import React from 'react'
import { chf } from '../data/phosk.js'
import { useGet, api } from '../lib/api.js'
import { Awaiting } from '../components/states.jsx'
import { ScannerBg, Dot, Spark } from '../components/prims.jsx'
import { TopBar } from '../components/comps.jsx'
import { AiPanel } from '../components/shell.jsx'
import { useTweaks, TweaksPanel, TweakSection, TweakRadio, TweakToggle } from '../lib/tweaks.jsx'

/* ============================ PRESENTATION HELPERS ======================== */
const { useState: useStateSP, useMemo: useMemoSP, useEffect: useEffectSP } = React;

/* status → tone/label used across the page. Pure: maps a FETCHED sub record's
   `status` (and falls back to its `statusLabel`) — recomputes nothing financial. */
function subStatus(s) {
  const st = (s && s.status) || "ok";
  if (st === "due")   return { key: "due",   label: (s && s.statusLabel) || "NOT SEEN", tone: "coral" };
  if (st === "soon")  return { key: "soon",  label: (s && s.statusLabel) || "DUE SOON", tone: "warn" };
  if (st === "watch") return { key: "watch", label: (s && s.statusLabel) || "REVIEW",   tone: "warn" };
  return { key: "ok", label: (s && s.statusLabel) || "ACTIVE", tone: "blue" };
}

/* "/yr" for yearly cadence, "/mo" otherwise — display only. */
const cadUnit = (s) => (s && s.cadence === "yearly") ? "/ YR" : "/ MO";

/* ============================ BILLING SWEEP (hero) ========================== */
/* Sourced from GET /subscriptions/billing-sweep: cycle {day,days,asOf} + impulses[]
   {id,name,amount,day,status}. Keeps the SVG/layout math; positions come from the
   fetched impulses. Renders <Awaiting/> when its endpoint isn't 200. */
function BillingSweep({ sweep, res, loading, sel, onSelect, cycleLabel }) {
  const cycle = (sweep && sweep.cycle) || {};
  const impulses = (sweep && Array.isArray(sweep.impulses)) ? sweep.impulses : [];
  const footer = (sweep && sweep.footer) || {};
  const ok = res && res.status === 200 && impulses.length > 0;

  const days = cycle.days || 30, today = cycle.day || 0;
  const W = 1000, H = 178, padL = 50, padR = 50, padT = 30, padB = 38;
  const base = H - padB, usable = H - padT - padB;
  const x = (d) => padL + (d - 1) / (Math.max(2, days) - 1) * (W - padL - padR);
  const maxAmt = Math.max(1, ...impulses.map((s) => s.amount || 0));
  const hOf = (a) => 18 + Math.sqrt(a || 0) / Math.sqrt(maxAmt) * (usable - 18);

  const items = useMemoSP(() => {
    const byDay = {};
    impulses.forEach((s) => { const d = s.day; (byDay[d] = byDay[d] || []).push(s); });
    const out = [];
    Object.keys(byDay).forEach((day) => {
      const grp = byDay[day].sort((a, b) => (b.amount || 0) - (a.amount || 0));
      const n = grp.length;
      grp.forEach((s, i) => {
        const dx = (i - (n - 1) / 2) * 10;
        out.push({ s, cx: x(+day) + dx, top: base - hOf(s.amount) });
      });
    });
    return out;
  }, [sweep]);

  const tickDays = [1, 8, 15, 22, 29];
  // tone purely off the impulse's reported status (no recompute of "fired").
  const toneOf = (s) => {
    if (s.status === "due") return { col: "var(--neon)", glow: "drop-shadow(0 0 5px var(--neon))", dash: true, hollow: true };
    if (s.status === "soon") return { col: "var(--warn)", glow: "drop-shadow(0 0 5px var(--warn))" };
    if (s.status === "watch") return { col: "var(--warn)", glow: "drop-shadow(0 0 4px var(--warn))" };
    if (s.status === "paid") return { col: "rgba(143,125,255,.55)", glow: "none" };
    return { col: "var(--indigo-neon)", glow: "drop-shadow(0 0 4px var(--indigo-neon))" };
  };

  return (
    <div className="sweep osc-bkt blue">
      <span className="osc-leg">BILLING SWEEP</span>
      <div className="sweep-h">
        <span className="hud">⊟ RECURRING IMPULSE TRAIN{cycleLabel ? " · " + cycleLabel : ""}</span>
        <div className="sweep-key">
          <span><i className="k paid" /> PAID</span>
          <span><i className="k up" /> UPCOMING</span>
          <span><i className="k miss" /> NOT SEEN</span>
        </div>
      </div>

      {!ok ? (
        <Awaiting label="BILLING SWEEP" res={res} loading={loading} tone="blue" style={{ margin: "6px 0 2px" }} />
      ) : (
        <svg width="100%" height={H} viewBox={`0 0 ${W} ${H}`} preserveAspectRatio="none" className="sweep-svg">
          {/* ground line */}
          <line x1={padL} y1={base} x2={W - padR} y2={base} stroke="rgba(106,95,192,.4)" strokeWidth="1" />
          {/* weekly ticks */}
          {tickDays.map((d) => (
            <g key={d}>
              <line x1={x(d)} y1={padT - 6} x2={x(d)} y2={base} stroke="rgba(106,95,192,.14)" strokeWidth="1" strokeDasharray="2 5" />
              <text x={x(d)} y={base + 18} textAnchor="middle" fill="var(--ink-3)" fontSize="9" fontFamily="var(--font-body)" letterSpacing=".1em">{d}</text>
            </g>
          ))}
          <text x={padL} y={base + 18} textAnchor="start" fill="var(--ink-3)" fontSize="8" fontFamily="var(--font-body)" letterSpacing=".18em">DAY</text>
          {/* today marker */}
          {today > 0 && <>
            <line x1={x(today)} y1={padT - 8} x2={x(today)} y2={base + 6} stroke="rgba(255,59,46,.5)" strokeWidth="1.2" strokeDasharray="3 3" />
            <text x={x(today)} y={padT - 12} textAnchor="middle" fill="var(--neon-dim)" fontSize="8.5" fontFamily="var(--font-body)" letterSpacing=".12em">TODAY{cycle.asOf ? " · " + cycle.asOf : ""}</text>
          </>}

          {/* impulses */}
          {items.map(({ s, cx, top }) => {
            const t = toneOf(s);
            const active = sel === s.id;
            const r = active ? 5 : 3.5;
            return (
              <g key={s.id} onClick={() => onSelect(s.id)} style={{ cursor: "pointer" }}>
                <title>{s.name} · CHF {chf(s.amount)} · day {s.day}</title>
                {/* hit area */}
                <rect x={cx - 9} y={padT - 10} width={18} height={base - padT + 22} fill="transparent" />
                <line x1={cx} y1={base} x2={cx} y2={top} stroke={t.col} strokeWidth={active ? 2.4 : 1.6}
                  strokeDasharray={t.dash ? "3 3" : "0"} style={{ filter: t.glow }} />
                <circle cx={cx} cy={top} r={r} fill={t.hollow ? "var(--bg)" : t.col} stroke={t.col} strokeWidth={t.hollow ? 1.8 : 0}
                  style={{ filter: t.glow }} />
                {active && <circle cx={cx} cy={top} r={r + 4} fill="none" stroke={t.col} strokeWidth="1" opacity=".6" />}
                <text x={cx} y={top - 9} textAnchor="middle" fill={active ? "var(--ink)" : "var(--ink-2)"}
                  fontSize="8.5" fontFamily="var(--font-display)" style={active ? { textShadow: "0 0 6px " + t.col } : null}>
                  {chf(s.amount, (s.amount % 1) ? 2 : 0)}
                </text>
              </g>
            );
          })}
        </svg>
      )}

      {/* footer: read off the sweep `footer` / stats — em-dash when missing. */}
      <div className="sweep-foot">
        <span className="sf-stat"><i>PAID THIS CYCLE</i> <b>{footer.paidThisCycle != null ? "CHF " + chf(footer.paidThisCycle, 0) : "—"}</b></span>
        <span className="sf-stat"><i>STILL DUE</i> <b className="warn">{footer.stillDue != null ? "CHF " + chf(footer.stillDue, 0) : "—"}</b></span>
        <span className="sf-stat"><i>NEXT</i> {footer.next
          ? <b>{footer.next.name} · {footer.next.nextLabel || ""}{footer.next.amount != null ? " · CHF " + chf(footer.next.amount, 0) : ""}</b>
          : <b>—</b>}</span>
        {footer.note && <span className="sf-note"><Dot tone="blue" size={6} /> {footer.note}</span>}
      </div>
    </div>
  );
}

/* ============================ price history bars =========================== */
/* `hist` = fetched price history (oldest→newest). `axisLabel`/`priceRose` come
   off the fetched record. */
function SubHistBars({ s, w = 300, h = 96 }) {
  const data = Array.isArray(s.hist) ? s.hist : [];
  const yearly = s.cadence === "yearly";
  const labels = yearly ? ["'23", "'24", "'25"] : ["JAN", "FEB", "MAR", "APR", "MAY", "JUN"];
  if (!data.length) return <svg width="100%" height={h} viewBox={`0 0 ${w} ${h}`} preserveAspectRatio="none" style={{ display: "block" }} />;
  const max = Math.max(...data) * 1.16 || 1;
  const padB = 15, padT = 8;
  const bw = (w / data.length) * 0.5;
  const y = (v) => h - padB - (v / max) * (h - padT - padB);
  const rose = s.priceRose != null ? !!s.priceRose : (data[data.length - 1] > data[0]);
  return (
    <svg width="100%" height={h} viewBox={`0 0 ${w} ${h}`} preserveAspectRatio="none" style={{ display: "block" }}>
      {data.map((v, i) => {
        const cx = (i + 0.5) * (w / data.length);
        const last = i === data.length - 1;
        const bumped = i > 0 && v > data[i - 1];
        const col = last ? (rose ? "var(--neon)" : "var(--indigo-neon)") : bumped ? "rgba(255,94,77,.5)" : "rgba(132,116,222,.5)";
        return (
          <g key={i}>
            <rect x={cx - bw / 2} y={y(v)} width={bw} height={h - padB - y(v)} fill={col}
              style={last && rose ? { filter: "drop-shadow(0 0 4px var(--neon))" } : null} />
            <text x={cx} y={h - 3} textAnchor="middle" fill={last ? "var(--ink-2)" : "var(--ink-3)"} fontSize="7.5" fontFamily="var(--font-body)" letterSpacing=".06em">{labels[i] ?? ""}</text>
          </g>
        );
      })}
    </svg>
  );
}

/* ============================ billing-cycle meter ========================== */
/* `daysUntil` is read off the fetched record (not recomputed). The bar fraction
   is purely cosmetic positioning derived from that reported value + cycle length. */
function CycleMeter({ s, cycleDays = 30 }) {
  if (s.cadence !== "monthly") {
    return <div className="cyc-meter yearly"><div className="cyc-fill" style={{ width: "8%" }} /><span className="cyc-mk" style={{ left: "8%" }} /></div>;
  }
  const du = s.daysUntil;
  const overdue = s.status === "due";
  const frac = overdue ? 1 : (du == null ? 0.02 : Math.max(0.02, Math.min(1, 1 - du / cycleDays)));
  const col = overdue ? "var(--neon)" : (du != null && du <= 4) ? "var(--warn)" : "var(--indigo)";
  return (
    <div className={"cyc-meter" + (overdue ? " over" : "")}>
      <div className="cyc-fill" style={{ width: frac * 100 + "%", background: col, boxShadow: `0 0 6px ${col}` }} />
      <span className="cyc-mk" style={{ left: frac * 100 + "%" }} />
    </div>
  );
}

/* ============================ SUBSCRIPTION CARD =========================== */
/* All figures read off the fetched record: amount, monthlyEquiv, annual,
   daysUntil, nextLabel, since, hist, glyph, category, source. */
function SubCard({ s, active, onSelect, amountMode, cycleDays }) {
  const st = subStatus(s);
  const mo = s.monthlyEquiv, yr = s.annual;
  const showAnnual = amountMode === "annual";
  const yearly = s.cadence === "yearly";
  const big = showAnnual ? yr : s.amount;
  const unit = showAnnual ? "/ YR" : cadUnit(s);
  const second = showAnnual
    ? (mo != null ? `CHF ${chf(mo)} / mo` : "—")
    : (yr != null ? `CHF ${chf(yr, 0)} / yr` : "—");
  const du = s.cadence === "monthly" ? s.daysUntil : null;
  const auto = s.source === "llm";
  return (
    <div className={"sub osc-bkt " + st.tone + (active ? " on" : "")} role="button" tabIndex={0}
      onClick={() => onSelect(s.id)} onKeyDown={(e) => { if (e.key === "Enter" || e.key === " ") { e.preventDefault(); onSelect(s.id); } }}>
      <span className="osc-leg">{st.label}</span>
      <div className="sub-h">
        <div className="sub-gl"><b>{s.glyph}</b></div>
        <div className="sub-id">
          <span className="sub-nm">{s.name}</span>
          <span className="sub-cat"><Dot tone={auto ? "blue" : "ok"} size={5} />{s.category}</span>
        </div>
        <span className={"sub-src" + (auto ? " auto" : "")}>{auto ? "AUTO" : "USER"}</span>
      </div>

      <div className="sub-amt">
        <span className="sp"><span className="cur">CHF</span>{big != null ? chf(big, (big % 1) ? 2 : (showAnnual || yearly ? 0 : 2)) : "—"}</span>
        <span className="un">{unit}</span>
        <span className="eq">{second}</span>
      </div>

      <CycleMeter s={s} cycleDays={cycleDays} />
      <div className="sub-next">
        {s.status === "due"
          ? <span className="nx alert">⚠ NOT SEEN THIS CYCLE</span>
          : yearly
            ? <span className="nx">NEXT · {s.nextLabel || "—"}</span>
            : <span className={"nx" + (du != null && du <= 4 ? " warn" : "")}>NEXT · {s.nextLabel || "—"}{du != null ? " · IN " + du + "D" : ""}</span>}
        <span className="cad">{yearly ? "YEARLY" : "MONTHLY" + (s.day != null ? " · " + s.day : "")}</span>
      </div>

      <div className="sub-foot">
        <span className="since">SINCE {s.since || "—"}</span>
        <div className="pricetrack">
          <span className="pl">PRICE</span>
          {Array.isArray(s.hist) && s.hist.length > 1
            ? <Spark data={s.hist} w={70} h={20} tone="indigo" />
            : <span className="dim" style={{ fontSize: 9 }}>—</span>}
        </div>
      </div>
    </div>
  );
}

/* ============================ compact ROW variant ========================= */
function SubRow({ s, active, onSelect, amountMode, cycleDays }) {
  const st = subStatus(s);
  const yr = s.annual;
  const yearly = s.cadence === "yearly";
  const du = s.cadence === "monthly" ? s.daysUntil : null;
  const showAnnual = amountMode === "annual";
  const big = showAnnual ? yr : s.amount;
  const auto = s.source === "llm";
  return (
    <div className={"subrow" + (active ? " on" : "")} onClick={() => onSelect(s.id)}>
      <div className="sr-gl"><b>{s.glyph}</b></div>
      <span className="sr-nm">{s.name}</span>
      <span className={"sr-stat " + st.tone}>{st.label}</span>
      <div className="sr-meter"><CycleMeter s={s} cycleDays={cycleDays} /></div>
      <span className="sr-next">{s.status === "due" ? "⚠ —" : (s.nextLabel || "—")}{du != null && s.status !== "due" ? " · " + du + "D" : ""}</span>
      <span className="sr-amt">CHF <b>{big != null ? chf(big, (showAnnual || yearly) ? 0 : 2) : "—"}</b> <i>{showAnnual ? "/yr" : (yearly ? "/yr" : "/mo")}</i></span>
      <span className={"sr-src" + (auto ? " auto" : "")}>{auto ? "AUTO" : "USER"}</span>
    </div>
  );
}

/* ============================ INSPECTOR (right dock) ====================== */
/* `detail` = fetched GET /subscriptions/{id} (+ guidance{text,severity},
   histAxisLabel, recent/charges). All actions hit api.* then call onChanged()
   to reload the list + stats + detail. */
function SubInspector({ detail, res, loading, sel, onClose, onChanged, variant }) {
  if (!sel) {
    return (
      <aside className={"sig-panel sub-insp" + (variant ? " " + variant : "")}>
        <div className="sig-empty">
          <span className="mk">⊟</span>
          <div className="tx">No subscription selected.<br />Click any <b>impulse</b> on the sweep or a card to inspect its price history, cadence and AI guidance.</div>
        </div>
      </aside>
    );
  }
  // selected but the detail endpoint isn't 200 yet → on-brand awaiting body.
  // Require the payload to actually be the selected sub (guards the brief window
  // where `detail` is still the fallback/previous response while a fetch is in
  // flight, once the backend goes green).
  const ok = res && res.status === 200 && detail && (detail.id == null || detail.id === sel);
  if (!ok) {
    return (
      <aside className={"sig-panel sub-insp" + (variant ? " " + variant : "")}>
        <div className="sig-head">
          <div className="kls">⊟ SUBSCRIPTION</div>
          <div className="nm">{sel}</div>
          {onClose && <span className="x" onClick={onClose} title="Close">✕</span>}
        </div>
        <Awaiting label="SUBSCRIPTION DETAIL" res={res} loading={loading} tone="blue" style={{ margin: 14 }} />
      </aside>
    );
  }

  const s = detail;
  const mo = s.monthlyEquiv, yr = s.annual;
  const yearly = s.cadence === "yearly";
  const recent = Array.isArray(s.recent) ? s.recent : (Array.isArray(s.charges) ? s.charges : []);
  const guidance = (s.guidance && s.guidance.text) || s.note || "";
  const sev = (s.guidance && s.guidance.severity) || (s.status === "due" || s.status === "watch" ? "coral" : null);
  const axisLabel = s.histAxisLabel || (yearly ? "3 YEARS" : "6 CHARGES");

  const after = (r) => { if (r && (r.ok || r.status === 501)) { if (onChanged) onChanged(); } return r; };
  const act = (verb, body) => api.post(`/subscriptions/${encodeURIComponent(s.id)}/${verb}`, body).then(after);
  // primary lifecycle: MARK PAID when not seen this cycle, else PAUSE.
  const primaryLabel = s.status === "due" ? "MARK PAID" : "PAUSE";
  const onPrimary = () => s.status === "due" ? act("mark-paid") : act("pause");
  // candidate (AI-detected) subs get CONFIRM/DISMISS instead of cancel.
  const candidate = !!s.candidate;

  return (
    <aside className={"sig-panel sub-insp" + (variant ? " " + variant : "")}>
      <div className="sig-head">
        <div className="kls">⊟ SUBSCRIPTION · {s.category}</div>
        <div className="nm">{s.name}</div>
        <div className="ds">{s.note}</div>
        {onClose && <span className="x" onClick={onClose} title="Close">✕</span>}
      </div>

      <div className="sig-delta">
        <span className="big up"><span style={{ fontSize: 16, color: "var(--ink-3)", marginRight: 5, verticalAlign: 4 }}>CHF</span>{s.amount != null ? chf(s.amount, (s.amount % 1) ? 2 : 0) : "—"}</span>
        <span className="vs">per charge · {s.cadence}{yr != null ? " · annualized CHF " + chf(yr, 0) : ""}</span>
      </div>

      <div className="sig-chart">
        <SubHistBars s={s} />
        <div className="axis"><span>{axisLabel}</span><span>{s.priceRose ? "PRICE ROSE" : "FLAT"}</span></div>
      </div>

      <div className="sig-stats">
        <div className="st"><div className="k">Per charge</div><div className="v coral">{s.amount != null ? "CHF " + chf(s.amount, (s.amount % 1) ? 2 : 0) : "—"}</div></div>
        <div className="st"><div className="k">Monthly</div><div className="v">{mo != null ? "CHF " + chf(mo) : "—"}</div></div>
        <div className="st"><div className="k">Annualized</div><div className="v">{yr != null ? "CHF " + chf(yr, 0) : "—"}</div></div>
        <div className="st"><div className="k">Next charge</div><div className="v" style={{ fontSize: 16 }}>{s.nextLabel || "—"}</div></div>
        <div className="st"><div className="k">Cadence</div><div className="v" style={{ fontSize: 15 }}>{yearly ? "YEARLY" + (s.month ? " · " + s.month : "") : "MONTHLY" + (s.day != null ? " · " + s.day : "")}</div></div>
        <div className="st"><div className="k">Tracked since</div><div className="v" style={{ fontSize: 16 }}>{s.since || "—"}</div></div>
      </div>

      <div className="sig-recent">
        <div className="h">Recent charges</div>
        {recent.length === 0 && <div className="dim" style={{ fontSize: 11, padding: "8px 2px", letterSpacing: ".04em" }}>No charges recorded yet.</div>}
        {recent.map((o, i) => (
          <div className="sig-occ" key={o.id != null ? o.id : i}>
            <span className="dt">{o.date}</span>
            <span className="no" style={{ flex: 1 }}>{o.note || (s.source === "llm" ? "auto-detected" : "confirmed")}</span>
            <span className="pr">{o.amount != null ? "CHF " + chf(o.amount) : ""}</span>
          </div>
        ))}
      </div>

      <div className="insp-acts">
        {candidate ? (
          <>
            <button className="gbtn p" onClick={() => act("confirm")}>CONFIRM</button>
            <button className="gbtn" onClick={() => act("dismiss")}>DISMISS</button>
          </>
        ) : (
          <>
            <button className="gbtn p" onClick={onPrimary}>{primaryLabel}</button>
            {s.status === "paused"
              ? <button className="gbtn" onClick={() => act("resume")}>RESUME</button>
              : <button className="gbtn coral" onClick={() => act("cancel")}>CANCEL</button>}
          </>
        )}
      </div>

      <div className="insp-acts" style={{ marginTop: 8 }}>
        <button className="gbtn" onClick={() => act("snooze", {})}>SNOOZE</button>
      </div>

      {guidance && (
        <div className="sig-foot">
          <div className="tx">{sev === "coral" ? <b className="coral">{guidance}</b> : guidance}</div>
        </div>
      )}
    </aside>
  );
}

/* ============================== PAGE COMPONENT ============================ */
const SUB_TWEAK_DEFAULTS = /*EDITMODE-BEGIN*/{
  "subView": "cards",
  "subSort": "due",
  "subAmounts": "monthly",
  "subGroup": false,
  "subHlAuto": false,
  "aiOpen": true
} /*EDITMODE-END*/;

function SubTweaks({ tw, setTweak, onAi }) {
  if (!TweaksPanel) return null;
  return (
    <TweaksPanel title="Tweaks">
      <TweakSection label="Ledger view" />
      <TweakRadio label="Layout" value={tw.subView}
      options={[{ value: "cards", label: "Cards" }, { value: "rows", label: "Rows" }]}
      onChange={(v) => setTweak("subView", v)} />
      <TweakRadio label="Sort" value={tw.subSort}
      options={[{ value: "due", label: "Due" }, { value: "amount", label: "Cost" }, { value: "name", label: "A–Z" }]}
      onChange={(v) => setTweak("subSort", v)} />
      <TweakRadio label="Amounts" value={tw.subAmounts}
      options={[{ value: "monthly", label: "Per charge" }, { value: "annual", label: "Annual" }]}
      onChange={(v) => setTweak("subAmounts", v)} />
      <TweakToggle label="Group by cadence" value={tw.subGroup}
      onChange={(v) => setTweak("subGroup", v)} />
      <TweakToggle label="Highlight auto-detected" value={tw.subHlAuto}
      onChange={(v) => setTweak("subHlAuto", v)} />
      <TweakSection label="Assistant" />
      <TweakToggle label="AI panel open" value={tw.aiOpen}
      onChange={(v) => {setTweak("aiOpen", v);onAi(!v);}} />
    </TweaksPanel>);

}

function SubsPage() {
  const [tw, setTweak] = useTweaks(SUB_TWEAK_DEFAULTS);

  const [aiCollapsed, setAiCollapsed] = useStateSP(!tw.aiOpen);
  const [sel, setSel] = useStateSP(null);

  useEffectSP(() => {document.body.classList.toggle("hl-auto", !!tw.subHlAuto);}, [tw.subHlAuto]);

  const selectSub = (id) => setSel(id === sel ? null : id);

  /* ---- DATA: all from the backend (501 until green) ---- */
  // current cycle window (for labels + sweep fallback day/days).
  const { data: cycle } = useGet('/cycle/current');
  const C = cycle || {};
  const cycleDays = C.days || 30;

  // the subscription list — sort/group/amounts tweaks drive the querystring so
  // changing a control re-fetches. The backend owns ordering/grouping/figures.
  const { data: listData, status: listStatus, loading: listLoading, res: listRes, reload: reloadList } =
    useGet('/subscriptions', {
      sort: tw.subSort,
      group: tw.subGroup ? "cadence" : undefined,
      amounts: tw.subAmounts,
    });
  const subs = Array.isArray(listData) ? listData : ((listData && (listData.subscriptions || listData.items)) || []);
  const listOk = listStatus === 200 && subs.length > 0;

  // KPI tiles + sweep footer roll-ups.
  const { data: statsData, res: statsRes, reload: reloadStats } = useGet('/subscriptions/stats');
  const S = statsData || {};
  const statsOk = statsRes && statsRes.status === 200 && statsData;
  const next30 = (S.next30 && typeof S.next30 === "object") ? S.next30 : {};
  const flagged = (S.flagged && typeof S.flagged === "object") ? S.flagged : {};

  // billing sweep impulse train.
  const { data: sweepData, res: sweepRes, loading: sweepLoading, reload: reloadSweep } = useGet('/subscriptions/billing-sweep');

  // selected subscription detail. Follows the sibling-page idiom: fetch the
  // detail path when something is selected, else a harmless real endpoint
  // (never `/null`); `[sel]` re-fires the fetch on selection change. The
  // inspector only reads `detailData` when `sel` is set.
  const { data: detailData, res: detailRes, loading: detailLoading, reload: reloadDetail } =
    useGet(sel ? `/subscriptions/${encodeURIComponent(sel)}` : '/subscriptions/stats', undefined, [sel]);

  // after any mutation, resync everything that could have changed.
  const reloadAll = () => { reloadList(); reloadStats(); reloadSweep(); if (sel) reloadDetail(); };

  // AI detect recurring charges, then reload.
  const onDetect = () => api.post('/subscriptions/detect', { lookbackMonths: 6 }).then((r) => { if (r && (r.ok || r.status === 501)) reloadAll(); return r; });

  /* ---- grouping: trust the backend's order; only split into cadence sections
     for display when "group by cadence" is on. No re-sorting (sort is a param). */
  const groups = useMemoSP(() => {
    if (!tw.subGroup) return [{ label: null, items: subs }];
    const m = subs.filter((s) => s.cadence === "monthly");
    const y = subs.filter((s) => s.cadence === "yearly");
    const out = [];
    if (m.length) out.push({ label: "MONTHLY · " + m.length, items: m });
    if (y.length) out.push({ label: "YEARLY · " + y.length, items: y });
    return out;
  }, [subs, tw.subGroup]);

  const renderItems = (items) => tw.subView === "rows" ?
  <div className="sub-rows">
      {items.map((s) =>
    <div key={s.id} data-src={s.source}>
          <SubRow s={s} active={sel === s.id} onSelect={selectSub} amountMode={tw.subAmounts} cycleDays={cycleDays} />
        </div>
    )}
    </div> :

  <div className="sub-grid">
      {items.map((s) =>
    <div key={s.id} data-src={s.source} style={{ display: "contents" }}>
          <SubCard s={s} active={sel === s.id} onSelect={selectSub} amountMode={tw.subAmounts} cycleDays={cycleDays} />
        </div>
    )}
    </div>;


  return (
    <div className="pk" style={{ height: "100vh", minHeight: 0 }}>
      {/* Subscriptions field — the molten signal is anchored RIGHT-CENTRE (a
           recurring axis), a faint blob upper-left, wire + faint to the lower-left. */}
      <ScannerBg className="pk-bg" seed={71} shapes={[
      { char: "6", cx: .88, cy: .5, scale: .42, style: "red", morph: "vein", live: true, fill: .52 },
      { char: "2", cx: .16, cy: .26, scale: .3, style: "faint", morph: "blob", live: false, fill: .5 },
      { char: "9", cx: .4, cy: .14, scale: .16, style: "wire", morph: "vein", live: false, fill: .34 },
      { char: "0", cx: .27, cy: .86, scale: .26, style: "faint", morph: "blob", live: false, fill: .42 }]
      } />

      <div className="app-shell swap">
        <AiPanel collapsed={aiCollapsed} onToggle={() => setAiCollapsed((c) => !c)} onTrack={() => {}} />

        <div className="app-main">
          <TopBar active="SUBSCRIPTIONS" />
          <div className="app-scroll" data-screen-label="SUBSCRIPTIONS">
            <div className="subs-wrap">

              <div className="subs-top">
                <div>
                  <div className="ttl">Subscriptions</div>
                  <div className="sum">
                    <b>{S.count != null ? S.count : "—"}</b> standing charges · <span className="coral">CHF {S.monthly != null ? chf(S.monthly, 0) : "—"}</span>/mo ·
                    <b> CHF {S.annual != null ? chf(S.annual, 0) : "—"}</b>/yr{C.label ? " · " + C.label : ""}
                  </div>
                </div>
                <div className="subs-controls">
                  <div className="modes">
                    <span className="mlbl">SORT</span>
                    <button className={"m" + (tw.subSort === "due" ? " on" : "")} onClick={() => setTweak("subSort", "due")}>DUE</button>
                    <button className={"m" + (tw.subSort === "amount" ? " on" : "")} onClick={() => setTweak("subSort", "amount")}>COST</button>
                    <button className={"m" + (tw.subSort === "name" ? " on" : "")} onClick={() => setTweak("subSort", "name")}>A–Z</button>
                  </div>
                  <div className="modes">
                    <button className={"m" + (tw.subGroup ? " on" : "")} onClick={() => setTweak("subGroup", !tw.subGroup)}>⊞ GROUP BY CADENCE</button>
                  </div>
                  <div className="modes">
                    <button className={"m" + (tw.subView === "cards" ? " on" : "")} onClick={() => setTweak("subView", "cards")}>▦ CARDS</button>
                    <button className={"m" + (tw.subView === "rows" ? " on" : "")} onClick={() => setTweak("subView", "rows")}>≡ ROWS</button>
                  </div>
                  <div className="modes">
                    <button className="m" onClick={onDetect} title="AI: scan transactions for recurring charges">⌁ DETECT</button>
                  </div>
                </div>
              </div>

              {/* KPI band — read off /subscriptions/stats; em-dash when missing. */}
              <div className="subs-kpis">
                <div className="subs-kpi accent">
                  <div className="lbl"><span>MONTHLY RECURRING</span><span>RUN-RATE</span></div>
                  <div className="big"><span className="cur">CHF</span>{S.monthly != null ? chf(S.monthly, 0) : "—"}</div>
                  <div className="ksub">{S.count != null ? S.count : "—"} active{S.autoCount != null ? " · " + S.autoCount + " auto-detected by GEMMA4" : ""}</div>
                </div>
                <div className="subs-kpi blue">
                  <div className="lbl"><span>ANNUALIZED</span><span>12 MO</span></div>
                  <div className="big"><span className="cur">CHF</span>{S.annual != null ? chf(S.annual, 0) : "—"}</div>
                  <div className="ksub">Committed across every standing charge</div>
                </div>
                <div className="subs-kpi">
                  <div className="lbl"><span>NEXT 30 DAYS</span><span>{next30.count != null ? next30.count + " CHARGES" : "—"}</span></div>
                  <div className="big"><span className="cur">CHF</span>{next30.total != null ? chf(next30.total, 0) : "—"}</div>
                  <div className="ksub">{Array.isArray(next30.items) && next30.items[0]
                    ? <>Next · {next30.items[0].name}{next30.items[0].daysUntil != null ? " in " + next30.items[0].daysUntil + "d" : ""}</>
                    : "Nothing scheduled"}</div>
                </div>
                <div className="subs-kpi">
                  <div className="lbl"><span>NEEDS ATTENTION</span><span>AI</span></div>
                  <div className="big" style={{ color: "var(--neon)", textShadow: "var(--glow-text)" }}>{flagged.count != null ? flagged.count : "—"}</div>
                  <div className="ksub">{flagged.note || (statsOk ? "Flagged by GEMMA4 for review" : "Awaiting backend")}</div>
                </div>
              </div>

              {/* billing sweep */}
              <BillingSweep sweep={sweepData} res={sweepRes} loading={sweepLoading}
                sel={sel} onSelect={selectSub} cycleLabel={C.label} />

              <div className="subs-sec">
                <span className="lbl">⊟ STANDING CHARGES</span>
                <span className="ct">{S.count != null ? S.count : "—"}</span>
                <span className="rule" />
                <span className="meta">▌ IMPULSE = CHARGE · ━ CYCLE COUNTDOWN · CLICK TO INSPECT</span>
              </div>

              {!listOk ? (
                <Awaiting label="STANDING CHARGES" res={listRes} loading={listLoading} tone="blue" />
              ) : (
                groups.map((g, i) =>
                <React.Fragment key={g.label || i}>
                    {g.label && <div className="subs-group">{g.label}<span className="gr" /></div>}
                    {renderItems(g.items)}
                  </React.Fragment>
                )
              )}

            </div>
          </div>
        </div>

        <SubInspector detail={detailData} res={detailRes} loading={detailLoading}
          sel={sel} onClose={() => setSel(null)} onChanged={reloadAll} />
      </div>

      <SubTweaks tw={tw} setTweak={setTweak} onAi={setAiCollapsed} />
    </div>);

}

export default SubsPage;
