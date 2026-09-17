/* Phoskonomia — final composed dashboard.
   Console-style HERO up top (oscilloscope instrument), scroll reveals the
   Terminal-style dense grid below. Reuses the shared blocks/primitives. */
const { useState: useStateD, useEffect: useEffectD } = React;

function fmtRowF(D) {
  return {
    budget: D.chf(D.totals.budget, 0),
    spent0: D.chf(D.totals.spent, 0),
    spent: D.chf(D.totals.spent),
    remaining0: D.chf(D.totals.remaining, 0),
    perDay: D.chf(D.totals.remaining / (D.cycle.days - D.cycle.day), 0),
    saved0: D.chf(D.totals.saved, 0),
    spentPct: Math.round(D.totals.spent / D.totals.budget * 100),
    savePct: Math.round(D.totals.saved / D.totals.savingsTarget * 100)
  };
}
function catSparkF(seed) {
  const a = [];let v = 0.5;
  for (let i = 0; i < 12; i++) {v += (Math.sin(seed * 3.1 + i * 1.7) + Math.cos(seed + i)) * 0.18;a.push(v);}
  return a;
}

/* upcoming recurring charges, soonest first (drives the hero NEXT dock) */
const MONTHS_F = ["JAN", "FEB", "MAR", "APR", "MAY", "JUN", "JUL", "AUG", "SEP", "OCT", "NOV", "DEC"];
function parseDayMon(s) {
  if (!s || s === "\u2014") return null;
  const toks = String(s).toUpperCase().split(/[\s\u00b7,]+/).filter(Boolean);
  let mo = -1,day = null;
  for (const t of toks) {
    const mi = MONTHS_F.indexOf(t.slice(0, 3));
    if (mi >= 0) mo = mi;else {const num = parseInt(t, 10);if (!isNaN(num)) day = num;}
  }
  return mo >= 0 && day != null ? { mo, day } : null;
}
function nextDueItems(D, count) {
  const ref = parseDayMon(D.cycle.asOf) || { mo: 0, day: 1 };
  const Y = 2026;
  const refDate = new Date(Y, ref.mo, ref.day);
  return D.recurring.map((r) => {
    const p = parseDayMon(r.next);
    if (!p) return null;
    let d = new Date(Y, p.mo, p.day);
    if (d < refDate) d = new Date(Y + 1, p.mo, p.day);
    return { ...r, days: Math.round((d - refDate) / 86400000) };
  }).filter(Boolean).sort((a, b) => a.days - b.days).slice(0, count);
}

function DashFull() {
  const D = window.PHOSK,f = fmtRowF(D);
  const recTotal = D.chf(D.recurring.reduce((s, r) => s + r.amount, 0), 0);
  const topShops = (() => {
    const m = {};
    D.transactions.forEach((t) => {m[t.shop] = (m[t.shop] || 0) + t.amount;});
    return Object.entries(m).sort((a, b) => b[1] - a[1]).slice(0, 4);
  })();
  const maxShop = topShops[0][1];
  const chans = ["GROCERIES", "DINING & CAFÉS", "TRANSPORT", "SUBSCRIPTIONS", "LEISURE"].
  map((n) => D.categories.find((c) => c.name === n));
  const nextDue = nextDueItems(D, 3);

  const [aiCollapsed, setAiCollapsed] = useStateD(false);
  const [sel, setSel] = useStateD(null);
  const [drawerSig, setDrawerSig] = useStateD(false);
  const [narrow, setNarrow] = useStateD(typeof window !== "undefined" && window.innerWidth < 1280);
  useEffectD(() => {
    const on = () => setNarrow(window.innerWidth < 1280);
    on();window.addEventListener("resize", on);
    return () => window.removeEventListener("resize", on);
  }, []);
  const dockable = !narrow;
  const sigObj = window.PHOSK.signalById(sel);
  const selectSig = (id) => {setSel(id);if (!dockable) setDrawerSig(true);};

  return (
    <div className="pk" style={{ height: "100vh", minHeight: 0 }}>
      <window.ScannerBg className="pk-bg" seed={31} shapes={[
      { char: "8", cx: .14, cy: .5, scale: .3, style: "wire", morph: "vein", live: false, fill: .42 },
      { char: "8", cx: .93, cy: .82, scale: .14, style: "wire", live: false, fill: .24 },
      { char: "e", cx: .8, cy: .26, scale: .18, style: "faint", morph: "blob", live: false, fill: .46 }]
      } />

      <div className="app-shell swap">
        <window.AiPanel collapsed={aiCollapsed} onToggle={() => setAiCollapsed((c) => !c)} onTrack={selectSig} />

        <div className="app-main">
          <window.TopBar active="DASHBOARD" />
          <div className="app-scroll">

        {/* ===================== CONSOLE HERO ===================== */}
        <section className="pk-hero dash-c" data-screen-label="HERO">
          <div className="c-dock glass">
            <div className="c-hero">
              <div className="lbl">REMAINING · {D.cycle.label}</div>
              <div className="big"><span className="cur">CHF</span>{f.remaining0}</div>
              <div className="sub">of CHF {f.budget} budget · {f.spentPct}% spent · 11 days left</div>
            </div>
            <div style={{ display: "flex", justifyContent: "center", padding: "4px 0" }}>
              <window.SavingsDial size={150} saved={D.totals.saved} target={D.totals.savingsTarget} projected={D.totals.savingsProjected} />
            </div>
            <div>
              <div className="hud sm" style={{ marginBottom: 6 }}>SNAPSHOT</div>
              <div className="ministat"><span className="k">Spent</span><span className="v coral">CHF {f.spent0}</span></div>
              <div className="ministat"><span className="k">Saved</span><span className="v">CHF {f.saved0}</span></div>
              <div className="ministat"><span className="k">Savings rate</span><span className="v">{Math.round(D.totals.savingsRate * 100)}%</span></div>
              <div className="ministat"><span className="k">vs last cycle</span><span className="v" style={{ color: "var(--ok)" }}>−34%</span></div>
            </div>
            <div className="osc-bkt coral" style={{ marginTop: "auto", border: "1px solid var(--hairline-warm)", background: "rgba(22,8,16,.4)", padding: "14px 13px 11px" }}>
              <span className="osc-leg">NEXT</span>
              {nextDue.map((n, i) =>
                  <div key={n.name} style={i === 0 ? undefined : { marginTop: 9, paddingTop: 9, borderTop: "1px solid var(--hairline)" }}>
                <div style={{ display: "flex", justifyContent: "space-between", alignItems: "baseline", gap: 10 }}>
                  <span className="num" style={{ fontSize: i === 0 ? 17 : 14, color: "var(--ink)", minWidth: 0, overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" }}>{n.name}</span>
                  <span className="num coral" style={{ fontSize: i === 0 ? 19 : 15, whiteSpace: "nowrap" }}>CHF {D.chf(n.amount)}</span>
                </div>
                <div className="dim" style={{ fontSize: 9.5, letterSpacing: ".12em", marginTop: 3 }}>DUE {n.next} · {n.days <= 0 ? "TODAY" : n.days + (n.days === 1 ? " DAY" : " DAYS")}</div>
              </div>
                  )}
            </div>
          </div>

          <div className="c-main">
            <div className="c-screen">
              <window.ScannerBg className="c-screen-bg" seed={91} bg={false} grid={false} dish={false}
                  shapes={[{ char: "8", cx: .5, cy: .56, scale: .52, r: .92, style: "redneg", morph: "mass", live: true, fill: .6 }]} />
              <div className="scr-hud">
                <span className="hud">SPEND TRACE · {D.cycle.label}</span>
                <span className="hud" style={{ color: "var(--neon-dim)" }}>CHF {f.spent0} / {f.budget} · {f.spentPct}%</span>
              </div>
              <window.PhoskChart width={1150} height={300} showBars={false} showPace showArea showLast padT={34} padB={22} padL={14} padR={14} />
              <div style={{ position: "absolute", bottom: 8, left: 16, display: "flex", gap: 16 }}>
                <span className="hud sm" style={{ color: "var(--neon)" }}>━ THIS CYCLE</span>
                <span className="hud sm" style={{ color: "var(--indigo-neon)" }}>┄ BUDGET PACE</span>
                <span className="hud sm" style={{ color: "rgba(143,125,255,.7)" }}>┄ LAST CYCLE</span>
              </div>
            </div>

            <div className="c-channels">
              {chans.map((c, i) => {
                    const p = c.budget > 0 ? c.spent / c.budget : 0,tone = window.pctTone(p);
                    return (
                      <div className="chan" key={c.name}>
                    <span className="cn">{c.name}</span>
                    <window.Spark data={catSparkF(i + 2)} w={150} h={28} tone={tone === "alert" ? "neon" : "indigo"} />
                    <div className="cv">
                      <span className={"p pct " + tone}>{Math.round(p * 100)}%</span>
                      <span className="s">CHF {D.chf(c.spent, 0)} / {D.chf(c.budget, 0)}</span>
                    </div>
                  </div>);

                  })}
            </div>

            <div className="c-watch">
              <span className="hud sm" style={{ flex: "0 0 auto" }}>WATCH</span>
              <span style={{ color: "var(--neon)" }}>⚠ DINING & CAFÉS 17% OVER CAP</span>
              <span className="dim">·</span>
              <span style={{ color: "var(--indigo-neon)" }}>⌁ ACTIV FITNESS NOT SEEN THIS CYCLE</span>
              <span className="dim">·</span>
              <span style={{ color: "var(--warn)" }}>◷ GROCERIES ON PACE TO EXCEED ~CHF 70</span>
            </div>
          </div>
        </section>

        {/* ===================== ITEM-SIGNALS ===================== */}
        <window.SignalStrip sel={sel} onSelect={selectSig} />

        {/* scroll seam */}
        <div className="pk-seam">
          <span className="rule" />
          <span className="lbl">DETAIL · TERMINAL ▾</span>
          <span className="rule" />
        </div>

        {/* ===================== TERMINAL DETAIL ===================== */}
        <section className="pk-terminal dash-b" data-screen-label="TERMINAL">
          <div className="pk-term-grid">
            {/* LEFT RAIL */}
            <div className="col left">
              <div className="b-kpi osc-bkt blue"><span className="osc-leg">BUDGET</span><div className="lbl" style={{ justifyContent: "flex-end" }}><span>{D.cycle.label}</span></div><div className="big">CHF {f.budget}</div><div className="sub">{D.cycle.days}-day cycle · day {D.cycle.day}</div></div>
              <div className="b-kpi accent osc-bkt coral"><span className="osc-leg">SPENT</span><div className="lbl" style={{ justifyContent: "flex-end" }}><span>{f.spentPct}%</span></div><div className="big">CHF {f.spent0}</div><div className="sub">CHF {f.spent} · {D.transactions.length}+ entries</div></div>
              <div className="b-kpi osc-bkt blue"><span className="osc-leg">REMAINING</span><div className="lbl" style={{ justifyContent: "flex-end" }}><span>11 D LEFT</span></div><div className="big">CHF {f.remaining0}</div><div className="sub">CHF {f.perDay}/day to stay on budget</div></div>
              <div className="b-mini osc-bkt blue" style={{ paddingTop: 14 }}><span className="osc-leg">RATES</span>
                <div className="ministat"><span className="k">Savings rate</span><span className="v">{Math.round(D.totals.savingsRate * 100)}%</span></div>
                <div className="ministat"><span className="k">vs last cycle</span><span className="v" style={{ color: "var(--ok)" }}>−34%</span></div>
                <div className="ministat"><span className="k">Top category</span><span className="v" style={{ fontSize: 13 }}>HOUSING</span></div>
              </div>
              <div className="b-shops osc-bkt blue" style={{ paddingTop: 16 }}><span className="osc-leg">TOP SHOPS</span>
                <div className="hud sm" style={{ marginBottom: 11 }}>THIS CYCLE</div>
                <div style={{ display: "flex", flexDirection: "column", gap: 11 }}>
                  {topShops.map(([shop, amt]) =>
                      <div key={shop}>
                      <div style={{ display: "flex", justifyContent: "space-between", alignItems: "baseline", marginBottom: 4 }}>
                        <span className="num" style={{ fontSize: 12.5, color: "var(--ink)", textTransform: "uppercase", letterSpacing: ".03em" }}>{shop}</span>
                        <span className="mono" style={{ fontSize: 11, color: "var(--ink-2)" }}>CHF {D.chf(amt)}</span>
                      </div>
                      <div className="phosk-bar" style={{ height: 5 }}>
                        <div className="phosk-bar-fill" style={{ width: amt / maxShop * 100 + "%", background: "var(--indigo)", boxShadow: "0 0 6px var(--indigo)" }} />
                      </div>
                    </div>
                      )}
                </div>
              </div>
            </div>

            {/* MID */}
            <div className="col mid">
              <div className="matrix">
                <div className="panel-h" style={{ padding: "10px 0", borderColor: "var(--hairline)" }}>
                  <span className="ttl">Category budgets</span><span className="ct">{D.categories.length}</span><span className="rule" /><span className="meta">SPENT / CAP / USED</span><a className="gbtn p" href="Phoskonomia%20Budgets.html" style={{ marginLeft: 11, whiteSpace: "nowrap" }}>BUDGETS ↗</a>
                </div>
                <window.CatRows cats={D.categories} />
              </div>
              <div className="b-recent">
                <div className="panel-h" style={{ padding: "10px 0", borderColor: "var(--hairline)" }}>
                  <span className="ttl blue">Recent</span><span className="rule" /><span className="meta">SHOP / CATEGORY / AMOUNT</span><a className="gbtn p" href="Phoskonomia%20Transactions.html" style={{ marginLeft: 11, whiteSpace: "nowrap" }}>ALL TXNS ↗</a>
                </div>
                <window.TxnTape rows={D.transactions.slice(0, 9)} />
              </div>
            </div>

            {/* RIGHT RAIL */}
            <div className="col right">
              <div className="hud" style={{ display: "flex", justifyContent: "space-between", alignItems: "center" }}><span>⌁ NEEDS ATTENTION</span><span className="coral num" style={{ fontSize: 15 }}>{D.alerts.length}</span></div>
              <div style={{ display: "flex", flexDirection: "column", gap: 9 }}>
                {D.alerts.slice(0, 3).map((a, i) => <window.AlertItem key={i} a={a} />)}
              </div>
              <div className="hud" style={{ display: "flex", justifyContent: "space-between", alignItems: "center", marginTop: 4 }}><span>RECURRING · CLEAN</span><span className="dim">CHF {recTotal}/MO</span></div>
              <div style={{ display: "flex", flexDirection: "column", gap: 6 }}>
                {D.recurring.slice(0, 4).map((r, i) => <window.RecRow key={i} r={r} />)}
              </div>
              <div className="b-insight osc-bkt blue">
                <div className="hud sm" style={{ display: "flex", alignItems: "center", gap: 7 }}><window.Dot tone="blue" size={6} />GEMMA4 · INSIGHT</div>
                <div className="q">Dining has crept up <b>3 cycles running</b>. Cutting two restaurant visits a month keeps you under the CHF 350 cap and adds ~<b>CHF 130</b> to savings.</div>
              </div>
            </div>
          </div>
        </section>
          </div>
        </div>

        {dockable && sigObj && <window.SignalPanel sig={sigObj} onClose={() => setSel(null)} />}
      </div>

      {!dockable && drawerSig && sigObj &&
      <div className="sig-drawer-back" onClick={() => setDrawerSig(false)}>
          <div className="sig-drawer" onClick={(e) => e.stopPropagation()}>
            <window.SignalPanel sig={sigObj} onClose={() => setDrawerSig(false)} variant="drawer" />
          </div>
        </div>
      }
    </div>);

}

Object.assign(window, { DashFull });