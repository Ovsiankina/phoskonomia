/* Phoskonomia — Debts page components.
   Outstanding balances = a DECAYING WAVEFORM. The hero is the PAYOFF
   TRAJECTORY: the combined balance envelope damping from history, through
   today, down to the projected debt-free point. Below it, cards/rows per debt
   with a payoff-progress meter and balance decay; a right-dock inspector shows
   the amortization line, interest cost and AI payoff guidance. */
const { useMemo: useMemoDC } = React;

/* ============================ PAYOFF TRAJECTORY (hero) ===================== */
function PayoffTrajectory({ showProjection = true, strategy = "none" }) {
  const D = window.PHOSK;
  const S = D.debtStats;
  const traj = D.debtTrajectory;
  const W = 1000, H = 196, padL = 54, padR = 64, padT = 26, padB = 34;
  const base = H - padB;
  const mMin = traj[0].m, mMax = traj[traj.length - 1].m;
  const maxY = Math.max(...traj.map((p) => p.total)) * 1.06;
  const x = (m) => padL + (m - mMin) / (mMax - mMin) * (W - padL - padR);
  const y = (v) => base - (v / maxY) * (base - padT);

  const histPts = traj.filter((p) => p.m <= 0);
  const projPts = traj.filter((p) => p.m >= 0);
  const toPts = (arr) => arr.map((p) => `${x(p.m).toFixed(1)},${y(p.total).toFixed(1)}`).join(" ");
  const histLine = toPts(histPts);
  const projLine = toPts(projPts);
  const histArea = `${x(histPts[0].m)},${base} ${histLine} ${x(0)},${base}`;
  const projArea = `${x(0)},${base} ${projLine} ${x(mMax)},${base}`;

  // y gridlines at round franc levels
  const step = maxY > 30000 ? 10000 : 5000;
  const lines = [];
  for (let v = step; v < maxY; v += step) lines.push(v);

  // x ticks: today, +12, +24, debt-free
  const ticks = [0];
  if (mMax >= 12) ticks.push(12);
  if (mMax >= 24) ticks.push(24);
  ticks.push(mMax);

  const dfX = x(mMax), dfY = y(0);

  return (
    <div className="traj osc-bkt blue">
      <span className="osc-leg">PAYOFF TRAJECTORY</span>
      <div className="traj-h">
        <span className="hud">∿ COMBINED BALANCE DECAY · {D.cycle.label} → {S.debtFreeLabel}</span>
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
            {m === 0 ? "NOW" : D.debtMonthLabel(m)}
          </text>
        ))}

        {/* projection area + line (under history) */}
        {showProjection && <polygon points={projArea} fill="url(#trajproj)" />}
        {showProjection && <polyline points={projLine} fill="none" stroke="var(--indigo-neon)" strokeWidth="1.6" strokeDasharray="5 4" opacity=".85" />}

        {/* history area + line */}
        <polygon points={histArea} fill="url(#trajfill)" />
        <polyline points={histLine} fill="none" stroke="var(--neon)" strokeWidth="2.2" strokeLinejoin="round"
          style={{ filter: "drop-shadow(0 0 3px var(--neon))" }} />

        {/* today marker */}
        <line x1={x(0)} y1={padT - 8} x2={x(0)} y2={base + 6} stroke="rgba(255,59,46,.5)" strokeWidth="1.2" strokeDasharray="3 3" />
        <text x={x(0)} y={padT - 12} textAnchor="middle" fill="var(--neon-dim)" fontSize="8.5" fontFamily="var(--font-body)" letterSpacing=".12em">TODAY · {D.cycle.asOf}</text>
        <circle cx={x(0)} cy={y(traj.find((p) => p.m === 0).total)} r="3.5" fill="var(--neon-white)" style={{ filter: "drop-shadow(0 0 4px var(--neon))" }} />

        {/* debt-free endpoint */}
        {showProjection && (
          <g>
            <circle cx={dfX} cy={dfY} r="4" fill="var(--bg)" stroke="var(--ok)" strokeWidth="1.8" style={{ filter: "drop-shadow(0 0 5px var(--ok))" }} />
            <text x={dfX} y={dfY - 11} textAnchor="end" fill="var(--ok)" fontSize="9" fontFamily="var(--font-display)" letterSpacing=".04em">DEBT-FREE</text>
            <text x={dfX} y={dfY - 1} textAnchor="end" fill="var(--ink-3)" fontSize="8" fontFamily="var(--font-body)" letterSpacing=".06em">{S.debtFreeLabel}</text>
          </g>
        )}
      </svg>

      <div className="traj-foot">
        <span className="tf-stat"><i>PAID DOWN</i> <b>CHF {D.chf(S.totalOrig - S.totalOwed, 0)}</b> <em>of {D.chf(S.totalOrig, 0)}</em></span>
        <span className="tf-stat"><i>AVG RATE</i> <b className="warn">{(S.weightedApr * 100).toFixed(1)}%</b></span>
        <span className="tf-stat"><i>DEBT-FREE IN</i> <b>{S.horizon} MO</b></span>
        <span className="tf-note">
          <window.Dot tone="alert" size={6} /> {strategy === "snowball"
            ? <>SNOWBALL · smallest first → {S.snowballTarget.name}</>
            : <>AVALANCHE · {S.avalancheTarget.name} costs {(S.avalancheTarget.apr * 100).toFixed(1)}% — target it first</>}
        </span>
      </div>
    </div>
  );
}

/* ============================ payoff-progress meter ======================== */
function PayoffMeter({ d, tone }) {
  const D = window.PHOSK;
  const p = D.debtPaidOffPct(d);
  const col = tone === "coral" ? "var(--neon)" : tone === "warn" ? "var(--warn)" : "var(--indigo-neon)";
  return (
    <div className="pay-meter">
      <div className="pay-fill" style={{ width: Math.max(2, p * 100) + "%", background: col, boxShadow: `0 0 6px ${col}` }} />
      <span className="pay-mk" style={{ left: p * 100 + "%" }} />
    </div>
  );
}

/* ============================ balance decay line (inspector) =============== */
function DecayLine({ d, w = 300, h = 104 }) {
  const D = window.PHOSK;
  const hist = d.hist;                          // 6 months, oldest→now
  const fwd = D.debtForwardSeries(d, 14).slice(1, 13); // next 12 months
  const series = [...hist, ...fwd];
  const todayIdx = hist.length - 1;
  const maxY = Math.max(...series) * 1.08;
  const padB = 16, padT = 8, padL = 4, padR = 4;
  const x = (i) => padL + i / (series.length - 1) * (w - padL - padR);
  const y = (v) => h - padB - (v / maxY) * (h - padT - padB);
  const histPts = series.slice(0, todayIdx + 1).map((v, i) => `${x(i).toFixed(1)},${y(v).toFixed(1)}`).join(" ");
  const projPts = series.slice(todayIdx).map((v, i) => `${x(i + todayIdx).toFixed(1)},${y(v).toFixed(1)}`).join(" ");
  const rose = hist[hist.length - 1] > hist[0];
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
function DebtCard({ d, active, onSelect, target }) {
  const D = window.PHOSK;
  const st = D.debtStatus(d);
  const pct = Math.round(D.debtPaidOffPct(d) * 100);
  const months = D.debtMonthsToPayoff(d);
  const du = D.debtDaysUntil(d);
  const isTarget = target === d.id;
  return (
    <div className={"debt osc-bkt " + st.tone + (active ? " on" : "") + (isTarget ? " target" : "")} role="button" tabIndex={0}
      onClick={() => onSelect(d.id)} onKeyDown={(e) => { if (e.key === "Enter" || e.key === " ") { e.preventDefault(); onSelect(d.id); } }}>
      <span className="osc-leg">{isTarget ? "◎ TARGET" : st.label}</span>
      <div className="debt-h">
        <div className="debt-gl"><b>{d.glyph}</b></div>
        <div className="debt-id">
          <span className="debt-nm">{d.name}</span>
          <span className="debt-len"><window.Dot tone={d.src === "llm" ? "blue" : "ok"} size={5} />{d.lender}</span>
        </div>
        <span className="debt-type">{d.type}</span>
      </div>

      <div className="debt-amt">
        <span className="sp"><span className="cur">CHF</span>{D.chf(d.balance, 0)}</span>
        <span className="un">OWED</span>
        <span className="apr">{(d.apr * 100).toFixed(1)}% APR</span>
      </div>

      <div className="debt-prog">
        <PayoffMeter d={d} tone={st.tone} />
        <div className="debt-progmeta">
          <span>{pct}% PAID OFF</span>
          <span>CHF {D.chf(d.orig - d.balance, 0)} OF {D.chf(d.orig, 0)}</span>
        </div>
      </div>

      <div className="debt-next">
        {d.status === "high"
          ? <span className="nx alert">⚠ COSTLIEST RATE YOU CARRY</span>
          : d.status === "due"
            ? <span className="nx warn">⚠ DUE IN {du}D · {D.debtNextLabel(d)}</span>
            : <span className="nx">NEXT · {D.debtNextLabel(d)} · IN {du}D</span>}
        <span className="pay">CHF {D.chf(d.monthly, 0)}/MO</span>
      </div>

      <div className="debt-foot">
        <span className="term">{d.type === "CARD" ? "REVOLVING" : "PAYOFF " + D.debtMonthLabel(months)} · {months >= 600 ? "—" : months + " MO"}</span>
        <div className="decaytrack">
          <span className="pl">BALANCE</span>
          <window.Spark data={d.hist} w={70} h={20} tone={d.status === "high" ? "neon" : "indigo"} />
        </div>
      </div>
    </div>
  );
}

/* ============================ compact ROW variant ========================= */
function DebtRow({ d, active, onSelect, target }) {
  const D = window.PHOSK;
  const st = D.debtStatus(d);
  const pct = Math.round(D.debtPaidOffPct(d) * 100);
  const du = D.debtDaysUntil(d);
  const isTarget = target === d.id;
  return (
    <div className={"debtrow" + (active ? " on" : "") + (isTarget ? " target" : "")} onClick={() => onSelect(d.id)}>
      <div className="dr-gl"><b>{d.glyph}</b></div>
      <span className="dr-nm">{d.name}</span>
      <span className={"dr-stat " + st.tone}>{isTarget ? "◎ TARGET" : st.label}</span>
      <div className="dr-meter"><PayoffMeter d={d} tone={st.tone} /></div>
      <span className="dr-apr">{(d.apr * 100).toFixed(1)}%</span>
      <span className="dr-next">{D.debtNextLabel(d)} · {du}D</span>
      <span className="dr-amt">CHF <b>{D.chf(d.balance, 0)}</b></span>
      <span className="dr-pay">CHF {D.chf(d.monthly, 0)}<i>/mo</i></span>
    </div>
  );
}

/* ============================ INSPECTOR (right dock) ====================== */
function DebtInspector({ d, onClose, variant, target }) {
  const D = window.PHOSK;
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
  const st = D.debtStatus(d);
  const months = D.debtMonthsToPayoff(d);
  const intRem = D.debtInterestRemaining(d);
  const pct = Math.round(D.debtPaidOffPct(d) * 100);
  const isTarget = target === d.id;
  const guidance = d.status === "high"
    ? <><b className="coral">⚠ Highest rate you carry.</b> {d.note} Direct every spare franc here first — the avalanche clears the most interest per franc.</>
    : d.status === "due"
      ? <><b style={{ color: "var(--warn)" }}>Instalment due shortly.</b> {d.note}</>
      : d.status === "watch"
        ? <><b style={{ color: "var(--warn)" }}>Worth a review.</b> {d.note}</>
        : <>{d.note} On track — at CHF {D.chf(d.monthly, 0)}/mo this clears in {months} months.</>;
  return (
    <aside className={"sig-panel debt-insp" + (variant ? " " + variant : "")}>
      <div className="sig-head">
        <div className="kls">∿ DEBT · {d.type} · {d.lender}</div>
        <div className="nm">{d.name}</div>
        <div className="ds">{d.note}</div>
        {onClose && <span className="x" onClick={onClose} title="Close">✕</span>}
      </div>

      <div className="sig-delta">
        <span className="big up"><span style={{ fontSize: 16, color: "var(--ink-3)", marginRight: 5, verticalAlign: 4 }}>CHF</span>{D.chf(d.balance, 0)}</span>
        <span className="vs">outstanding · {pct}% paid off · {(d.apr * 100).toFixed(1)}% APR</span>
      </div>

      <div className="sig-chart">
        <DecayLine d={d} />
        <div className="axis"><span>6 MO BACK</span><span>{d.type === "CARD" ? "REVOLVING" : "PROJECTED →"}</span></div>
      </div>

      <div className="sig-stats">
        <div className="st"><div className="k">Outstanding</div><div className="v coral">CHF {D.chf(d.balance, 0)}</div></div>
        <div className="st"><div className="k">Monthly</div><div className="v">CHF {D.chf(d.monthly, 0)}</div></div>
        <div className="st"><div className="k">Interest / yr</div><div className="v">CHF {D.chf(D.debtAnnualInterest(d), 0)}</div></div>
        <div className="st"><div className="k">Interest left</div><div className="v" style={{ fontSize: 16 }}>{intRem === Infinity ? "∞" : "CHF " + D.chf(intRem, 0)}</div></div>
        <div className="st"><div className="k">Payoff</div><div className="v" style={{ fontSize: 16 }}>{months >= 600 ? "—" : D.debtMonthLabel(months)}</div></div>
        <div className="st"><div className="k">Since</div><div className="v" style={{ fontSize: 16 }}>{d.since}</div></div>
      </div>

      <div className="sig-recent">
        <div className="h">Recent payments</div>
        {d.hist.slice().reverse().slice(0, 4).map((bal, i) => {
          const prev = d.hist[d.hist.length - 1 - i - 1];
          const paid = prev != null ? prev - bal : null;
          return (
            <div className="sig-occ" key={i}>
              <span className="dt">{D.debtMonthLabel(-i)}</span>
              <span className="no" style={{ flex: 1 }}>{paid != null && paid > 0 ? "− CHF " + D.chf(paid, 0) + " paid" : paid != null && paid < 0 ? "+ CHF " + D.chf(-paid, 0) + " added" : "balance " + D.chf(bal, 0)}</span>
              <span className="pr">CHF {D.chf(bal, 0)}</span>
            </div>
          );
        })}
      </div>

      <div className="insp-acts">
        <button className="gbtn p" onClick={() => window.phoskApi.post('/debts/' + d.id + '/payments', { amount: d.monthly })}>PAY EXTRA</button>
        <button className={"gbtn" + (d.status === "high" ? " coral" : "")}
          onClick={() => d.apr > 0.08
            ? window.phoskApi.post('/debts/' + d.id + '/refinance', { apr: d.apr })
            : window.phoskApi.patch('/debts/' + d.id + '/plan', { monthly: d.monthly, day: d.day, term: d.term })}>
          {d.apr > 0.08 ? "REFINANCE" : "ADJUST PLAN"}</button>
      </div>

      <div className="sig-foot">
        <div className="tx">{guidance}</div>
      </div>
    </aside>
  );
}

Object.assign(window, { PayoffTrajectory, PayoffMeter, DecayLine, DebtCard, DebtRow, DebtInspector });

/* ============================ PERSONAL · IOU LEDGER ======================== */
/* A two-sided net-position beam: you-owe (coral) diverges left of zero,
   owed-to-you (blue) diverges right. Informal money between people — kept
   visually and structurally apart from the institutional debt above. */
function NetBeam() {
  const D = window.PHOSK;
  const S = D.personalStats;
  const W = 1000, H = 96, pad = 150, cx0 = W / 2, axisY = 52;
  const half = W / 2 - pad;
  const maxTotal = Math.max(S.owedToYou, S.youOwe, 1);
  const rx = cx0 + (S.owedToYou / maxTotal) * half;
  const lx = cx0 - (S.youOwe / maxTotal) * half;
  const netX = cx0 + (S.net / maxTotal) * half;
  const bh = 16;
  return (
    <div className="iou-beam osc-bkt">
      <span className="osc-leg">NET POSITION</span>
      <div className="beam-h">
        <span className="hud">⟷ PERSONAL · IOU LEDGER · {S.count} OPEN</span>
        <span className={"beam-net " + (S.net >= 0 ? "pos" : "neg")}>
          NET {S.net >= 0 ? "+" : "−"}CHF {D.chf(Math.abs(S.net), 0)} {S.net >= 0 ? "IN YOUR FAVOUR" : "YOU'RE BEHIND"}
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
        <path d={`M ${netX} ${axisY - bh / 2 - 11} l -5 -7 l 10 0 z`} fill={S.net >= 0 ? "var(--indigo-neon)" : "var(--neon)"} style={{ filter: `drop-shadow(0 0 4px ${S.net >= 0 ? "var(--indigo-neon)" : "var(--neon)"})` }} />
        {/* end labels */}
        <text x={lx - 10} y={axisY + 4} textAnchor="end" fill="var(--neon-hot)" fontSize="14" fontFamily="var(--font-display)">{D.chf(S.youOwe, 0)}</text>
        <text x={lx - 10} y={axisY - 11} textAnchor="end" fill="var(--ink-3)" fontSize="7.5" fontFamily="var(--font-body)" letterSpacing=".14em">YOU OWE</text>
        <text x={rx + 10} y={axisY + 4} textAnchor="start" fill="var(--text-blue)" fontSize="14" fontFamily="var(--font-display)">{D.chf(S.owedToYou, 0)}</text>
        <text x={rx + 10} y={axisY - 11} textAnchor="start" fill="var(--ink-3)" fontSize="7.5" fontFamily="var(--font-body)" letterSpacing=".14em">OWED TO YOU</text>
      </svg>
    </div>
  );
}

function PersonCard({ p }) {
  const D = window.PHOSK;
  const inbound = p.dir === "in";
  const pct = p.of ? Math.round((1 - p.amount / p.of) * 100) : null;
  return (
    <div className={"person " + (inbound ? "in" : "out")}>
      <div className="person-h">
        <div className="person-av"><b>{p.initials}</b></div>
        <div className="person-id">
          <span className="person-nm">{p.person}</span>
          <span className={"person-dir " + (inbound ? "in" : "out")}>{inbound ? "← OWED TO YOU" : "YOU OWE →"}</span>
        </div>
        <div className="person-amt">
          <span className="cur">CHF</span>{D.chf(p.amount, 0)}
        </div>
      </div>
      <div className="person-reason">{p.reason}</div>
      {pct != null && (
        <div className="person-prog">
          <div className="pp-bar"><div className="pp-fill" style={{ width: pct + "%" }} /></div>
          <span className="pp-meta">{pct}% REPAID · CHF {D.chf(p.of - p.amount, 0)} OF {D.chf(p.of, 0)}</span>
        </div>
      )}
      <div className="person-foot">
        <span className="since">SINCE {p.since}</span>
        <div className="person-acts">
          <button className="gbtn"
            onClick={() => inbound
              ? window.phoskApi.post('/personal-ious/' + p.id + '/remind', {})
              : window.phoskApi.post('/personal-ious/' + p.id + '/settle-up', {})}>
            {inbound ? "REMIND" : "SETTLE UP"}</button>
          <button className="gbtn p" onClick={() => window.phoskApi.post('/personal-ious/' + p.id + '/settle', {})}>MARK SETTLED</button>
        </div>
      </div>
    </div>
  );
}

Object.assign(window, { NetBeam, PersonCard });
