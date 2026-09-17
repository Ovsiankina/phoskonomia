/* Phoskonomia — three dashboard layout variations. */

function fmtRow(D) {
  return {
    budget: D.chf(D.totals.budget, 0),
    spent0: D.chf(D.totals.spent, 0),
    spent: D.chf(D.totals.spent),
    remaining0: D.chf(D.totals.remaining, 0),
    remaining: D.chf(D.totals.remaining),
    perDay: D.chf(D.totals.remaining / (D.cycle.days - D.cycle.day), 0),
    saved0: D.chf(D.totals.saved, 0),
    spentPct: Math.round((D.totals.spent / D.totals.budget) * 100),
    savePct: Math.round((D.totals.saved / D.totals.savingsTarget) * 100),
  };
}
// tiny synthetic per-category spark
function catSpark(seed) {
  const a = []; let v = 0.5;
  for (let i = 0; i < 12; i++) { v += ((Math.sin(seed * 3.1 + i * 1.7) + Math.cos(seed + i)) * 0.18); a.push(v); }
  return a;
}

/* ===================================================== A · LEDGER ========= */
function DashA() {
  const D = window.PHOSK, f = fmtRow(D);
  return (
    <div className="phosk dash-a">
      <window.ScannerBg seed={11} shapes={[
        { char: "P", cx: .87, cy: .2, scale: .22, style: "faint", live: false, fill: .45 },
        { char: "8", cx: .08, cy: .85, scale: .16, style: "wire", live: false, fill: .3 },
      ]} />
      <div className="phosk-shell">
        <window.TopBar active="DASHBOARD" />
        <div className="a-body">
          <div className="a-kpis">
            <window.Kpi label="MONTHLY BUDGET" value={f.budget} sub={<><span>{D.cycle.label}</span><span className="d">· {D.cycle.days} DAYS</span></>} />
            <window.Kpi label="SPENT TO DATE" value={f.spent0} accent sub={<><span>{f.spentPct}% OF BUDGET</span></>} />
            <window.Kpi label="REMAINING" value={f.remaining0} sub={<><span>11 DAYS LEFT</span><span className="d">· CHF {f.perDay}/DAY</span></>} />
            <window.Kpi label="SAVED THIS CYCLE" value={f.saved0} blue sub={<><span>TARGET CHF 900</span><span className="d">· {f.savePct}%</span></>} />
          </div>

          <div className="a-chart">
            <div className="chartbox">
              <div className="sechead" style={{ margin: "0 0 8px" }}>
                <span className="lbl">SPENDING · {D.cycle.label}</span>
                <span className="rule" />
                <span className="meta" style={{ color: "var(--neon)" }}>━ CUMULATIVE</span>
                <span className="meta" style={{ color: "var(--indigo-neon)" }}>┄ BUDGET PACE</span>
              </div>
              <window.PhoskChart width={1084} height={196} showBars showPace showArea />
            </div>
          </div>

          <div className="a-cols">
            <div>
              <div className="sechead"><span className="lbl">CATEGORY BUDGETS</span><span className="ct">{D.categories.length}</span><span className="rule" /><span className="meta">SPENT / CAP</span></div>
              <window.CatRows cats={D.categories} />
            </div>
            <div>
              <div className="sechead"><span className="lbl">NEEDS ATTENTION</span><span className="ct">{D.alerts.length}</span><span className="rule" /></div>
              <div style={{ display: "flex", flexDirection: "column", gap: 9 }}>
                {D.alerts.slice(0, 3).map((a, i) => <window.AlertItem key={i} a={a} />)}
              </div>
              <div className="sechead"><span className="lbl">RECURRING</span><span className="ct">{D.recurring.length}</span><span className="rule" /><span className="meta">CHF {D.chf(D.recurring.reduce((s, r) => s + r.amount, 0), 0)}/MO</span></div>
              <div style={{ display: "flex", flexDirection: "column", gap: 7 }}>
                {D.recurring.slice(0, 4).map((r, i) => <window.RecRow key={i} r={r} />)}
              </div>
            </div>
          </div>

          <div className="sechead"><span className="lbl">RECENT TRANSACTIONS</span><span className="ct">{D.transactions.length}</span><span className="rule" /><span className="meta">SHOP / CATEGORY / AMOUNT</span></div>
          <div style={{ display: "grid", gridTemplateColumns: "1fr 1fr", gap: "0 44px" }}>
            <div><window.TxnTape rows={D.transactions.slice(0, 7)} /></div>
            <div><window.TxnTape rows={D.transactions.slice(7, 13)} /></div>
          </div>
        </div>
      </div>
    </div>
  );
}

/* ===================================================== B · TERMINAL ======= */
function DashB() {
  const D = window.PHOSK, f = fmtRow(D);
  const recTotal = D.chf(D.recurring.reduce((s, r) => s + r.amount, 0), 0);
  const topShops = (() => {
    const m = {};
    D.transactions.forEach(t => { m[t.shop] = (m[t.shop] || 0) + t.amount; });
    return Object.entries(m).sort((a, b) => b[1] - a[1]).slice(0, 4);
  })();
  const maxShop = topShops[0][1];
  return (
    <div className="phosk dash-b">
      <window.ScannerBg seed={23} shapes={[
        { char: "3", cx: .5, cy: .55, scale: .12, style: "faint", live: false, fill: .35 },
        { char: "e", cx: .94, cy: .12, scale: .12, style: "wire", live: false, fill: .25 },
      ]} />
      <div className="phosk-shell">
        <window.TopBar active="DASHBOARD" />
        <div className="b-body">
          {/* LEFT RAIL */}
          <div className="col left">
            <div className="b-kpi"><div className="lbl"><span>MONTHLY BUDGET</span><span>{D.cycle.label}</span></div><div className="big">CHF {f.budget}</div><div className="sub">{D.cycle.days}-day cycle · day {D.cycle.day}</div></div>
            <div className="b-kpi accent"><div className="lbl"><span>SPENT TO DATE</span><span>{f.spentPct}%</span></div><div className="big">CHF {f.spent0}</div><div className="sub">CHF {f.spent} · {D.transactions.length}+ entries</div></div>
            <div className="b-kpi"><div className="lbl"><span>REMAINING</span><span>11 D LEFT</span></div><div className="big">CHF {f.remaining0}</div><div className="sub">CHF {f.perDay}/day to stay on budget</div></div>
            <div style={{ border: "1px solid var(--hairline)", background: "rgba(10,6,20,.5)", padding: "12px 13px", display: "flex", gap: 12, alignItems: "center" }}>
              <window.SavingsDial size={108} saved={D.totals.saved} target={D.totals.savingsTarget} projected={D.totals.savingsProjected} />
              <div style={{ flex: 1 }}>
                <div className="hud sm" style={{ marginBottom: 8 }}>SAVINGS</div>
                <div className="ministat"><span className="k">Saved</span><span className="v coral">{f.saved0}</span></div>
                <div className="ministat"><span className="k">Target</span><span className="v">{D.chf(D.totals.savingsTarget, 0)}</span></div>
                <div className="ministat"><span className="k">Projected</span><span className="v">{D.chf(D.totals.savingsProjected, 0)}</span></div>
              </div>
            </div>
            <div style={{ border: "1px solid var(--hairline)", background: "rgba(10,6,20,.5)", padding: "10px 13px" }}>
              <div className="ministat"><span className="k">Savings rate</span><span className="v">{Math.round(D.totals.savingsRate * 100)}%</span></div>
              <div className="ministat"><span className="k">vs last cycle</span><span className="v" style={{ color: "var(--ok)" }}>−34%</span></div>
              <div className="ministat"><span className="k">Top category</span><span className="v" style={{ fontSize: 13 }}>HOUSING</span></div>
            </div>
            <div style={{ flex: "1 1 auto", border: "1px solid var(--hairline)", background: "rgba(10,6,20,.5)", padding: "11px 13px", overflow: "hidden" }}>
              <div className="hud sm" style={{ marginBottom: 9 }}>TOP SHOPS · THIS CYCLE</div>
              <div style={{ display: "flex", flexDirection: "column", gap: 9 }}>
                {topShops.map(([shop, amt]) => (
                  <div key={shop}>
                    <div style={{ display: "flex", justifyContent: "space-between", alignItems: "baseline", marginBottom: 4 }}>
                      <span className="num" style={{ fontSize: 12.5, color: "var(--ink)", textTransform: "uppercase", letterSpacing: ".03em" }}>{shop}</span>
                      <span className="mono" style={{ fontSize: 11, color: "var(--ink-2)" }}>CHF {D.chf(amt)}</span>
                    </div>
                    <div className="phosk-bar" style={{ height: 5 }}>
                      <div className="phosk-bar-fill" style={{ width: (amt / maxShop * 100) + "%", background: "var(--indigo)", boxShadow: "0 0 6px var(--indigo)" }} />
                    </div>
                  </div>
                ))}
              </div>
            </div>
          </div>

          {/* MID */}
          <div className="col mid">
            <div className="scope">
              <div className="sechead" style={{ margin: "0 0 6px" }}>
                <span className="lbl">SPEND VS BUDGET PACE</span><span className="rule" />
                <span className="meta" style={{ color: "var(--neon)" }}>━ ACTUAL</span>
                <span className="meta" style={{ color: "var(--indigo-neon)" }}>┄ PACE</span>
              </div>
              <window.PhoskChart width={846} height={184} showBars showPace showArea />
            </div>
            <div className="matrix">
              <div className="panel-h" style={{ padding: "10px 0", borderColor: "var(--hairline)" }}>
                <span className="ttl">Category budgets</span><span className="ct">{D.categories.length}</span><span className="rule" /><span className="meta">SPENT / CAP / USED</span>
              </div>
              <window.CatRows cats={D.categories} />
            </div>
            <div className="b-recent">
              <div className="panel-h" style={{ padding: "10px 0", borderColor: "var(--hairline)" }}>
                <span className="ttl blue">Recent</span><span className="rule" /><span className="meta">SHOP / CATEGORY / AMOUNT</span>
              </div>
              <window.TxnTape rows={D.transactions.slice(0, 6)} />
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
            <div className="b-insight">
              <div className="hud sm" style={{ display: "flex", alignItems: "center", gap: 7 }}><window.Dot tone="blue" size={6} />GEMMA4 · INSIGHT</div>
              <div className="q">Dining has crept up <b>3 cycles running</b>. Cutting two restaurant visits a month keeps you under the CHF 350 cap and adds ~<b>CHF 130</b> to savings.</div>
            </div>
          </div>
        </div>
      </div>
    </div>
  );
}

/* ===================================================== C · CONSOLE ======== */
function DashC() {
  const D = window.PHOSK, f = fmtRow(D);
  const chans = ["GROCERIES", "DINING & CAFÉS", "TRANSPORT", "SUBSCRIPTIONS", "LEISURE"]
    .map(n => D.categories.find(c => c.name === n));
  return (
    <div className="phosk dash-c">
      <window.ScannerBg seed={31} shapes={[
        { char: "P", cx: -.15, cy: .5, scale: .66, style: "red", live: true, fill: .66, clip: [0, 0, .034, 1] },
        { char: "8", cx: .62, cy: .85, scale: .14, style: "wire", live: false, fill: .25 },
      ]} />
      <div className="phosk-shell">
        <window.TopBar active="DASHBOARD" />
        <div className="c-body">
          {/* DOCK */}
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
            <div style={{ marginTop: "auto", border: "1px solid var(--hairline-warm)", background: "rgba(22,8,16,.4)", padding: "11px 13px" }}>
              <div className="hud sm" style={{ marginBottom: 6 }}>NEXT CHARGE</div>
              <div style={{ display: "flex", justifyContent: "space-between", alignItems: "baseline" }}>
                <span className="num" style={{ fontSize: 17, color: "var(--ink)" }}>SUNRISE</span>
                <span className="num coral" style={{ fontSize: 19 }}>CHF 45.00</span>
              </div>
              <div className="dim" style={{ fontSize: 9.5, letterSpacing: ".12em", marginTop: 4 }}>DUE 22 JUN · 3 DAYS</div>
            </div>
          </div>

          {/* MAIN */}
          <div className="c-main">
            <div className="c-screen">
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
                const p = c.budget > 0 ? c.spent / c.budget : 0, tone = window.pctTone(p);
                return (
                  <div className="chan" key={c.name}>
                    <span className="cn">{c.name}</span>
                    <window.Spark data={catSpark(i + 2)} w={150} h={28} tone={tone === "alert" ? "neon" : "indigo"} />
                    <div className="cv">
                      <span className={"p pct " + tone}>{Math.round(p * 100)}%</span>
                      <span className="s">CHF {D.chf(c.spent, 0)} / {D.chf(c.budget, 0)}</span>
                    </div>
                  </div>
                );
              })}
            </div>

            <div style={{ display: "flex", alignItems: "center", gap: 18, padding: "9px 18px", borderTop: "1px solid var(--hairline)", background: "rgba(8,5,18,.5)", fontSize: 10.5, letterSpacing: ".06em", textTransform: "uppercase", whiteSpace: "nowrap", overflow: "hidden" }}>
              <span className="hud sm" style={{ flex: "0 0 auto" }}>WATCH</span>
              <span style={{ color: "var(--neon)" }}>⚠ DINING & CAFÉS 17% OVER CAP</span>
              <span className="dim">·</span>
              <span style={{ color: "var(--indigo-neon)" }}>⌁ ACTIV FITNESS NOT SEEN THIS CYCLE</span>
              <span className="dim">·</span>
              <span style={{ color: "var(--warn)" }}>◷ GROCERIES ON PACE TO EXCEED ~CHF 70</span>
            </div>
          </div>
        </div>
      </div>
    </div>
  );
}

Object.assign(window, { DashA, DashB, DashC });
