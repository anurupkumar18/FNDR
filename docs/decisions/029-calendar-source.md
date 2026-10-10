# ADR 029: Calendar source for meeting prep, read on this Mac through EventKit

## Status

Accepted 2026-10-09. The owner approved building calendar meeting prep on 2026-10-09.

## Context

Meeting prep (`proactive_signals/meeting_prep.rs`) matches a meeting title to a Resume Work thread and is due from 5 minutes before a meeting until 2 minutes after it starts. FNDR knew of no meeting before it began: the only input was a recording started in FNDR, so "before a meeting" never happened (`docs/evidence/W04/proactive-signals.md`, CHANGELOG entry "Proactive signals, backend").

ADR-018 rules out anything FNDR does by itself in the background going to a cloud service, and the meeting prep loop runs in the background every 30 seconds.

## Decision

1. **Source: macOS EventKit, read-only, on this Mac.** FNDR reads events starting in the next 30 minutes (and those that started within the last 2) from the calendars Calendar.app already syncs. FNDR never writes, creates or changes an event.
2. **Off by default.** It asks for a new macOS permission, so Settings carries a switch, "Meeting prep from your calendar", under Setup and updates. Turning it on asks macOS for calendar access (`requestFullAccessToEventsWithCompletion:` on macOS 14 and later, `requestAccessToEntityType:completion:` on 13). The switch stays off unless access is granted.
3. **Which events.** Every calendar the person did not subscribe to (subscribed and birthday calendars are left out), or only the ids listed in `[proactive_signals] calendar_ids` when that list is not empty. All-day, declined, cancelled and untitled events are skipped.
4. **Where a title may go.** Only to `meeting_prep::match_thread` and the hybrid search fallback on this Mac, and into FNDR's own toast. The macOS banner keeps its fixed text. A title is never stored (LanceDB, the state store, config), never logged, and never sent to a model or any host. The once-per-day claim key is the EventKit identifier and start time, not the title.
5. **Same rules as every proactive signal.** Off in Private Mode, under the `meeting_prep` switch, one notification per event per day across restarts, and one per related thread per day, so a calendar event and a recording of the same meeting do not both notify.
6. **Bridge: `objc2` message sends against EventKit, linked as a framework.** `proactive_signals/eventkit.rs` is behind the `CalendarSource` trait; tests use a fake.

Permission states are typed (`CalendarStatus`): not determined, restricted, denied, granted. macOS 14's write-only access cannot read and counts as denied.

## Options considered

| | A. Cloud calendar API (Google, Microsoft Graph) | B. AppleScript to Calendar.app | C. Swift sidecar with EventKit | D. `objc2-event-kit` crate | E. `objc2` sends to EventKit (chosen) |
|---|---|---|---|---|---|
| Leaves the Mac | Event data and an OAuth token per poll | Nothing | Nothing | Nothing | Nothing |
| Allowed by ADR-018 | No: a background request to a cloud service | Yes | Yes | Yes | Yes |
| Accounts covered | One provider per integration | Everything Calendar.app syncs | Same | Same | Same |
| Reliability | Token refresh, rate limits, network | Launches Calendar.app, slow, an Automation prompt, breaks across releases | A second binary to build, sign and bundle (`externalBin`), a protocol to keep | Not in the offline crate cache; its 0.3 line needs `objc2` 0.6 while the app code uses 0.5 | Uses `objc2` 0.5 and `block2` 0.5 already in `Cargo.toml`, the same way `ocr/vision.rs` and `ipc/onboarding.rs` call Vision and AVFoundation |
| Permission attributed to | n/a | Calendar.app via Automation | FNDR.app (responsible process) | FNDR.app | FNDR.app |

A was rejected because it breaks ADR-018. B was rejected as fragile and for asking for Automation access on top of Calendars. C works but adds a build and signing step for about 100 lines of calls. D was rejected because it is not available offline here and would mix two `objc2` major lines in one crate; it remains the move if the app code moves to `objc2` 0.6.

## Consequences

- `Info.plist` carries `NSCalendarsFullAccessUsageDescription` (macOS 14 and later) and `NSCalendarsUsageDescription` (macOS 13). Without them macOS ends the process when access is requested. `Entitlements.plist` gains `com.apple.security.personal-information.calendars`, the hardened runtime's calendar entitlement, next to the `device.audio-input` one already there, so a build signed with the hardened runtime is not refused.
- The Privacy Activity ledger (`privacy_proof.rs`) records model requests and network egress only; a local read has no row there. This ADR and the Settings switch are where the calendar read is stated.
- Meeting prep now has two inputs, a recording and the calendar; `meeting_prep.rs` is unchanged apart from its header.

Open question: whether reading the calendar every minute while the switch is on costs anything measurable on battery. It was not measured.
