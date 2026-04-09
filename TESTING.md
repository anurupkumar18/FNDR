# Testing FNDR

## Quick commands

| Command | Description |
|---------|-------------|
| `npm run typecheck` | TypeScript `--noEmit` |
| `npm test` | Vitest (SearchBar, Timeline) |
| `npm run build` | Production frontend build |
| `cd src-tauri && cargo fmt --check` | Rust formatting |
| `cd src-tauri && cargo clippy --all-targets` | Lints |
| `cd src-tauri && cargo test` | Rust unit + integration tests (`tests/search_flow.rs`) |
| `make rust-test` | fmt + clippy + cargo test |

## CI parity

GitLab (see [.gitlab-ci.yml](.gitlab-ci.yml)):

- **frontend** job: Linux `node:20` — `npm ci`, typecheck, build, Vitest.
- **rust_macos** job: **requires a macOS runner** (`tags: [macos]`). This crate depends on Apple frameworks; Linux runners cannot compile the full `src-tauri` workspace.

If you have no macOS runner yet, rely on the frontend job in CI and run Rust checks locally on a Mac before merging.

## Manual QA (demo)

- Cold start; **Readiness** panel values.
- First search latency; empty query; no results.
- Pause/resume capture; app filter list; timeline selection.
- Revoke Screen Recording → confirm messaging; **Use demo data only** path still searchable.

## Environment matrix (before presentation)

- Laptop on battery vs plugged in; after reboot; offline; projector/screen share; empty DB vs seeded DB.
