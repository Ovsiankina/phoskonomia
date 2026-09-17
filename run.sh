#!/usr/bin/env bash
# Launches the Phoskonomia app: the Dioxus fullstack crate in frontend/dioxus-app
# via `dx serve`. The UI and its `#[server]` fns are served by the same process,
# so there is no separate backend to start.
#
# Usage:
#   ./run.sh                      # web (default platform)
#   ./run.sh --platform desktop   # native desktop window
#   ./run.sh --port 8080          # any extra args are passed to `dx serve`
#
# The adapter stack is chosen by env (see frontend/dioxus-app/README.md), e.g.
#   PHOSK_DB=surreal ./run.sh
# With nothing set: seeded in-memory DB + fake OCR, no external services.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
APP_DIR="$ROOT/frontend/dioxus-app"
DX_VERSION="0.7.6" # must match the pinned `dioxus = "=0.7.6"` in the app's Cargo.toml

if ! command -v dx >/dev/null 2>&1; then
  echo "dx (Dioxus CLI) not found. Install it with:" >&2
  echo "  cargo install dioxus-cli --version $DX_VERSION --locked" >&2
  exit 1
fi

if ! dx --version 2>/dev/null | grep -q "$DX_VERSION"; then
  echo "warning: dx $DX_VERSION expected, found: $(dx --version 2>/dev/null || echo unknown)" >&2
  echo "         dx and the dioxus crate must match or the build aborts." >&2
fi

cd "$APP_DIR"
exec dx serve "$@"
