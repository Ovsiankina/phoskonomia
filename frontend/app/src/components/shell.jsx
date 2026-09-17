/* Phoskonomia — shared shell components: left AI panel + right Signal panel.
   Dual-published: ES named exports + window net.
   Top-level components defined here: SigSpark, AiPanel, SignalPanel, SignalCard, SignalStrip.
   Wired to phosk_api: feed/chat/status load from the backend, every action POSTs. */
import React from 'react'
import { chf } from '../data/phosk.js'
import { api, useGet } from '../lib/api.js'

const { useState: useStateS, useRef: useRefS, useEffect: useEffectS } = React;

/* ---- larger sparkline with up/down tone + area, for the signal inspector ---- */
function SigSpark({ data = [], w = 312, h = 88 }) {
  if (!data.length) return <svg width="100%" height={h} viewBox={`0 0 ${w} ${h}`} preserveAspectRatio="none" style={{ display: "block" }} />;
  const max = Math.max(...data), min = Math.min(...data), rng = max - min || 1;
  const up = data[data.length - 1] >= data[0];
  const col = up ? "var(--neon)" : "var(--ok)";
  const X = (i) => (i / (data.length - 1) * (w - 2) + 1);
  const Y = (v) => (h - 6 - ((v - min) / rng) * (h - 14));
  const pts = data.map((v, i) => `${X(i).toFixed(1)},${Y(v).toFixed(1)}`).join(" ");
  const area = `1,${h} ${pts} ${(w - 1)},${h}`;
  return (
    <svg width="100%" height={h} viewBox={`0 0 ${w} ${h}`} preserveAspectRatio="none" style={{ display: "block" }}>
      <defs>
        <linearGradient id="sigfill" x1="0" y1="0" x2="0" y2="1">
          <stop offset="0%" stopColor={up ? "rgba(255,94,77,.26)" : "rgba(95,208,138,.22)"} />
          <stop offset="100%" stopColor="rgba(0,0,0,0)" />
        </linearGradient>
      </defs>
      {[0.5].map(f => <line key={f} x1="1" y1={Y(min + rng * f)} x2={w - 1} y2={Y(min + rng * f)} stroke="rgba(106,95,192,.18)" strokeWidth="1" strokeDasharray="2 4" />)}
      <polygon points={area} fill="url(#sigfill)" />
      <polyline points={pts} fill="none" stroke={col} strokeWidth="2" strokeLinejoin="round" strokeLinecap="round" style={{ filter: `drop-shadow(0 0 3px ${col})` }} />
      <circle cx={X(data.length - 1)} cy={Y(data[data.length - 1])} r="3" fill="var(--neon-white)" style={{ filter: `drop-shadow(0 0 4px ${col})` }} />
    </svg>
  );
}

/* ===================== LEFT — AI PANEL ===================== */
/* Feed (/ai/feed), chat history (/ai/chat GET) and status (/ai/status) all load
   from the backend; sending a message POSTs /ai/chat; feed actions POST track /
   dismiss then re-fetch the feed. No mock seed — empty until the backend serves. */
function AiPanel({ collapsed, onToggle, onTrack }) {
  const { data: feedData, reload: reloadFeed } = useGet('/ai/feed');
  const { data: chatData } = useGet('/ai/chat');
  const { data: aiStatus } = useGet('/ai/status');

  const feed = Array.isArray(feedData) ? feedData : (feedData && (feedData.feed || feedData.items)) || [];
  const online = aiStatus ? !!aiStatus.online : false;
  const model = (aiStatus && aiStatus.model) || "GEMMA4";
  const engine = (aiStatus && aiStatus.engine) || "OLLAMA";
  const location = (aiStatus && aiStatus.location) || "LOCAL";

  const [msgs, setMsgs] = useStateS([]);
  const [dismissed, setDismissed] = useStateS(() => new Set());
  const [draft, setDraft] = useStateS("");
  const chatRef = useRefS(null);
  useEffectS(() => { if (chatRef.current) chatRef.current.scrollTop = chatRef.current.scrollHeight; }, [msgs]);
  // Seed the transcript from the backend's chat history once it loads.
  useEffectS(() => {
    if (!chatData) return;
    const raw = Array.isArray(chatData) ? chatData : (chatData.messages || chatData.history || []);
    setMsgs(raw.map((m) => ({
      who: (m.role === "user" || m.who === "usr" || m.from === "user") ? "usr" : "sys",
      t: m.text != null ? m.text : (m.reply || m.content || ""),
    })));
  }, [chatData]);

  const dismissFeed = (id) => setDismissed((s) => { const n = new Set(s); n.add(id); return n; });
  const send = () => {
    const q = draft.trim(); if (!q) return;
    setMsgs(m => [...m, { who: "usr", t: q }]);
    setDraft("");
    // REAL backend call — show whatever the backend returns (currently 501).
    api.post('/ai/chat', { text: q }).then(res => {
      let r;
      if (res.status === 501)
        r = <><b>NOT IMPLEMENTED YET.</b> The backend received this (POST /ai/chat → 501){res.data && res.data.todo ? <> — {res.data.todo}</> : null}.</>;
      else if (res.ok)
        r = <>{(res.data && (res.data.reply || res.data.text)) || "(backend replied with empty body)"}</>;
      else if (res.status === 0)
        r = <>Backend unreachable. Start it: <code>cargo run -p phosk_api</code> (binds 127.0.0.1:3000).</>;
      else
        r = <>Backend error {res.status}.</>;
      setMsgs(m => [...m, { who: "sys", t: r }]);
    });
  };

  const icon = (k) => k === "categorize" ? "✓" : k === "reprocess" ? "⟳" : k === "suggest" ? "⌁" : "∿";
  const shown = feed.filter((f, i) => !dismissed.has(f.id != null ? f.id : i));

  return (
    <aside className={"ai-panel" + (collapsed ? " collapsed" : "")}>
      <div className="ai-rail">
        <span className="exp" onClick={onToggle} title="Open assistant">▸</span>
        <span className="vlabel">{model} · ASSISTANT</span>
        <span className="pulse" style={{ width: 6, height: 6, borderRadius: 999, background: online ? "var(--ok)" : "var(--ink-3)", boxShadow: online ? "0 0 8px var(--ok)" : "none" }} />
      </div>

      <div className="ai-body">
        <div className="ai-head">
          <window.Dot tone={online ? "ok" : "blue"} size={7} />
          <div>
            <div className="who">Assistant</div>
            <div className="mdl">{engine} · {model} · {location}</div>
          </div>
          <span className="col" onClick={onToggle} title="Collapse">◂</span>
        </div>

        <div className="ai-feed">
          <div className="fh"><span className="pulse" /><span className="lbl">Live · auto-maintenance</span></div>
          {shown.length === 0 && <div className="dim" style={{ fontSize: 11, padding: "10px 2px", letterSpacing: ".04em" }}>No activity yet — awaiting backend (/ai/feed).</div>}
          {shown.map((f, i) => {
            const key = f.id != null ? f.id : i;
            return (
            <div key={key} className={"fitem " + f.kind}>
              <span className="ic">{icon(f.kind)}</span>
              <div className="ftx">
                <span>{f.text}</span>
                <div className="fmeta">
                  {f.conf != null && <span className="conf">CONF {Math.round(f.conf * 100)}%</span>}
                  {f.state === "running" && <span className="conf" style={{ color: "var(--indigo-neon)" }}>RUNNING…</span>}
                  <span className="tm">{f.time}</span>
                </div>
                {f.actions && (
                  <div className="facts">
                    {f.actions.map((a, k) => (
                      <button key={k} className={"gbtn" + (k === 0 ? (f.cand ? " p" : " coral") : "")}
                        onClick={() => {
                          if (f.cand && k === 0) {
                            api.post('/signals/candidates/' + (f.candidateId || f.sig || f.id) + '/track').then(reloadFeed);
                            if (onTrack && (f.candidateId || f.sig)) onTrack(f.candidateId || f.sig);
                          } else {
                            api.post('/ai/feed/' + (f.id != null ? f.id : i) + '/dismiss').then(reloadFeed);
                          }
                          dismissFeed(key);
                        }}>{a}</button>
                    ))}
                  </div>
                )}
              </div>
            </div>
          );})}
        </div>

        <div className="ai-chat" ref={chatRef}>
          {msgs.length === 0 && <div className="dim" style={{ fontSize: 11, padding: "8px 2px", letterSpacing: ".04em" }}>Ask the assistant anything — it replies from the local model.</div>}
          {msgs.map((m, i) => (
            <div key={i} className={"msg " + m.who}>
              <span className="nm">{m.who === "sys" ? model : "YOU"}</span>
              <div>{m.t}</div>
            </div>
          ))}
        </div>

        <div className="ai-input">
          <input value={draft} onChange={e => setDraft(e.target.value)} onKeyDown={e => e.key === "Enter" && send()}
            placeholder="Ask, or “track toothpaste”…" />
          <button className="send" onClick={send} title="Send">→</button>
        </div>
      </div>
    </aside>
  );
}

/* ===================== RIGHT — SIGNAL PANEL ===================== */
function SignalPanel({ sig, onClose, variant, cycleLabel = "THIS CYCLE", onChanged }) {
  if (!sig) {
    return (
      <aside className={"sig-panel" + (variant ? " " + variant : "")}>
        <div className="sig-empty">
          <span className="mk">⌁</span>
          <div className="tx">No signal selected.<br />Click a tracked item <b>⌁</b> in any receipt to inspect its trend across all time.</div>
        </div>
      </aside>
    );
  }
  const after = (r) => { if (onChanged) onChanged(); return r; };
  const up = (sig.deltaPct || 0) >= 0;
  const delta = sig.deltaPct == null ? "NEW" : (up ? "↑" : "↓") + Math.abs(sig.deltaPct) + "%";
  const recent = sig.recent || [];
  return (
    <aside className={"sig-panel" + (variant ? " " + variant : "")}>
      <div className="sig-head">
        <div className="kls">⌁ ITEM-SIGNAL · {sig.parent}</div>
        <div className="nm">{sig.label}</div>
        <div className="ds">{sig.desc}</div>
        {onClose && <span className="x" onClick={onClose} title="Close">✕</span>}
      </div>

      {!sig.candidate && (
        <>
          <div className="sig-delta">
            <span className={"big " + (up ? "up" : "down")}>{delta}</span>
            <span className="vs">vs last cycle · {sig.cycleQty} {sig.unit}</span>
          </div>
          <div className="sig-chart">
            <window.SigSpark data={sig.series} />
            <div className="axis"><span>12 MO AGO</span><span>{cycleLabel}</span></div>
          </div>
          <div className="sig-stats">
            <div className="st"><div className="k">This cycle</div><div className="v">{sig.cycleQty} <span style={{ fontSize: 11, color: "var(--ink-3)" }}>{sig.unit}</span></div></div>
            <div className="st"><div className="k">Spend</div><div className="v coral">CHF {chf(sig.cycleSpend)}</div></div>
            <div className="st"><div className="k">Avg / unit</div><div className="v">CHF {chf(sig.avgUnit)}</div></div>
            <div className="st"><div className="k">Receipts</div><div className="v">{sig.txns}</div></div>
            <div className="st"><div className="k">Tracked since</div><div className="v" style={{ fontSize: 15 }}>{sig.since}</div></div>
            <div className="st"><div className="k">Confidence</div><div className="v">{sig.conf != null ? Math.round(sig.conf * 100) + "%" : "—"}</div></div>
          </div>
          <div className="sig-recent">
            <div className="h">Recent occurrences</div>
            {recent.map((o, i) => (
              <div className="sig-occ" key={i}>
                <span className="dt">{o.date}</span>
                <span className="no">{o.note}</span>
                <span className="sh">{o.shop}</span>
                <span className="pr">CHF {chf(o.qty * o.price)}</span>
              </div>
            ))}
          </div>
        </>
      )}
      {sig.candidate && (
        <div className="sig-chart" style={{ paddingTop: 16 }}>
          <window.SigSpark data={sig.series} />
          <div className="axis"><span>12 MO AGO</span><span>{cycleLabel}</span></div>
          <div className="sig-foot" style={{ marginTop: 18 }}>
            <div className="tx">Candidate signal. The AI noticed <b>{sig.label}</b> {sig.desc}. Track it to follow it on its own axis from here on.</div>
            <div style={{ display: "flex", gap: 7, marginTop: 10 }}>
              <button className="gbtn p" onClick={() => api.post('/signals', { candidateId: sig.id, label: sig.label }).then(after)}>TRACK SIGNAL</button>
              <button className="gbtn" onClick={() => api.post('/signals/candidates/' + sig.id + '/dismiss').then(after)}>DISMISS</button>
            </div>
          </div>
        </div>
      )}

      {!sig.candidate && (
        <div className="sig-foot">
          <div className="tx">Tracked <b>distinctly</b> from {sig.parent}. Every matching line item rolls into this signal automatically — the AI maintains it.</div>
        </div>
      )}
    </aside>
  );
}

/* ===================== ITEM-SIGNAL STRIP (dashboard) ===================== */
function SignalCard({ sig, active, onSelect }) {
  const up = (sig.deltaPct || 0) >= 0;
  const delta = sig.candidate ? "NEW" : (up ? "↑" : "↓") + Math.abs(sig.deltaPct) + "%";
  return (
    <button className={"ss-card" + (active ? " on" : "") + (sig.candidate ? " cand" : "")} onClick={() => onSelect(sig.id)}>
      <div className="ss-top"><span className="g">⌁</span><span className="nm">{sig.label}</span></div>
      <div className="ss-mid">
        <span className={"dl " + (up ? "up" : "down")}>{delta}</span>
        <div className="ss-spk"><window.SigSpark data={sig.series} w={128} h={36} /></div>
      </div>
      <div className="ss-sub">{sig.candidate ? sig.desc : `${sig.cycleQty} ${sig.unit} · CHF ${chf(sig.cycleSpend)} this cycle`}</div>
    </button>
  );
}
/* `signals` is the fetched list (tracked + optional candidate) the page passes in. */
function SignalStrip({ signals = [], sel, onSelect }) {
  return (
    <section className="sig-strip" data-screen-label="SIGNALS">
      <div className="ss-head">
        <span className="hud">⌁ ITEM-SIGNALS</span>
        <span className="ss-rule" />
        <span className="ss-meta">AI-MAINTAINED · TRACKED DISTINCTLY FROM CATEGORIES · CLICK TO INSPECT</span>
      </div>
      <div className="ss-cards">
        {signals.length === 0 && <div className="dim" style={{ fontSize: 11, padding: "14px 2px", letterSpacing: ".04em" }}>No tracked item-signals yet — awaiting backend (/signals).</div>}
        {signals.map(s => <SignalCard key={s.id} sig={s} active={sel === s.id} onSelect={onSelect} />)}
      </div>
    </section>
  );
}

Object.assign(window, { AiPanel, SignalPanel, SigSpark, SignalCard, SignalStrip });

export { SigSpark, AiPanel, SignalPanel, SignalCard, SignalStrip };
