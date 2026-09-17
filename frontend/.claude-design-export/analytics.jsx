/* Phoskonomia — Analytics page. The retrospective read: where the money has
   been trending across cycles, which item-signals are moving, which categories
   have momentum, and when in the week you spend. Reuses the shared app shell;
   the left dock is the item-signal inspector. */
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
  const { TweaksPanel, TweakSection, TweakRadio, TweakToggle, TweakSelect } = window;
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
  const D = window.PHOSK;
  const [tw, setTweak] = window.useTweaks(AN_TWEAK_DEFAULTS);

  const [aiCollapsed, setAiCollapsed] = useStateA(!tw.aiOpen);
  const [sel, setSel] = useStateA(null);
  const [drawer, setDrawer] = useStateA(false);
  const [narrow, setNarrow] = useStateA(typeof window !== "undefined" && window.innerWidth < 1280);

  useEffectA(() => {
    const on = () => setNarrow(window.innerWidth < 1280);
    on(); window.addEventListener("resize", on);
    return () => window.removeEventListener("resize", on);
  }, []);

  const dockable = !narrow && tw.sigInsp === "dock";
  const selectSig = (id) => { setSel(id === sel ? null : id); if (id !== sel && !dockable) setDrawer(true); };

  const S = D.histStats;
  const movers = D.signalMovers();

  // item-signals (+ candidate), sorted
  const signals = useMemoA(() => {
    const arr = [...D.trackedSignals()];
    if (tw.sigSort === "spend") arr.sort((a, b) => b.cycleSpend - a.cycleSpend);
    else if (tw.sigSort === "az") arr.sort((a, b) => a.label.localeCompare(b.label));
    else arr.sort((a, b) => (b.deltaPct || 0) - (a.deltaPct || 0));
    return arr;
  }, [tw.sigSort]);

  const cats = useMemoA(() => {
    const arr = [...D.catTrends];
    if (tw.momentumSort === "spend") arr.sort((a, b) => b.now - a.now);
    else if (tw.momentumSort === "az") arr.sort((a, b) => a.name.localeCompare(b.name));
    else arr.sort((a, b) => (b.fixed ? -999 : Math.abs(b.deltaPct)) - (a.fixed ? -999 : Math.abs(a.deltaPct)));
    return arr;
  }, [tw.momentumSort]);

  const selSig = sel ? D.signalById(sel) : null;
  const showDrawer = !dockable && drawer && selSig;
  const sigCount = D.trackedSignals().length;

  return (
    <div className="pk" style={{ height: "100vh", minHeight: 0 }}>
      {/* Analytics field — a wide READ-OUT sweep. Signature: an open molten growth
          line winding low-left (the trend tail), a faint blue blob mass upper-right
          parked in negative space behind the movers rail, a thin wire vein mid. */}
      <window.ScannerBg className="pk-bg" seed={211} shapes={[
        { char: "2", cx: .2, cy: .78, scale: .5, style: "red", morph: "vein", live: true, fill: .5 },
        { char: "8", cx: .88, cy: .26, scale: .44, style: "faint", morph: "blob", live: false, fill: .62 },
        { char: "e", cx: .52, cy: .5, scale: .24, style: "wire", morph: "vein", live: false, fill: .4 },
        { char: "5", cx: .07, cy: .2, scale: .26, style: "faint", morph: "blob", live: false, fill: .42 },
        { char: "3", cx: .7, cy: .9, scale: .18, style: "wire", morph: "vein", live: false, fill: .35 }
      ]} />

      <div className="app-shell swap">
        <window.AiPanel collapsed={aiCollapsed} onToggle={() => setAiCollapsed((c) => !c)} onTrack={selectSig} />

        <div className="app-main">
          <window.TopBar active="ANALYTICS" />
          <div className="app-scroll" data-screen-label="ANALYTICS">
            <div className="an-wrap">

              <div className="an-top">
                <div>
                  <div className="ttl">Analytics</div>
                  <div className="sum">
                    <b>{S.months}</b> cycles on record · trending <span className="coral">CHF {D.chf(S.cur.spend, 0)}</span> this cycle ·
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
                  <div className="big"><span className="cur">CHF</span>{D.chf(S.cur.spend, 0)}</div>
                  <div className="sub">
                    <span className={S.curVsAvgPct > 0 ? "up" : "dn"}>{S.curVsAvgPct > 0 ? "↑" : "↓"} {Math.abs(S.curVsAvgPct)}%</span> vs 6-mo avg
                  </div>
                </div>
                <div className="akpi blue">
                  <div className="lbl"><span>6-MONTH AVG</span><span>SPEND</span></div>
                  <div className="big"><span className="cur">CHF</span>{D.chf(S.avg, 0)}</div>
                  <div className="sub">budget CHF {D.chf(S.cur.budget, 0)} · peak {S.peak.m} {D.chf(S.peak.spend, 0)}</div>
                </div>
                <div className="akpi blue bluebig">
                  <div className="lbl"><span>SAVINGS RATE</span><span>CASHFLOW</span></div>
                  <div className="big">{Math.round(S.avgRate * 100)}<span className="cur" style={{ marginLeft: 2 }}>%</span></div>
                  <div className="sub">avg of income · CHF {D.chf(S.totalSaved, 0)} saved over {S.months} cyc</div>
                </div>
                <div className="akpi">
                  <div className="lbl"><span>ITEM-SIGNALS</span><span>TRACKED</span></div>
                  <div className="big">{sigCount}</div>
                  <div className="sub">+1 candidate · {movers.riser.label} ↑{movers.riser.deltaPct}% leads</div>
                </div>
              </div>

              {/* SPEND TREND hero */}
              <window.SpendTrend windowN={parseInt(tw.trendWindow, 10)} mode={tw.trendMode} />

              {/* ITEM-SIGNALS */}
              <div className="an-sec">
                <span className="lbl">⌁ ITEM-SIGNALS</span>
                <span className="ct">{sigCount}</span>
                <span className="rule" />
                <span className="meta">12-MO TREND · CLICK TO INSPECT</span>
              </div>

              <div className="isig-list">
                {signals.map((s, i) => (
                  <window.ItemSignalRow key={s.id} sig={s} rank={i + 1} active={sel === s.id} onSelect={selectSig} />
                ))}
                {tw.showCand && (
                  <window.ItemSignalRow sig={D.signalCandidate} active={sel === D.signalCandidate.id} onSelect={selectSig} />
                )}
              </div>

              <div className="an-movers">
                <window.MoverCard sig={movers.riser} kind="riser" />
                <window.MoverCard sig={movers.faller} kind="faller" />
                <div className="an-insight">
                  <div className="ih"><window.Dot tone="blue" size={6} />GEMMA4 · READ</div>
                  <div className="q">
                    <b>Coffee</b> and <b>pain au chocolat</b> both climbing while <b>beer</b> cools — your weekday café habit is the live driver. Capping coffee at CHF&nbsp;80/cycle holds the dining envelope and feeds ~<b>CHF&nbsp;65</b> back to savings.
                  </div>
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
                  <div className="momo-grid">
                    {cats.map((c) => <window.MomentumCard key={c.name} c={c} />)}
                  </div>
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
                  <window.RhythmHeatmap />
                </>
              )}

            </div>
          </div>
        </div>

        {dockable && <window.SignalPanel sig={selSig} onClose={() => setSel(null)} />}
      </div>

      {showDrawer && (
        <div className="sig-drawer-back" onClick={() => setDrawer(false)}>
          <div className="sig-drawer" onClick={(e) => e.stopPropagation()}>
            <window.SignalPanel sig={selSig} onClose={() => setDrawer(false)} variant="drawer" />
          </div>
        </div>
      )}

      <AnalyticsTweaks tw={tw} setTweak={setTweak} onAi={setAiCollapsed} />
    </div>
  );
}

ReactDOM.createRoot(document.getElementById("root")).render(<AnalyticsPage />);
