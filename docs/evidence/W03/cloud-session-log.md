# Cloud session log (append-only, one line per decision that changed the plan)

- 2026-10-04 20:55 Gate 0: the crate does not build on Linux as-is; a local uncommitted shim makes lib tests and the retrieval gate run on Linux, so the cloud runs `make qa-retrieval-check` itself instead of relying on local.
- 2026-10-04 21:20 Branch deletion is refused by the environment (HTTP 403); `claude/probe` stays until local deletes it.
- 2026-10-04 21:45 Ask ranks changed between identical runs; root cause is the keyword route's 320 ms per-variant time budget in an unoptimized build. The eval now lifts route time budgets (second VS-04 commit); the product consequence is filed under VS-07 and VS-20.
- 2026-10-04 22:05 VS-03: most time and app queries are answerable by topic alone; added three discriminating queries per persona and recommended that VS-13 be judged on them.
- 2026-10-04 22:25 GitHub Rust CI is red on main since at least 2026-09-30 (Swift speech helper needs the macOS 26 SDK on a macos-14 runner); opened draft PR #24 moving the job to macos-26.
- 2026-10-04 22:50 VS-05: removing the hybrid relevance gate's overlap rules gave no recall gain; reverted and kept the change to the reranker only.
- 2026-10-04 22:55 Found Ask results depend on whether Search ran earlier in the same process (rank 2 alone, rank 11 after Search); assigned to VS-21.
