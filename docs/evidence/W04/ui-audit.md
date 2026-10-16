# App-wide UI and UX audit, 2026-10-09

Browser preview only (`ui-preview.html` through Vite, Tauri IPC mocked by
`src/dev/previewIpc.ts`), driven with Playwright at 1280x820 and 560x800, light
and dark. The packaged app was not run, so WKWebView rendering, real
permissions and native windows are not covered. Screenshots are synthetic
preview data, saved small under `docs/evidence/W04/ui-audit/`.

Out of scope by instruction and not edited: `src/domains/notch/*`,
`src/app/HomeHero.*`, `src/app/ResumeWork.*`, `src/shared/voice/*`,
`AgentReply*`, `PeerDirectory*`. Findings there are recorded for their owners.

Checks run per panel: horizontal overflow, controls past the right edge, hit
targets under 24 px, unnamed buttons, 30 Tab presses recording focus
indicators, Escape and Tab containment in dialogs.

## Findings

Severity: High blocks a task or a WCAG A/AA criterion for many people;
Medium is a real defect with a workaround; Low is polish.

| # | Where | Finding | Severity | Status | Evidence |
| --- | --- | --- | --- | --- | --- |
| 1 | Settings | Sections were placed with CSS `order`, so the screen read Capture, Privacy, Voice, Agent access, Models, Updates, About while Tab and VoiceOver went About, Updates, Capture, Voice, Agent access, Privacy, Models (WCAG 1.3.2, 2.4.3). | High | Fixed `a4ddea0` | `settings-light-desk.jpg` |
| 2 | Memory Vault, open memory | The expanded memory card is `aria-modal` but Tab left it for the activity trace behind it (WCAG 2.4.3). The Vault panel's own trap ignores keys aimed at a nested dialog, so nobody trapped them. | High | Fixed `1fbfc58` | `vault-expanded-light.jpg` |
| 3 | Search & Ask, Daily Summary, Screen Guide | Focused text fields showed only a border color change (Ask, date) or a 10 % alpha ring (Screen Guide inputs) (WCAG 2.4.7). Now the shared `--focus-ring` token, as the Vault search uses. | Medium | Fixed `1cda490` | `focus-ask-after-light.jpg`, `focus-daily-after-dark.jpg`, `focus-sg-after-light.jpg` |
| 4 | Memory Vault | A failed load showed the raw error with no way to retry; an empty result said "No memory cards yet for this filter" even with no filter, and gave no way out of a filter that matched nothing. | Medium | Fixed `0d5db33` | `vault-nomatch-after-light.jpg` |
| 5 | Settings > Updates, Notch Do permissions | State shown only as a glyph (check, cross, bullet); "No computer-use helper is installed." rendered at browser default 16 px as an `alert` on every open; "Open Settings" wrapped to two lines; four identical button names. Now a word per row (Allowed, Not allowed, Not checked), buttons named per permission, the note is a status at row size. | Medium | Fixed `4ce099e` | before `operator-permissions-before-dark.jpg`, after `operator-permissions-after-dark.jpg`, `operator-permissions-after-light.jpg` |
| 6 | Privacy activity | `text-transform: capitalize` on every list row title-cased hosts and times ("Chatgpt.com", "3:00 Pm") and labels ("Blocked App Or Site"); two notes rendered at 16 px against 12 px rows. | Medium | Fixed `a6281ba` | before `privacy-activity-light-desk.jpg`, after `privacy-activity-after-light.jpg` |
| 7 | Screen Guide > Operate my Mac | With no helper, the copy told people to run `npm install -g open-computer-use`, which ADR 026 keeps unadvertised for Beta. | Medium | Fixed `366de75`, test `89ceb65` | `screen-guide-light-desk.jpg` |
| 8 | Preview | `setup_components` and `computer_use_permissions` were unmodeled, so Settings showed two "Unhandled UI preview command" alerts and the Updates section could not be reviewed. | Low | Fixed `fa68655` | `settings-updates-after-dark.jpg` |
| 9 | Agent (Hermes) setup | "Which plans work?" link was 113x15 px (WCAG 2.5.8 minimum 24 px). | Low | Fixed `13c8852` | `hermes-agent-light-desk.jpg` |
| 10 | Activity Stats, under 920 px | One-column grid keeps each card at 520 px with its own scroll, so tiles are cut off ("0.08" half visible) and the page has nested scroll areas. Two CSS attempts collapsed the cards to 83 px because the card content depends on the fixed height; reverted. | Medium | Open | `stats-dark-narrow.jpg` |
| 11 | Home | Greeting says "Good evening" while the search placeholder says "this afternoon" at the same time. | Low | Open, Home is being rebuilt | `home-light.jpg` |
| 12 | Home | Preview shows "Recent work couldn't be loaded" because `resume_work` is not modeled in the preview; left to the Home rebuild, which owns that command. | Low | Open | `home-light.jpg` |
| 13 | Daily Summary | Chromium draws its own date picker icon beside FNDR's, so two calendar icons show. WKWebView draws none, so the packaged app likely shows one; not verified there. | Low | Open, needs the packaged app | `daily-summary-light-narrow.jpg` |
| 14 | Memory Vault rows | App names and tags truncate hard ("Visual Studio ...", "2 FI...", "RE03-PRE...") at 1280 px while the row has free space; rows with "Open source" are wider than rows without, so the right edge is ragged. | Low | Open | `vault-overlap.jpg`, `memory-vault-dark-narrow.jpg` |
| 15 | Memory Vault, narrow | The pinned activity trace at the bottom covers the last row's meta line. | Low | Open | `memory-vault-dark-narrow.jpg` |
| 16 | Engine diagnostics | "FNDR PROCESS" heading sits flush on the row above while "HOST SYSTEM" has a gap. Also a developer view in the main sidebar with terms like RSS and physical footprint; acceptable under Labs. | Low | Open | `engine-diagnostics-light-desk.jpg` |
| 17 | To-dos | "Task type" label sits about 5 px lower than "New task title" on the same row. | Low | Open | `to-dos-dark-desk.jpg` |
| 18 | Settings | Heading levels jump: section h3, "Appearance" h4 inside About, "Privacy alerts" h2 inside the Privacy h3. | Low | Open | `settings-light-desk.jpg` |
| 19 | Settings > Voice | Section has one sentence and no control. The voice settings in `docs/product/voice-ux.md` are meant to land here. | Low | Open, spec P1.4 | none |
| 20 | Search & Ask | "Search & Ask" button is shorter than the two-line field beside it. | Low | Open | `search-ask-fndr-light-desk.jpg` |

What held up: no horizontal overflow on any panel at 560 px; no unnamed
buttons; Settings and the command palette keep Tab inside and close on Escape
with focus returned to the opener; the Vault panel, Daily Summary, To-dos,
Privacy and Agent all pass the 30-Tab focus-indicator sweep after the fixes.

## Checks

- `npm run typecheck`: passed.
- `npm test`: 723 passed, 1 failed (97 files). The failure is
  `src/shared/ipc/ipcDrift.test.ts`, which reads other sessions' uncommitted
  IPC work in this shared checkout (`work_set.rs`, `src/shared/ipc/tauri.ts`):
  one run flagged `open_work_set` and `resolve_work_set` as uncalled, the next
  an unused wrapper, and run alone a minute later it passed. None of the files
  it names are in this audit's commits. My first full run also caught two
  Screen Guide tests broken by the finding 7 copy change; fixed in `89ceb65`.
- Each fix has a unit test except the CSS-only ones (findings 3 and 9), which
  were checked by screenshot.

## Open question

Whether the Settings drawer at roughly 380 px wide should keep the Notch Do
permissions under "Updates" at all; they are permissions, not components, and
the voice spec adds a Voice group that would be a more natural home.
