/* Phoskonomia — client-side presentation helpers.

   This module used to hold the mock dataset. The frontend is now wired to the
   real backend (phosk_api): every page fetches its data over HTTP and renders
   "awaiting backend" until the 501 stubs go green. NO mock data lives here any
   more — only `chf`, the pure Swiss-currency formatter the UI applies to amounts
   the backend returns (CHF 1'234.50: apostrophe thousands, dot decimal).

   Kept exported as `PHOSK` (= { chf }) + on `window.PHOSK` so existing call
   sites (`PHOSK.chf` / `window.PHOSK.chf` / `D.chf`) keep resolving. */

// Swiss currency format: apostrophe thousands (’), dot decimal, − for negatives.
function chf(n, dp = 2) {
  const v = Number(n)
  if (!isFinite(v)) return '—'
  const neg = v < 0
  const s = Math.abs(v).toFixed(dp)
  const [int, dec] = s.split('.')
  const grouped = int.replace(/\B(?=(\d{3})+(?!\d))/g, '’')
  return (neg ? '−' : '') + grouped + (dec ? '.' + dec : '')
}

const PHOSK = { chf }

if (typeof window !== 'undefined') window.PHOSK = PHOSK

export { PHOSK, chf }
export default PHOSK
