/* Phoskonomia — Subscriptions page components.
   Standing charges = a periodic impulse train on the scope. The hero is the
   BILLING SWEEP (one impulse per charge across the cycle); below it, cards/rows
   per subscription, and a right-dock inspector with price history + AI guidance. */
const { useMemo: useMemoSC } = React;

/* ============================ BILLING SWEEP (hero) ========================== */
function BillingSweep({ subs, sel, onSelect }) {
  const D = window.PHOSK;
  const days = D.cycle.days, today = D.cycle.day;
  const W = 1000, H = 178, padL = 50, padR = 50, padT = 30, padB = 38;
  const base = H - padB, usable = H - padT - padB;
  const x = (d) => padL + (d - 1) / (days - 1) * (W - padL - padR);
  const monthly = subs.filter((s) => s.cadence === "monthly");
  const maxAmt = Math.max(...monthly.map((s) => s.amount));
  const hOf = (a) => 18 + Math.sqrt(a) / Math.sqrt(maxAmt) * (usable - 18);

  const items = useMemoSC(() => {
    const byDay = {};
    monthly.forEach((s) => { (byDay[s.day] = byDay[s.day] || []).push(s); });
    const out = [];
    Object.keys(byDay).forEach((day) => {
      const grp = byDay[day].sort((a, b) => b.amount - a.amount);
      const n = grp.length;
      grp.forEach((s, i) => {
        const dx = (i - (n - 1) / 2) * 10;
        out.push({ s, cx: x(+day) + dx, top: base - hOf(s.amount) });
      });
    });
    return out;
  }, [subs]);

  const tickDays = [1, 8, 15, 22, 29];
  const toneOf = (s) => {
    const fired = D.subFired(s) && s.status !== "due";
    if (s.status === "due") return { col: "var(--neon)", glow: "drop-shadow(0 0 5px var(--neon))", dash: true, hollow: true };
    if (s.status === "soon") return { col: "var(--warn)", glow: "drop-shadow(0 0 5px var(--warn))" };
    if (s.status === "watch") return { col: "var(--warn)", glow: "drop-shadow(0 0 4px var(--warn))", soft: fired };
    if (fired) return { col: "rgba(143,125,255,.55)", glow: "none" };
    return { col: "var(--indigo-neon)", glow: "drop-shadow(0 0 4px var(--indigo-neon))" };
  };

  return (
    <div className="sweep osc-bkt blue">
      <span className="osc-leg">BILLING SWEEP</span>
      <div className="sweep-h">
        <span className="hud">⊟ RECURRING IMPULSE TRAIN · {D.cycle.label}</span>
        <div className="sweep-key">
          <span><i className="k paid" /> PAID</span>
          <span><i className="k up" /> UPCOMING</span>
          <span><i className="k miss" /> NOT SEEN</span>
        </div>
      </div>

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
        <line x1={x(today)} y1={padT - 8} x2={x(today)} y2={base + 6} stroke="rgba(255,59,46,.5)" strokeWidth="1.2" strokeDasharray="3 3" />
        <text x={x(today)} y={padT - 12} textAnchor="middle" fill="var(--neon-dim)" fontSize="8.5" fontFamily="var(--font-body)" letterSpacing=".12em">TODAY · {D.cycle.asOf}</text>

        {/* impulses */}
        {items.map(({ s, cx, top }) => {
          const t = toneOf(s);
          const active = sel === s.id;
          const r = active ? 5 : 3.5;
          return (
            <g key={s.id} onClick={() => onSelect(s.id)} style={{ cursor: "pointer" }}>
              <title>{s.name} · CHF {D.chf(s.amount)} · day {s.day}</title>
              {/* hit area */}
              <rect x={cx - 9} y={padT - 10} width={18} height={base - padT + 22} fill="transparent" />
              <line x1={cx} y1={base} x2={cx} y2={top} stroke={t.col} strokeWidth={active ? 2.4 : 1.6}
                strokeDasharray={t.dash ? "3 3" : "0"} opacity={t.soft ? 0.7 : 1} style={{ filter: t.glow }} />
              <circle cx={cx} cy={top} r={r} fill={t.hollow ? "var(--bg)" : t.col} stroke={t.col} strokeWidth={t.hollow ? 1.8 : 0}
                style={{ filter: t.glow }} />
              {active && <circle cx={cx} cy={top} r={r + 4} fill="none" stroke={t.col} strokeWidth="1" opacity=".6" />}
              <text x={cx} y={top - 9} textAnchor="middle" fill={active ? "var(--ink)" : "var(--ink-2)"}
                fontSize="8.5" fontFamily="var(--font-display)" style={active ? { textShadow: "0 0 6px " + t.col } : null}>
                {D.chf(s.amount, s.amount % 1 ? 2 : 0)}
              </text>
            </g>
          );
        })}
      </svg>

      <div className="sweep-foot">
        <span className="sf-stat"><i>PAID THIS CYCLE</i> <b>CHF {D.chf(D.subStats.chargedThisCycle, 0)}</b></span>
        <span className="sf-stat"><i>STILL DUE</i> <b className="warn">CHF {D.chf(D.subStats.upcomingThisCycle, 0)}</b></span>
        <span className="sf-stat"><i>NEXT</i> {D.subStats.next30[0]
          ? <b>{D.subStats.next30[0].s.name} · {D.subNextLabel(D.subStats.next30[0].s)} · CHF {D.chf(D.subStats.next30[0].s.amount, 0)}</b>
          : <b>—</b>}</span>
        <span className="sf-note"><window.Dot tone="blue" size={6} /> + 1 annual charge — SERAFE · MAR · CHF 335</span>
      </div>
    </div>
  );
}

/* ============================ price history bars =========================== */
function SubHistBars({ s, w = 300, h = 96 }) {
  const D = window.PHOSK;
  const data = s.hist;
  const yearly = s.cadence === "yearly";
  const labels = yearly ? ["'23", "'24", "'25"] : ["JAN", "FEB", "MAR", "APR", "MAY", "JUN"];
  const max = Math.max(...data) * 1.16;
  const padB = 15, padT = 8;
  const bw = (w / data.length) * 0.5;
  const y = (v) => h - padB - (v / max) * (h - padT - padB);
  const rose = data[data.length - 1] > data[0];
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
function CycleMeter({ s }) {
  const D = window.PHOSK;
  if (s.cadence !== "monthly") {
    return <div className="cyc-meter yearly"><div className="cyc-fill" style={{ width: "8%" }} /><span className="cyc-mk" style={{ left: "8%" }} /></div>;
  }
  const du = D.subDaysUntil(s);
  const overdue = s.status === "due";
  const frac = overdue ? 1 : Math.max(0.02, Math.min(1, 1 - du / D.cycle.days));
  const col = overdue ? "var(--neon)" : du <= 4 ? "var(--warn)" : "var(--indigo)";
  return (
    <div className={"cyc-meter" + (overdue ? " over" : "")}>
      <div className="cyc-fill" style={{ width: frac * 100 + "%", background: col, boxShadow: `0 0 6px ${col}` }} />
      <span className="cyc-mk" style={{ left: frac * 100 + "%" }} />
    </div>
  );
}

/* ============================ SUBSCRIPTION CARD =========================== */
function SubCard({ s, active, onSelect, amountMode }) {
  const D = window.PHOSK;
  const st = D.subStatus(s);
  const mo = D.subMonthlyEquiv(s), yr = D.subAnnual(s);
  const showAnnual = amountMode === "annual";
  const big = showAnnual ? yr : s.amount;
  const unit = showAnnual ? "/ YR" : (s.cadence === "yearly" ? "/ YR" : "/ MO");
  const second = showAnnual ? `CHF ${D.chf(mo)} / mo` : `CHF ${D.chf(yr, 0)} / yr`;
  const du = s.cadence === "monthly" ? D.subDaysUntil(s) : null;
  return (
    <div className={"sub osc-bkt " + st.tone + (active ? " on" : "")} role="button" tabIndex={0}
      onClick={() => onSelect(s.id)} onKeyDown={(e) => { if (e.key === "Enter" || e.key === " ") { e.preventDefault(); onSelect(s.id); } }}>
      <span className="osc-leg">{st.label}</span>
      <div className="sub-h">
        <div className="sub-gl"><b>{s.glyph}</b></div>
        <div className="sub-id">
          <span className="sub-nm">{s.name}</span>
          <span className="sub-cat"><window.Dot tone={s.src === "llm" ? "blue" : "ok"} size={5} />{s.cat}</span>
        </div>
        <span className={"sub-src" + (s.src === "llm" ? " auto" : "")}>{s.src === "llm" ? "AUTO" : "USER"}</span>
      </div>

      <div className="sub-amt">
        <span className="sp"><span className="cur">CHF</span>{D.chf(big, big % 1 ? 2 : (showAnnual || s.cadence === "yearly" ? 0 : 2))}</span>
        <span className="un">{unit}</span>
        <span className="eq">{second}</span>
      </div>

      <CycleMeter s={s} />
      <div className="sub-next">
        {s.status === "due"
          ? <span className="nx alert">⚠ NOT SEEN THIS CYCLE</span>
          : s.cadence === "yearly"
            ? <span className="nx">NEXT · {D.subNextLabel(s)}</span>
            : <span className={"nx" + (du <= 4 ? " warn" : "")}>NEXT · {D.subNextLabel(s)} · IN {du}D</span>}
        <span className="cad">{s.cadence === "yearly" ? "YEARLY" : "MONTHLY · " + s.day}</span>
      </div>

      <div className="sub-foot">
        <span className="since">SINCE {s.since}</span>
        <div className="pricetrack">
          <span className="pl">PRICE</span>
          <window.Spark data={s.hist} w={70} h={20} tone="indigo" />
        </div>
      </div>
    </div>
  );
}

/* ============================ compact ROW variant ========================= */
function SubRow({ s, active, onSelect, amountMode }) {
  const D = window.PHOSK;
  const st = D.subStatus(s);
  const yr = D.subAnnual(s);
  const du = s.cadence === "monthly" ? D.subDaysUntil(s) : null;
  const showAnnual = amountMode === "annual";
  return (
    <div className={"subrow" + (active ? " on" : "")} onClick={() => onSelect(s.id)}>
      <div className="sr-gl"><b>{s.glyph}</b></div>
      <span className="sr-nm">{s.name}</span>
      <span className={"sr-stat " + st.tone}>{st.label}</span>
      <div className="sr-meter"><CycleMeter s={s} /></div>
      <span className="sr-next">{s.status === "due" ? "⚠ —" : D.subNextLabel(s)}{du != null && s.status !== "due" ? " · " + du + "D" : ""}</span>
      <span className="sr-amt">CHF <b>{D.chf(showAnnual ? yr : s.amount, (showAnnual || s.cadence === "yearly") ? 0 : 2)}</b> <i>{showAnnual ? "/yr" : (s.cadence === "yearly" ? "/yr" : "/mo")}</i></span>
      <span className={"sr-src" + (s.src === "llm" ? " auto" : "")}>{s.src === "llm" ? "AUTO" : "USER"}</span>
    </div>
  );
}

/* ============================ INSPECTOR (right dock) ====================== */
function SubInspector({ s, onClose, variant }) {
  const D = window.PHOSK;
  if (!s) {
    return (
      <aside className={"sig-panel sub-insp" + (variant ? " " + variant : "")}>
        <div className="sig-empty">
          <span className="mk">⊟</span>
          <div className="tx">No subscription selected.<br />Click any <b>impulse</b> on the sweep or a card to inspect its price history, cadence and AI guidance.</div>
        </div>
      </aside>
    );
  }
  const st = D.subStatus(s);
  const mo = D.subMonthlyEquiv(s), yr = D.subAnnual(s);
  const rose = s.hist[s.hist.length - 1] > s.hist[0];
  const recent = D.subRecent(s);
  const guidance = s.status === "due"
    ? <><b className="coral">⚠ Not seen this cycle.</b> {s.note} Mark it paid, or pause if you've cancelled.</>
    : s.status === "watch"
      ? <><b className="coral">Review suggested.</b> {s.note}</>
      : s.status === "soon"
        ? <><b style={{ color: "var(--warn)" }}>Charging soon.</b> {s.note}</>
        : rose
          ? <><b>Price rose</b> over the last year. {s.note} The AI flags creep on standing charges.</>
          : <>{s.note} On schedule — the AI watches for missed or duplicated charges.</>;
  return (
    <aside className={"sig-panel sub-insp" + (variant ? " " + variant : "")}>
      <div className="sig-head">
        <div className="kls">⊟ SUBSCRIPTION · {s.cat}</div>
        <div className="nm">{s.name}</div>
        <div className="ds">{s.note}</div>
        {onClose && <span className="x" onClick={onClose} title="Close">✕</span>}
      </div>

      <div className="sig-delta">
        <span className="big up"><span style={{ fontSize: 16, color: "var(--ink-3)", marginRight: 5, verticalAlign: 4 }}>CHF</span>{D.chf(s.amount, s.amount % 1 ? 2 : 0)}</span>
        <span className="vs">per charge · {s.cadence} · annualized CHF {D.chf(yr, 0)}</span>
      </div>

      <div className="sig-chart">
        <SubHistBars s={s} />
        <div className="axis"><span>{s.cadence === "yearly" ? "3 YEARS" : "6 CHARGES"}</span><span>{rose ? "PRICE ROSE" : "FLAT"}</span></div>
      </div>

      <div className="sig-stats">
        <div className="st"><div className="k">Per charge</div><div className="v coral">CHF {D.chf(s.amount, s.amount % 1 ? 2 : 0)}</div></div>
        <div className="st"><div className="k">Monthly</div><div className="v">CHF {D.chf(mo)}</div></div>
        <div className="st"><div className="k">Annualized</div><div className="v">CHF {D.chf(yr, 0)}</div></div>
        <div className="st"><div className="k">Next charge</div><div className="v" style={{ fontSize: 16 }}>{D.subNextLabel(s)}</div></div>
        <div className="st"><div className="k">Cadence</div><div className="v" style={{ fontSize: 15 }}>{s.cadence === "yearly" ? "YEARLY · " + s.month : "MONTHLY · " + s.day}</div></div>
        <div className="st"><div className="k">Tracked since</div><div className="v" style={{ fontSize: 16 }}>{s.since}</div></div>
      </div>

      <div className="sig-recent">
        <div className="h">Recent charges</div>
        {recent.map((o, i) => (
          <div className="sig-occ" key={i}>
            <span className="dt">{o.date}</span>
            <span className="no" style={{ flex: 1 }}>{s.src === "llm" ? "auto-detected" : "confirmed"}</span>
            <span className="pr">CHF {D.chf(o.amount)}</span>
          </div>
        ))}
      </div>

      <div className="insp-acts">
        <button className="gbtn p" onClick={() => s.status === "due"
          ? window.phoskApi.post('/subscriptions/' + s.id + '/mark-paid')
          : window.phoskApi.post('/subscriptions/' + s.id + '/pause')}>{s.status === "due" ? "MARK PAID" : "PAUSE"}</button>
        <button className="gbtn coral" onClick={() => window.phoskApi.post('/subscriptions/' + s.id + '/cancel')}>CANCEL</button>
      </div>

      <div className="sig-foot">
        <div className="tx">{guidance}</div>
      </div>
    </aside>
  );
}

Object.assign(window, { BillingSweep, SubHistBars, CycleMeter, SubCard, SubRow, SubInspector });
