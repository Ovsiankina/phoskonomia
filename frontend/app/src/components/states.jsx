/* Phoskonomia — shared "no data yet" states. Every GET endpoint returns 501
   until the backend behind it is built, so each data surface renders one of
   these instead of mock data. On-brand Oscillocore: corner-bracket readout,
   indigo structure, VG5000 body. Dual-published: ES exports + window net. */
import React from 'react'

/* Reads the useGet result (`res` = { status, data:{todo} }) and explains why a
   surface is empty. status 0 = backend offline, 501 = endpoint not implemented. */
function awaitingMessage(res, loading) {
  if (loading) return 'Loading…'
  const status = res ? res.status : null
  const todo = res && res.data ? res.data.todo : null
  if (status === 0) return 'Backend offline'
  if (status === 501) return 'Not implemented yet' + (todo ? ' · ' + todo : '')
  if (status == null) return 'Awaiting backend'
  if (status >= 400) return 'Error ' + status
  return 'No data'
}

/* Full-width block placeholder for a page section / chart / list. */
function Awaiting({ label = 'DATA', res, loading, tone = 'blue', style }) {
  const status = res ? res.status : null
  return (
    <div className={'osc-bkt ' + tone} data-awaiting="1"
      style={{ padding: '20px 16px', textAlign: 'center', ...style }}>
      <span className="osc-leg">{loading ? 'LOADING' : 'AWAITING BACKEND'}</span>
      <div className="hud sm" style={{ justifyContent: 'center', color: 'var(--indigo-neon)' }}>⌁ {label}</div>
      <div className="dim" style={{ marginTop: 7, fontSize: 11, lineHeight: 1.5 }}>{awaitingMessage(res, loading)}</div>
      {status === 0 && <div className="dim" style={{ marginTop: 4, fontSize: 10 }}>start it: cargo run -p phosk_api</div>}
    </div>
  )
}

/* Compact inline placeholder for a side-panel / dock empty body. */
function AwaitingInline({ glyph = '⌁', label = 'No data', res, loading, variant }) {
  return (
    <aside className={'sig-panel' + (variant ? ' ' + variant : '')}>
      <div className="sig-empty">
        <span className="mk">{glyph}</span>
        <div className="tx">{label}<br /><span className="dim">{awaitingMessage(res, loading)}</span></div>
      </div>
    </aside>
  )
}

/* A single placeholder value for a KPI numeral when totals haven't loaded. */
function Dash() { return <span style={{ color: 'var(--ink-3)' }}>—</span> }

Object.assign(window, { Awaiting, AwaitingInline, awaitingMessage, PhoskDash: Dash })

export { Awaiting, AwaitingInline, awaitingMessage, Dash }
