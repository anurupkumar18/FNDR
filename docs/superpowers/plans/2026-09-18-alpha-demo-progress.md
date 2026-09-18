# Alpha demo progress ledger (append-only)

Resume rule: read this file, run `git log --oneline origin/main..alpha-demo`, start the first unchecked task in 2026-09-18-alpha-demo-hardening.md.
Fallback if the session hits the usage limit: stop at the last green, pushed commit (owner decision).

| time | task | commit | verification | notes |
|---|---|---|---|---|
| 07:11 | T0 branch + baseline | (this commit) | typecheck ✅ · vitest 33 files / 180 tests ✅ | branch from origin/main 715cc8c; cherry-picked 0465dac → 7411402; AppPanels conflict resolved with --theirs; skipped `npm ci` (node_modules present, package.json unchanged) |
| 07:17 | T1 surface policy | eec69a9 | memory_quality 29 ✅ · low_signal_surface ✅ | pushed (GitLab SSH blipped once at 07:11, recovered) |
| 07:25 | T2 read boundaries + review-queue cmd | (this commit) | drop_low_signal_hits ✅ · search 46 ✅ · cargo check bin ✅ · typecheck ✅ | |
Note: session idled ~00:45–07:10 at a rejected tool call; overnight window lost. Re-prioritizing with owner at 07:25.
| 07:33 | T3 data-dir resolver, backfill hook, no omnibar/autofill hotkeys | (this commit) | data_dir_override ✅ · config 24 ✅ · cargo check bin ✅ | all 14 app_data_dir call sites routed; formatting noise in teammate files avoided |
| 07:44 | T4 seed corpus + seeder + scripts | (this commit) | seeder gate: 82 stored / 79 surfaced / 3 needs-signal ✅ · make test ✅ (vitest 180, cargo lib 635 + all integration suites) | corpus trimmed to 82 entries (time); real profile untouched |
| 07:47 | T5 Ask FNDR panel | 67b12a5 | AskPanel 3 ✅ · typecheck ✅ | |
| 07:47 | T6 curated surface | (this commit) | palette ✅ · app tests ✅ · typecheck ✅ · demo app booted on com.fndr.app.demo, backfill queued | self-QA via screencapture -l <FNDR window id> |
| 07:49 | HANDOFF | (this commit) | tree green (typecheck 0 errors); partial T9 Settings edit reverted | pushed alpha-demo and fast-forwarded main per owner request; see 2026-09-18-alpha-demo-HANDOFF.md |
| 07:57 | T9 Settings clean sheet | pending commit | typecheck ✅ · ControlPanel 3 tests ✅ | replaced tabbed settings with Profile, Capture, Privacy/blocklist, and read-only Local Models; removed update, retention, auto-fill, MCP, model mutation, appearance picker, and Danger Zone UI |
