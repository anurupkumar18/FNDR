# Alpha demo progress ledger (append-only)

Resume rule: read this file, run `git log --oneline origin/main..alpha-demo`, start the first unchecked task in 2026-09-18-alpha-demo-hardening.md.
Fallback if the session hits the usage limit: stop at the last green, pushed commit (owner decision).

| time | task | commit | verification | notes |
|---|---|---|---|---|
| 00:58 | T0 branch + baseline | (this commit) | typecheck ✅ · vitest 33 files / 180 tests ✅ | branch from origin/main 715cc8c; cherry-picked 0465dac → 7411402; AppPanels conflict resolved with --theirs; skipped `npm ci` (node_modules present, package.json unchanged) |
