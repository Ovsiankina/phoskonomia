/* Phoskonomia — Analytics page components. The retrospective surface: a 12-cycle
   SPEND TREND oscilloscope, the ITEM-SIGNAL matrix (the brand's signature read),
   per-category MOMENTUM small-multiples, and a weekday SPENDING RHYTHM. */
const { useMemo: useMemoAC } = React;

/* ============================ SPEND TREND (hero) ========================== */
/* A multi-cycle scope: monthly spend as molten bars under a connecting trace,
   a dashed budget reference, and the current cycle drawn hollow (projected).
   mode 'rate' swaps to the cashflow savings-rate curve. */
function SpendTrend({ windowN = 12, mode = "spend" }) {
  const D = window.PHOSK;
  const all = D.spendHistory;
  const data = all.slice(Math.max(0, all.length - windowN));
  const S = D.histStats;
  const W = 1000, H = 212, padL = 52, padR = 20, padT = 24, padB = 30;
  const base = H - padB;
  const n = data.length;
  const slot = (W - padL - padR) / n;
  const cx = (i) => padL + slot * (i + 0.5);

  const isRate = mode === "rate";
  const maxY = isRate
    ? Math.max(...data.map((d) => d.rate)) * 1.25
    : Math.max(D.spendHistory[0].budget, ...data.map((d) => d.spend)) * 1.08;
  const val = (d) => (isRate ? d.rate : d.spend);
  const y = (v) => base - (v / maxY) * (base - padT);

  // y gridlines
  const lines = isRate
    ? [0.1, 0.2, 0.3].map((v) => ({ v, lab: Math.round(v * 100) + "%" }))
    : [2000, 4000].map((v) => ({ v, lab: (v / 1000) + "k" }));

  const linePts = data.map((d, i) => `${cx(i).toFixed(1)},${y(val(d)).toFixed(1)}`).join(" ");
  const areaPts = `${cx(0)},${base} ${linePts} ${cx(n - 1)},${base}`;
  const barW = Math.min(slot * 0.5, 30);
  const budgetY = y(D.spendHistory[0].budget);

  return (
    <div className="atrend osc-bkt blue">
      <span className="osc-leg">{isRate ? "SAVINGS-RATE TREND" : "SPEND TREND"}</span>
      <div className="atrend-h">
        <span className="hud">⌁ {isRate ? "CASHFLOW SAVED" : "SPEND"} · LAST {n} CYCLES · {data[0].m}{data[0].yr} → {S.cur.m}{S.cur.yr}</span>
        <div className="atrend-key">
          {!isRate && <span><i className="k spend" /> SPEND</span>}
          {!isRate && <span><i className="k bud" /> BUDGET {D.chf(D.spendHistory[0].budget, 0)}</span>}
          {isRate && <span><i className="k rate" /> SAVED / INCOME</span>}
          <span><i className="k proj" /> CURRENT · PROJECTED</span>
        </div>
      </div>

      <svg width="100%" height={H} viewBox={`0 0 ${W} ${H}`} preserveAspectRatio="none" className="atrend-svg">
        <defs>
          <linearGradient id="atrendfill" x1="0" y1="0" x2="0" y2="1">
            <stop offset="0%" stopColor={isRate ? "rgba(143,125,255,.26)" : "rgba(255,94,77,.28)"} />
            <stop offset="100%" stopColor={isRate ? "rgba(143,125,255,0)" : "rgba(255,94,77,0)"} />
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
        {!isRate && (
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
          const col = isRate ? "rgba(143,125,255,.42)" : over ? "rgba(255,59,46,.5)" : "rgba(255,94,77,.34)";
          if (proj) {
            return (
              <g key={i}>
                <rect x={bx} y={by} width={barW} height={Math.max(0, bh)} fill="none"
                  stroke={isRate ? "var(--indigo-neon)" : "var(--neon)"} strokeWidth="1.3" strokeDasharray="3 2" opacity=".9" />
                <rect x={bx} y={by} width={barW} height={Math.max(0, bh)} fill={isRate ? "rgba(143,125,255,.1)" : "rgba(255,94,77,.1)"} />
              </g>
            );
          }
          return <rect key={i} x={bx} y={by} width={barW} height={Math.max(0, bh)} fill={col} />;
        })}

        {/* connecting trace */}
        <polygon points={areaPts} fill="url(#atrendfill)" />
        <polyline points={linePts} fill="none" stroke={isRate ? "var(--indigo-neon)" : "var(--neon)"} strokeWidth="2.2"
          strokeLinejoin="round" strokeLinecap="round" style={{ filter: `drop-shadow(0 0 3px ${isRate ? "var(--indigo-neon)" : "var(--neon)"})` }} />

        {/* point markers + current */}
        {data.map((d, i) => {
          const last = i === n - 1;
          return <circle key={i} cx={cx(i)} cy={y(val(d))} r={last ? 4 : 2.4}
            fill={last ? "var(--neon-white)" : (isRate ? "var(--indigo-neon)" : "var(--neon)")}
            style={last ? { filter: `drop-shadow(0 0 4px ${isRate ? "var(--indigo-neon)" : "var(--neon)"})` } : {}} />;
        })}

        {/* x labels */}
        {data.map((d, i) => (
          <text key={i} x={cx(i)} y={base + 16} textAnchor="middle"
            fill={d.projected ? "var(--neon-dim)" : "var(--ink-3)"} fontSize="8.5" fontFamily="var(--font-body)" letterSpacing=".06em">{d.m}</text>
        ))}
      </svg>

      <div className="atrend-foot">
        <span className="tf"><i>6-MO AVG</i> <b>CHF {D.chf(S.avg, 0)}</b></span>
        <span className="tf"><i>PEAK</i> <b className="warn">CHF {D.chf(S.peak.spend, 0)}</b> <em>{S.peak.m} {S.peak.yr}</em></span>
        <span className="tf"><i>LEANEST</i> <b style={{ color: "var(--ok)" }}>CHF {D.chf(S.low.spend, 0)}</b> <em>{S.low.m} {S.low.yr}</em></span>
        <span className="tf-note">
          <window.Dot tone={S.curVsAvgPct > 0 ? "alert" : "ok"} size={6} />
          THIS CYCLE TRACKING {S.curVsAvgPct > 0 ? "+" : "−"}{Math.abs(S.curVsAvgPct)}% vs 6-MO AVG · {S.curVsPrevPct > 0 ? "↑" : "↓"} {Math.abs(S.curVsPrevPct)}% vs {S.prev.m}
        </span>
      </div>
    </div>
  );
}

/* ============================ ITEM-SIGNAL MATRIX ROW ====================== */
function ItemSignalRow({ sig, active, onSelect, rank }) {
  const D = window.PHOSK;
  const cand = !!sig.candidate;
  const up = (sig.deltaPct || 0) >= 0;
  const delta = cand ? "NEW" : (up ? "↑" : "↓") + Math.abs(sig.deltaPct) + "%";
  return (
    <button className={"isig" + (active ? " on" : "") + (cand ? " cand" : "")} onClick={() => { onSelect(sig.id); if (cand) window.phoskApi.post('/signals/candidates/' + sig.id + '/track'); }}>
      <span className="isig-rk">{cand ? "—" : rank}</span>
      <span className="isig-gl">⌁</span>
      <div className="isig-id">
        <span className="isig-nm">{sig.label}</span>
        <span className="isig-pa">{cand ? sig.desc : sig.parent + " · since " + sig.since}</span>
      </div>
      <div className="isig-spk"><window.SigSpark data={sig.series} w={150} h={38} /></div>
      <span className={"isig-dl " + (cand ? "cand" : up ? "up" : "down")}>{delta}</span>
      <div className="isig-num">
        <span className="v">{cand ? sig.cycleQty : sig.cycleQty} <i>{sig.unit}</i></span>
        <span className="s">{cand ? "this cycle" : "CHF " + D.chf(sig.cycleSpend) + " · " + sig.txns + " txns"}</span>
      </div>
      <span className="isig-go">{cand ? "TRACK ▸" : "▸"}</span>
    </button>
  );
}

/* ============================ SIGNAL MOVER CARD =========================== */
function MoverCard({ sig, kind }) {
  const D = window.PHOSK;
  const up = kind === "riser";
  return (
    <div className={"mover " + (up ? "up" : "down")}>
      <div className="mover-h">
        <span className="lbl">{up ? "↑ FASTEST RISER" : "↓ FASTEST FALLER"}</span>
        <span className={"dl " + (up ? "up" : "down")}>{up ? "+" : ""}{sig.deltaPct}%</span>
      </div>
      <div className="mover-nm">{sig.label}</div>
      <div className="mover-spk"><window.SigSpark data={sig.series} w={232} h={44} /></div>
      <div className="mover-sub">{sig.cycleQty} {sig.unit} · CHF {D.chf(sig.cycleSpend)} this cycle · {sig.parent}</div>
    </div>
  );
}

/* ============================ CATEGORY MOMENTUM =========================== */
function MomentumCard({ c }) {
  const D = window.PHOSK;
  const up = c.deltaPct >= 0;
  const strong = Math.abs(c.deltaPct) >= 15;
  const tone = c.fixed ? "flat" : up ? (strong ? "hot" : "up") : "down";
  return (
    <div className={"momo " + tone}>
      <div className="momo-h">
        <span className="cn">{c.name}</span>
        <span className={"dl " + tone}>{c.fixed ? "FIXED" : (up ? "↑" : "↓") + Math.abs(c.deltaPct) + "%"}</span>
      </div>
      <div className="momo-spk">
        <window.Spark data={c.series} w={150} h={30} tone={!c.fixed && up && strong ? "neon" : "indigo"} />
      </div>
      <div className="momo-f">
        <span className="now">CHF {D.chf(c.now, 0)}</span>
        <span className="vs">vs 3-cyc avg</span>
      </div>
    </div>
  );
}

/* ============================ SPENDING RHYTHM ============================= */
function RhythmHeatmap() {
  const D = window.PHOSK;
  const wd = D.weekday, S = D.weekdayStats;
  return (
    <div className="rhythm osc-bkt coral">
      <span className="osc-leg">SPENDING RHYTHM</span>
      <div className="rhythm-h">
        <span className="hud">∿ DISCRETIONARY SPEND · AVG BY WEEKDAY</span>
        <span className="rhythm-meta">{S.weekendShare}% LANDS FRI–SUN · RENT &amp; INSURANCE EXCLUDED</span>
      </div>
      <div className="rhythm-grid">
        {wd.map((x) => {
          const p = x.v / S.max;
          const peak = x.d === S.peak.d;
          return (
            <div className={"rcol" + (peak ? " peak" : "")} key={x.d}>
              <span className="rv">CHF {x.v}</span>
              <div className="rbar-wrap">
                <div className="rbar" style={{ height: Math.max(6, p * 100) + "%" }} />
              </div>
              <span className="rd">{x.d}</span>
            </div>
          );
        })}
      </div>
    </div>
  );
}

Object.assign(window, { SpendTrend, ItemSignalRow, MoverCard, MomentumCard, RhythmHeatmap });
