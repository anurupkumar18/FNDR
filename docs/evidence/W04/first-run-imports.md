# First-run imports, 2026-10-15

The onboarding flow places the existing Privacy Proof component before the optional Bring your past step. The step presents six import kinds with local read descriptions and permission needs. All consent keys are absent by default. The recommended action enables browser history, recent files, and calendar for a 30 day scan. Mail, Slack export, and git remain off until selected. The person can skip imports and later manage every source in Settings.

The Rust runner checks persisted consent before it creates a run slot or starts scanning. Disabling a source requests cancellation. Forget waits for the run to stop, removes that source's rows and graph links through the existing runner, then clears the cursor, tombstones, and consent. A corrupt consent value fails closed. Guided search offers up to three deterministic queries from imported titles, shortening a title when fewer than three titles exist. The first answer timer stores only elapsed milliseconds in the local state store after a guided imported result successfully reopens, and records a count and duration in the existing runtime metrics. Diagnostics shows the observed time and count. Opening a result is a proxy for finding an answer; the UI cannot determine whether the answer is correct.

## Checks

| Behavior | Boundary | Result |
| --- | --- | --- |
| Consent refusal, enable, Forget, and refusal again | Store-backed Rust test with a fake mail source | Passed in targeted `cargo test --lib` run |
| Missing and corrupt consent | StateStore Rust test | Included in focused import suite |
| Every card and single-source opt-in | Vitest with mocked IPC | Passed |
| Recommended path and 30 day window | Vitest with mocked IPC | Passed |
| Progress, cancel, Forget confirmation, focus return | Vitest with mocked IPC | Passed |
| Guided title query and reopen timer | Vitest with mocked search and reopen IPC | Passed |
| Empty sources, empty query, large window, Unicode title, load failure, permission refusal | Vitest with mocked IPC | Passed |
| Preview import behavior | Synthetic preview IPC test | Passed; no real source read |

The source registry in this worktree remains empty until connector integration. The browser preview server started, but the available browser automation surfaces did not expose an isolated browser, so the rendered page was not visually inspected. The preview IPC test uses a synthetic Atlas title and verifies the consent refusal and Forget path. No real history, mail, calendar, vault, or token was read.
