/* Phoskonomia — Subscriptions page. Standing/recurring charges as a periodic
   impulse train. Billing sweep up top, then a tunable grid/list of every
   subscription, with a right-dock inspector. Reuses the shared shell. */
const { useState: useStateSP, useMemo: useMemoSP, useEffect: useEffectSP } = React;

const SUB_TWEAK_DEFAULTS = /*EDITMODE-BEGIN*/{
  "subView": "cards",
  "subSort": "due",
  "subAmounts": "monthly",
  "subGroup": false,
  "subHlAuto": false,
  "aiOpen": true
} /*EDITMODE-END*/;

function SubTweaks({ tw, setTweak, onAi }) {
  const { TweaksPanel, TweakSection, TweakRadio, TweakToggle } = window;
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
  const D = window.PHOSK;
  const [tw, setTweak] = window.useTweaks(SUB_TWEAK_DEFAULTS);

  const [aiCollapsed, setAiCollapsed] = useStateSP(!tw.aiOpen);
  const [sel, setSel] = useStateSP(null);

  useEffectSP(() => {document.body.classList.toggle("hl-auto", !!tw.subHlAuto);}, [tw.subHlAuto]);

  const selectSub = (id) => setSel(id === sel ? null : id);

  const S = D.subStats;
  const subs = D.subscriptions;

  const sorted = useMemoSP(() => {
    const arr = [...subs];
    const rank = (s) => s.status === "due" ? 0 : s.status === "soon" ? 1 : s.status === "watch" ? 2 : 3;
    const du = (s) => s.cadence === "monthly" ? D.subDaysUntil(s) : 999;
    if (tw.subSort === "amount") arr.sort((a, b) => D.subMonthlyEquiv(b) - D.subMonthlyEquiv(a));else
    if (tw.subSort === "name") arr.sort((a, b) => a.name.localeCompare(b.name));else
    arr.sort((a, b) => rank(a) - rank(b) || du(a) - du(b));
    return arr;
  }, [tw.subSort, subs]);

  const groups = useMemoSP(() => {
    if (!tw.subGroup) return [{ label: null, items: sorted }];
    const m = sorted.filter((s) => s.cadence === "monthly");
    const y = sorted.filter((s) => s.cadence === "yearly");
    const out = [];
    if (m.length) out.push({ label: "MONTHLY · " + m.length, items: m });
    if (y.length) out.push({ label: "YEARLY · " + y.length, items: y });
    return out;
  }, [sorted, tw.subGroup]);

  const selSub = sel ? D.subById(sel) : null;

  const renderItems = (items) => tw.subView === "rows" ?
  <div className="sub-rows">
      {items.map((s) =>
    <div key={s.id} data-src={s.src}>
          <window.SubRow s={s} active={sel === s.id} onSelect={selectSub} amountMode={tw.subAmounts} />
        </div>
    )}
    </div> :

  <div className="sub-grid">
      {items.map((s) =>
    <div key={s.id} data-src={s.src} style={{ display: "contents" }}>
          <window.SubCard s={s} active={sel === s.id} onSelect={selectSub} amountMode={tw.subAmounts} />
        </div>
    )}
    </div>;


  return (
    <div className="pk" style={{ height: "100vh", minHeight: 0 }}>
      {/* Subscriptions field — the molten signal is anchored RIGHT-CENTRE (a
           recurring axis), a faint blob upper-left, wire + faint to the lower-left. */}
      <window.ScannerBg className="pk-bg" seed={71} shapes={[
      { char: "6", cx: .88, cy: .5, scale: .42, style: "red", morph: "vein", live: true, fill: .52 },
      { char: "2", cx: .16, cy: .26, scale: .3, style: "faint", morph: "blob", live: false, fill: .5 },
      { char: "9", cx: .4, cy: .14, scale: .16, style: "wire", morph: "vein", live: false, fill: .34 },
      { char: "0", cx: .27, cy: .86, scale: .26, style: "faint", morph: "blob", live: false, fill: .42 }]
      } />

      <div className="app-shell swap">
        <window.AiPanel collapsed={aiCollapsed} onToggle={() => setAiCollapsed((c) => !c)} onTrack={() => {}} />

        <div className="app-main">
          <window.TopBar active="SUBSCRIPTIONS" />
          <div className="app-scroll" data-screen-label="SUBSCRIPTIONS">
            <div className="subs-wrap">

              <div className="subs-top">
                <div>
                  <div className="ttl">Subscriptions</div>
                  <div className="sum">
                    <b>{S.count}</b> standing charges · <span className="coral">CHF {D.chf(S.monthly, 0)}</span>/mo ·
                    <b> CHF {D.chf(S.annual, 0)}</b>/yr · {D.cycle.label}
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
                </div>
              </div>

              {/* KPI band */}
              <div className="subs-kpis">
                <div className="subs-kpi accent">
                  <div className="lbl"><span>MONTHLY RECURRING</span><span>RUN-RATE</span></div>
                  <div className="big"><span className="cur">CHF</span>{D.chf(S.monthly, 0)}</div>
                  <div className="ksub">{S.count} active · {S.autoCount} auto-detected by GEMMA4</div>
                </div>
                <div className="subs-kpi blue">
                  <div className="lbl"><span>ANNUALIZED</span><span>12 MO</span></div>
                  <div className="big"><span className="cur">CHF</span>{D.chf(S.annual, 0)}</div>
                  <div className="ksub">Committed across every standing charge</div>
                </div>
                <div className="subs-kpi">
                  <div className="lbl"><span>NEXT 30 DAYS</span><span>{S.next30.length} CHARGES</span></div>
                  <div className="big"><span className="cur">CHF</span>{D.chf(S.next30Total, 0)}</div>
                  <div className="ksub">{S.next30[0] ? <>Next · {S.next30[0].s.name} in {S.next30[0].d}d</> : "Nothing scheduled"}</div>
                </div>
                <div className="subs-kpi">
                  <div className="lbl"><span>NEEDS ATTENTION</span><span>AI</span></div>
                  <div className="big" style={{ color: "var(--neon)", textShadow: "var(--glow-text)" }}>{S.flagged.length}</div>
                  <div className="ksub">1 not seen · 1 due soon · 2 to review</div>
                </div>
              </div>

              {/* billing sweep */}
              <window.BillingSweep subs={subs} sel={sel} onSelect={selectSub} />

              <div className="subs-sec">
                <span className="lbl">⊟ STANDING CHARGES</span>
                <span className="ct">{S.count}</span>
                <span className="rule" />
                <span className="meta">▌ IMPULSE = CHARGE · ━ CYCLE COUNTDOWN · CLICK TO INSPECT</span>
              </div>

              {groups.map((g, i) =>
              <React.Fragment key={g.label || i}>
                  {g.label && <div className="subs-group">{g.label}<span className="gr" /></div>}
                  {renderItems(g.items)}
                </React.Fragment>
              )}

            </div>
          </div>
        </div>

        <window.SubInspector s={selSub} onClose={() => setSel(null)} />
      </div>

      <SubTweaks tw={tw} setTweak={setTweak} onAi={setAiCollapsed} />
    </div>);

}

ReactDOM.createRoot(document.getElementById("root")).render(<SubsPage />);