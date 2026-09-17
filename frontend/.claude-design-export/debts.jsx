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

function DebtTweaks({ tw, setTweak, onAi }) {
  const { TweaksPanel, TweakSection, TweakRadio, TweakToggle, TweakSelect } = window;
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
        onChange={(v) => { setTweak("debtStrategy", v); window.phoskApi.put('/debts/strategy', { strategy: v }); }} />
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
  const D = window.PHOSK;
  const [tw, setTweak] = window.useTweaks(DEBT_TWEAK_DEFAULTS);

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

  const dockable = !narrow && tw.debtInsp === "dock";
  const selectDebt = (id) => { setSel(id === sel ? null : id); if (id !== sel && !dockable) setDrawer(true); };

  const S = D.debtStats;
  const debts = D.debts;

  const target = tw.debtStrategy === "avalanche" ? S.avalancheTarget.id
    : tw.debtStrategy === "snowball" ? S.snowballTarget.id : null;

  const sorted = useMemoDP(() => {
    const arr = [...debts];
    if (tw.debtSort === "apr") arr.sort((a, b) => b.apr - a.apr || b.balance - a.balance);
    else if (tw.debtSort === "name") arr.sort((a, b) => a.name.localeCompare(b.name));
    else if (tw.debtSort === "payoff") arr.sort((a, b) => D.debtMonthsToPayoff(a) - D.debtMonthsToPayoff(b));
    else arr.sort((a, b) => b.balance - a.balance);
    return arr;
  }, [tw.debtSort, debts]);

  const TYPE_LABEL = { LEASE: "LEASES & LOANS", LOAN: "LEASES & LOANS", CARD: "REVOLVING CREDIT", TAX: "OBLIGATIONS", BNPL: "OBLIGATIONS", MEDICAL: "OBLIGATIONS" };
  const groups = useMemoDP(() => {
    if (!tw.debtGroup) return [{ label: null, items: sorted }];
    const order = ["LEASES & LOANS", "REVOLVING CREDIT", "OBLIGATIONS"];
    const buckets = {};
    sorted.forEach((d) => { const g = TYPE_LABEL[d.type]; (buckets[g] = buckets[g] || []).push(d); });
    return order.filter((g) => buckets[g]).map((g) => ({ label: g + " · " + buckets[g].length, items: buckets[g] }));
  }, [sorted, tw.debtGroup]);

  const selDebt = sel ? D.debtById(sel) : null;
  const showDrawer = !dockable && drawer && selDebt;

  const renderItems = (items) => tw.debtView === "rows" ? (
    <div className="debt-rows">
      {items.map((d) => (
        <div key={d.id} data-src={d.src}>
          <window.DebtRow d={d} active={sel === d.id} onSelect={selectDebt} target={target} />
        </div>
      ))}
    </div>
  ) : (
    <div className="debt-grid">
      {items.map((d) => (
        <div key={d.id} data-src={d.src} style={{ display: "contents" }}>
          <window.DebtCard d={d} active={sel === d.id} onSelect={selectDebt} target={target} />
        </div>
      ))}
    </div>
  );

  return (
    <div className="pk" style={{ height: "100vh", minHeight: 0 }}>
      {/* Debts field — a DECAYING waveform. Signature: a SOLID coral principal
          mass upper-right (no other page leads with a solid), a molten growth
          tail sweeping down to lower-left, faint blue blobs deep in the corners. */}
      <window.ScannerBg className="pk-bg" seed={137} shapes={[
        { char: "8", cx: .82, cy: .27, scale: .47, style: "solid", morph: "blob", live: false, fill: .82 },
        { char: "3", cx: .29, cy: .74, scale: .54, style: "red", morph: "vein", live: true, fill: .5 },
        { char: "e", cx: .57, cy: .45, scale: .22, style: "wire", morph: "vein", live: false, fill: .4 },
        { char: "0", cx: .1, cy: .19, scale: .3, style: "faint", morph: "blob", live: false, fill: .5 },
        { char: "5", cx: .94, cy: .9, scale: .2, style: "faint", morph: "blob", live: false, fill: .4 }
      ]} />

      <div className="app-shell swap">
        <window.AiPanel collapsed={aiCollapsed} onToggle={() => setAiCollapsed((c) => !c)} onTrack={() => {}} />

        <div className="app-main">
          <window.TopBar active="DEBTS" />
          <div className="app-scroll" data-screen-label="DEBTS">
            <div className="debts-wrap">

              <div className="debts-top">
                <div>
                  <div className="ttl">Debts</div>
                  <div className="sum">
                    <b>{S.count}</b> open balances · <span className="coral">CHF {D.chf(S.totalOwed, 0)}</span> owed ·
                    <b> CHF {D.chf(S.totalMonthly, 0)}</b>/mo · debt-free {S.debtFreeLabel}
                  </div>
                </div>
                <div className="modes">
                  <button className={"m" + (tw.debtView === "cards" ? " on" : "")} onClick={() => setTweak("debtView", "cards")}>▦ CARDS</button>
                  <button className={"m" + (tw.debtView === "rows" ? " on" : "")} onClick={() => setTweak("debtView", "rows")}>≡ ROWS</button>
                </div>
              </div>

              {/* KPI band */}
              <div className="debts-kpis">
                <div className="debts-kpi accent">
                  <div className="lbl"><span>TOTAL OWED</span><span>OUTSTANDING</span></div>
                  <div className="big"><span className="cur">CHF</span>{D.chf(S.totalOwed, 0)}</div>
                  <div className="ksub">{Math.round(S.paidOffTotalPct * 100)}% paid down of CHF {D.chf(S.totalOrig, 0)} borrowed</div>
                </div>
                <div className="debts-kpi blue">
                  <div className="lbl"><span>MONTHLY OUTFLOW</span><span>SCHEDULED</span></div>
                  <div className="big"><span className="cur">CHF</span>{D.chf(S.totalMonthly, 0)}</div>
                  <div className="ksub">{S.count} payments · {S.autoCount} auto-detected by GEMMA4</div>
                </div>
                <div className="debts-kpi">
                  <div className="lbl"><span>INTEREST</span><span>RUN-RATE / YR</span></div>
                  <div className="big" style={{ color: "var(--warn)" }}><span className="cur">CHF</span>{D.chf(S.totalInterestYr, 0)}</div>
                  <div className="ksub">Avg {(S.weightedApr * 100).toFixed(1)}% · {S.avalancheTarget.name} is the leak</div>
                </div>
                <div className="debts-kpi">
                  <div className="lbl"><span>DEBT-FREE</span><span>PROJECTED</span></div>
                  <div className="big" style={{ color: "var(--ok)" }}>{S.debtFreeLabel}</div>
                  <div className="ksub">{S.horizon} months at the current pace</div>
                </div>
              </div>

              {/* payoff trajectory hero */}
              <window.PayoffTrajectory showProjection={tw.debtProjection} strategy={tw.debtStrategy === "snowball" ? "snowball" : "avalanche"} />

              <div className="debts-sec">
                <span className="lbl">∿ OPEN BALANCES</span>
                <span className="ct">{S.count}</span>
                <span className="rule" />
                <span className="meta">
                  {target ? <>◎ {tw.debtStrategy === "snowball" ? "SNOWBALL" : "AVALANCHE"} TARGET MARKED · </> : null}
                  ▌ BAR = PAID OFF · CLICK TO INSPECT
                </span>
              </div>

              {groups.map((g, i) => (
                <React.Fragment key={g.label || i}>
                  {g.label && <div className="debts-group">{g.label}<span className="gr" /></div>}
                  {renderItems(g.items)}
                </React.Fragment>
              ))}

              {tw.iouShow && (
                <div className="ious">
                  <div className="debts-sec ious-sec">
                    <span className="lbl">⟷ PERSONAL · IOU</span>
                    <span className="ct">{D.personalStats.count}</span>
                    <span className="rule" />
                    <span className="meta">INFORMAL · NO INTEREST · KEPT OUT OF YOUR REAL DEBT</span>
                  </div>

                  <window.NetBeam />

                  <div className="iou-cols">
                    <div className="iou-col">
                      <div className="iou-colh in">← OWED TO YOU<span className="n">{D.personalStats.countIn} · CHF {D.chf(D.personalStats.owedToYou, 0)}</span></div>
                      {D.personal.filter((p) => p.dir === "in").map((p) => <window.PersonCard key={p.id} p={p} />)}
                    </div>
                    <div className="iou-col">
                      <div className="iou-colh out">YOU OWE →<span className="n">{D.personalStats.countOut} · CHF {D.chf(D.personalStats.youOwe, 0)}</span></div>
                      {D.personal.filter((p) => p.dir === "out").map((p) => <window.PersonCard key={p.id} p={p} />)}
                    </div>
                  </div>
                </div>
              )}

            </div>
          </div>
        </div>

        {dockable && <window.DebtInspector d={selDebt} onClose={() => setSel(null)} target={target} />}
      </div>

      {showDrawer && (
        <div className="sig-drawer-back" onClick={() => setDrawer(false)}>
          <div className="sig-drawer" onClick={(e) => e.stopPropagation()}>
            <window.DebtInspector d={selDebt} onClose={() => setDrawer(false)} variant="drawer" target={target} />
          </div>
        </div>
      )}

      <DebtTweaks tw={tw} setTweak={setTweak} onAi={setAiCollapsed} />
    </div>
  );
}

ReactDOM.createRoot(document.getElementById("root")).render(<DebtsPage />);
