# dioxus-app — the Phoskonomia UI

Dioxus 0.7.6 fullstack app: one Rust codebase for web (default), desktop and
mobile. The UI talks to the backend feature crates through `#[server]` functions
with shared Rust types (no REST, no JavaScript). `Money` crosses the wire as
exact `i64` centimes.

```
dioxus-app/
├─ assets/       # stylesheets, fonts, images (loaded via asset!())
├─ src/
│  ├─ main.rs    # entry point, router, app shell
│  └─ data/      # #[server] fns, wire view types, and the composition root (mod.rs)
├─ Cargo.toml    # features: web (default) | desktop | mobile | server
└─ Dioxus.toml   # dx config (default_platform = "web")
```

## Prerequisites

- A Rust toolchain with the `wasm32-unknown-unknown` target
  (`rustup target add wasm32-unknown-unknown`).
- The `dx` CLI at **exactly 0.7.6** — it must match the pinned `dioxus = "=0.7.6"`
  or `dx` aborts with a version-mismatch error
  (`cargo install dioxus-cli --version 0.7.6 --locked`).

## Run

From this directory (or `./run.sh` from the repository root):

```bash
dx serve                      # web: builds the WASM client + server, serves on localhost
dx serve --platform desktop   # native desktop window, server fns run in-process
```

`dx serve` prints the URL it listens on; pass `--port <n>` to choose one.

## Configuration

The adapter stack is chosen by environment variables read at the composition
root (`src/data/mod.rs`). With no variables set you get a seeded in-memory
database and a fake OCR — no external services needed.

| Variable          | Values                                | Default                  |
|-------------------|---------------------------------------|--------------------------|
| `PHOSK_DB`        | `memory` \| `surreal` (file-backed)   | `memory` (seeded)        |
| `PHOSK_OCR`       | `auto` \| `paddle` \| `vision`        | `auto` (fake if none reachable) |
| `PHOSK_LLM_MODEL` | any Ollama model tag                  | see `src/data/mod.rs`    |
| `PHOSK_DATA_DIR`  | path for file-backed adapters         | `./phosk-data`           |

`PHOSK_DATA_DIR` holds encrypted photos and keys at runtime — never commit it.

## Check

Both sides must type-check (run from this directory):

```bash
cargo check --no-default-features --features server   # server side
cargo check --target wasm32-unknown-unknown            # WASM client side
```

## Test

The data-layer tests (`src/data/tests/`) run natively with the server feature:

```bash
cargo test --no-default-features --features server
```

With that feature a `#[server]` fn runs its body in-process, so the tests call
the real fns. In test builds the composition root always uses the seeded
in-memory database and the in-process OCR / LLM / storage fakes, so no `PHOSK_*`
variable, local service or data directory is involved. All tests share that
store and never write through it: each write path has a
`pub(crate) <name>_with(db, …)` inner fn that tests drive on a fresh
`MemoryDb::seeded()`.

## Backend gates

The backend gates (from the repository root) exclude this crate:

```bash
cargo fmt --all --check
cargo clippy --workspace --exclude dioxus-app --all-targets -- -D warnings
cargo test --workspace --exclude dioxus-app
```

## Design

The visual language is Oscillocore; `frontend/.claude-design-export/` is the
read-only visual reference. Use design tokens (`var(--…)`), never raw hex or
`px` values.
