# Native file finding

The feature is prepared on `session/pensive-swift-d8h6`, based on `64e45609`, for a draft PR targeting main. Kunj authorized testing and publication on 2026-11-17. No owner-profile migration or branch switch was performed. This change owns the Files implementation and its integration changes; committed source was reused from `a762a80`, not another worktree's uncommitted UI changes.

The [implementation record](../superpowers/plans/2026-11-17-native-file-finding.md) contains verification, red-green evidence, architecture and limitations. [ADR 039](../decisions/039-native-file-recall-boundary.md) defines the separate index/model boundary.

Verified: 1,041 frontend tests, 107 focused native tests, 128 native integration tests (4 ignored), 18 Files/workspace browser journeys, frontend and app binary builds, ten real reference vectors, 20/20 real file retrieval cases, two real code lookups, three negative code cases, and the main-memory software-engineer retrieval gate. Browser checks used synthetic IPC, not an end-to-end native app session. Helper startup and pipe operations have a tested 120-second deadline that kills and reaps a stuck child. Keyboard filter activation and typed file/code scopes have regression coverage.

Full repository gates are not green: the Rust library has five expired-date Mail failures (2,158 passed, 40 ignored), and the full browser suite has two stale-heading Review failures (24 passed). Both sets reproduce on untouched main `64e45609`. Do not merge this draft under a claim that all checks passed. The [implementation record](../superpowers/plans/2026-11-17-native-file-finding.md) names the failures and environment requirements.

Ordered follow-ups:

1. Resolve macOS 13 versus 14 compatibility and verify an independently built, signed bundle on a fresh machine. Local packaging must use `tauri.files.conf.json` as the README specifies.
2. Expand small-corpus semantic calibration beyond the synthetic code fixture before changing its cosine threshold.
3. Measure helper deadlines on CPU before changing the shared 120-second limit.

Process correction: visibility tests must also inspect physical deletion. Read-time source filtering hid an FSEvents canonical-path mismatch until the lifecycle test checked the SQLite row itself.
