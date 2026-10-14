# Home and Resume UX checkpoint, 2026-10-14

## Current state

- The Home implementation commit `07f08eb` is based on `a897107`. The isolated `codex/home-resume-ux` branch is on both GitLab and GitHub, and [draft PR 52](https://github.com/anurupkumar18/FNDR/pull/52) is open. GitLab's HTTPS API certificate has expired, so no GitLab merge request was created.
- The shared `/Users/anurupkumar/FNDR` checkout remains on an older `main` with other sessions' uncommitted edits. This lane did not edit, stash, reset or switch it.
- The broad product-quality goal remains active. This Home slice is implemented and browser-reviewed; native Home and real reopen behavior are still unverified.

## Delivered and checked

Home now shows recent work on the first desktop screen, puts Continue before Open all, reports a typed reopen result, and keeps the actions from running together. The scroll blur component and unused liquid search action were deleted. The current Home contract is in [DESIGN_DIRECTION.md](../product/DESIGN_DIRECTION.md), and detailed checks are in [home-rebuild.md](../evidence/W04/home-rebuild.md).

| Check | Result and limit |
| --- | --- |
| `npm run typecheck`, `npm run build`, `git diff --check` | Passed. The production build still reports its existing large main-chunk warning. |
| `npm test` on published commit `07f08eb` | 108 files and 843 tests passed. Home's 43 focused tests passed with fake IPC. |
| Browser preview at desktop and 390 px widths | Recent work and Continue appeared on the first desktop screen in light and dark themes. At 390 px, Speak and Submit fit and the page had no horizontal overflow. The preview's no-target outcome was visible. This is presentation evidence, not native reopening. |
| Native attempt | The current debug binary launched with `FNDR_DATA_DIR` set to the paused synthetic office-pm profile. Computer use instead selected an older installed bundle. That bundle used the default owner profile and briefly captured screen context. It was quit, and no FNDR process remained. Its macOS authentication overlay blocked a useful Home check. No owner vault cleanup was attempted. |

## Ordered next steps

1. Resolve native process identity before computer use. Launch only a current build against the paused synthetic profile, verify the selected window's executable path and profile, and confirm there is exactly one FNDR process. Do not select the older installed bundle or use the default owner profile.
2. After normal macOS authentication, inspect Home with synthetic recent work and exercise Continue on a supported source. Record the native layout and the actual typed reopen outcome. A browser fixture or mocked IPC result does not satisfy this check. The operator may need to unlock the app; do not bypass authentication.
3. Review the unexpected owner-profile captures through the normal Vault UI if desired. Do not rewrite or delete owner data without FNDR closed, a backup and the owner's go-ahead.
4. Update the evidence note and draft PR with the native result. Review CI, then make the PR ready when that result is satisfactory. Retry a GitLab merge request only after its HTTPS certificate is fixed.

## Process correction

An app lookup by bundle ID attached the old bundle despite a separate current binary running. Next time, verify the executable path and profile before interacting with a native window. An initial parallel test, typecheck and build run produced two unrelated five-second test timeouts; the same tests and the full suite passed when run without competing checks.
