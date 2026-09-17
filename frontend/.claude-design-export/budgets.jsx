/* Phoskonomia — Budgets page. Envelope console: tune category caps (channel
   thresholds) against the monthly budget, watch projection, inspect any channel.
   Reuses the shared shell (AI panel left, inspector dock right). */
const { useState: useStateBP, useMemo: useMemoBP, useEffect: useEffectBP } = React;

const BUD_TWEAK_DEFAULTS = /*EDITMODE-BEGIN*/{
  "envLayout": "cards",
  "sort": "order",
  "showProj": true,
  "aiOpen": true
} /*EDITMODE-END*/;

function BudTweaks({ tw, setTweak, onAi }) {
  const { TweaksPanel, TweakSection, TweakRadio, TweakToggle } = window;
  if (!TweaksPanel) return null;
  return (
    <TweaksPanel title="Tweaks">
      <TweakSection label="Envelope view" />
      <TweakRadio label="Layout" value={tw.envLayout}
      options={[{ value: "cards", label: "Cards" }, { value: "rows", label: "Rows" }]}
      onChange={(v) => setTweak("envLayout", v)} />
      <TweakRadio label="Sort" value={tw.sort}
      options={[{ value: "order", label: "Order" }, { value: "used", label: "Used" }, { value: "over", label: "Over" }]}
      onChange={(v) => setTweak("sort", v)} />
      <TweakSection label="Readout" />
      <TweakToggle label="Projection markers" value={tw.showProj}
      onChange={(v) => setTweak("showProj", v)} />
      <TweakSection label="Assistant" />
      <TweakToggle label="AI panel open" value={tw.aiOpen}
      onChange={(v) => {setTweak("aiOpen", v);onAi(!v);}} />
    </TweaksPanel>);

}

function BudgetPage() {
  const D = window.PHOSK;
  const [tw, setTweak] = window.useTweaks(BUD_TWEAK_DEFAULTS);

  const [aiCollapsed, setAiCollapsed] = useStateBP(!tw.aiOpen);
  const [caps, setCaps] = useStateBP(() => Object.fromEntries(D.categories.map((c) => [c.name, c.budget])));
  const [sel, setSel] = useStateBP(null);
  const [drawer, setDrawer] = useStateBP(false);
  const [narrow, setNarrow] = useStateBP(typeof window !== "undefined" && window.innerWidth < 1280);

  useEffectBP(() => {
    const on = () => setNarrow(window.innerWidth < 1280);
    on();window.addEventListener("resize", on);
    return () => window.removeEventListener("resize", on);
  }, []);
  const dockable = !narrow;

  const step = (name, d) => {
    setCaps((c) => ({ ...c, [name]: Math.max(0, (c[name] ?? 0) + d) }));
    window.phoskApi.patch('/categories/' + name, { delta: d });
  };
  const selectCat = (name) => {setSel(name);if (!dockable) setDrawer(true);};

  // ---- derived figures (envelope-truth: the page is internally consistent) ----
  const budget = D.totals.budget;
  const allocated = D.categories.reduce((s, c) => s + (caps[c.name] ?? c.budget), 0);
  const spent = D.categories.reduce((s, c) => s + c.spent, 0);
  const projected = D.categories.reduce((s, c) => s + c.proj, 0);
  const spentPct = Math.round(spent / budget * 100);
  const overAlloc = allocated - budget;
  const projOver = projected - budget;
  const daysLeft = D.cycle.days - D.cycle.day;

  const ordered = useMemoBP(() => {
    const arr = [...D.categories];
    const rank = (c) => {
      const cap = caps[c.name] ?? c.budget;
      const st = D.budgetStatus({ ...c, budget: cap });
      return st.key === "over" ? 0 : st.key === "willexceed" ? 1 : 2;
    };
    if (tw.sort === "used") arr.sort((a, b) => b.spent / (caps[b.name] || 1) - a.spent / (caps[a.name] || 1));else
    if (tw.sort === "over") arr.sort((a, b) => rank(a) - rank(b) || b.proj / (caps[b.name] || 1) - a.proj / (caps[a.name] || 1));
    return arr;
  }, [tw.sort, caps]);

  const selCat = sel ? D.categories.find((c) => c.name === sel) : null;
  const selCap = selCat ? caps[selCat.name] ?? selCat.budget : 0;
  const showDrawer = !dockable && drawer && selCat;

  return (
    <div className="pk" style={{ height: "100vh", minHeight: 0 }}>
      {/* Budgets field — the molten signal sits LOWER-RIGHT, with a faint
           envelope mass upper-left and a light wire scaffold across the top. */}
      <window.ScannerBg className="pk-bg" seed={63} shapes={[
      { char: "4", cx: .8, cy: .77, scale: .44, style: "red", morph: "vein", live: true, fill: .5 },
      { char: "8", cx: .14, cy: .3, scale: .31, style: "faint", morph: "blob", live: false, fill: .5 },
      { char: "5", cx: .48, cy: .12, scale: .18, style: "wire", morph: "vein", live: false, fill: .36 },
      { char: "2", cx: .92, cy: .18, scale: .14, style: "wire", morph: "vein", live: false, fill: .3 }]
      } />

      <div className="app-shell swap">
        <window.AiPanel collapsed={aiCollapsed} onToggle={() => setAiCollapsed((c) => !c)} onTrack={() => {}} />

        <div className="app-main">
          <window.TopBar active="BUDGETS" />
          <div className="app-scroll" data-screen-label="BUDGETS">
            <div className="bud-wrap">

              <div className="bud-top">
                <div>
                  <div className="ttl">Budgets</div>
                  <div className="sum">
                    <b>{D.categories.length}</b> envelopes · <b>CHF {D.chf(budget, 0)}</b> monthly ·
                    <span className="coral"> {spentPct}% spent</span> · {D.cycle.label} · day {D.cycle.day}/{D.cycle.days}
                  </div>
                </div>
                <div className="bud-controls">
                  <div className="modes">
                    <span className="mlbl">SORT</span>
                    <button className={"m" + (tw.sort === "order" ? " on" : "")} onClick={() => setTweak("sort", "order")}>ORDER</button>
                    <button className={"m" + (tw.sort === "used" ? " on" : "")} onClick={() => setTweak("sort", "used")}>USED</button>
                    <button className={"m" + (tw.sort === "over" ? " on" : "")} onClick={() => setTweak("sort", "over")}>OVER</button>
                  </div>
                  <div className="modes">
                    <button className={"m" + (tw.envLayout === "cards" ? " on" : "")} onClick={() => setTweak("envLayout", "cards")}>▦ CARDS</button>
                    <button className={"m" + (tw.envLayout === "rows" ? " on" : "")} onClick={() => setTweak("envLayout", "rows")}>≡ ROWS</button>
                  </div>
                </div>
              </div>

              {/* KPI band */}
              <div className="bud-kpis">
                <div className="bud-kpi">
                  <div className="lbl"><span>MONTHLY BUDGET</span><span>{D.cycle.label}</span></div>
                  <div className="big"><span className="cur">CHF</span>{D.chf(budget, 0)}</div>
                  <div className="sub">{D.cycle.days}-day cycle · {daysLeft} days left</div>
                </div>
                <div className="bud-kpi blue">
                  <div className="lbl"><span>ALLOCATED</span><span>{overAlloc > 0 ? "OVER" : "OK"}</span></div>
                  <div className="big"><span className="cur">CHF</span>{D.chf(allocated, 0)}</div>
                  <div className="sub">{overAlloc > 0 ? "CHF " + D.chf(overAlloc, 0) + " over budget" : "CHF " + D.chf(-overAlloc, 0) + " unallocated"}</div>
                </div>
                <div className="bud-kpi accent">
                  <div className="lbl"><span>SPENT</span><span>{spentPct}%</span></div>
                  <div className="big"><span className="cur">CHF</span>{D.chf(spent, 0)}</div>
                  <div className="sub">CHF {D.chf(budget - spent, 0)} left of monthly budget</div>
                </div>
                <div className="bud-kpi">
                  <div className="lbl"><span>PROJECTED</span><span>30 JUN</span></div>
                  <div className="big" style={projOver > 0 ? { color: "var(--neon)", textShadow: "var(--glow-text)" } : null}><span className="cur">CHF</span>{D.chf(projected, 0)}</div>
                  <div className="sub" style={projOver > 0 ? { color: "var(--neon-hot)" } : null}>{projOver > 0 ? "CHF " + D.chf(projOver, 0) + " over at current pace" : "CHF " + D.chf(-projOver, 0) + " under at current pace"}</div>
                </div>
              </div>

              {/* allocation console */}
              <window.AllocationBar caps={caps} />

              <div className="bud-sec">
                <span className="lbl">⊞ ENVELOPES</span>
                <span className="ct">{D.categories.length}</span>
                <span className="rule" />
                <span className="meta">CAP = THRESHOLD · ━ SIGNAL · ┊ PROJECTION · CLICK TO INSPECT</span>
              </div>

              {tw.envLayout === "rows" ?
              <div className="env-rows">
                  {ordered.map((c) =>
                <window.EnvRow key={c.name} c={c} cap={caps[c.name] ?? c.budget} active={sel === c.name}
                onSelect={selectCat} onStep={step} showProj={tw.showProj} />
                )}
                </div> :

              <div className="env-grid">
                  {ordered.map((c) =>
                <window.EnvCard key={c.name} c={c} cap={caps[c.name] ?? c.budget} active={sel === c.name}
                onSelect={selectCat} onStep={step} showProj={tw.showProj} />
                )}
                </div>
              }

            </div>
          </div>
        </div>

        {dockable && <window.BudgetInspector cat={selCat} cap={selCap} onStep={step} onClose={() => setSel(null)} />}
      </div>

      {showDrawer &&
      <div className="sig-drawer-back" onClick={() => setDrawer(false)}>
          <div className="sig-drawer" onClick={(e) => e.stopPropagation()}>
            <window.BudgetInspector cat={selCat} cap={selCap} onStep={step} onClose={() => setDrawer(false)} variant="drawer" />
          </div>
        </div>
      }

      <BudTweaks tw={tw} setTweak={setTweak} onAi={setAiCollapsed} />
    </div>);

}

ReactDOM.createRoot(document.getElementById("root")).render(<BudgetPage />);