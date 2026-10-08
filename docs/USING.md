# Using Phoskonomia day to day

One command starts the app on your own data:

```bash
scripts/phosk            # your data
scripts/phosk --demo     # the seeded demo on port 3718 (nothing you enter is kept)
```

The launcher:

1. checks that Ollama answers (`http://127.0.0.1:11434`). If it does not and
   `ollama` is installed, it runs `ollama serve` in the background (with
   `OLLAMA_MODELS=/var/lib/ollama` when that directory exists and
   `OLLAMA_MODELS` is not already set) and logs to
   `~/.local/state/phoskonomia/ollama.log`. Without Ollama the app still runs,
   but the assistant and receipt reading are unavailable.
2. builds the release app with `dx build --release --platform web` — on the
   first run, and again only when a source file is newer than the built
   server. `--build` forces a rebuild.
3. runs the release server bound to **127.0.0.1 only**, waits until it
   answers and opens it in your browser (`xdg-open`; `--no-open` skips that).
   `Ctrl-C` stops it.

Requirements: a Rust toolchain with the `wasm32-unknown-unknown` target, `dx`
0.7.6 (`cargo install dioxus-cli --version 0.7.6 --locked`), `curl`, and
optionally `ollama` and `xdg-open`.

## Your data

With `PHOSK_DB=surreal` and without `PHOSK_DEMO=1` (what `scripts/phosk` sets)
the app runs in **real mode**:

- data lives in `$XDG_DATA_HOME/phoskonomia` (usually
  `~/.local/share/phoskonomia`): `surreal/` holds the database, `photos/` the
  receipt photos, encrypted at rest;
- the first run writes only a starter set: ten categories with no cap, a zero
  monthly budget, the default preferences, and one empty chat. The ledger
  starts empty;
- "today" is your local date;
- a receipt photo is read by a vision model on Ollama. If none is available
  the photo is refused; the app never makes up a receipt.

Only one process can open the database at a time. A second `scripts/phosk`
detects the running one and just opens the browser.

## Receipts, from photo to ledger

1. **/receipt**: upload a JPEG or PNG. The photo is stripped of metadata,
   stored encrypted, read and turned into a *proposal*. Nothing is booked yet.
   The date printed on the receipt is used when it is plausible (not in the
   future, at most a year old); otherwise today's date.
2. The model can only pick from your categories. If it answers with a name
   that is not one of yours anyway, the receipt gets the category most of its
   lines carry (else `Other`), and a line whose category had to be guessed is
   flagged *LOW CONF · REVIEW*.
3. **APPROVE** (on /receipt or **/approvals**) books it. Then it shows in
   Transactions with its lines, counts against its envelope on Budgets, and in
   the dashboard totals.
4. A proposal that names a category you no longer have (for example, you
   deleted it after uploading) is shown as *UNKNOWN CATEGORY* and cannot be
   approved. Create the category on /categories, or reject the proposal.

## Categories

Categories are edited on **/categories**. Budgets, the dashboard, the NEW form
and the receipt reader all see the same list:

- **create**: shows up on Budgets right away (no cap until you set one);
- **rename**: keeps its cap, colour and history. Every receipt, line,
  subscription and signal moves to the new name;
- **merge A into B**: everything that named A names B. B's cap becomes A's
  cap plus B's when both had one (if B had no cap it stays unlimited; if only
  B had a cap, B keeps it);
- **delete**: only for a category nothing uses yet. Merge it away otherwise;
- **set a cap** on Budgets: the dashboard uses the same cap.

## Environment variables

`scripts/phosk` options, all overridable:

| Variable           | Meaning                                         | Default                   |
|--------------------|-------------------------------------------------|---------------------------|
| `PHOSK_PORT`       | port the server listens on                      | `3717` (`3718` with `--demo`) |
| `PHOSK_IP`         | address it binds to (keep it on loopback)       | `127.0.0.1`               |
| `PHOSK_OLLAMA_URL` | where the launcher checks for Ollama            | `http://127.0.0.1:11434`  |
| `OLLAMA_MODELS`    | model dir for an `ollama serve` it starts       | `/var/lib/ollama` if present |
| `CARGO_TARGET_DIR` | build output (the release server lives in `dx/dioxus-app/release/web/`) | `<repo>/target` |

Read by the app itself (`frontend/dioxus-app/src/data/mod.rs` and the
adapters):

| Variable                 | Meaning                                       | Default                     |
|--------------------------|-----------------------------------------------|-----------------------------|
| `PHOSK_DB`               | `surreal` (file) or `memory` (seeded demo)    | `memory`                    |
| `PHOSK_DEMO`             | `1` = seeded demo even with `PHOSK_DB=surreal` | unset                      |
| `PHOSK_DATA_DIR`         | data directory                                | `~/.local/share/phoskonomia` in real mode, `./phosk-data` otherwise |
| `PHOSK_LLM_MODEL`        | Ollama model for chat and receipt extraction  | `qwen3.6:35b-custom`        |
| `PHOSK_OCR`              | `auto`, `paddle` or `vision`                  | `auto`                      |
| `PHOSK_OCR_VISION_URL`   | Ollama URL for vision OCR                     | `http://localhost:11434`    |
| `PHOSK_OCR_VISION_MODEL` | vision model (else the first installed one that can see) | `llava`          |
| `PHOSK_OCR_URL`          | PaddleOCR service URL                         | `http://localhost:8868`     |
| `IP`, `PORT`             | bind address of the Dioxus 0.7 server (the launcher sets them from `PHOSK_IP` / `PHOSK_PORT`) | `127.0.0.1:8080` |

## Development

`dx serve` (in `frontend/dioxus-app`) is still the edit-and-reload loop. See
`frontend/dioxus-app/README.md`.
