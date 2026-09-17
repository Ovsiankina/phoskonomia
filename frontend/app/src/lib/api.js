/* Phoskonomia web — backend API client (real fetch to phosk_api).
   User-initiated calls surface their result as a toast; a 501 shows "NO IMPL YET"
   with the backend's todo tag. Background loads (useGet) are `quiet` — the page's
   empty state communicates the 501 instead of spamming toasts. No mock fallback.
   ES module: `import { api, useGet } from '../lib/api.js'`.
   Also published on window for any non-module call site. */

import React from 'react'

const { useState, useEffect, useCallback, useRef } = React

const BASE = import.meta.env.VITE_API_BASE || 'http://127.0.0.1:3819/api/v1'

export function toast(text, kind = 'warn') {
  let host = document.getElementById('phosk-toasts')
  if (!host) {
    host = document.createElement('div')
    host.id = 'phosk-toasts'
    host.style.cssText =
      'position:fixed;right:16px;bottom:16px;z-index:99999;display:flex;' +
      'flex-direction:column;gap:8px;max-width:400px;' +
      'font-family:ui-monospace,SFMono-Regular,Menlo,monospace;'
    document.body.appendChild(host)
  }
  const tone = kind === 'err' ? '#ff5d73' : kind === 'ok' ? '#56e39f' : '#8b7bff'
  const el = document.createElement('div')
  el.style.cssText =
    'background:rgba(8,5,20,.92);border:2px solid ' + tone + ';color:#e7e7f2;' +
    'padding:10px 12px;font-size:12px;line-height:1.4;letter-spacing:.02em;' +
    'box-shadow:0 8px 30px rgba(0,0,0,.5);white-space:pre-wrap;'
  el.textContent = text
  host.appendChild(el)
  setTimeout(() => {
    el.style.transition = 'opacity .4s'
    el.style.opacity = '0'
    setTimeout(() => el.remove(), 400)
  }, 4200)
}

function qs(q) {
  if (!q) return ''
  const p = Object.entries(q)
    .filter(([, v]) => v !== undefined && v !== null && v !== '')
    .map(([k, v]) => encodeURIComponent(k) + '=' + encodeURIComponent(v))
  return p.length ? '?' + p.join('&') : ''
}

export async function apiCall(method, path, body, opts = {}) {
  const quiet = !!opts.quiet
  const init = { method, headers: {} }
  if (body !== undefined && body !== null) {
    init.headers['Content-Type'] = 'application/json'
    init.body = JSON.stringify(body)
  }
  let res
  let data = null
  try {
    res = await fetch(BASE + path, init)
    try { data = await res.json() } catch { /* no/empty body */ }
  } catch (e) {
    if (!quiet) toast('NO BACKEND — ' + method + ' ' + path + '\nstart it: cargo run -p phosk_api', 'err')
    return { ok: false, status: 0, data: null, error: String(e) }
  }
  if (!quiet) {
    if (res.status === 501) toast('NO IMPL YET — ' + method + ' ' + path + '\n' + ((data && data.todo) || ''), 'warn')
    else if (res.ok) toast('OK — ' + method + ' ' + path, 'ok')
    else toast('ERR ' + res.status + ' — ' + method + ' ' + path, 'err')
  }
  return { ok: res.ok, status: res.status, data }
}

export const api = {
  base: BASE,
  // `opts.quiet` suppresses toasts (used by useGet for background loads).
  get: (p, q, opts) => apiCall('GET', p + qs(q), undefined, opts),
  post: (p, b, opts) => apiCall('POST', p, b, opts),
  patch: (p, b, opts) => apiCall('PATCH', p, b, opts),
  put: (p, b, opts) => apiCall('PUT', p, b, opts),
  del: (p, opts) => apiCall('DELETE', p, undefined, opts),
}

/* useGet — fetch a GET endpoint on mount and whenever (path, params, ...deps)
   change. Background load: quiet (no toast). Returns the parsed body plus the
   raw status so a page can branch on 501 / 0 (offline) and render an empty
   "awaiting backend" state. `reload()` re-runs the fetch (call it after a
   successful mutation to resync the screen). NO mock fallback — `data` is null
   until the backend serves something real. */
export function useGet(path, params, deps = []) {
  const [state, setState] = useState({ data: null, status: null, loading: true, error: null, res: null })
  const key = path + qs(params)
  const mounted = useRef(true)
  useEffect(() => () => { mounted.current = false }, [])
  const reload = useCallback(() => {
    setState((p) => ({ ...p, loading: true }))
    return apiCall('GET', key, undefined, { quiet: true }).then((res) => {
      if (mounted.current) {
        setState({ data: res.data, status: res.status, loading: false, error: res.error || null, res })
      }
      return res
    })
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [key, ...deps])
  useEffect(() => { reload() }, [reload])
  return { data: state.data, status: state.status, loading: state.loading, error: state.error, res: state.res, reload }
}

if (typeof window !== 'undefined') {
  window.phoskApi = api
  window.phoskToast = toast
}
