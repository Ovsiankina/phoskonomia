/* Phoskonomia — transactions page components: signal pill, line item,
   accordion row, filter bar, full receipt screen. */
const { useState: useStateT, useMemo: useMemoT } = React;

const SIG_SHORT = { coffee: "COFFEE", pain: "PAIN AU CHOC.", beer: "BEER", gruyere: "GRUYÈRE" };

function ConfDot({ conf }) {
  const tone = conf >= 0.85 ? "ok" : conf >= 0.7 ? "blue" : "alert";
  return <window.Dot tone={tone} size={6} />;
}

/* tracked item-signal — distinct coral pill + tiny spark + delta */
function SignalPill({ sigId, active, onSelect }) {
  const s = window.PHOSK.signalById(sigId);
  if (!s) return null;
  const up = (s.deltaPct || 0) >= 0;
  const delta = s.deltaPct == null ? "NEW" : (up ? "↑" : "↓") + Math.abs(s.deltaPct) + "%";
  return (
    <span className={"spill" + (active ? " on" : "")} title="Inspect item-signal"
    onClick={(e) => {e.stopPropagation();onSelect(sigId);window.phoskApi.get('/signals/' + sigId);}}>
      <span className="g">⌁</span>
      <span>{SIG_SHORT[sigId] || s.label}</span>
      <window.Spark data={s.series} w={28} h={12} tone="neon" />
      <span className={"dl " + (up ? "up" : "down")}>{delta}</span>
    </span>);

}

/* normal budget category — quiet indigo tag */
function CatTag({ cat }) {
  return <span className="ctag"><span className="d" />{cat}</span>;
}

function LineItem({ l, sel, onSelectSig }) {
  const D = window.PHOSK;
  const low = l.conf < 0.7;
  return (
    <div className="line">
      <div className="nmwrap">
        <span className="cdot"><ConfDot conf={l.conf} /></span>
        <span className={"nm" + (low ? " low" : "")}>{l.n}{low && " ⚠"}</span>
      </div>
      {l.sig ?
      <SignalPill sigId={l.sig} active={sel === l.sig} onSelect={onSelectSig} /> :
      <CatTag cat={l.cat} />}
      <div style={{ textAlign: "right" }}>
        <div className="lp">{D.chf(l.q * l.p)}</div>
        <div className="qty">{l.q}×{D.chf(l.p)}</div>
      </div>
    </div>);

}

function TxnRow({ t, open, onToggle, onDetails, sel, onSelectSig }) {
  const D = window.PHOSK;
  const [d, mo] = t.date.split(" ");
  return (
    <div className={"trow" + (open ? " open" : "")}>
      <div className="trow-main" onClick={onToggle}>
        <div className="dt">{d}<small>{mo}</small></div>
        <div className="who">
          <span className="sh">{t.shop}</span>
          <div className="meta">
            <span className="ct">{t.cat}</span>
            <span className="ct" style={{ color: "var(--ink-3)" }}>· {t.lines.length} items</span>
            <div className="marks">
              {t.sigs.map((s, i) => <span key={i} className="mk-sig" title="tracked item-signal">⌁</span>)}
              {t.lowConf > 0 && <span className="mk-warn" title="low-confidence items">⚠</span>}
              {t.fixed && <span className="ct" style={{ color: "var(--text-blue)" }}>· FIXED</span>}
            </div>
          </div>
        </div>
        <div className="amt"><span className="c">CHF</span>{D.chf(t.amount)}</div>
        <div className="chev">▸</div>
      </div>

      {open &&
      <div className="trow-body">
          <div className="lh"><span>ITEM</span><span>TRACK / CATEGORY</span><span>CHF</span></div>
          {t.lines.map((l, i) => <LineItem key={i} l={l} sel={sel} onSelectSig={onSelectSig} />)}
          <div className="trow-foot">
            <span className="src"><window.Dot tone="ok" size={5} /> SOURCE · PHOTO · OCR + LLM</span>
            {t.lowConf > 0 &&
          <div className="ai-nudge"><span className="g">⌁</span><span><b>{t.lowConf}</b> low-confidence — AI re-reading now</span>
                <button className="gbtn" onClick={(e) => {e.stopPropagation();onDetails(t);window.phoskApi.post('/transactions/' + t.id + '/reprocess', {});}}>REVIEW</button></div>
          }
            <span className="spacer" />
            <button className="gbtn p" onClick={(e) => {e.stopPropagation();onDetails(t);window.phoskApi.get('/transactions/' + t.id);}}>OPEN RECEIPT ⤢</button>
          </div>
        </div>
      }
    </div>);

}

function FilterBar({ horizon, setHorizon, shop, setShop, cat, setCat, sort, setSort, q, setQ, shops, cats }) {
  const HZ = ["TODAY", "7D", "MONTH", "QUARTER", "YEAR", "ALL"];
  return (
    <div className="txn-filter">
      <div className="txn-horizon">
        {HZ.map((h) => <span key={h} className={"h" + (h === horizon ? " on" : "")} onClick={() => setHorizon(h)}>{h}</span>)}
      </div>
      <div className="txn-sel"><select value={shop} onChange={(e) => setShop(e.target.value)}>
        <option value="">ALL SHOPS</option>{shops.map((s) => <option key={s} value={s}>{s}</option>)}
      </select></div>
      <div className="txn-sel"><select value={cat} onChange={(e) => setCat(e.target.value)}>
        <option value="">ALL CATEGORIES</option>{cats.map((c) => <option key={c} value={c}>{c}</option>)}
      </select></div>
      <div className="txn-sel"><select value={sort} onChange={(e) => setSort(e.target.value)}>
        <option value="date">SORT · DATE</option>
        <option value="amount">SORT · AMOUNT</option>
        <option value="shop">SORT · SHOP</option>
      </select></div>
      <div className="txn-search">
        <span className="mk">⊙</span>
        <input value={q} onChange={(e) => setQ(e.target.value)} placeholder="Search shop or item…" />
      </div>
    </div>);

}

/* ---- full receipt screen with OCR bounding-box reference ---- */
function ReceiptScreen({ t, onClose, sel, onSelectSig }) {
  const D = window.PHOSK;
  if (!t) return null;
  const avgConf = t.lines.reduce((s, l) => s + l.conf, 0) / t.lines.length;
  return (
    <div className="rscreen-back" onClick={onClose}>
      <div className="rscreen" onClick={(e) => e.stopPropagation()}>
        <div className="rscreen-h">
          <span className="bc"><b>TRANSACTIONS</b> / {t.date} /</span>
          <span className="sh">{t.shop}</span>
          <span className="x" onClick={onClose} title="Close">✕</span>
        </div>
        <div className="rscreen-b">
          {/* OCR photo reference */}
          <div className="ocr-pane">
            <div className="ph-h"><span className="t">⌁ OCR · ANNOTATED SCAN</span><span className="s">PADDLEOCR · {t.lines.length} REGIONS</span></div>
            <div className="receipt-paper">
              <div className="rp-shop">{t.shop}</div>
              <div className="rp-meta">{t.date} · 2026 · CHF · TICKET</div>
              <div className="rp-rule" />
              {t.lines.map((l, i) => {
                const low = l.conf < 0.7;
                return (
                  <div key={i} className={"ocrline" + (low ? " low" : "") + (l.sig ? " sigl" : "")}>
                    <span className="box" />
                    <span className="nm">{l.q > 1 ? l.q + "× " : ""}{l.n}</span>
                    <span className="pr">{D.chf(l.q * l.p)}</span>
                    <span className="cf">{l.conf.toFixed(2)}</span>
                  </div>);

              })}
              <div className="rp-total"><span>TOTAL</span><span>CHF {D.chf(t.amount)}</span></div>
            </div>
            <div style={{ marginTop: 12, fontSize: 9.5, color: "var(--ink-3)", lineHeight: 1.6, letterSpacing: ".04em" }}>
              Boxes = detected regions. <span style={{ color: "var(--neon)" }}>Coral</span> = low confidence, queued for AI re-read. <span style={{ color: "var(--neon-hot)" }}>⌁</span> = rolled into a tracked item-signal.
            </div>
          </div>

          {/* structured items + AI */}
          <div className="items-pane">
            <div className="meta-row">
              <div className="kv"><span className="k">Total</span><span className="v" style={{ color: "var(--neon)" }}>CHF {D.chf(t.amount)}</span></div>
              <div className="kv"><span className="k">Items</span><span className="v">{t.lines.length}</span></div>
              <div className="kv"><span className="k">Category</span><span className="v" style={{ fontSize: 13 }}>{t.cat}</span></div>
              <span className="badge">SOURCE · PHOTO</span>
            </div>
            <div className="conf-sum">
              <span>READING CONFIDENCE</span>
              <div className="bar"><i style={{ width: Math.round(avgConf * 100) + "%", background: avgConf >= 0.85 ? "var(--ok)" : "var(--warn)" }} /></div>
              <span style={{ fontFamily: "var(--font-display)", color: "var(--ink)" }}>{Math.round(avgConf * 100)}%</span>
            </div>

            {t.lowConf > 0 &&
            <div className="ai-nudge" style={{ margin: "10px 0 4px" }}>
                <span className="g">⌁</span>
                <span>AI is re-reading <b>{t.lowConf}</b> blurred {t.lowConf > 1 ? "items" : "item"}. Confirm or correct below.</span>
              </div>
            }

            <div className="lh" style={{ display: "grid", gridTemplateColumns: "1fr auto auto", gap: 14, padding: "12px 0 7px", fontSize: 8.5, letterSpacing: ".18em", textTransform: "uppercase", color: "var(--ink-3)", borderBottom: "1px solid var(--hairline)" }}>
              <span>ITEM</span><span>TRACK / CATEGORY</span><span>CHF</span>
            </div>
            {t.lines.map((l, i) =>
            <div className="il" key={i}>
                <div className="nmwrap">
                  <span className="cdot"><ConfDot conf={l.conf} /></span>
                  <span className={"nm" + (l.conf < 0.7 ? " low" : "")} style={{ fontSize: 12.5 }}>{l.n}{l.conf < 0.7 && " ⚠"}</span>
                </div>
                {l.sig ? <SignalPill sigId={l.sig} active={sel === l.sig} onSelect={onSelectSig} /> : <CatTag cat={l.cat} />}
                <div style={{ textAlign: "right" }}>
                  <div className="lp">{D.chf(l.q * l.p)}</div>
                  <div className="qty">{l.q}×{D.chf(l.p)}</div>
                </div>
              </div>
            )}

            {t.sigs.length > 0 &&
            <div className="ai-nudge" style={{ marginTop: 16, borderColor: "var(--hairline)", background: "rgba(20,14,44,.34)" }}>
                <span className="g" style={{ color: "var(--indigo-neon)" }}>⌁</span>
                <span>This receipt feeds <b>{t.sigs.length}</b> tracked {t.sigs.length > 1 ? "signals" : "signal"}. Click any ⌁ pill to inspect its trend across all time.</span>
              </div>
            }
          </div>
        </div>
      </div>
    </div>);

}

Object.assign(window, { SignalPill, CatTag, LineItem, TxnRow, FilterBar, ReceiptScreen, ConfDot });