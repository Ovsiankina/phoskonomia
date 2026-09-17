/* Phoskonomia — final composed dashboard.
   Console-style HERO up top (oscilloscope instrument), scroll reveals the
   Terminal-style dense grid below. Reuses the shared blocks/primitives.

   DATA SOURCE: 100% backend fetch (phosk_api) via useGet — no mock data, no
   fallbacks. Each section renders <Awaiting/> until its endpoint goes green
   (currently 501). Every interactive control hits the matching api.* endpoint
   and re-fetches the affected useGet on success. */
import React from 'react'
import { Link } from 'react-router-dom'
import { chf } from '../data/phosk.js'
import { ScannerBg, Dot, Spark, PhoskChart, SavingsDial, pctTone } from '../components/prims.jsx'
import { TopBar, CatRows, TxnTape, RecRow, AlertItem } from '../components/comps.jsx'
import { AiPanel, SignalPanel, SignalStrip } from '../components/shell.jsx'
import { Awaiting } from '../components/states.jsx'
import { useGet } from '../lib/api.js'

const { useState: useStateD, useEffect: useEffectD } = React;

/* whole-number CHF (no decimals) — used for the big console numerals */
const chf0 = (n) => chf(n, 0);
/* a numeric KPI that hasn't loaded shows an em-dash, never NaN */
const pct = (n) => (n == null || !isFinite(Number(n)) ? '—' : Math.round(Number(n) * 100) + '%');
const ok200 = (status) => status === 200;

function DashFull() {
  /* ---- backend data loads ---- */
  const cycle = useGet('/cycle/current');
  const totals = useGet('/cycle/current/totals');
  const series = useGet('/cycle/current/spend-series', { compare: 'lastCycle' });
  const shops = useGet('/cycle/current/top-shops');
  const insight = useGet('/insights/dashboard');
  const cats = useGet('/categories');
  const txns = useGet('/transactions', { limit: 9 });
  const recurring = useGet('/recurring');
  const alerts = useGet('/alerts');
  const signals = useGet('/signals');
  const candidates = useGet('/signals/candidates');

  const C = cycle.data || {};
  const T = totals.data || {};
  const S = series.data || {};
  const shopsData = shops.data || {};
  const shopList = Array.isArray(shopsData.shops) ? shopsData.shops : [];
  const insightData = insight.data || {};
  const categories = Array.isArray(cats.data) ? cats.data : (cats.data && cats.data.categories) || [];
  const txnList = Array.isArray(txns.data) ? txns.data : (txns.data && txns.data.transactions) || [];
  const recList = Array.isArray(recurring.data) ? recurring.data : (recurring.data && recurring.data.recurring) || [];
  const monthlyTotal = recurring.data && recurring.data.monthlyTotal;
  const alertList = Array.isArray(alerts.data) ? alerts.data : (alerts.data && alerts.data.alerts) || [];
  const trackedList = Array.isArray(signals.data) ? signals.data : (signals.data && (signals.data.signals || signals.data.items)) || [];
  const candList = Array.isArray(candidates.data) ? candidates.data : (candidates.data && (candidates.data.candidates || candidates.data.items)) || [];
  // tracked signals + AI candidates (flagged so SignalStrip/SignalPanel render them as candidates)
  const signalList = [...trackedList, ...candList.map((c) => ({ ...c, candidate: true }))];

  /* upcoming recurring charges, soonest first (drives the hero NEXT dock).
     Sort/labels come straight off the API (daysUntil / next) — no client date math. */
  const nextDue = [...recList]
    .filter((r) => r && r.daysUntil != null)
    .sort((a, b) => a.daysUntil - b.daysUntil)
    .slice(0, 3);

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

  /* selected item-signal detail — fetched on demand; path guarded when none selected */
  const sigDetail = useGet(sel ? '/signals/' + encodeURIComponent(sel) : '/signals', undefined, [sel]);
  const sigObj = sel && ok200(sigDetail.status) ? sigDetail.data : null;
  const selectSig = (id) => {setSel(id);if (!dockable) setDrawerSig(true);};

  /* ---- WATCH strip: derived from the top few alerts (tag + head) ---- */
  const watchItems = alertList.slice(0, 3);
  const watchTone = (tone) => tone === "alert" ? "var(--neon)" : tone === "warn" ? "var(--warn)" : "var(--indigo-neon)";
  const watchGlyph = (tone) => tone === "alert" ? "⚠" : tone === "warn" ? "◷" : "⌁";

  return (
    <div className="pk" style={{ height: "100vh", minHeight: 0 }}>
      <ScannerBg className="pk-bg" seed={31} shapes={[
      { char: "8", cx: .14, cy: .5, scale: .3, style: "wire", morph: "vein", live: false, fill: .42 },
      { char: "8", cx: .93, cy: .82, scale: .14, style: "wire", live: false, fill: .24 },
      { char: "e", cx: .8, cy: .26, scale: .18, style: "faint", morph: "blob", live: false, fill: .46 }]
      } />

      <div className="app-shell swap">
        <AiPanel collapsed={aiCollapsed} onToggle={() => setAiCollapsed((c) => !c)} onTrack={selectSig} />

        <div className="app-main">
          <TopBar active="DASHBOARD" />
          <div className="app-scroll">

        {/* ===================== CONSOLE HERO ===================== */}
        <section className="pk-hero dash-c" data-screen-label="HERO">
          <div className="c-dock glass">
            <div className="c-hero">
              <div className="lbl">REMAINING · {C.label || ""}</div>
              {ok200(totals.status) ? (
                <>
                  <div className="big"><span className="cur">CHF</span>{T.remaining != null ? chf0(T.remaining) : "—"}</div>
                  <div className="sub">of CHF {T.budget != null ? chf0(T.budget) : "—"} budget · {pct(T.spentPct != null ? T.spentPct / 100 : null)} spent · {C.daysLeft != null ? C.daysLeft + " days left" : "—"}</div>
                </>
              ) : (
                <div className="big"><span className="cur">CHF</span>—</div>
              )}
            </div>
            <div style={{ display: "flex", justifyContent: "center", padding: "4px 0" }}>
              {ok200(totals.status) ? (
                <SavingsDial size={150} saved={T.saved} target={T.savingsTarget} projected={T.savingsProjected} />
              ) : (
                <Awaiting label="SAVINGS" res={totals.res} loading={totals.loading} />
              )}
            </div>
            <div>
              <div className="hud sm" style={{ marginBottom: 6 }}>SNAPSHOT</div>
              {ok200(totals.status) ? (
                <>
                  <div className="ministat"><span className="k">Spent</span><span className="v coral">CHF {T.spent != null ? chf0(T.spent) : "—"}</span></div>
                  <div className="ministat"><span className="k">Saved</span><span className="v">CHF {T.saved != null ? chf0(T.saved) : "—"}</span></div>
                  <div className="ministat"><span className="k">Savings rate</span><span className="v">{pct(T.savingsRate)}</span></div>
                  <div className="ministat"><span className="k">vs last cycle</span><span className="v" style={{ color: "var(--ok)" }}>{T.vsLastCyclePct != null ? (T.vsLastCyclePct > 0 ? "+" : "") + T.vsLastCyclePct + "%" : "—"}</span></div>
                </>
              ) : (
                <Awaiting label="SNAPSHOT" res={totals.res} loading={totals.loading} />
              )}
            </div>
            <div className="osc-bkt coral" style={{ marginTop: "auto", border: "1px solid var(--hairline-warm)", background: "rgba(22,8,16,.4)", padding: "14px 13px 11px" }}>
              <span className="osc-leg">NEXT</span>
              {!ok200(recurring.status) ? (
                <Awaiting label="NEXT DUE" res={recurring.res} loading={recurring.loading} tone="coral" />
              ) : nextDue.length === 0 ? (
                <div className="dim" style={{ fontSize: 11, padding: "8px 0", letterSpacing: ".04em" }}>No upcoming charges.</div>
              ) : nextDue.map((n, i) =>
                  <div key={n.id || n.name} style={i === 0 ? undefined : { marginTop: 9, paddingTop: 9, borderTop: "1px solid var(--hairline)" }}>
                <div style={{ display: "flex", justifyContent: "space-between", alignItems: "baseline", gap: 10 }}>
                  <span className="num" style={{ fontSize: i === 0 ? 17 : 14, color: "var(--ink)", minWidth: 0, overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" }}>{n.name}</span>
                  <span className="num coral" style={{ fontSize: i === 0 ? 19 : 15, whiteSpace: "nowrap" }}>CHF {chf(n.amount)}</span>
                </div>
                <div className="dim" style={{ fontSize: 9.5, letterSpacing: ".12em", marginTop: 3 }}>DUE {n.next} · {n.daysUntil <= 0 ? "TODAY" : n.daysUntil + (n.daysUntil === 1 ? " DAY" : " DAYS")}</div>
              </div>
                  )}
            </div>
          </div>

          <div className="c-main">
            <div className="c-screen">
              <ScannerBg className="c-screen-bg" seed={91} bg={false} grid={false} dish={false}
                  shapes={[{ char: "8", cx: .5, cy: .56, scale: .52, r: .92, style: "redneg", morph: "mass", live: true, fill: .6 }]} />
              <div className="scr-hud">
                <span className="hud">SPEND TRACE · {C.label || ""}</span>
                <span className="hud" style={{ color: "var(--neon-dim)" }}>{ok200(totals.status) ? <>CHF {T.spent != null ? chf0(T.spent) : "—"} / {T.budget != null ? chf0(T.budget) : "—"} · {pct(T.spentPct != null ? T.spentPct / 100 : null)}</> : "—"}</span>
              </div>
              {ok200(series.status) ? (
                <PhoskChart width={1150} height={300} showBars={false} showPace showArea showLast padT={34} padB={22} padL={14} padR={14}
                  days={C.days} today={S.todayIndex} budget={T.budget}
                  daily={S.daily} cumulative={S.cumulative} pace={S.pace} lastCumulative={S.lastCycleCumulative} />
              ) : (
                <div style={{ padding: "40px 14px" }}><Awaiting label="SPEND TRACE" res={series.res} loading={series.loading} tone="coral" /></div>
              )}
              <div style={{ position: "absolute", bottom: 8, left: 16, display: "flex", gap: 16 }}>
                <span className="hud sm" style={{ color: "var(--neon)" }}>━ THIS CYCLE</span>
                <span className="hud sm" style={{ color: "var(--indigo-neon)" }}>┄ BUDGET PACE</span>
                <span className="hud sm" style={{ color: "rgba(143,125,255,.7)" }}>┄ LAST CYCLE</span>
              </div>
            </div>

            <div className="c-channels">
              {!ok200(cats.status) ? (
                <Awaiting label="CHANNELS" res={cats.res} loading={cats.loading} />
              ) : categories.slice(0, 5).map((c) => {
                    const p = c.budget > 0 ? c.spent / c.budget : 0,tone = pctTone(p);
                    const spark = Array.isArray(c.spark) ? c.spark : null;
                    return (
                      <div className="chan" key={c.name}>
                    <span className="cn">{c.name}</span>
                    {spark && spark.length > 1
                      ? <Spark data={spark} w={150} h={28} tone={tone === "alert" ? "neon" : "indigo"} />
                      : <div style={{ height: 28 }} />}
                    <div className="cv">
                      <span className={"p pct " + tone}>{c.usedPct != null ? Math.round(c.usedPct) + "%" : "—"}</span>
                      <span className="s">CHF {chf0(c.spent)} / {chf0(c.budget)}</span>
                    </div>
                  </div>);

                  })}
            </div>

            <div className="c-watch">
              <span className="hud sm" style={{ flex: "0 0 auto" }}>WATCH</span>
              {!ok200(alerts.status) ? (
                <span className="dim">{alerts.loading ? "Loading…" : "awaiting backend (/alerts)"}</span>
              ) : watchItems.length === 0 ? (
                <span className="dim">No active alerts.</span>
              ) : watchItems.map((a, i) => (
                <React.Fragment key={a.id || i}>
                  {i > 0 && <span className="dim">·</span>}
                  <span style={{ color: watchTone(a.tone) }}>{watchGlyph(a.tone)} {a.tag ? a.tag + " " : ""}{a.head}</span>
                </React.Fragment>
              ))}
            </div>
          </div>
        </section>

        {/* ===================== ITEM-SIGNALS ===================== */}
        <SignalStrip signals={signalList} sel={sel} onSelect={selectSig} />

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
              <div className="b-kpi osc-bkt blue"><span className="osc-leg">BUDGET</span><div className="lbl" style={{ justifyContent: "flex-end" }}><span>{C.label || ""}</span></div><div className="big">CHF {ok200(totals.status) && T.budget != null ? chf0(T.budget) : "—"}</div><div className="sub">{C.days != null ? C.days + "-day cycle · day " + (C.day != null ? C.day : "—") : "—"}</div></div>
              <div className="b-kpi accent osc-bkt coral"><span className="osc-leg">SPENT</span><div className="lbl" style={{ justifyContent: "flex-end" }}><span>{ok200(totals.status) ? pct(T.spentPct != null ? T.spentPct / 100 : null) : "—"}</span></div><div className="big">CHF {ok200(totals.status) && T.spent != null ? chf0(T.spent) : "—"}</div><div className="sub">{ok200(totals.status) && T.spent != null ? "CHF " + chf(T.spent) : "—"} · {txnList.length}+ entries</div></div>
              <div className="b-kpi osc-bkt blue"><span className="osc-leg">REMAINING</span><div className="lbl" style={{ justifyContent: "flex-end" }}><span>{C.daysLeft != null ? C.daysLeft + " D LEFT" : "—"}</span></div><div className="big">CHF {ok200(totals.status) && T.remaining != null ? chf0(T.remaining) : "—"}</div><div className="sub">{ok200(totals.status) && T.perDayToStayOnBudget != null ? "CHF " + chf0(T.perDayToStayOnBudget) + "/day to stay on budget" : "—"}</div></div>
              <div className="b-mini osc-bkt blue" style={{ paddingTop: 14 }}><span className="osc-leg">RATES</span>
                {ok200(totals.status) ? (
                  <>
                    <div className="ministat"><span className="k">Savings rate</span><span className="v">{pct(T.savingsRate)}</span></div>
                    <div className="ministat"><span className="k">vs last cycle</span><span className="v" style={{ color: "var(--ok)" }}>{T.vsLastCyclePct != null ? (T.vsLastCyclePct > 0 ? "+" : "") + T.vsLastCyclePct + "%" : "—"}</span></div>
                    <div className="ministat"><span className="k">Last cycle spent</span><span className="v" style={{ fontSize: 13 }}>{T.lastCycleSpent != null ? "CHF " + chf0(T.lastCycleSpent) : "—"}</span></div>
                  </>
                ) : (
                  <Awaiting label="RATES" res={totals.res} loading={totals.loading} />
                )}
              </div>
              <div className="b-shops osc-bkt blue" style={{ paddingTop: 16 }}><span className="osc-leg">TOP SHOPS</span>
                <div className="hud sm" style={{ marginBottom: 11 }}>THIS CYCLE</div>
                {!ok200(shops.status) ? (
                  <Awaiting label="TOP SHOPS" res={shops.res} loading={shops.loading} />
                ) : shopList.length === 0 ? (
                  <div className="dim" style={{ fontSize: 11 }}>No shops this cycle.</div>
                ) : (
                  <div style={{ display: "flex", flexDirection: "column", gap: 11 }}>
                    {shopList.slice(0, 4).map((s) => {
                      const denom = shopsData.maxTotal || shopList[0].total || 1;
                      return (
                      <div key={s.shop}>
                        <div style={{ display: "flex", justifyContent: "space-between", alignItems: "baseline", marginBottom: 4 }}>
                          <span className="num" style={{ fontSize: 12.5, color: "var(--ink)", textTransform: "uppercase", letterSpacing: ".03em" }}>{s.shop}</span>
                          <span className="mono" style={{ fontSize: 11, color: "var(--ink-2)" }}>CHF {chf(s.total)}</span>
                        </div>
                        <div className="phosk-bar" style={{ height: 5 }}>
                          <div className="phosk-bar-fill" style={{ width: (denom > 0 ? s.total / denom * 100 : 0) + "%", background: "var(--indigo)", boxShadow: "0 0 6px var(--indigo)" }} />
                        </div>
                      </div>);
                    })}
                  </div>
                )}
              </div>
            </div>

            {/* MID */}
            <div className="col mid">
              <div className="matrix">
                <div className="panel-h" style={{ padding: "10px 0", borderColor: "var(--hairline)" }}>
                  <span className="ttl">Category budgets</span><span className="ct">{ok200(cats.status) ? categories.length : ""}</span><span className="rule" /><span className="meta">SPENT / CAP / USED</span><Link className="gbtn p" to="/budgets" style={{ marginLeft: 11, whiteSpace: "nowrap" }}>BUDGETS ↗</Link>
                </div>
                {!ok200(cats.status) ? (
                  <Awaiting label="CATEGORY BUDGETS" res={cats.res} loading={cats.loading} />
                ) : (
                  <CatRows cats={categories} />
                )}
              </div>
              <div className="b-recent">
                <div className="panel-h" style={{ padding: "10px 0", borderColor: "var(--hairline)" }}>
                  <span className="ttl blue">Recent</span><span className="rule" /><span className="meta">SHOP / CATEGORY / AMOUNT</span><Link className="gbtn p" to="/transactions" style={{ marginLeft: 11, whiteSpace: "nowrap" }}>ALL TXNS ↗</Link>
                </div>
                {!ok200(txns.status) ? (
                  <Awaiting label="RECENT TXNS" res={txns.res} loading={txns.loading} />
                ) : (
                  <TxnTape rows={txnList} />
                )}
              </div>
            </div>

            {/* RIGHT RAIL */}
            <div className="col right">
              <div className="hud" style={{ display: "flex", justifyContent: "space-between", alignItems: "center" }}><span>⌁ NEEDS ATTENTION</span><span className="coral num" style={{ fontSize: 15 }}>{ok200(alerts.status) ? alertList.length : "—"}</span></div>
              {!ok200(alerts.status) ? (
                <Awaiting label="NEEDS ATTENTION" res={alerts.res} loading={alerts.loading} tone="coral" />
              ) : alertList.length === 0 ? (
                <div className="dim" style={{ fontSize: 11, padding: "6px 0" }}>Nothing needs attention.</div>
              ) : (
                <div style={{ display: "flex", flexDirection: "column", gap: 9 }}>
                  {alertList.slice(0, 3).map((a, i) => <AlertItem key={a.id || i} a={a} onChanged={alerts.reload} />)}
                </div>
              )}
              <div className="hud" style={{ display: "flex", justifyContent: "space-between", alignItems: "center", marginTop: 4 }}><span>RECURRING · CLEAN</span><span className="dim">{ok200(recurring.status) && monthlyTotal != null ? "CHF " + chf0(monthlyTotal) + "/MO" : "—"}</span></div>
              {!ok200(recurring.status) ? (
                <Awaiting label="RECURRING" res={recurring.res} loading={recurring.loading} />
              ) : recList.length === 0 ? (
                <div className="dim" style={{ fontSize: 11, padding: "6px 0" }}>No recurring charges.</div>
              ) : (
                <div style={{ display: "flex", flexDirection: "column", gap: 6 }}>
                  {recList.slice(0, 4).map((r, i) => <RecRow key={r.id || i} r={r} />)}
                </div>
              )}
              <div className="b-insight osc-bkt blue">
                <div className="hud sm" style={{ display: "flex", alignItems: "center", gap: 7 }}><Dot tone="blue" size={6} />{insightData.model || "GEMMA4"} · INSIGHT</div>
                {!ok200(insight.status) ? (
                  <Awaiting label="GEMMA4 INSIGHT" res={insight.res} loading={insight.loading} />
                ) : (
                  <div className="q">{insightData.text || "No insight yet."}{insightData.estimatedSavings != null ? <> · est. <b>CHF {chf(insightData.estimatedSavings)}</b></> : null}</div>
                )}
              </div>
            </div>
          </div>
        </section>
          </div>
        </div>

        {dockable && sel && <SignalPanel sig={sigObj} onClose={() => setSel(null)} cycleLabel={C.label} onChanged={() => { sigDetail.reload(); signals.reload(); candidates.reload(); }} />}
      </div>

      {!dockable && drawerSig && sel &&
      <div className="sig-drawer-back" onClick={() => setDrawerSig(false)}>
          <div className="sig-drawer" onClick={(e) => e.stopPropagation()}>
            <SignalPanel sig={sigObj} onClose={() => setDrawerSig(false)} variant="drawer" cycleLabel={C.label} onChanged={() => { sigDetail.reload(); signals.reload(); candidates.reload(); }} />
          </div>
        </div>
      }
    </div>);

}

export default DashFull;
