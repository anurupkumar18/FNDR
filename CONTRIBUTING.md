# Contributing to FNDR

## Branching

- Use short-lived feature branches from `main` (or your team’s default branch), e.g. `feature/readiness-panel`, `fix/dedupe-test`.
- **Evaluation / TA demo**: optional branch `demo/eval-stable` with `VITE_EVAL_UI=true` and no risky merges after the freeze tag.

## Merge requests

1. Open an MR with a clear title and description (what changed, why).
2. Link related GitLab issues.
3. Request at least one review from another teammate.
4. Ensure **CI is green** (frontend job; macOS Rust job when runners exist).
5. Squash or merge per team preference; keep commit messages informative.

## Review checklist

- [ ] Typed IPC commands have matching Rust + TypeScript types.
- [ ] User-visible strings are clear on errors (no silent failures for demo paths).
- [ ] New features behind **Experimental** or **eval UI** when unstable.

## Issue tracking

Use GitLab issues with **title**, **description**, **label**, **assignee**, and **weight/estimate** so the board reflects real ownership.
