/* Phoskonomia — composed dashboard blocks shared across variations.
   Router-ified: nav/brand/hovercard/mega links use react-router <Link>. */
import React from 'react'
import { createPortal } from 'react-dom'
import { Link, useNavigate } from 'react-router-dom'
import { chf } from '../data/phosk.js'
import { api, useGet } from '../lib/api.js'
import { pctTone, CatBar, Dot, HudCell } from './prims.jsx'

/* ---- Top bar (Phoskonomia chrome) ---- */
const PHOSK_PAGES = [
  { key: "DASHBOARD",     abbr: "DASH", glyph: "◳", href: "Phoskonomia%20Dashboard.html",     to: "/dashboard",     desc: "Spend trace, savings & alerts" },
  { key: "TRANSACTIONS",  abbr: "TXN",  glyph: "⊟", href: "Phoskonomia%20Transactions.html",  to: "/transactions",  desc: "Every receipt, itemized" },
  { key: "BUDGETS",       abbr: "BUDG", glyph: "▦", href: "Phoskonomia%20Budgets.html",       to: "/budgets",       desc: "Envelopes & monthly caps" },
  { key: "SUBSCRIPTIONS", abbr: "SUBS", glyph: "⊠", href: "Phoskonomia%20Subscriptions.html", to: "/subscriptions", desc: "Standing recurring charges" },
  { key: "DEBTS",         abbr: "DEBT", glyph: "∿", href: "Phoskonomia%20Debts.html",         to: "/debts",         desc: "Balances, payoff & IOUs" },
  { key: "ANALYTICS",     abbr: "ANLY", glyph: "⌁", href: "Phoskonomia%20Analytics.html",     to: "/analytics",     desc: "Trends & item-signals" },
  { key: "CONFIG",        abbr: "CFG",  glyph: "⊙", href: "Phoskonomia%20Config.html",        to: "/config",        desc: "Preferences for every surface" },
];

const TOPDATE_DEFAULT = "{label} · DAY {day}/{days}";

function TopBar({ active = "DASHBOARD" }) {
  // The current cycle window (label / day / days / asOf) drives the date readout.
  // Fetched from the backend; blank until /cycle/current goes green.
  const { data: cycle } = useGet('/cycle/current');
  const C = cycle || {};
  const [menu, setMenu] = React.useState(false);       // full grid (hamburger)
  const [navMode, setNavMode] = React.useState("full"); // "full" | "abbr" | "compact"
  const [hover, setHover] = React.useState(null);       // inline hover-card { key, left }
  const btnRef = React.useRef(null);
  const [menuLeft, setMenuLeft] = React.useState(null);
  const navwrapRef = React.useRef(null);
  const probeFullRef = React.useRef(null);
  const probeAbbrRef = React.useRef(null);
  const hideTimer = React.useRef(null);
  const compact = navMode === "compact";

  /* ---- configurable date format (shared phosk.cfg store) ---- */
  const readFmt = () => {
    const cfg = window.__phoskReadCfg ? window.__phoskReadCfg() : {};
    return cfg.topDateFmt != null ? cfg.topDateFmt : TOPDATE_DEFAULT;
  };
  const [dateFmt, setDateFmt] = React.useState(readFmt);
  React.useEffect(() => {
    const onCfg = (e) => { const d = (e && e.detail) || {}; if ("topDateFmt" in d) setDateFmt(d.topDateFmt || ""); };
    window.addEventListener("phoskcfg", onCfg);
    return () => window.removeEventListener("phoskcfg", onCfg);
  }, []);
  const dateText = String(dateFmt || "")
    .replace(/\{label\}/g, C.label != null ? C.label : "")
    .replace(/\{day\}/g, C.day != null ? C.day : "")
    .replace(/\{days\}/g, C.days != null ? C.days : "")
    .replace(/\{asOf\}/g, C.asOf || "")
    .trim()
    || " ";

  /* ---- date readout: ticker ONLY when its slot is too small to show the text.
     The slot shrinks responsively (flex) as the bar gets cramped; we compare the
     text's natural width to the slot's real width, so a long format on a wide
     screen stays static and only scrolls once space actually runs out. ---- */
  const monthTxRef = React.useRef(null);
  const monthVpRef = React.useRef(null);
  const [tick, setTick] = React.useState(false);
  const [tickDur, setTickDur] = React.useState(8);

  /* ---- one layout pass picks the nav tier AND the date ticker from real widths.
     navwrap uses flex-basis:0 so its measured width is the free middle space,
     independent of which tier is currently rendered (no flip-flop). Full labels
     show when they fit; abbreviations only when they don't; hamburger last. ---- */
  React.useLayoutEffect(() => {
    const measure = () => {
      const wrap = navwrapRef.current, pf = probeFullRef.current, pa = probeAbbrRef.current;
      if (wrap && pf && pa) {
        const avail = wrap.clientWidth;
        setNavMode(avail >= pf.offsetWidth + 4 ? "full"
                 : avail >= pa.offsetWidth + 4 ? "abbr" : "compact");
      }
      const tx = monthTxRef.current, vp = monthVpRef.current;
      if (tx && vp) {
        const w = tx.scrollWidth;
        const over = w > vp.clientWidth + 1;
        setTick(over);
        if (over) setTickDur(Math.max(6, Math.round((w + 40) / 32)));
      }
    };
    measure();
    let ro;
    if (typeof ResizeObserver !== "undefined") {
      ro = new ResizeObserver(measure);
      if (navwrapRef.current) ro.observe(navwrapRef.current);
      if (monthVpRef.current) ro.observe(monthVpRef.current);
      ro.observe(document.documentElement);
    }
    window.addEventListener("resize", measure);
    return () => { if (ro) ro.disconnect(); window.removeEventListener("resize", measure); };
  }, [dateText]);

  React.useEffect(() => {
    if (!menu) return;
    const onKey = (e) => { if (e.key === "Escape") setMenu(false); };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [menu]);

  const handleMenuToggle = () => {
    if (!menu && btnRef.current) setMenuLeft(btnRef.current.getBoundingClientRect().left);
    setMenu((m) => !m);
  };

  const onNavEnter = (e, p) => {
    clearTimeout(hideTimer.current);
    setHover({ key: p.key, left: e.currentTarget.getBoundingClientRect().left });
  };
  const onNavLeave = () => { hideTimer.current = setTimeout(() => setHover(null), 90); };
  const hoverPage = hover && PHOSK_PAGES.find((p) => p.key === hover.key);

  return (
    <header className="phosk-top">
      <Link className="phosk-brand" to="/dashboard" aria-label="Phoskonomia — home">
        <span className="mark">P<b>O</b></span>
      </Link>

      <div className={"phosk-navwrap" + (compact ? " compact" : "")} ref={navwrapRef}>
        {/* hidden probes — natural widths of full + abbreviated nav drive the tier */}
        <nav className="phosk-nav-probe" ref={probeFullRef} aria-hidden="true">
          {PHOSK_PAGES.map((p) => <span key={p.key} className="t">{p.key}</span>)}
        </nav>
        <nav className="phosk-nav-probe" ref={probeAbbrRef} aria-hidden="true">
          {PHOSK_PAGES.map((p) => <span key={p.key} className="t">{p.abbr}</span>)}
        </nav>

        {!compact &&
        <nav className="phosk-nav" onMouseLeave={onNavLeave}>
          {PHOSK_PAGES.map((p) =>
          <Link key={p.key} className={"t" + (p.key === active ? " on" : "")} to={p.to}
            onMouseEnter={(e) => onNavEnter(e, p)} onFocus={(e) => onNavEnter(e, p)}>{navMode === "abbr" ? p.abbr : p.key}</Link>
          )}
        </nav>}

        {compact &&
        <button ref={btnRef} className={"phosk-menu-btn" + (menu ? " on" : "")} onClick={handleMenuToggle} aria-label="Pages" aria-expanded={menu}>
          <span className="mi">{menu ? "✕" : "▤"}</span><span className="ml">MENU</span>
        </button>}
      </div>

      <div className="phosk-month" data-tick={tick ? "1" : "0"}>
        <span className="month-vp" ref={monthVpRef}>
          <span className="month-track" style={tick ? { animationDuration: tickDur + "s" } : undefined}>
            <span className="month-tx" ref={monthTxRef}>{dateText}</span>
            {tick && <span className="month-tx" aria-hidden="true">{dateText}</span>}
          </span>
        </span>
      </div>
      <HudCell glyph="P" size={38} />

      {/* inline hover-card (expanded nav only) — full name + what the page is for */}
      {!compact && hoverPage && createPortal(
        <div className="phosk-hovercard osc-glass" style={{ left: hover.left }}
          onMouseEnter={() => clearTimeout(hideTimer.current)} onMouseLeave={onNavLeave}>
          <div className="hc-gl"><b>{hoverPage.glyph}</b></div>
          <div className="hc-tx">
            <span className="nm">{hoverPage.key}</span>
            <span className="ds">{hoverPage.desc}</span>
          </div>
          {hoverPage.key === active && <span className="hc-cur">● ACTIVE</span>}
        </div>,
        document.body
      )}

      {/* full grid menu (hamburger, compact width only) */}
      {menu && createPortal(
        <React.Fragment>
          <div className="phosk-mega-back" onClick={() => setMenu(false)} />
          <div className="phosk-mega osc-glass" role="menu" style={menuLeft !== null ? { left: menuLeft } : undefined}>
            <div className="mega-h">Jump to</div>
            <div className="mega-grid">
              {PHOSK_PAGES.map((p) => {
                const on = p.key === active;
                const inner = (
                  <React.Fragment>
                    <div className="mega-gl"><b>{p.glyph}</b></div>
                    <div className="mega-tx"><span className="nm">{p.key}</span><span className="ds">{p.desc}</span></div>
                    {on ? <span className="mega-cur">● ACTIVE</span> : p.to ? null : <span className="soon">SOON</span>}
                  </React.Fragment>
                );
                return p.to
                  ? <Link key={p.key} className={"mega-card" + (on ? " on" : "")} to={p.to} role="menuitem" onClick={() => setMenu(false)}>{inner}</Link>
                  : <div key={p.key} className="mega-card disabled" role="menuitem" aria-disabled="true">{inner}</div>;
              })}
            </div>
          </div>
        </React.Fragment>,
        document.body
      )}
    </header>);

}

/* ---- KPI readout (framed, corner brackets + Pilowlava numeral) ---- */
function Kpi({ label, value, cur = "CHF", sub, accent, blue }) {
  return (
    <div className={"kpi" + (accent ? " accent" : "") + (blue ? " blue" : "")}>
      <div className="lbl">{label}</div>
      <div className="big">{cur && <span className="cur">{cur}</span>}{value}</div>
      {sub && <div className="sub">{sub}</div>}
    </div>);

}

/* ---- Category budget rows ---- */
function CatRows({ cats = [], showItems = true }) {
  return (
    <>
      {cats.map((c) => {
        const p = c.budget > 0 ? c.spent / c.budget : 0;
        const tone = c.fixed ? "ok" : pctTone(p);
        return (
          <div key={c.name} className="catrow">
            <span className={"cn" + (c.fixed ? " fixed" : "")}>{c.name}</span>
            <div className="barwrap">
              <CatBar spent={c.spent} budget={c.budget} tone={tone} />
              <div className="barmeta">
                <span>{showItems ? c.items + " items" : c.fixed ? "FIXED" : "VARIABLE"}</span>
                <span>{c.budget === 0 ? "NO BUDGET" : "CAP CHF " + chf(c.budget, 0)}</span>
              </div>
            </div>
            <span className="amt">CHF <b>{chf(c.spent)}</b></span>
            <span className={"pct " + tone}>{c.budget > 0 ? Math.round(p * 100) + "%" : "—"}</span>
          </div>);

      })}
    </>);

}

/* ---- Transaction tape ---- */
function TxnTape({ rows = [] }) {
  return (
    <>
      {rows.map((t, i) =>
      <div key={t.id || i} className="txn">
          <span className="dt">{t.date}</span>
          <span className="sh">{t.flag && <span className="flag" title="needs review">⚠</span>}{t.shop}</span>
          <span className="tag"><Dot tone={t.fixed ? "blue" : "ok"} size={5} />{t.category || t.cat}</span>
          <span className="am"><span className="c">CHF</span>{chf(t.amount)}</span>
        </div>
      )}
    </>);

}

/* ---- Recurring list item ---- */
function RecRow({ r }) {
  const tone = r.status === "due" ? "alert" : r.status === "soon" ? "warn" : "ok";
  return (
    <div className="rec">
      <Dot tone={tone} size={8} />
      <div className="body">
        <span className="nm">{r.name}</span>
        <span className="cy">{r.cycle}</span>
      </div>
      <div className="right">
        <span className="am">CHF {chf(r.amount)}</span>
        <span className="nx">{r.status === "due" ? "⚠ DUE" : "NEXT " + r.next}</span>
      </div>
      <span className={"src" + (r.src === "llm" ? " llm" : "")}>{r.src === "llm" ? "AUTO" : "USER"}</span>
    </div>);

}

/* ---- Alert item — actions wired to the alerts/recurring endpoints.
   Label → endpoint: DISMISS·SNOOZE·(APPLY|RAISE CAP|LOWER CAP) hit /alerts/{id}/*;
   MARK PAID hits /recurring/{name}/mark-paid (the missing-charge case); VIEW
   resolves the alert's deep-link target and navigates. `onChanged` re-fetches
   the alert list after a successful mutation. ---- */
function AlertItem({ a, onChanged }) {
  const navigate = useNavigate();
  const ic = a.tone === "llm" ? "⌁" : a.tone === "warn" ? "◷" : "⚠";
  const id = a.id;
  const after = (r) => { if (onChanged) onChanged(); return r; };
  const runAction = (label) => {
    const L = String(label).toUpperCase();
    if (L === "VIEW") {
      return api.get(`/alerts/${id}/target`).then((r) => {
        const t = r.data || {};
        if (t.category) navigate("/transactions?category=" + encodeURIComponent(t.category));
        return r;
      });
    }
    if (L === "DISMISS") return api.post(`/alerts/${id}/dismiss`).then(after);
    if (L === "SNOOZE") return api.post(`/alerts/${id}/snooze`, {}).then(after);
    if (L === "MARK PAID") {
      return (a.relatedRecurring
        ? api.post(`/recurring/${encodeURIComponent(a.relatedRecurring)}/mark-paid`)
        : api.post(`/alerts/${id}/apply`, {})).then(after);
    }
    // APPLY / RAISE CAP / LOWER CAP / anything else → execute the carried suggestion
    return api.post(`/alerts/${id}/apply`, {}).then(after);
  };
  return (
    <div className={"alert-i " + a.tone}>
      <span className="ic">{ic}</span>
      <div className="main">
        <div className="ah"><span className="tg">{a.tag}</span><span className="hd">{a.head}</span></div>
        <div className="bd">{a.body}</div>
        <div className="acts">
          {(a.actions || []).map((act, i) =>
            <button key={i} className={"btn" + (i === 0 ? " p" : "")} onClick={() => runAction(act)}>{act}</button>)}
        </div>
      </div>
    </div>);

}

Object.assign(window, { TopBar, Kpi, CatRows, TxnTape, RecRow, AlertItem, PHOSK_PAGES });

export { PHOSK_PAGES, TopBar, Kpi, CatRows, TxnTape, RecRow, AlertItem };
