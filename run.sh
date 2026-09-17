#!/usr/bin/env bash
# Launches the real client/server pair: backend/bin/phosk_api (axum, :3819)
# then frontend/app (Vite/React, :3717). Order matters — frontend/app/src/lib/api.js
# fetches the backend on mount with no mock fallback, so the backend must already
# be answering /api/v1/health before the frontend starts or the dashboard's first
# load shows "NO BACKEND" toasts.
set -euo pipefail
set -m # each background job gets its own process group, so `kill -TERM -$pid` below reaps cargo's child binary too, not just the cargo wrapper

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
HEALTH_URL="http://127.0.0.1:3819/api/v1/health"

cleanup() {
  echo
  echo "stopping..."
  [[ -n "${FRONTEND_PID:-}" ]] && kill -TERM "-${FRONTEND_PID}" 2>/dev/null || true
  [[ -n "${BACKEND_PID:-}" ]] && kill -TERM "-${BACKEND_PID}" 2>/dev/null || true
}
trap cleanup EXIT INT TERM

echo "starting backend (phosk_api on :3819)..."
(cd "$ROOT/backend" && cargo run -p phosk_api) &
BACKEND_PID=$!

echo -n "waiting for backend health check"
for _ in $(seq 1 60); do
  if curl -fsS "$HEALTH_URL" >/dev/null 2>&1; then
    echo " — up"
    break
  fi
  echo -n "."
  sleep 1
done
if ! curl -fsS "$HEALTH_URL" >/dev/null 2>&1; then
  echo
  echo "backend never came up on $HEALTH_URL — aborting" >&2
  exit 1
fi

echo "starting frontend (Vite dev server on :3717)..."
(cd "$ROOT/frontend/app" && npm run dev) &
FRONTEND_PID=$!

wait
