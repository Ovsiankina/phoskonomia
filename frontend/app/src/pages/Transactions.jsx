import React from 'react'
import { chf } from '../data/phosk.js'
import { api, useGet } from '../lib/api.js'
import { ScannerBg, Dot } from '../components/prims.jsx'
import { TopBar } from '../components/comps.jsx'
import { AiPanel, SignalPanel } from '../components/shell.jsx'
import { Awaiting } from '../components/states.jsx'
import { useTweaks, TweaksPanel, TweakSection, TweakToggle } from '../lib/tweaks.jsx'

/* Phoskonomia — transactions page components: signal pill, line item,
   accordion row, filter bar, full receipt screen.

   Fetch-only + wired buttons: the list comes from GET /transactions (filters
   drive its params); each opened row / receipt modal fetches GET
   /transactions/{id}/lines; the signal pill inspects GET /signals/{id}; per-line
   correct/confirm PATCHes /transactions/{id}/lines/{lineIndex}; the AI re-read
   nudge POSTs /transactions/{id}/reprocess. No mock data; each surface renders
   <Awaiting/> while the backend stubs answer 501. */

const SIG_SHORT = { coffee: "COFFEE", pain: "PAIN AU CHOC.", beer: "BEER", gruyere: "GRUYÈRE" };

/* Map the horizon buttons to the backend `period` query param (ALL → omit). */
const HZ_PERIOD = { TODAY: "day", "7D": "week", MONTH: "month", QUARTER: "quarter", YEAR: "year", ALL: "" };

function ConfDot({ conf }) {
  const c = conf == null ? 0 : conf;
  const tone = c >= 0.85 ? "ok" : c >= 0.7 ? "blue" : "alert";
  return <Dot tone={tone} size={6} />;
}

/* tracked item-signal — distinct coral pill + tiny spark + delta.
   The pill renders from the line's own signal_id; the trend spark is best-effort
   (the line endpoint does not carry the full series), so it omits the spark when
   no series is available rather than fabricating one. */
function SignalPill({ sigId, label, active, onSelect }) {
  if (!sigId) return null;
  return (
    <span className={"spill" + (active ? " on" : "")} title="Inspect item-signal"
      onClick={(e) => { e.stopPropagation(); onSelect(sigId); }}>
      <span className="g">⌁</span>
      <span>{SIG_SHORT[sigId] || label || sigId}</span>
    </span>);
}

/* normal budget category — quiet indigo tag */
function CatTag({ cat }) {
  return <span className="ctag"><span className="d" />{cat}</span>;
}

/* A single fetched line. Fields per API.md: name/qty/unit_price/line_total/
   category/signal_id/confidence. line_total is read off the record (not
   recomputed). The per-line correct/confirm controls PATCH the line. */
function LineItem({ l, idx, sel, onSelectSig, onPatchLine }) {
  const conf = l.confidence;
  const low = conf != null && conf < 0.7;
  const total = l.line_total; // backend-derived; chf() renders “—” when absent
  return (
    <div className="line">
      <div className="nmwrap">
        <span className="cdot"><ConfDot conf={conf} /></span>
        <span className={"nm" + (low ? " low" : "")}>{l.name}{low && " ⚠"}</span>
      </div>
      {l.signal_id ?
        <SignalPill sigId={l.signal_id} label={l.name} active={sel === l.signal_id} onSelect={onSelectSig} /> :
        <CatTag cat={l.category} />}
      <div style={{ textAlign: "right" }}>
        <div className="lp">{chf(total)}</div>
        <div className="qty">{l.qty}×{chf(l.unit_price)}</div>
      </div>
      {low && onPatchLine &&
        <div style={{ gridColumn: "1 / -1", display: "flex", gap: 7, marginTop: 6 }}>
          <button className="gbtn p" onClick={(e) => { e.stopPropagation(); onPatchLine(idx, { confirmed: true }); }}>CONFIRM</button>
          <button className="gbtn" onClick={(e) => { e.stopPropagation(); onCorrect(idx, l, onPatchLine); }}>CORRECT</button>
        </div>}
    </div>);
}

/* Prompt-driven correction (kept lightweight — the on-brand inline editor lives
   in the receipt modal; this just lets the user fix the parsed name/qty/price). */
function onCorrect(idx, l, onPatchLine) {
  const name = window.prompt("Item name", l.name != null ? l.name : "");
  if (name == null) return;
  const qtyS = window.prompt("Qty", l.qty != null ? String(l.qty) : "");
  if (qtyS == null) return;
  const priceS = window.prompt("Unit price (CHF)", l.unit_price != null ? String(l.unit_price) : "");
  if (priceS == null) return;
  const patch = { name, confirmed: true };
  const qty = Number(qtyS); if (isFinite(qty)) patch.qty = qty;
  const price = Number(priceS); if (isFinite(price)) patch.unit_price = price;
  onPatchLine(idx, patch);
}

/* Accordion body — fetches this receipt's lines. Mounted ONLY while the row is
   open (see TxnRow), so the GET only fires for opened receipts. */
function TxnRowBody({ t, sel, onSelectSig, onDetails }) {
  const { data, status, loading, res, reload } = useGet(`/transactions/${t.id}/lines`);
  const body = data || {};
  const lines = Array.isArray(body.lines) ? body.lines : [];
  const lowConf = body.lowConf != null ? body.lowConf : t.low_conf_count;

  const patchLine = (lineIndex, patch) =>
    api.patch(`/transactions/${t.id}/lines/${lineIndex}`, patch).then(() => reload());
  const reprocess = () =>
    api.post(`/transactions/${t.id}/reprocess`, {}).then(() => reload());

  if (status !== 200 || lines.length === 0) {
    return (
      <div className="trow-body">
        <Awaiting label="RECEIPT LINES" res={res} loading={loading} tone="blue" />
      </div>);
  }
  return (
    <div className="trow-body">
      <div className="lh"><span>ITEM</span><span>TRACK / CATEGORY</span><span>CHF</span></div>
      {lines.map((l, i) => <LineItem key={i} l={l} idx={i} sel={sel} onSelectSig={onSelectSig} onPatchLine={patchLine} />)}
      <div className="trow-foot">
        <span className="src"><Dot tone="ok" size={5} /> SOURCE · PHOTO · OCR + LLM</span>
        {lowConf > 0 &&
          <div className="ai-nudge"><span className="g">⌁</span><span><b>{lowConf}</b> low-confidence — AI re-reading now</span>
            <button className="gbtn" onClick={(e) => { e.stopPropagation(); reprocess(); }}>RE-READ</button>
            <button className="gbtn" onClick={(e) => { e.stopPropagation(); onDetails(t); }}>REVIEW</button></div>}
        <span className="spacer" />
        <button className="gbtn p" onClick={(e) => { e.stopPropagation(); onDetails(t); }}>OPEN RECEIPT ⤢</button>
      </div>
    </div>);
}

function TxnRow({ t, open, onToggle, onDetails, sel, onSelectSig }) {
  const dateStr = String(t.date || "");
  const [d, mo] = dateStr.split(" ");
  const sigIds = Array.isArray(t.signal_ids) ? t.signal_ids : [];
  const itemCount = t.item_count != null ? t.item_count : 0;
  const low = t.low_conf_count != null ? t.low_conf_count : 0;
  return (
    <div className={"trow" + (open ? " open" : "")}>
      <div className="trow-main" onClick={onToggle}>
        <div className="dt">{d || dateStr}<small>{mo || ""}</small></div>
        <div className="who">
          <span className="sh">{t.shop}</span>
          <div className="meta">
            <span className="ct">{t.category}</span>
            <span className="ct" style={{ color: "var(--ink-3)" }}>· {itemCount} items</span>
            <div className="marks">
              {sigIds.map((s, i) => <span key={i} className="mk-sig" title="tracked item-signal">⌁</span>)}
              {low > 0 && <span className="mk-warn" title="low-confidence items">⚠</span>}
              {t.fixed && <span className="ct" style={{ color: "var(--text-blue)" }}>· FIXED</span>}
            </div>
          </div>
        </div>
        <div className="amt"><span className="c">CHF</span>{chf(t.amount)}</div>
        <div className="chev">▸</div>
      </div>
      {open && <TxnRowBody t={t} sel={sel} onSelectSig={onSelectSig} onDetails={onDetails} />}
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

/* ---- full receipt screen with OCR bounding-box reference.
   Fetches GET /transactions/{id}/lines for the line table and GET
   /transactions/{id} for detail (avg_confidence, source, ocr_regions). Per-line
   confirm/correct PATCHes; the AI re-read nudge POSTs reprocess. ---- */
function ReceiptScreen({ t, onClose, sel, onSelectSig }) {
  // Thin wrapper: only mount the fetching body when a receipt is actually open,
  // so the line/detail GETs never fire (and never hit a null path) while closed.
  if (!t) return null;
  return <ReceiptBody t={t} onClose={onClose} sel={sel} onSelectSig={onSelectSig} />;
}

function ReceiptBody({ t, onClose, sel, onSelectSig }) {
  const id = t.id;
  const linesRes = useGet(`/transactions/${id}/lines`);
  const detailRes = useGet(`/transactions/${id}`);

  const lbody = linesRes.data || {};
  const lines = Array.isArray(lbody.lines) ? lbody.lines : [];
  const sigs = Array.isArray(lbody.sigs) ? lbody.sigs : [];
  const lowConf = lbody.lowConf != null ? lbody.lowConf
    : (t.low_conf_count != null ? t.low_conf_count : 0);

  const detail = detailRes.data || {};
  const avgConf = detail.avg_confidence != null ? detail.avg_confidence : null;
  const source = detail.source || {};
  const ocrEngine = source.ocr_engine || "PADDLEOCR";
  const ocrRegions = Array.isArray(detail.ocr_regions) ? detail.ocr_regions : [];
  const regionCount = ocrRegions.length || lines.length;

  const patchLine = (lineIndex, patch) =>
    api.patch(`/transactions/${id}/lines/${lineIndex}`, patch).then(() => linesRes.reload());
  const reprocess = () =>
    api.post(`/transactions/${id}/reprocess`, {}).then(() => linesRes.reload());

  const hasLines = linesRes.status === 200 && lines.length > 0;

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
            <div className="ph-h"><span className="t">⌁ OCR · ANNOTATED SCAN</span><span className="s">{String(ocrEngine).toUpperCase()} · {regionCount} REGIONS</span></div>
            {!hasLines
              ? <Awaiting label="ANNOTATED SCAN" res={linesRes.res} loading={linesRes.loading} tone="blue" />
              : (
                <div className="receipt-paper">
                  <div className="rp-shop">{t.shop}</div>
                  <div className="rp-meta">{t.date} · CHF · TICKET</div>
                  <div className="rp-rule" />
                  {lines.map((l, i) => {
                    const conf = l.confidence;
                    const low = conf != null && conf < 0.7;
                    const total = l.line_total; // backend-derived; chf() renders “—” when absent
                    return (
                      <div key={i} className={"ocrline" + (low ? " low" : "") + (l.signal_id ? " sigl" : "")}>
                        <span className="box" />
                        <span className="nm">{l.qty > 1 ? l.qty + "× " : ""}{l.name}</span>
                        <span className="pr">{chf(total)}</span>
                        <span className="cf">{conf != null ? conf.toFixed(2) : "—"}</span>
                      </div>);
                  })}
                  <div className="rp-total"><span>TOTAL</span><span>CHF {chf(t.amount)}</span></div>
                </div>)}
            <div style={{ marginTop: 12, fontSize: 9.5, color: "var(--ink-3)", lineHeight: 1.6, letterSpacing: ".04em" }}>
              Boxes = detected regions. <span style={{ color: "var(--neon)" }}>Coral</span> = low confidence, queued for AI re-read. <span style={{ color: "var(--neon-hot)" }}>⌁</span> = rolled into a tracked item-signal.
            </div>
          </div>

          {/* structured items + AI */}
          <div className="items-pane">
            <div className="meta-row">
              <div className="kv"><span className="k">Total</span><span className="v" style={{ color: "var(--neon)" }}>CHF {chf(t.amount)}</span></div>
              <div className="kv"><span className="k">Items</span><span className="v">{hasLines ? lines.length : (t.item_count != null ? t.item_count : "—")}</span></div>
              <div className="kv"><span className="k">Category</span><span className="v" style={{ fontSize: 13 }}>{t.category}</span></div>
              <span className="badge">SOURCE · {(source.type || "PHOTO").toString().toUpperCase()}</span>
            </div>
            <div className="conf-sum">
              <span>READING CONFIDENCE</span>
              <div className="bar"><i style={{ width: (avgConf != null ? Math.round(avgConf * 100) : 0) + "%", background: (avgConf != null && avgConf >= 0.85) ? "var(--ok)" : "var(--warn)" }} /></div>
              <span style={{ fontFamily: "var(--font-display)", color: "var(--ink)" }}>{avgConf != null ? Math.round(avgConf * 100) + "%" : "—"}</span>
            </div>

            {lowConf > 0 &&
              <div className="ai-nudge" style={{ margin: "10px 0 4px" }}>
                <span className="g">⌁</span>
                <span>AI is re-reading <b>{lowConf}</b> blurred {lowConf > 1 ? "items" : "item"}. Confirm or correct below.</span>
                <button className="gbtn" onClick={(e) => { e.stopPropagation(); reprocess(); }}>RE-READ</button>
              </div>}

            <div className="lh" style={{ display: "grid", gridTemplateColumns: "1fr auto auto", gap: 14, padding: "12px 0 7px", fontSize: 8.5, letterSpacing: ".18em", textTransform: "uppercase", color: "var(--ink-3)", borderBottom: "1px solid var(--hairline)" }}>
              <span>ITEM</span><span>TRACK / CATEGORY</span><span>CHF</span>
            </div>
            {!hasLines
              ? <Awaiting label="LINE ITEMS" res={linesRes.res} loading={linesRes.loading} tone="blue" />
              : lines.map((l, i) => {
                const conf = l.confidence;
                const low = conf != null && conf < 0.7;
                const total = l.line_total; // backend-derived; chf() renders “—” when absent
                return (
                  <div className="il" key={i}>
                    <div className="nmwrap">
                      <span className="cdot"><ConfDot conf={conf} /></span>
                      <span className={"nm" + (low ? " low" : "")} style={{ fontSize: 12.5 }}>{l.name}{low && " ⚠"}</span>
                    </div>
                    {l.signal_id ? <SignalPill sigId={l.signal_id} label={l.name} active={sel === l.signal_id} onSelect={onSelectSig} /> : <CatTag cat={l.category} />}
                    <div style={{ textAlign: "right" }}>
                      <div className="lp">{chf(total)}</div>
                      <div className="qty">{l.qty}×{chf(l.unit_price)}</div>
                    </div>
                    {low &&
                      <div style={{ gridColumn: "1 / -1", display: "flex", gap: 7, marginTop: 6 }}>
                        <button className="gbtn p" onClick={(e) => { e.stopPropagation(); patchLine(i, { confirmed: true }); }}>CONFIRM</button>
                        <button className="gbtn" onClick={(e) => { e.stopPropagation(); onCorrect(i, l, patchLine); }}>CORRECT</button>
                      </div>}
                  </div>);
              })}

            {sigs.length > 0 &&
              <div className="ai-nudge" style={{ marginTop: 16, borderColor: "var(--hairline)", background: "rgba(20,14,44,.34)" }}>
                <span className="g" style={{ color: "var(--indigo-neon)" }}>⌁</span>
                <span>This receipt feeds <b>{sigs.length}</b> tracked {sigs.length > 1 ? "signals" : "signal"}. Click any ⌁ pill to inspect its trend across all time.</span>
              </div>}
          </div>
        </div>
      </div>
    </div>);
}

/* Phoskonomia — Transactions page. Assembles AI panel + list + signal panel,
   with the detail+drill interaction exposed as live tweaks. */
const { useState: useStateP, useEffect: useEffectP } = React;

const TXN_TWEAK_DEFAULTS = /*EDITMODE-BEGIN*/{
  "aiOpen": true,
  "showSparks": true
} /*EDITMODE-END*/;

function TxnTweaks({ tw, setTweak, onAi }) {
  if (!TweaksPanel) return null;
  return (
    <TweaksPanel title="Tweaks">
      <TweakSection label="Assistant" />
      <TweakToggle label="AI panel open" value={tw.aiOpen}
        onChange={(v) => { setTweak("aiOpen", v); onAi(!v); }} />
      <TweakSection label="Item-signals" />
      <TweakToggle label="Trend sparks on pills" value={tw.showSparks}
        onChange={(v) => setTweak("showSparks", v)} />
    </TweaksPanel>);
}

/* Group fetched rows by their day label for the dated list, preserving order. */
function groupByDay(rows) {
  const g = [];
  let cur = null;
  rows.forEach((t) => {
    if (t.date !== cur) { cur = t.date; g.push({ day: t.date, items: [] }); }
    g[g.length - 1].items.push(t);
  });
  return g;
}

/* Right-side signal inspector sheet. Mounted ONLY when a signal id is selected,
   so GET /signals/{id} fires for the inspected pill (and never on a null path).
   Candidate track/dismiss are wired inside SignalPanel; onChanged re-fetches. */
function SignalSheet({ sigId, cycleLabel, onClose }) {
  const { data, reload } = useGet(`/signals/${sigId}`);
  return (
    <div className="sig-sheet-back" onClick={onClose}>
      <div className="sig-sheet" onClick={(e) => e.stopPropagation()}>
        <SignalPanel sig={data || null} onClose={onClose} variant="sheet" cycleLabel={cycleLabel} onChanged={reload} />
      </div>
    </div>);
}

function TxnPage() {
  const [tw, setTweak] = useTweaks(TXN_TWEAK_DEFAULTS);

  const [aiCollapsed, setAiCollapsed] = useStateP(!tw.aiOpen);
  const [openIds, setOpenIds] = useStateP(() => new Set());
  const toggleOpen = (id) => setOpenIds((prev) => {
    const next = new Set(prev);
    if (next.has(id)) next.delete(id); else next.add(id);
    return next;
  });
  const collapseAll = () => setOpenIds(new Set());
  const [detail, setDetail] = useStateP(null);
  const [sel, setSel] = useStateP(null); // selected signal id
  const [drawerSig, setDrawerSig] = useStateP(false);

  useEffectP(() => { document.body.classList.toggle("no-sparks", !tw.showSparks); }, [tw.showSparks]);

  // filters → /transactions query params (changing any re-fetches via useGet)
  const [horizon, setHorizon] = useStateP("MONTH");
  const [shop, setShop] = useStateP("");
  const [cat, setCat] = useStateP("");
  const [sort, setSort] = useStateP("date");
  const [q, setQ] = useStateP("");

  // Debounce the free-text query so each keystroke doesn't fire a fetch.
  const [qParam, setQParam] = useStateP("");
  useEffectP(() => {
    const h = setTimeout(() => setQParam(q), 300);
    return () => clearTimeout(h);
  }, [q]);

  const params = {
    period: HZ_PERIOD[horizon],   // '' → dropped by qs() (ALL)
    shop: shop || undefined,
    category: cat || undefined,
    sort,
    q: qParam || undefined,
  };

  const { data: txnData, status, loading, res } = useGet('/transactions', params);
  const body = txnData || {};
  const rows = Array.isArray(body.transactions) ? body.transactions : [];
  const summary = body.summary || {};
  const shops = Array.isArray(body.available_shops) ? body.available_shops : [];
  const cats = Array.isArray(body.available_categories) ? body.available_categories : [];

  const entryCount = summary.entry_count != null ? summary.entry_count : rows.length;
  const periodLabel = summary.period_label != null ? summary.period_label : horizon;

  const selectSig = (id) => { setSel(id); setDrawerSig(true); };
  const onDetails = (t) => setDetail(t);

  const grouped = groupByDay(rows);

  // SignalPanel content comes from GET /signals/{id} (inspect a tracked pill),
  // fetched inside <SignalSheet/> so it only loads while a signal is selected.
  const showSheet = drawerSig && !!sel;

  const hasRows = status === 200 && rows.length > 0;

  return (
    <div className="pk" style={{ height: "100vh", minHeight: 0 }}>
      {/* Transactions field — the molten signal rides HIGH-LEFT (a stream
           entering the frame), trailing wire + faint blobs toward the bottom. */}
      <ScannerBg className="pk-bg" seed={47} shapes={[
        { char: "8", cx: .24, cy: .21, scale: .4, style: "red", morph: "vein", live: true, fill: .52 },
        { char: "e", cx: .87, cy: .6, scale: .28, style: "faint", morph: "blob", live: false, fill: .48 },
        { char: "1", cx: .62, cy: .9, scale: .2, style: "wire", morph: "vein", live: false, fill: .36 },
        { char: "5", cx: .11, cy: .84, scale: .18, style: "faint", morph: "blob", live: false, fill: .36 }]
      } />

      <div className="app-shell swap">
        <AiPanel collapsed={aiCollapsed} onToggle={() => setAiCollapsed((c) => !c)} onTrack={selectSig} />

        <div className="app-main">
          <TopBar active="TRANSACTIONS" />
          <div className="app-scroll" data-screen-label="TRANSACTIONS">
            <div className="txn-wrap">
              <div className="txn-top">
                <div>
                  <div className="ttl">Transactions</div>
                  <div className="sum"><b>{entryCount}</b> entries · <span className="coral">CHF {chf(summary.total_amount)}</span> · {periodLabel}</div>
                </div>
              </div>

              <FilterBar horizon={horizon} setHorizon={setHorizon} shop={shop} setShop={setShop}
                cat={cat} setCat={setCat} sort={sort} setSort={setSort} q={q} setQ={setQ} shops={shops} cats={cats} />

              <div className="txn-list">
                {!hasRows
                  ? <Awaiting label="TX · LIST" res={res} loading={loading} tone="blue" />
                  : grouped.map((g) =>
                    <React.Fragment key={g.day}>
                      <div className="day">{g.day} <span className="ru" /></div>
                      {g.items.map((t) =>
                        <TxnRow key={t.id} t={t} open={openIds.has(t.id)}
                          onToggle={() => toggleOpen(t.id)}
                          onDetails={onDetails} sel={sel} onSelectSig={selectSig} />)}
                    </React.Fragment>)}
              </div>
            </div>
          </div>
        </div>

      </div>

      {showSheet &&
        <SignalSheet sigId={sel} cycleLabel={periodLabel} onClose={() => setDrawerSig(false)} />}

      {openIds.size > 1 &&
        <button className={"collapse-all" + (aiCollapsed ? " ai-min" : "")} onClick={collapseAll} title="Collapse all open transactions">
          <span className="ca-n">{openIds.size}</span>
          <span className="ca-l">COLLAPSE ALL</span>
          <span className="ca-i">▴</span>
        </button>}

      <ReceiptScreen t={detail} onClose={() => setDetail(null)} sel={sel} onSelectSig={selectSig} />

      <TxnTweaks tw={tw} setTweak={setTweak} onAi={setAiCollapsed} />
    </div>);
}

export default TxnPage;
