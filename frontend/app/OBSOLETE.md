# ⚠️ OBSOLETE — do not run, extend, or fix this app

**Superseded by `frontend/dioxus-app/` (Rust, Dioxus 0.7 fullstack).** Marked obsolete 2026-09-17.

- This React/Vite app was built against a REST API (`backend/bin/phosk_api`) that never got past `501`
  stubs and is itself being deleted. **It has no working backend and never will.**
- The Dioxus app is a port of the same 7 pages and is the only frontend wired to the real backend
  services. All new UI work goes there. The project rule is: no JavaScript/npm in new work.
- This directory survives only so it exists once in git history as a fidelity reference. It is scheduled
  for deletion by task **T05** in the root `CLAUDE.md`. To look at it afterwards:
  `git log --diff-filter=D --oneline -- frontend/app` then `git checkout <that-commit>^ -- frontend/app`.
- The visual source of truth is, and remains, `frontend/.claude-design-export/`.

Agents: do not read this directory for guidance, do not port "missing" behaviour from it without a task
saying so, and never modify it except to delete it under T05.
