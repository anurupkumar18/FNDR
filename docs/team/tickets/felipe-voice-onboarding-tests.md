# Felipe: Voice, onboarding and polish, and product-oriented tests

Mission: speaking to FNDR works the same way everywhere, starts instantly, shows what it is doing, and costs almost nothing when idle; a new user gets from download to a first useful moment without confusion; and our tests protect what users actually do.

Measurements from 2026-09-23: voice records in the web view, writes a file, then transcribes with a whisper.cpp CLI if one is installed or a Python helper (`whisper_cpp_python`, needs Homebrew Python 3.10 to 3.13) that loads the 466 MB `ggml-small` model from disk on every request (`src-tauri/src/speech.rs:350-460,712`); no partial text; Home, Search, and Screen Guide each have their own recording code; `src-tauri/Info.plist` has no speech recognition usage description; the Touch ID step runs the `swift` compiler at runtime (`src-tauri/src/ipc/onboarding.rs:218-243`), which fails on Macs without Xcode Command Line Tools; 55 frontend test files, 8 of them for the memory graph.

Pairing: the Swift helper (VO-03) is new ground; pair with Kunj for one session at the start and ask for review. Kunj's command router (GS-13) consumes your voice events.

## VO-01 Measure every voice entry point today
- assignee: u1442515
- labels: area::voice, type::qa, prio::p0
- milestone: W02-Measure
- estimate: 3h
- depends: none

**Do.**
1. List every place FNDR listens or speaks: Home hero (`src/app/HomeHero.tsx`, `useHeroVoice`), Search bar voice and commands (`src/domains/search/SearchBar.tsx`), Screen Guide hold-to-talk (`src/domains/screen-guide/`), spoken answers (`start_say_process` in `ipc/commands/screen_guide.rs`), Meetings transcription (hidden), and the backend path (`transcribe_voice_input` in `ipc/commands/stats.rs:228`, `speech.rs`).
2. For each visible one, 20 short utterances: seconds from release to text, failures, wrong words, and CPU and memory during transcription (Activity Monitor or the metrics dump).
3. Write `docs/evidence/W02/voice-baseline.md`.

**Done when.** The baseline table exists for every entry point.

**Evidence.** The file.

## VO-02 Write the one voice pipeline contract
- assignee: u1442515
- labels: area::voice, type::decision, prio::p0
- milestone: W02-Measure
- estimate: 3h
- depends: VO-01, PD-16

**Do.**
1. `docs/product/voice-pipeline.md`: one Rust module `voice` owns the microphone and recognition; every feature subscribes to one event stream `voice://state` with states `idle`, `requesting_permission`, `preparing_model`, `listening { level }`, `partial { text }`, `final { text }`, `error { code, message }`, `unavailable { reason }`.
2. Who starts and stops a session (push-to-talk and tap-to-toggle), cancellation, one session at a time, and what happens when a second feature asks.
3. Review with Kunj (router) and Anurup.

**Done when.** Merged with both reviews.

**Evidence.** The doc.

## VO-03 Build the native speech helper
- assignee: u1442515
- labels: area::voice, type::feature, prio::p0
- milestone: W03-Build
- estimate: 10h
- depends: VO-02

**Why.** Apple's on-device recognizer streams partial text, needs no Python, and loads no 466 MB model into FNDR.

**Do.**
1. `src-tauri/helpers/fndr-speech/main.swift`: capture the microphone with `AVAudioEngine`; on macOS 26 use `SpeechAnalyzer` with `SpeechTranscriber` (volatile results for partial text); on macOS 13 to 15 use `SFSpeechRecognizer` with `requiresOnDeviceRecognition = true`. Handle the case where the on-device language asset must be installed first (report `preparing_model`).
2. Protocol: read commands on stdin (`start`, `stop`, `cancel`, `quit`), write one JSON object per line on stdout (`ready`, `level`, `partial`, `final`, `error`).
3. Build: compile with `swiftc -O` from `src-tauri/build.rs` into `src-tauri/binaries/fndr-speech-aarch64-apple-darwin` and list it under `bundle.externalBin` in `src-tauri/tauri.conf.json`.
4. Manual test script: start, speak, stop, cancel mid-sentence, quit.

**Done when.** The helper prints partial and final lines for spoken English on your Mac, and the release build bundles it.

**Evidence.** A terminal recording.

## VO-04 Rust voice module: one session, events to the UI
- assignee: u1442515
- labels: area::voice, type::feature, prio::p0
- milestone: W03-Build
- estimate: 6h
- depends: VO-03

**Do.**
1. `src-tauri/src/voice/mod.rs`: spawn the helper on first use, keep it while a session is active, quit it after 30 seconds idle; parse lines into the VO-02 states and emit `voice://state`; commands `voice_start`, `voice_stop`, `voice_cancel`.
2. Tests with a fake helper (a small script that prints scripted JSON lines), covering partial then final, error, helper crash (restart once, then `unavailable`), and cancel.

**Done when.** `cd src-tauri && cargo test voice` passes.

**Evidence.** Test output.

## VO-05 Permissions for speech and microphone
- assignee: u1442515
- labels: area::voice, type::feature, prio::p0
- milestone: W03-Build
- estimate: 3h
- depends: VO-03

**Do.**
1. Add `NSSpeechRecognitionUsageDescription` to `src-tauri/Info.plist` with plain wording; confirm `NSMicrophoneUsageDescription` wording.
2. Map denied, restricted, and not-determined states to `requesting_permission` and `unavailable { reason }` with an "Open System Settings" action (the Privacy and Security panes for Microphone and Speech Recognition).
3. Test on a fresh profile: first use, deny, re-enable in Settings, return.

**Done when.** Each state shows the right message in a recording.

**Evidence.** Recording.

## VO-06 One shared voice control for the whole app
- assignee: u1442515
- labels: area::voice, type::feature, prio::p0
- milestone: W03-Build
- estimate: 5h
- depends: VO-02

**Do.**
1. `src/shared/voice/useVoice.ts` (subscribes to `voice://state`, exposes start, stop, cancel) and `VoiceButton` plus `VoiceStatus` components: level meter while listening, partial text in the field as you speak, clear error with retry, and "unavailable" with the reason.
2. Accessible: announced state changes through a live region, keyboard start and stop, reduced motion respected.
3. Component tests for every state using mocked events.

**Done when.** Tests pass; the preview harness shows each state.

**Evidence.** Screenshots of each state.

## VO-07 Home uses the shared voice control
- assignee: u1442515
- labels: area::voice, type::feature, prio::p0
- milestone: W03-Build
- estimate: 2h
- depends: VO-04, VO-06

**Do.** Replace `useHeroVoice` and its `MediaRecorder` code in `src/app/HomeHero.tsx` with `useVoice`; partial text fills the hero field; final text waits for Enter (confirmation first, per PD-16). Update `HomeHero.test.tsx`.

**Done when.** No `MediaRecorder` left in Home; tests pass.

**Evidence.** Recording.

## VO-08 Search uses the shared voice control and hands commands to the router
- assignee: u1442515
- labels: area::voice, type::feature, prio::p0
- milestone: W03-Build
- estimate: 3h
- depends: VO-04, VO-06

**Do.** Replace the Search bar recording code with `useVoice`; send final text to Kunj's router entry point (GS-13) instead of the `includes()` checks; keep plain search as the fallback when the router returns nothing.

**Done when.** No recording code left in `SearchBar.tsx`; tests updated.

**Evidence.** Recording.

## VO-09 Command bar and "about this screen" use the shared voice control
- assignee: u1442515
- labels: area::voice, type::feature, prio::p1
- milestone: W04-Prove
- estimate: 3h
- depends: VO-04, GS-08

**Do.** Hold-to-talk in Quick Find and in the Screen Guide tool (GS-10) through `useVoice`; remove the old Screen Guide microphone watchdog code once nothing uses it.

**Done when.** One microphone code path remains in the app (`git grep getUserMedia` returns nothing in `src/`).

**Evidence.** The grep output.

## VO-10 Remove Python Whisper from the normal path
- assignee: u1442515
- labels: area::voice, type::chore, prio::p1
- milestone: W04-Prove
- estimate: 3h
- depends: VO-07, VO-08

**Do.**
1. Stop offering the Whisper model download in onboarding and Settings; remove `transcribe_voice_input` callers from the UI.
2. Keep the sidecar code only if Meetings (hidden) still needs it; otherwise delete `sidecars/whisper_gguf_runner.py` and `parakeet_runner.py` and the Python discovery code in `speech.rs`, with the owner's OK.

**Done when.** A fresh install downloads no speech model and needs no Python.

**Evidence.** The onboarding recording and the disk space saved.

## VO-11 Measure voice before and after
- assignee: u1442515
- labels: area::voice, type::qa, prio::p1
- milestone: W04-Prove
- estimate: 2h
- depends: VO-07, VO-08

**Do.** Repeat VO-01 on the new pipeline: time to first partial text, time to final after release, failures, and CPU and memory while listening and while idle.

**Done when.** First partial within 0.5 s, final within 1 s of release, near-zero idle cost, or the gap is stated.

**Evidence.** `docs/evidence/W04/voice-after.md`.

## VO-12 A repeatable voice test script
- assignee: u1442515
- labels: area::voice, type::qa, prio::p1
- milestone: W04-Prove
- estimate: 3h
- depends: VO-11

**Do.** `docs/product/voice-qa-script.md`: 50 utterances (searches, commands, names, numbers, app names) in quiet and noisy rooms, read by at least two teammates; record success and wrong words; rerun before Beta.

**Done when.** One full run recorded.

**Evidence.** The results table.

## VO-13 Spoken answers without spawning a process each time
- assignee: u1442515
- labels: area::voice, type::feature, prio::p2
- milestone: W05-Retro
- estimate: 3h
- depends: VO-03

**Do.** Add a `speak` command to the helper using `AVSpeechSynthesizer`, interruptible when a new request starts; replace `start_say_process`.

**Done when.** Spoken answers stop instantly on a new request.

**Evidence.** Recording.

## OB-01 Walk through onboarding on a clean profile and log every problem
- assignee: u1442515
- labels: area::onboarding, type::qa, prio::p0
- milestone: W02-Measure
- estimate: 3h
- depends: none

**Do.**
1. `FNDR_DATA_DIR="$HOME/Library/Application Support/com.fndr.app.firstrun" npm run tauri dev` with the empty directory; go through every step and every skip.
2. Time each step; screenshot each screen in light and dark; note every sentence you would not say to a friend, every wait without progress, and every dead end.
3. Also run it on a teammate's Mac if possible.

**Done when.** `docs/product/onboarding-audit.md` lists every issue with a severity.

**Evidence.** The audit.

## OB-02 Touch ID without the Swift compiler at runtime
- assignee: u1442515
- labels: area::onboarding, type::bug, prio::p0
- milestone: W03-Build
- estimate: 4h
- depends: VO-03

**Why.** The Touch ID step writes a Swift script and runs `swift` (`ipc/onboarding.rs:218-243`); on a Mac without Xcode Command Line Tools that fails, so a normal user cannot turn on the lock.

**Do.**
1. Call LocalAuthentication from compiled code: add an `authenticate` command to the Swift helper from VO-03 (or a second tiny helper built the same way).
2. Keep the fail-closed behavior; test success, cancel, unavailable, and failure.

**Done when.** Works on a Mac where `xcode-select -p` fails.

**Evidence.** Recording on such a Mac, or on a test user account without the tools.

## OB-03 Onboarding words for knowledge workers
- assignee: u1442515
- labels: area::onboarding, type::feature, prio::p0
- milestone: W03-Build
- estimate: 3h
- depends: OB-01, PD-02

**Do.**
1. Rewrite each onboarding screen: what FNDR does in one sentence (from PD-02), what it stores, what never leaves the Mac, what is optional, and exactly what each permission unlocks.
2. Make "Continue" and "Skip for now" do visibly different things and say what skipping means.
3. Run the copy through the team's UX copy review before merging.

**Done when.** A non-team person reads the screens and can explain what FNDR stores.

**Evidence.** Before and after screenshots and the person's one-sentence explanation.

## OB-04 A permissions step that shows exactly where you stand
- assignee: u1442515
- labels: area::onboarding, type::feature, prio::p0
- milestone: W03-Build
- estimate: 4h
- depends: OB-01

**Do.**
1. One row per permission (Screen Recording required; Accessibility, Microphone, Speech Recognition optional) with status, what it unlocks, and a button that opens the exact System Settings pane.
2. Detect the return from System Settings and refresh; explain when macOS requires a relaunch.
3. Tests with mocked statuses for granted, denied, and partial.

**Done when.** Tests pass; a recording shows grant, deny, and re-grant.

**Evidence.** Recording.

## OB-05 A models step that downloads only what is needed
- assignee: u1442515
- labels: area::onboarding, type::feature, prio::p1
- milestone: W03-Build
- estimate: 4h
- depends: OB-01

**Do.**
1. Download only the required search model by default; list optional models with size, what they unlock, and a later "Get it in Settings" path.
2. Check free disk space first; show progress, resume after quit, and a readable error with retry.
3. Drop the speech model once VO-10 lands.

**Done when.** A clean-profile onboarding downloads one model and finishes in under three minutes on a normal connection.

**Evidence.** Recording with a timer.

## OB-06 A first useful moment right after onboarding
- assignee: u1442515
- labels: area::onboarding, type::feature, prio::p1
- milestone: W04-Prove
- estimate: 4h
- depends: OB-03, PD-14

**Do.**
1. After onboarding, a short guided moment: "Work for a few minutes, then try: search for something you just read, or hold the key and say 'open' plus an app name."
2. Empty states on Home, Search, and Vault that teach the next action instead of saying "No results."

**Done when.** Two outside users reach a successful first search within 10 minutes without help (PD-13 sessions).

**Evidence.** Session notes.

## OB-07 Count where onboarding loses people, locally
- assignee: u1442515
- labels: area::onboarding, type::feature, prio::p2
- milestone: W04-Prove
- estimate: 3h
- depends: OB-01

**Do.** Record step start and finish times, skips, and errors to a local, content-free log that `make vault-health` or a small script can summarize; nothing leaves the Mac.

**Done when.** A summary from three test runs.

**Evidence.** The summary.

## PX-01 Five destinations and a Labs group
- assignee: u1442515
- labels: area::ui-polish, type::feature, prio::p0
- milestone: W02-Measure
- estimate: 4h
- depends: PD-15

**Today.** `SIDEBAR_GROUPS` in `src/app/App.tsx` (Memory, Reflect, Assist) and `MOUNTED_PANEL_KEYS` in `src/app/panels.ts`.

**Do.** Apply the PD-15 decision: Home, Search and Ask, Memory Vault, Daily Brief, Trust and Settings; everything else under Labs; update the command palette to match; tests in `panels.test.ts` and `AppShell.test.tsx`.

**Done when.** Tests pass and screenshots match the decision.

**Evidence.** Screenshots.

## PX-02 One visual language
- assignee: u1442515
- labels: area::ui-polish, type::chore, prio::p1
- milestone: W03-Build
- estimate: 4h
- depends: GS-02

**Do.** Make Film and Paper the default pair (UI-UX program decision D-02); move other palettes to an optional setting; remove hard-coded colors you find while doing so; coordinate with Kunj's notch HUD port (GS-02) so tokens are changed once.

**Done when.** No hard-coded colors in the mounted panels (grep for hex values in their CSS returns only token files).

**Evidence.** The grep output and screenshots.

## PX-03 Every panel handles loading, empty, and error the same way
- assignee: u1442515
- labels: area::ui-polish, type::feature, prio::p1
- milestone: W03-Build
- estimate: 5h
- depends: PX-01

**Do.** One set of components for loading, empty (with the next action), and error (with retry); apply to every mounted destination; component tests for each state.

**Done when.** A checklist in the MR shows all three states for every destination.

**Evidence.** The checklist with screenshots.

## PX-04 Settings organized by what people are looking for
- assignee: u1442515
- labels: area::ui-polish, type::feature, prio::p1
- milestone: W04-Prove
- estimate: 3h
- depends: PX-01

**Do.** Sections in this order: Capture, Privacy, Voice, Agent access, Models, Updates, About; each with a one-line description; `ControlPanel.test.tsx` updated.

**Done when.** Tests pass; screenshots.

**Evidence.** Screenshots.

## PX-05 Accessibility pass on onboarding, Home, and voice
- assignee: u1442515
- labels: area::ui-polish, type::qa, prio::p1
- milestone: W04-Prove
- estimate: 4h
- depends: OB-04, VO-06

**Do.** Contrast at WCAG AA, visible focus, keyboard-only completion of onboarding, reduced motion, and 200% zoom at 720 by 600; fix what fails.

**Done when.** A checklist with pass for every item.

**Evidence.** The checklist.

## PX-06 Release readiness on a clean Mac
- assignee: u1442515
- labels: area::ui-polish, type::qa, prio::p1
- milestone: W04-Prove
- estimate: 4h
- depends: OB-05, VO-10

**Do.** Following `docs/product/qa-gate0-checklist.md`: install the DMG on a second Mac (or a new user account), first launch, onboarding, one working hour, update check, quit and relaunch; add anything missing to the checklist.

**Done when.** The checklist passes, or each failure has a bug ticket.

**Evidence.** The filled checklist.

## QT-01 Audit the test suite by the user outcome each test protects
- assignee: u1442515
- labels: area::tests, type::qa, prio::p0
- milestone: W02-Measure
- estimate: 4h
- depends: none

**Do.**
1. For each of the 55 frontend test files and each file in `src-tauri/tests/`, write one line: the user outcome it protects (or "implementation detail"), whether the feature is mounted, and its run time.
2. Flag candidates to delete or merge: tests for unmounted or cut features (for example the 8 files in `src/domains/memory-vault/graph/__tests__` if the graph moves to Labs), tests that only restate markup, duplicates.
3. `docs/team/test-audit.md`.

**Done when.** The audit covers every file.

**Evidence.** The audit.

## QT-02 Remove or merge low-value tests
- assignee: u1442515
- labels: area::tests, type::chore, prio::p1
- milestone: W03-Build
- estimate: 3h
- depends: QT-01, PX-01

**Do.** Delete or merge the tests the audit marks, only for features that are cut or unmounted by PX-01, or for true duplicates; show suite time before and after; never delete the only test for a mounted feature.

**Done when.** `npm test` is faster and every mounted destination still has at least one test.

**Evidence.** Before and after timings.

## QT-03 Journey tests: what a person does, end to end
- assignee: u1442515
- labels: area::tests, type::feature, prio::p0
- milestone: W03-Build
- estimate: 6h
- depends: QT-01

**Why.** Component tests pass while journeys break; a journey test fails when a user would fail.

**Do.**
1. Render the real `AppShell` with the preview IPC handler (`src/dev/previewIpc.ts`) in Vitest.
2. Six journeys: complete onboarding; search and open a memory; pause capture and see it stay paused; turn on agent access and copy the command; voice states from mocked `voice://state` events; open the Daily Brief.
3. Name each test as the outcome ("a person can find a memory and open it").

**Done when.** `npm test -- journeys` passes the six.

**Evidence.** Test output.

## QT-04 Browser tests on the preview page with real clicks
- assignee: u1442515
- labels: area::tests, type::feature, prio::p1
- milestone: W04-Prove
- estimate: 6h
- depends: QT-03

**Do.** Add Playwright as a dev dependency; `npm run test:e2e` starts Vite and drives `ui-preview.html` through the same six journeys with keyboard-only variants and light and dark screenshots.

**Done when.** Runs locally in under three minutes.

**Evidence.** Output and screenshots.

## QT-05 A native smoke checklist for what tests cannot reach
- assignee: u1442515
- labels: area::tests, type::qa, prio::p1
- milestone: W04-Prove
- estimate: 2h
- depends: QT-01

**Do.** `docs/product/native-smoke.md`: permissions, microphone and speech, reopen (link Minh's matrix), shortcuts, command bar, and update; a pass or fail log per release candidate.

**Done when.** One run logged.

**Evidence.** The log.

## QT-06 Write tests in the language of the product
- assignee: u1442515
- labels: area::tests, type::docs, prio::p2
- milestone: W04-Prove
- estimate: 2h
- depends: QT-03

**Do.** `docs/team/testing.md`: when to write a journey test, a component test, or a Rust test; naming by outcome; examples from QT-03; link from `docs/team/TEAM.md`.

**Done when.** Merged and linked.

**Evidence.** The doc.

## QT-07 A fast test command under two minutes
- assignee: u1442515
- labels: area::tests, type::chore, prio::p2
- milestone: W05-Retro
- estimate: 3h
- depends: QT-02

**Do.** `make test-fast` (typecheck, frontend tests, Rust unit tests without slow integration tests); `make test` stays the full sweep; mark slow tests; document in `docs/team/testing.md`.

**Done when.** `make test-fast` finishes under two minutes on the M1.

**Evidence.** Timing output.
