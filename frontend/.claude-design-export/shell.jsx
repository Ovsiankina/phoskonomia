/* Phoskonomia — shared shell components: left AI panel + right Signal panel. */
const { useState: useStateS, useRef: useRefS, useEffect: useEffectS } = React;

/* ---- larger sparkline with up/down tone + area, for the signal inspector ---- */
function SigSpark({ data, w = 312, h = 88 }) {
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
function AiPanel({ collapsed, onToggle, onTrack }) {
  const D = window.PHOSK;
  const [feed, setFeed] = useStateS(D.aiFeed);
  const [msgs, setMsgs] = useStateS([
    { who: "sys", t: <>Reading June. <b>3</b> item-signals under watch — <span className="sig">coffee</span>, <span className="sig">pain au chocolat</span>, <span className="sig">beer</span>. I file every line item to its signal automatically; you only step in on low confidence.</> },
    { who: "sys", t: <>Coffee is up <b>28%</b> this cycle. Want me to set a soft cap?</> },
  ]);
  const [draft, setDraft] = useStateS("");
  const chatRef = useRefS(null);
  useEffectS(() => { if (chatRef.current) chatRef.current.scrollTop = chatRef.current.scrollHeight; }, [msgs]);

  const dismissFeed = (i) => setFeed(f => f.filter((_, k) => k !== i));
  const send = () => {
    const q = draft.trim(); if (!q) return;
    setMsgs(m => [...m, { who: "usr", t: q }]);
    window.phoskApi.post('/ai/chat', { text: q });
    setDraft("");
    setTimeout(() => {
      const ql = q.toLowerCase();
      let r;
      if (ql.includes("coffee")) r = <>Coffee: <b>23</b> servings, CHF <b>86.40</b> this cycle — <span className="sig">+28%</span>. Mostly to-go (Avec, Coop Pronto). I can split café vs beans if useful.</>;
      else if (ql.includes("pain") || ql.includes("chocolat")) r = <>Pain au chocolat: <b>14</b> pieces, <span className="sig">+20%</span> vs last cycle. Six receipts. Trending with weekday mornings.</>;
      else if (ql.includes("beer") || ql.includes("bier")) r = <>Beer is <b>down 14%</b> — 19 vs 22 units. Spend CHF 58.10. Want it excluded from the dining cap?</>;
      else if (ql.includes("cap") || ql.includes("budget")) r = <>I can hold coffee under CHF 80/cycle and nudge you at 90%. Confirm and I'll maintain it.</>;
      else r = <>Tracked. Ask me to isolate any item — "track toothpaste", "show beer trend" — and I'll maintain it as its own signal from here on.</>;
      setMsgs(m => [...m, { who: "sys", t: r }]);
    }, 650);
  };

  const icon = (k) => k === "categorize" ? "✓" : k === "reprocess" ? "⟳" : k === "suggest" ? "⌁" : "∿";

  return (
    <aside className={"ai-panel" + (collapsed ? " collapsed" : "")}>
      <div className="ai-rail">
        <span className="exp" onClick={onToggle} title="Open assistant">▸</span>
        <span className="vlabel">GEMMA4 · ASSISTANT</span>
        <span className="pulse" style={{ width: 6, height: 6, borderRadius: 999, background: "var(--ok)", boxShadow: "0 0 8px var(--ok)" }} />
      </div>

      <div className="ai-body">
        <div className="ai-head">
          <window.Dot tone="ok" size={7} />
          <div>
            <div className="who">Assistant</div>
            <div className="mdl">OLLAMA · GEMMA4 · LOCAL</div>
          </div>
          <span className="col" onClick={onToggle} title="Collapse">◂</span>
        </div>

        <div className="ai-feed">
          <div className="fh"><span className="pulse" /><span className="lbl">Live · auto-maintenance</span></div>
          {feed.map((f, i) => (
            <div key={i} className={"fitem " + f.kind}>
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
                            window.phoskApi.post('/signals/candidates/' + (f.candidateId || f.sig || f.id) + '/track');
                            if (onTrack) onTrack("gruyere");
                          } else {
                            window.phoskApi.post('/ai/feed/' + f.id + '/dismiss');
                          }
                          dismissFeed(i);
                        }}>{a}</button>
                    ))}
                  </div>
                )}
              </div>
            </div>
          ))}
        </div>

        <div className="ai-chat" ref={chatRef}>
          {msgs.map((m, i) => (
            <div key={i} className={"msg " + m.who}>
              <span className="nm">{m.who === "sys" ? "GEMMA4" : "YOU"}</span>
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
function SignalPanel({ sig, onClose, variant }) {
  const D = window.PHOSK;
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
  const up = (sig.deltaPct || 0) >= 0;
  const delta = sig.deltaPct == null ? "NEW" : (up ? "↑" : "↓") + Math.abs(sig.deltaPct) + "%";
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
            <div className="axis"><span>12 MO AGO</span><span>{D.cycle.label}</span></div>
          </div>
          <div className="sig-stats">
            <div className="st"><div className="k">This cycle</div><div className="v">{sig.cycleQty} <span style={{ fontSize: 11, color: "var(--ink-3)" }}>{sig.unit}</span></div></div>
            <div className="st"><div className="k">Spend</div><div className="v coral">CHF {D.chf(sig.cycleSpend)}</div></div>
            <div className="st"><div className="k">Avg / unit</div><div className="v">CHF {D.chf(sig.avgUnit)}</div></div>
            <div className="st"><div className="k">Receipts</div><div className="v">{sig.txns}</div></div>
            <div className="st"><div className="k">Tracked since</div><div className="v" style={{ fontSize: 15 }}>{sig.since}</div></div>
            <div className="st"><div className="k">Confidence</div><div className="v">{Math.round(sig.conf * 100)}%</div></div>
          </div>
          <div className="sig-recent">
            <div className="h">Recent occurrences</div>
            {sig.recent.map((o, i) => (
              <div className="sig-occ" key={i}>
                <span className="dt">{o.date}</span>
                <span className="no">{o.note}</span>
                <span className="sh">{o.shop}</span>
                <span className="pr">CHF {D.chf(o.qty * o.price)}</span>
              </div>
            ))}
          </div>
        </>
      )}
      {sig.candidate && (
        <div className="sig-chart" style={{ paddingTop: 16 }}>
          <window.SigSpark data={sig.series} />
          <div className="axis"><span>12 MO AGO</span><span>{D.cycle.label}</span></div>
          <div className="sig-foot" style={{ marginTop: 18 }}>
            <div className="tx">Candidate signal. The AI noticed <b>{sig.label}</b> {sig.desc}. Track it to follow it on its own axis from here on.</div>
            <div style={{ display: "flex", gap: 7, marginTop: 10 }}>
              <button className="gbtn p" onClick={() => window.phoskApi.post('/signals', { candidateId: sig.id, label: sig.label })}>TRACK SIGNAL</button>
              <button className="gbtn" onClick={() => window.phoskApi.post('/signals/candidates/' + sig.id + '/dismiss')}>DISMISS</button>
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

Object.assign(window, { AiPanel, SignalPanel, SigSpark });

/* ===================== ITEM-SIGNAL STRIP (dashboard) ===================== */
function SignalCard({ sig, active, onSelect }) {
  const D = window.PHOSK;
  const up = (sig.deltaPct || 0) >= 0;
  const delta = sig.candidate ? "NEW" : (up ? "↑" : "↓") + Math.abs(sig.deltaPct) + "%";
  return (
    <button className={"ss-card" + (active ? " on" : "") + (sig.candidate ? " cand" : "")} onClick={() => onSelect(sig.id)}>
      <div className="ss-top"><span className="g">⌁</span><span className="nm">{sig.label}</span></div>
      <div className="ss-mid">
        <span className={"dl " + (up ? "up" : "down")}>{delta}</span>
        <div className="ss-spk"><window.SigSpark data={sig.series} w={128} h={36} /></div>
      </div>
      <div className="ss-sub">{sig.candidate ? sig.desc : `${sig.cycleQty} ${sig.unit} · CHF ${D.chf(sig.cycleSpend)} this cycle`}</div>
    </button>
  );
}
function SignalStrip({ sel, onSelect }) {
  const D = window.PHOSK;
  const sigs = [...D.trackedSignals(), D.signalCandidate];
  return (
    <section className="sig-strip" data-screen-label="SIGNALS">
      <div className="ss-head">
        <span className="hud">⌁ ITEM-SIGNALS</span>
        <span className="ss-rule" />
        <span className="ss-meta">AI-MAINTAINED · TRACKED DISTINCTLY FROM CATEGORIES · CLICK TO INSPECT</span>
      </div>
      <div className="ss-cards">
        {sigs.map(s => <SignalCard key={s.id} sig={s} active={sel === s.id} onSelect={onSelect} />)}
      </div>
    </section>
  );
}

Object.assign(window, { SignalCard, SignalStrip });
