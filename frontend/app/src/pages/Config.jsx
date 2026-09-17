/* Phoskonomia — Config page. The single control surface that drives every
   per-page tweak. Writes a shared "phosk.cfg" localStorage store (via the same
   helpers tweaks-panel.jsx reads back on each page) and broadcasts "phoskcfg"
   so any open surface updates live. On-brand Oscillocore instrumentation. */
import React from 'react'
import { useGet, api } from '../lib/api.js'
import { ScannerBg } from '../components/prims.jsx'
import { TopBar } from '../components/comps.jsx'
import { PHOSK_CFG_KEY, __phoskReadCfg, __phoskWriteCfg } from '../lib/tweaks.jsx'

const { useState: useStateC, useMemo: useMemoC, useEffect: useEffectC } = React;

/* The union of every tweak across all surfaces — these keys & defaults mirror
   the EDITMODE blocks in transactions/budgets/subscriptions/debts/analytics. */
const CFG_DEFAULTS = {
  aiOpen: true,
  // Top bar
  topDateFmt: "{label} · DAY {day}/{days}",
  // Transactions
  drillMode: "inspector", showSparks: true,
  // Budgets
  envLayout: "cards", sort: "order", showProj: true,
  // Subscriptions
  subView: "cards", subSort: "due", subAmounts: "monthly", subGroup: false, subHlAuto: false, subInsp: "dock",
  // Debts
  debtView: "cards", debtSort: "balance", debtStrategy: "avalanche", debtProjection: true,
  debtGroup: false, debtHlAuto: false, debtInsp: "dock", iouShow: true,
  // Analytics
  trendWindow: "12", trendMode: "spend", sigSort: "momentum", showCand: true,
  showMomentum: true, momentumSort: "momentum", showRhythm: true, sigInsp: "dock",
};

/* Keep only keys we know about (the backend may return extras or a wider set). */
function pickKnown(src) {
  const out = {};
  if (!src || typeof src !== "object") return out;
  for (const k in CFG_DEFAULTS) if (k in src) out[k] = src[k];
  return out;
}

/* ---- shared-store hook: read overrides over defaults, persist + broadcast ----
   Layering on init: CFG_DEFAULTS < localStorage. Backend prefs (fetched async)
   are merged UNDER localStorage once they arrive (mergeBackend), so a device's
   live cross-page localStorage channel always wins.
   Mutations: write localStorage + dispatch "phoskcfg" (live sync) AND hit the
   backend (PATCH on set, DELETE on reset). The toast surfaces 501 / errors. */
function useCfg() {
  const read = __phoskReadCfg || (() => ({}));
  const write = __phoskWriteCfg || (() => {});
  const init = () => ({ ...CFG_DEFAULTS, ...pickKnown(read()) });
  const [cfg, setCfg] = useStateC(init);

  // Merge backend-stored prefs UNDER localStorage (defaults < backend < local).
  const mergeBackend = (prefs) => {
    const back = pickKnown(prefs);
    if (!Object.keys(back).length) return;
    setCfg((prev) => ({ ...CFG_DEFAULTS, ...back, ...pickKnown(read()) }));
  };

  const set = (keyOrEdits, val) => {
    const edits = typeof keyOrEdits === "object" && keyOrEdits !== null ? keyOrEdits : { [keyOrEdits]: val };
    setCfg((prev) => ({ ...prev, ...edits }));
    write(edits);
    window.dispatchEvent(new CustomEvent("phoskcfg", { detail: edits }));
    // Persist to the backend too — toast surfaces 501 / errors. Fire-and-forget;
    // localStorage already holds the authoritative live value.
    api.patch("/settings/preferences", edits);
  };
  const reset = () => {
    setCfg({ ...CFG_DEFAULTS });
    try { localStorage.removeItem(PHOSK_CFG_KEY || "phosk.cfg"); } catch (e) {}
    window.dispatchEvent(new CustomEvent("phoskcfg", { detail: { ...CFG_DEFAULTS } }));
    api.del("/settings/preferences");
  };
  return [cfg, set, reset, mergeBackend];
}

/* ============================ on-brand controls ============================ */
function CfgRow({ label, hint, children }) {
  return (
    <div className="cfg-row">
      <div className="rl">
        <div className="lab">{label}</div>
        {hint && <div className="hint">{hint}</div>}
      </div>
      <div className="rc">{children}</div>
    </div>
  );
}

function CfgSeg({ value, options, onChange }) {
  return (
    <div className="cfg-seg" role="radiogroup">
      {options.map((o) => (
        <button key={o.value} type="button" role="radio" aria-checked={o.value === value}
          className={o.value === value ? "on" : ""} onClick={() => onChange(o.value)}>
          {o.label}
        </button>
      ))}
    </div>
  );
}

function CfgSelect({ value, options, onChange }) {
  return (
    <div className="cfg-select">
      <select value={value} onChange={(e) => onChange(e.target.value)}>
        {options.map((o) => <option key={o.value} value={o.value}>{o.label}</option>)}
      </select>
    </div>
  );
}

function CfgText({ value, placeholder, onChange }) {
  return (
    <div className="cfg-text">
      <input type="text" value={value} placeholder={placeholder}
        spellCheck={false} onChange={(e) => onChange(e.target.value)} />
    </div>
  );
}

function CfgSwitch({ value, onChange }) {
  return (
    <button type="button" className="cfg-switch" data-on={value ? "1" : "0"}
      role="switch" aria-checked={!!value} onClick={() => onChange(!value)}>
      <span className="st off">OFF</span>
      <span className="st on">ON</span>
      <span className="knob" />
    </button>
  );
}

function CfgPanel({ glyph, title, sub, accent, span2, children }) {
  return (
    <section className={"cfg-panel" + (accent ? " acc-" + accent : "") + (span2 ? " span-2" : "")}>
      <div className="cfg-legend"><span className="gl">{glyph}</span><span className="nm">{title}</span></div>
      {sub && <div className="cfg-panel-sub">{sub}</div>}
      {children}
    </section>
  );
}

/* ================================== page ================================== */
function ConfigPage() {
  const [cfg, set, reset, mergeBackend] = useCfg();
  const [justReset, setJustReset] = useStateC(false);

  // Backend prefs: merged UNDER localStorage once loaded (501 → defaults stand).
  const prefsGet = useGet("/settings/preferences");
  useEffectC(() => {
    if (prefsGet.status === 200 && prefsGet.data) mergeBackend(prefsGet.data);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [prefsGet.status, prefsGet.data]);

  // Status-ribbon roll-up. Falls back to locally computed counts when missing.
  const summaryGet = useGet("/settings/summary");
  // Engine/model for the ASSISTANT tile (account first, AI status as backup).
  const accountGet = useGet("/account");
  const aiStatusGet = useGet("/ai/status");

  const localChanged = useMemoC(
    () => Object.keys(CFG_DEFAULTS).filter((k) => cfg[k] !== CFG_DEFAULTS[k]).length,
    [cfg]
  );
  const localTotal = Object.keys(CFG_DEFAULTS).length;

  const summary = (summaryGet.status === 200 && summaryGet.data) ? summaryGet.data : null;
  const total = (summary && summary.totalPreferences != null) ? summary.totalPreferences : localTotal;
  const changed = (summary && summary.changedCount != null) ? summary.changedCount : localChanged;

  const acct = (accountGet.status === 200 && accountGet.data) ? accountGet.data : null;
  const ai = (aiStatusGet.status === 200 && aiStatusGet.data) ? aiStatusGet.data : null;
  // No hardcoded fallback — leave null when no backend serves it, so the
  // ASSISTANT tile renders its "—" empty state instead of fabricated status.
  const engine = (summary && summary.engine) || (acct && acct.engine) || (ai && ai.engine) || null;
  const model = (summary && summary.model) || (acct && acct.model) || (ai && ai.model) || null;

  // Global inspector placement drives the three per-surface inspector keys.
  const inspKeys = ["subInsp", "debtInsp", "sigInsp"];
  const inspAll = inspKeys.every((k) => cfg[k] === cfg.subInsp) ? cfg.subInsp : "mixed";
  const setInspAll = (v) => set({ subInsp: v, debtInsp: v, sigInsp: v });

  const onReset = () => { reset(); setJustReset(true); setTimeout(() => setJustReset(false), 1400); };

  return (
    <div className="pk" style={{ height: "100vh", minHeight: 0 }}>
      {/* Config field — molten signal parked LOWER-RIGHT in negative space, a
          faint blob upper-left, thin wire scaffold; no shape under the panels. */}
      <ScannerBg className="pk-bg" seed={117} shapes={[
        { char: "8", cx: .94, cy: .82, scale: .34, style: "red", morph: "vein", live: true, fill: .46 },
        { char: "P", cx: .08, cy: .28, scale: .27, style: "faint", morph: "blob", live: false, fill: .42 },
        { char: "2", cx: .46, cy: .93, scale: .15, style: "wire", morph: "vein", live: false, fill: .3 },
        { char: "5", cx: .95, cy: .14, scale: .12, style: "wire", morph: "vein", live: false, fill: .26 }
      ]} />

      <div className="app-shell">
        <div className="app-main">
          <TopBar active="CONFIG" />
          <div className="app-scroll" data-screen-label="CONFIG">
            <div className="cfg-wrap">

              {/* head */}
              <div className="cfg-top">
                <div>
                  <div className="ttl">Config</div>
                  <div className="sum">
                    <b>{total}</b> preferences across <b>5</b> surfaces ·
                    <span className="coral"> {changed} changed</span> from defaults · stored on this device
                  </div>
                </div>
                <div className="cfg-top-actions">
                  <button className={"cfg-reset" + (justReset ? " saved" : "")} onClick={onReset}>
                    {justReset ? "✓ Reset" : "↺ Reset all"}
                  </button>
                </div>
              </div>

              {/* status ribbon */}
              <div className="cfg-ribbon">
                <div className="cfg-rib">
                  <div className="k">SURFACES TUNED</div>
                  <div className="v">5</div>
                  <div className="s">txns · budgets · subs · debts · analytics</div>
                </div>
                <div className="cfg-rib">
                  <div className="k">PREFERENCES</div>
                  <div className="v">{total}</div>
                  <div className="s">selectors, switches & orders</div>
                </div>
                <div className="cfg-rib">
                  <div className="k">CHANGED</div>
                  <div className={"v " + (changed ? "coral" : "blue")}>{changed}</div>
                  <div className="s">{changed ? "differs from defaults" : "all at defaults"}</div>
                </div>
                <div className="cfg-rib">
                  <div className="k">ASSISTANT</div>
                  <div className="v blue">{cfg.aiOpen ? "ON" : "OFF"}</div>
                  <div className="s">{engine || "—"} · {model || "—"}</div>
                </div>
              </div>

              {/* panel grid */}
              <div className="cfg-grid">

                {/* GENERAL — the one coral panel (app-wide defaults) */}
                <CfgPanel glyph="⊙" title="General" accent="coral" span2
                  sub="App-wide defaults applied on every surface. Inspector placement sets where signal & detail docks open across Subscriptions, Debts and Analytics at once.">
                  <CfgRow label="Assistant panel open by default"
                    hint="The local GEMMA4 dock on the left edge of every surface">
                    <CfgSwitch value={cfg.aiOpen} onChange={(v) => set("aiOpen", v)} />
                  </CfgRow>
                  <CfgRow label="Inspector placement"
                    hint={inspAll === "mixed" ? "Mixed across surfaces — pick one to unify" : "Where inspectors open across all surfaces"}>
                    <CfgSeg value={inspAll} onChange={setInspAll} options={[
                      { value: "dock", label: "Dock" }, { value: "drawer", label: "Drawer" }
                    ]} />
                  </CfgRow>
                  <CfgRow label="Top-bar date format"
                    hint={"Tokens: {label} {day} {days} {asOf} · overlong text scrolls as a ticker"}>
                    <CfgText value={cfg.topDateFmt} placeholder={"{label} · DAY {day}/{days}"}
                      onChange={(v) => set("topDateFmt", v)} />
                  </CfgRow>
                </CfgPanel>

                {/* TRANSACTIONS */}
                <CfgPanel glyph="⊟" title="Transactions"
                  sub="Every receipt, itemized — and how its detail + item-signal drill opens.">
                  <CfgRow label="Signal panel" hint="Where a tapped line item's trend appears">
                    <CfgSeg value={cfg.drillMode} onChange={(v) => set("drillMode", v)} options={[
                      { value: "inspector", label: "Dock" }, { value: "drawer", label: "Drawer" }, { value: "sheet", label: "Sheet" }
                    ]} />
                  </CfgRow>
                  <CfgRow label="Trend sparks on pills" hint="Inline sparkline on tracked-item tags">
                    <CfgSwitch value={cfg.showSparks} onChange={(v) => set("showSparks", v)} />
                  </CfgRow>
                </CfgPanel>

                {/* BUDGETS */}
                <CfgPanel glyph="▦" title="Budgets"
                  sub="Envelope caps against the monthly budget — layout, ordering and projection.">
                  <CfgRow label="Envelope layout">
                    <CfgSeg value={cfg.envLayout} onChange={(v) => set("envLayout", v)} options={[
                      { value: "cards", label: "Cards" }, { value: "rows", label: "Rows" }
                    ]} />
                  </CfgRow>
                  <CfgRow label="Sort order">
                    <CfgSeg value={cfg.sort} onChange={(v) => set("sort", v)} options={[
                      { value: "order", label: "Order" }, { value: "used", label: "Used" }, { value: "over", label: "Over" }
                    ]} />
                  </CfgRow>
                  <CfgRow label="Projection markers" hint="Projected end-of-cycle tick on each envelope">
                    <CfgSwitch value={cfg.showProj} onChange={(v) => set("showProj", v)} />
                  </CfgRow>
                </CfgPanel>

                {/* SUBSCRIPTIONS */}
                <CfgPanel glyph="⊠" title="Subscriptions"
                  sub="Standing recurring charges as a periodic impulse train.">
                  <CfgRow label="Layout">
                    <CfgSeg value={cfg.subView} onChange={(v) => set("subView", v)} options={[
                      { value: "cards", label: "Cards" }, { value: "rows", label: "Rows" }
                    ]} />
                  </CfgRow>
                  <CfgRow label="Sort">
                    <CfgSeg value={cfg.subSort} onChange={(v) => set("subSort", v)} options={[
                      { value: "due", label: "Due" }, { value: "amount", label: "Cost" }, { value: "name", label: "A–Z" }
                    ]} />
                  </CfgRow>
                  <CfgRow label="Amounts" hint="Show per-charge cost or annualized total">
                    <CfgSeg value={cfg.subAmounts} onChange={(v) => set("subAmounts", v)} options={[
                      { value: "monthly", label: "Per charge" }, { value: "annual", label: "Annual" }
                    ]} />
                  </CfgRow>
                  <CfgRow label="Group by cadence">
                    <CfgSwitch value={cfg.subGroup} onChange={(v) => set("subGroup", v)} />
                  </CfgRow>
                  <CfgRow label="Highlight auto-detected" hint="Flag charges the AI surfaced on its own">
                    <CfgSwitch value={cfg.subHlAuto} onChange={(v) => set("subHlAuto", v)} />
                  </CfgRow>
                </CfgPanel>

                {/* DEBTS */}
                <CfgPanel glyph="∿" title="Debts"
                  sub="Outstanding balances as a decaying waveform, with a payoff strategy overlay.">
                  <CfgRow label="Layout">
                    <CfgSeg value={cfg.debtView} onChange={(v) => set("debtView", v)} options={[
                      { value: "cards", label: "Cards" }, { value: "rows", label: "Rows" }
                    ]} />
                  </CfgRow>
                  <CfgRow label="Sort">
                    <CfgSelect value={cfg.debtSort} onChange={(v) => set("debtSort", v)} options={[
                      { value: "balance", label: "Largest balance" }, { value: "apr", label: "Highest rate" },
                      { value: "payoff", label: "Soonest payoff" }, { value: "name", label: "A–Z" }
                    ]} />
                  </CfgRow>
                  <CfgRow label="Payoff strategy" hint="Which debt the overlay marks to target next">
                    <CfgSeg value={cfg.debtStrategy} onChange={(v) => set("debtStrategy", v)} options={[
                      { value: "avalanche", label: "Avalanche" }, { value: "snowball", label: "Snowball" }, { value: "none", label: "Off" }
                    ]} />
                  </CfgRow>
                  <CfgRow label="Projected trajectory">
                    <CfgSwitch value={cfg.debtProjection} onChange={(v) => set("debtProjection", v)} />
                  </CfgRow>
                  <CfgRow label="Group by type">
                    <CfgSwitch value={cfg.debtGroup} onChange={(v) => set("debtGroup", v)} />
                  </CfgRow>
                  <CfgRow label="Highlight auto-detected">
                    <CfgSwitch value={cfg.debtHlAuto} onChange={(v) => set("debtHlAuto", v)} />
                  </CfgRow>
                  <CfgRow label="Show IOU ledger" hint="Personal money owed to / by people">
                    <CfgSwitch value={cfg.iouShow} onChange={(v) => set("iouShow", v)} />
                  </CfgRow>
                </CfgPanel>

                {/* ANALYTICS */}
                <CfgPanel glyph="⌁" title="Analytics" span2
                  sub="The retrospective read — spend trend, item-signals, category momentum and weekday rhythm.">
                  <CfgRow label="Spend-trend window">
                    <CfgSeg value={cfg.trendWindow} onChange={(v) => set("trendWindow", v)} options={[
                      { value: "12", label: "12 cyc" }, { value: "6", label: "6 cyc" }
                    ]} />
                  </CfgRow>
                  <CfgRow label="Trend series" hint="Plot spend, or the savings rate">
                    <CfgSeg value={cfg.trendMode} onChange={(v) => set("trendMode", v)} options={[
                      { value: "spend", label: "Spend" }, { value: "rate", label: "Savings" }
                    ]} />
                  </CfgRow>
                  <CfgRow label="Item-signal sort">
                    <CfgSelect value={cfg.sigSort} onChange={(v) => set("sigSort", v)} options={[
                      { value: "momentum", label: "Momentum" }, { value: "spend", label: "Spend" }, { value: "az", label: "A–Z" }
                    ]} />
                  </CfgRow>
                  <CfgRow label="Show candidate signal" hint="Surface the AI's not-yet-tracked candidate">
                    <CfgSwitch value={cfg.showCand} onChange={(v) => set("showCand", v)} />
                  </CfgRow>
                  <CfgRow label="Category momentum section">
                    <CfgSwitch value={cfg.showMomentum} onChange={(v) => set("showMomentum", v)} />
                  </CfgRow>
                  <CfgRow label="Momentum order">
                    <CfgSelect value={cfg.momentumSort} onChange={(v) => set("momentumSort", v)} options={[
                      { value: "momentum", label: "Most movement" }, { value: "spend", label: "Largest spend" }, { value: "az", label: "A–Z" }
                    ]} />
                  </CfgRow>
                  <CfgRow label="Weekday spending rhythm">
                    <CfgSwitch value={cfg.showRhythm} onChange={(v) => set("showRhythm", v)} />
                  </CfgRow>
                </CfgPanel>

              </div>

              <div className="cfg-foot">
                <span className="mk">⌁</span>
                <span>Preferences sync to every surface on next visit</span>
                <span className="rule" />
                <span>PHOSK.CFG · LOCAL</span>
              </div>

            </div>
          </div>
        </div>
      </div>
    </div>
  );
}

export default ConfigPage;
