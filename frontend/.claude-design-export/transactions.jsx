/* Phoskonomia — Transactions page. Assembles AI panel + list + signal panel,
   with the detail+drill interaction exposed as live tweaks. */
const { useState: useStateP, useMemo: useMemoP, useEffect: useEffectP } = React;

const TXN_TWEAK_DEFAULTS = /*EDITMODE-BEGIN*/{
  "aiOpen": true,
  "showSparks": true
} /*EDITMODE-END*/;

function TxnTweaks({ tw, setTweak, onAi }) {
  const { TweaksPanel, TweakSection, TweakRadio, TweakToggle } = window;
  if (!TweaksPanel) return null;
  return (
    <TweaksPanel title="Tweaks">
      <TweakSection label="Assistant" />
      <TweakToggle label="AI panel open" value={tw.aiOpen}
      onChange={(v) => {setTweak("aiOpen", v);onAi(!v);}} />
      <TweakSection label="Item-signals" />
      <TweakToggle label="Trend sparks on pills" value={tw.showSparks}
      onChange={(v) => setTweak("showSparks", v)} />
    </TweaksPanel>);

}

function TxnPage() {
  const D = window.PHOSK;
  const [tw, setTweak] = window.useTweaks(TXN_TWEAK_DEFAULTS);

  const [aiCollapsed, setAiCollapsed] = useStateP(!tw.aiOpen);
  const [openIds, setOpenIds] = useStateP(() => new Set([D.transactions[0].id]));
  const toggleOpen = (id) => setOpenIds((prev) => {
    const next = new Set(prev);
    if (next.has(id)) next.delete(id);else next.add(id);
    return next;
  });
  const collapseAll = () => setOpenIds(new Set());
  const [detail, setDetail] = useStateP(null);
  const [sel, setSel] = useStateP(null); // selected signal id
  const [drawerSig, setDrawerSig] = useStateP(false);

  useEffectP(() => {document.body.classList.toggle("no-sparks", !tw.showSparks);}, [tw.showSparks]);

  // filters
  const [horizon, setHorizon] = useStateP("MONTH");
  const [shop, setShop] = useStateP("");
  const [cat, setCat] = useStateP("");
  const [sort, setSort] = useStateP("date");
  const [q, setQ] = useStateP("");

  const shops = useMemoP(() => [...new Set(D.transactions.map((t) => t.shop))], []);
  const cats = useMemoP(() => [...new Set(D.transactions.map((t) => t.cat))], []);

  const rows = useMemoP(() => {
    let r = D.transactions.filter((t) => {
      if (shop && t.shop !== shop) return false;
      if (cat && t.cat !== cat) return false;
      if (q) {
        const hay = (t.shop + " " + t.cat + " " + t.lines.map((l) => l.n).join(" ")).toLowerCase();
        if (!hay.includes(q.toLowerCase())) return false;
      }
      return true;
    });
    const idx = (d) => D.transactions.indexOf(d);
    if (sort === "amount") r = [...r].sort((a, b) => b.amount - a.amount);else
    if (sort === "shop") r = [...r].sort((a, b) => a.shop.localeCompare(b.shop));else
    r = [...r].sort((a, b) => idx(a) - idx(b));
    return r;
  }, [shop, cat, q, sort]);

  const total = rows.reduce((s, t) => s + t.amount, 0);

  const selectSig = (id) => {
    setSel(id);
    setDrawerSig(true);
  };
  const onDetails = (t) => setDetail(t);

  const grouped = useMemoP(() => {
    const g = [];let cur = null;
    rows.forEach((t) => {if (t.date !== cur) {cur = t.date;g.push({ day: t.date, items: [] });}g[g.length - 1].items.push(t);});
    return g;
  }, [rows]);

  const sigObj = window.PHOSK.signalById(sel);
  const showSheet = drawerSig && sigObj;

  return (
    <div className="pk" style={{ height: "100vh", minHeight: 0 }}>
      {/* Transactions field — the molten signal rides HIGH-LEFT (a stream
           entering the frame), trailing wire + faint blobs toward the bottom. */}
      <window.ScannerBg className="pk-bg" seed={47} shapes={[
      { char: "8", cx: .24, cy: .21, scale: .4, style: "red", morph: "vein", live: true, fill: .52 },
      { char: "e", cx: .87, cy: .6, scale: .28, style: "faint", morph: "blob", live: false, fill: .48 },
      { char: "1", cx: .62, cy: .9, scale: .2, style: "wire", morph: "vein", live: false, fill: .36 },
      { char: "5", cx: .11, cy: .84, scale: .18, style: "faint", morph: "blob", live: false, fill: .36 }]
      } />

      <div className="app-shell swap">
        <window.AiPanel collapsed={aiCollapsed} onToggle={() => setAiCollapsed((c) => !c)} onTrack={selectSig} />

        <div className="app-main">
          <window.TopBar active="TRANSACTIONS" />
          <div className="app-scroll" data-screen-label="TRANSACTIONS">
            <div className="txn-wrap">
              <div className="txn-top">
                <div>
                  <div className="ttl">Transactions</div>
                  <div className="sum"><b>{rows.length}</b> entries · <span className="coral">CHF {D.chf(total)}</span> · {horizon === "MONTH" ? D.cycle.label : horizon}</div>
                </div>
              </div>

              <window.FilterBar horizon={horizon} setHorizon={setHorizon} shop={shop} setShop={setShop}
              cat={cat} setCat={setCat} sort={sort} setSort={setSort} q={q} setQ={setQ} shops={shops} cats={cats} />

              <div className="txn-list">
                {grouped.length === 0 && <div className="day" style={{ color: "var(--ink-3)", padding: "30px 0" }}>No transactions match.</div>}
                {grouped.map((g) =>
                <React.Fragment key={g.day}>
                    <div className="day">{g.day} <span className="ru" /></div>
                    {g.items.map((t) =>
                  <window.TxnRow key={t.id} t={t} open={openIds.has(t.id)}
                  onToggle={() => toggleOpen(t.id)}
                  onDetails={onDetails} sel={sel} onSelectSig={selectSig} />
                  )}
                  </React.Fragment>
                )}
              </div>
            </div>
          </div>
        </div>

      </div>

      {showSheet &&
      <div className="sig-sheet-back" onClick={() => setDrawerSig(false)}>
          <div className="sig-sheet" onClick={(e) => e.stopPropagation()}>
            <window.SignalPanel sig={sigObj} onClose={() => setDrawerSig(false)} variant="sheet" />
          </div>
        </div>
      }

      {openIds.size > 1 &&
      <button className={"collapse-all" + (aiCollapsed ? " ai-min" : "")} onClick={collapseAll} title="Collapse all open transactions">
          <span className="ca-n">{openIds.size}</span>
          <span className="ca-l">COLLAPSE ALL</span>
          <span className="ca-i">▴</span>
        </button>
      }

      <window.ReceiptScreen t={detail} onClose={() => setDetail(null)} sel={sel} onSelectSig={selectSig} />

      <TxnTweaks tw={tw} setTweak={setTweak} onAi={setAiCollapsed} />
    </div>);

}

ReactDOM.createRoot(document.getElementById("root")).render(<TxnPage />);