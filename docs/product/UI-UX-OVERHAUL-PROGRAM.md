# FNDR 1.0 UI/UX overhaul program

Status: **In progress** (2026-09-22)

Product boundary: **FNDR v1 in this repository**; ADR-015 makes v2 a read-only knowledge source.

Purpose: one implementation and evidence program for every user-facing FNDR surface. This document supplements, rather than replaces, `DESIGN_DIRECTION.md`, the Beta/Final master plan, and accepted ADRs.

## Outcome

FNDR should feel like one trustworthy macOS product: readable in light and dark mode, understandable without instruction, usable by keyboard and at 200% zoom, honest about foreground and background work, and consistent across the main window and auxiliary windows.

This is not a visual reskin. It covers information architecture, language, interaction states, native permission boundaries, accessibility, and the disposition of compiled-but-unreachable features.

### Success in one sentence

A user can install FNDR, understand what it is doing, find or ask about a memory, inspect the evidence, control capture and privacy, and recover from denied permissions or failed local services without encountering a dead control, unreadable state, misleading claim, or unexplained background action.

### Non-goals

- Rebuild the Rust capture, retrieval, or persistence architecture wholesale as part of a UI pass. Narrow fail-closed privacy and pause-persistence fixes required to make a visible control truthful are in scope.
- Change stable Tauri command names merely to fit a new component hierarchy.
- Treat a mocked browser run as evidence for macOS permissions, capture, biometrics, global shortcuts, microphone use, native overlays, file export, or source reopening.
- Restore every historical panel to navigation. Unclear surfaces are classified before they are redesigned.
- Add a component framework on top of the existing React, Vanilla CSS, tokens, and atoms without first proving an unmet need.
- Store real captures, credentials, database files, or private user content as visual-test fixtures.

## Repository constraints and stable boundaries

- `App.tsx` owns one `activePanel`; `AppPanels.tsx` is the mounted-panel boundary. Preserve the `isVisible` / `onClose` contract while slices are migrated.
- The stable product path remains capture → OCR → memory → hybrid retrieval → Memory Cards / Memory Vault → UI (`docs/architecture/ARCHITECTURE.md`).
- Backend status is event-driven after one initial read (ADR-011). New UI must not add always-on polling where a change event exists.
- Screen Guide remains opt-in, read-only, local, and ephemeral (ADR-014). A browser fixture may simulate its states but cannot prove those guarantees.
- Reuse `src/shared/theme/`, `src/shared/components/atoms/`, existing panels, and existing component tests before adding primitives.
- Preserve user data and working behavior unless a slice explicitly changes the product contract.

## Current-state evidence

| Evidence | What is confirmed | Consequence |
| --- | --- | --- |
| 2026-09-22 native lock screenshot | macOS attributes authentication to `swift-frontend`; `request_biometric_auth` currently launches `swift -e` from `src-tauri/src/ipc/onboarding.rs`. | P0 trust/identity defect. Authentication must run under FNDR's signed process identity; browser evidence is insufficient. |
| 2026-09-22 light-home screenshot | Text, placeholder, subtitle, date, and scroll prompt visually disappear into a moving background; the screen says “SCROLL TO EXPLORE” without a corresponding workflow. | Foreground content needs an invariant readable surface and false affordances must be removed. |
| 2026-09-22 Memory Vault screenshot | “Universe / History / Perspective,” `N cards`, raw filenames/bundle IDs, duplicated titles, unlabeled counts, and an `X` close control obscure the purpose of the screen. | Copy, hierarchy, card shaping, filter names, and control labels are part of the overhaul. |
| Initial source reachability trace (historical defect) | `PanelKey` defined 15 destinations, while `AppPanels.tsx` mounted only Ask, Vault, Daily Summary, Stats, To-dos, Wrapped, Screen Guide, Engine Metrics, and Privacy Proof (the two latter labels are historical). Voice/proactive flows could target `meeting`, `knowledgeGraph`, and `focusMode`. | This justified a P0 reachability slice; the checkpoint below records the implemented guard and reroutes. |
| Checkpoint source reachability trace | `MOUNTED_PANEL_KEYS` is now the shell boundary; sidebar items are typed to it, the Alpha palette allowlist exposes only mounted destinations, and `handleOpenPanel` rejects any other `PanelKey`. Meeting/graph voice requests and focus-mode proactive actions now resolve to mounted recall/reflection workflows. | The blank-panel defect is resolved in source and focused tests. A complete browser click/keyboard matrix remains open. |
| Source entry-point trace | Vite builds `main`, `omnibar`, `autofill`, and `screenGuide`; only `main.tsx` performs the current palette bootstrap. Autofill also carries a large inline, fixed-color style block. | Theme, typography, focus, and reduced-motion behavior must be verified per entry point. |
| Initial keyboard/scroll trace (historical defect) | Both `App.tsx` and `SearchBar.tsx` handled Cmd/Ctrl+K for different outcomes; root and global scrollbars were hidden. | Cmd/Ctrl+K now belongs to the command palette, and Search Escape clears only a focused Search input. Full overflow/zoom verification remains open. |
| Initial source onboarding trace (partially resolved) | “I’m in” and “Skip” took the same privacy transition; the permission step described Screen Recording as required but still offered “Skip for now”; locality/network copy exceeded the scope supported by model downloads and optional integrations. | Current copy now states bounded local-first behavior and exceptions and surfaces save failure. Distinct consent meaning and denied/degraded permission paths still need browser/native evidence and a product decision. |
| Browser preview, 2026-09-22 | A dev-only page can mount the real React shell with strict synthetic Tauri IPC. The browser renders the same frontend structure without receiving macOS capture, Accessibility, microphone, or biometric permission. | Use it for fast visual/interaction work; keep unknown IPC commands as failures and finish with bounded native QA. |

No screenshot alone proves a workflow is correct. Each claim above is paired with source evidence, and every closing claim below requires the specified test or recording.

## Implementation checkpoint — 2026-09-22

Status language in this checkpoint is deliberately strict:

- **Implemented** means the behavior exists in the current working tree.
- **Focused-test verified** means a relevant unit/component batch passed during this checkpoint.
- **Browser verified** means the stated interaction was exercised in the permission-free localhost preview; it does not imply native behavior.
- **Deferred / unverified** means the code may exist, but the required browser matrix, final integration gate, native Tauri run, or human product decision is still outstanding.

### Five-Ws implementation record

| Slice | Who | What changed | Why | Where / when | Foreground and background contract | Checkpoint status |
| --- | --- | --- | --- | --- | --- | --- |
| Capture admission safety | Anyone allowing FNDR to capture screen context | Known-sensitive app, bundle, URL, title, authentication/private-browser context, user blocklist, self-app context, and detected secret text can now fail closed before durable storage. Metadata is checked before content processing; transient extracted text is checked again before embeddings, model inference, vectors, or storage. A distinct `SensitiveContext` reason and content-free counters/signals were added, and sensitive admission logs no longer include raw URL/title values. | A reassuring foreground message is insufficient if the background pipeline can store a sensitive frame first. | Rust capture admission, safety gate, stats, and Privacy Activity data; on every background capture attempt and again after transient text extraction. | The background rejects the capture and records only bounded reason/count evidence. Settings and Privacy Activity provide the foreground explanation; native capture evidence is still required. | **Implemented; focused Rust tests reported green. Native capture/restart evidence deferred.** |
| Durable capture pause | A user who needs immediate, persistent privacy control | Pause/resume now stores `capture.user_paused`; pause takes effect before the persistence write, resume occurs only after durable `false`, malformed state starts paused, transient internal resume cannot override a user pause, and companion controls use the same boundary. | “Pause” must remain true after relaunch and must fail closed if persistence is damaged. | Settings, command palette, companion API, and `AppState`; at the moment the user pauses/resumes and when FNDR initializes. | Foreground controls show active/paused feedback while the background capture loop obeys the durable preference. | **Implemented; persistence-focused Rust tests reported green. Browser preview state sync is implemented; native relaunch proof deferred.** |
| Reachable navigation and commands | Any sidebar, keyboard, voice, or notification user | A mounted-panel registry now gates navigation; dead meeting/graph/focus destinations were removed or rerouted; the palette exposes state-correct Pause *or* Resume; Cmd/Ctrl+K has one owner; invalid destinations are rejected instead of displaying a blank foreground. | Every visible promise must lead somewhere real and recoverable. | Main shell, sidebar, command palette, Search voice interpretation, and proactive notifications whenever a route is requested. | Background events may propose an action, but only the foreground shell can open an approved mounted destination. | **Implemented and focused-test verified; complete browser traversal still open.** |
| Foreground ownership and modal recovery | Keyboard and assistive-technology users, especially when a panel overlays Home | The Home/shell layer becomes `inert` while a full-screen panel or palette is open; the recording banner remains a foreground status; panels receive labelled dialog semantics, focus containment, Escape handling, and focus restoration. A layered-modal guard gives the topmost command palette ownership of Escape/Tab. Panel crashes now offer Retry/Home recovery rather than raw runtime output. | Wallpaper, search, and shell controls must not remain interactive behind the task in front. | Main shell and mounted panels whenever a panel, Settings, or Cmd+K opens and closes. | Background ambience and controls are suppressed; ongoing recording remains visibly foreground because it may outlive a panel. | **Implemented and focused-test verified; representative layering is browser verified, full panel matrix open.** |
| Truthful setup and privacy copy | First-run and privacy-conscious users | Onboarding now describes local-first storage plus model-download/optional-integration network exceptions, surfaces save failure, and no longer requires a downloaded optional model to reopen the workspace. The biometric frontend has no “disable and bypass” recovery path. | Consent and trust require bounded claims and fail-closed behavior. | Welcome, biometrics, privacy, model, and permissions steps; before the workspace is available. | Foreground copy explains the background work and exceptions. | **Partially implemented and component-tested. Distinct consent semantics, permission denial/revocation, and FNDR-branded in-process native authentication remain deferred.** |
| Recall: Search, Ask, and Vault | A user trying to find, understand, or reuse prior work | Search owns clearer pending/result semantics; Ask and Vault use modal focus behavior; Vault presents human summaries, understandable filters, an excluded-capture review queue, progressive evidence, and bounded actions; meeting/graph voice requests resolve into recall workflows. | Recall is the core product purpose, so machine labels and dead destinations cannot dominate it. | Home/Search results, Ask, Memory Vault list/detail, and cited-memory transitions during active recall. | Retrieval/synthesis can run in the background, while visible loading, answer, citation, empty, failure, and review states stay in the foreground. | **Implemented across current Alpha scope and focused-test verified; browser action sweep and native reopen/clipboard behavior remain open.** |
| Reflection | A user reviewing a day/week or carrying work forward | Daily Summary binds output/export to the selected date and ignores stale requests; global follow-ups are labelled honestly. To-dos preserve edits on failed actions and Done completes rather than dismisses. Stats has explicit loading/refresh/zero states and a non-interactive grid. Wrapped exposes real week selection semantics, more of its data, and export-open errors. | Reflection should support decisions, not present stale or decorative telemetry. | Daily Summary, Stats (whose panel heading is “Activity Stats”), To-dos, and FNDR Wrapped when a user intentionally opens them. | Background generation/export is paired with visible busy, stale, error, and ready states. | **Implemented and focused-test verified; end-to-end browser controls, PDF open, and the larger merge decision remain open.** |
| Screen Guide | A user asking for help with the current display | The panel says it reads the current main display only after an explicit request, is local/read-only, does not click or type, and does not add the turn to Vault. It now has modal/focus behavior, listener/load failure recovery, retry, and state announcements. | The most permission-sensitive assistant needs clear purpose, timing, and limits. | Screen Guide settings/request panel in the main app; native overlay only after an explicit question/hold. | Configuration and answers are foreground; ephemeral native capture/OCR/inference are background and remain unproven in a browser. | **Implemented and focused-test verified; text-only preview path remains to be completed, while microphone/shortcut/overlay/multi-display proof is native-only.** |
| Diagnostics and trust evidence | Normal users reviewing privacy, and developers diagnosing the engine | User-facing **Privacy Proof** is now **Privacy Activity**, scoped to the current app session and explicit that recorded egress is not a complete network audit. **Engine Metrics** is now **Engine diagnostics**, with developer/best-effort framing plus loading, stale, retry, zero, and error states. Internal component/IPC names remain unchanged for compatibility. | Session counters and partial telemetry must not masquerade as a cryptographic proof or complete audit; developer data must not look like consumer value. | Assist navigation, command palette, Privacy Activity, and Engine diagnostics when deliberately opened. | Background counters feed a bounded foreground explanation with explicit omissions. | **Implemented and focused-test verified; browser path sweep and native counter validation remain open.** |
| Permission-free preview | Designers, engineers, and reviewers iterating without broad macOS access | A dev-only page now supplies deterministic, mutable synthetic IPC for all mounted Alpha paths, emits mocked capture/Screen Guide events, uses `preview-only://` PDF paths, rejects real file paths, and fails unknown commands loudly. | Fast UI work should not require Screen Recording, Accessibility, microphone, native capture, or filesystem access. | `ui-preview.html` on localhost during development only. | The browser renders real foreground components while the synthetic background performs no capture and no native side effects. | **Harness implemented; 13/13 preview IPC tests passed. Full viewport/theme/path matrix remains in progress.** |

### Automated evidence recorded at this checkpoint

The focused batches overlap and must not be added together. They are retained to show stable-boundary coverage; the final current-tree rows below are the integration result after all concurrent slices landed.

| Evidence batch | Result recorded | What it establishes | What it does not establish |
| --- | --- | --- | --- |
| App onboarding/background inert batch | **2/2 passed** | Optional-model completion, background inertness, and shell-focus restoration at the component boundary. | Native onboarding, permissions, or relaunch. |
| Panel registry, Search shortcut, error boundary, and command/navigation batches | **10/10 passed** | Mounted allowlist behavior, single Cmd+K ownership, accessible recovery, and bounded Alpha command exposure. | Every real-browser path or native notification/voice event. |
| Ask, Vault, and command-palette batch | **12/12 passed** | Modal contracts, user-facing actions/copy, capture-state command choice, and focus restoration. | Clipboard/source reopening or native storage mutation. |
| Shared modal, Ask/Vault, and reflection batch | **26/26 passed** | Topmost-modal ownership plus Daily/Stats/To-dos/Wrapped component behavior. | Browser reflow and native export/open. |
| Screen Guide and assistance batch | **32/32 passed**; slice typecheck passed | Focus/error/state contracts for Screen Guide and related assistance surfaces. | macOS capture, microphone, shortcut, overlay, or multi-display behavior. |
| Development preview IPC | **13/13 passed** | Strict command modeling, safe virtual exports, mutable fixtures, event sync, and unknown-command failure. | Native services or visual correctness. |
| Full frontend suite at the reflection milestone | **262/262 passed** before later concurrent changes | The then-current frontend integration was green. | Historical only; superseded by the final 297-test row below. |
| Focused Rust privacy/capture tests | **Passed** | Sensitive-context admission/counters and durable pause failure modes at their stable boundaries. | A signed native application run with real permissions and relaunch. |
| Diff hygiene during slices | **`git diff --check` passed** at recorded checkpoints | No whitespace-error regression at those checkpoints. | Type, behavior, build, or final-tree cleanliness. |
| Final current-tree frontend gate | **`npm run typecheck` passed; 55 files / 297 tests passed; `npm run build` passed** | The final TypeScript graph, component/unit suite, and all four production entry builds integrate successfully. | Native macOS behavior or the existing Rollup main-chunk size warning. |
| Final current-tree Rust gate | **`CARGO_BUILD_JOBS=1 cargo test` passed**; 712 library tests passed with 7 ignored, and every executed integration/doc test passed | Privacy admission, pause persistence, counters, capture, search/storage, and the rest of the Rust workspace remain integrated. | Ignored network/model/AppState-factory cases or a signed GUI run. Existing unrelated compiler warnings remain. |
| Final tree hygiene | **`git diff --check` passed** | The complete working diff has no whitespace errors. | Product approval, native evidence, or commit readiness. |
| Rust formatting baseline audit | **`cargo fmt --all -- --check` remains red across broad pre-existing files** | Confirms the repository still lacks a clean rustfmt baseline. | It is not evidence of a behavioral failure in this slice; autoformatting the entire Rust tree would create unrelated churn, so it was deliberately not done. |

## Foreground and background contract

The UI must distinguish an action the user is taking now from work FNDR continues on their behalf.

| Workflow | Foreground owner | Background owner | Required visible truth |
| --- | --- | --- | --- |
| Startup and lock | Onboarding or lock dialog blocks the main workspace. | Model discovery/download and authentication request may wait on native services. | Current step, why it is required/optional, progress, cancel/deny outcome, retry, and whether work continues after leaving the step. |
| Capture | Settings exposes pause/resume and status. | Rust safety-gates, captures, OCRs, enriches, and stores; a durable user pause overrides transient internal resume. | Capturing/paused/degraded state, last meaningful failure, current-session stored/skipped totals, and a direct path to privacy controls. Never imply that a browser fixture is capturing. |
| Search | Search/Ask input and result list are foreground. | Retrieval, reranking, summary/answer composition run locally. | Searching, no results, partial/refusal, timeout/error, result count, and evidence provenance. |
| Post-capture review | Vault shows lifecycle and review queue. | Local review workers enrich or reject records. | Human language for pending/reviewed/failed; no raw implementation narration as the primary card. |
| Privacy alerts | Badge, toast, and Trust/Settings decision are foreground. | Sensitive-context checks emit alerts. | What was detected, what adding a block does, whether existing records are deleted, success/error, and no unsupported “secure” claim. |
| Meetings | Recording banner is foreground when active. | Native audio capture/transcription may outlive a panel. | Recording identity, stop path, permission failure, and persistent visible status. A hidden/unmounted recorder cannot be the only stop path. |
| Screen Guide | Panel configures; hold/text request and overlay answer are foreground. | Global shortcut, ephemeral capture, OCR/inference, optional speech run natively. | Listening/capturing/thinking/answer/error/dismissed states; pixels and guide turns remain ephemeral. |
| Proactive suggestions | Toast is foreground. | Suggestion listener runs in the shell. | Every action passes the mounted-panel boundary. Focus/context-switch suggestions now omit a dead action or lead to Daily Summary instead of `focusMode`. |
| Omnibar / Autofill | Auxiliary native window is foreground while invoked. | Search, clipboard history, AX context, and injection are native/background services. | Source, confidence, confirmation before injection, success/error, and an immediate dismiss path. |
| Theme and motion | User controls appearance in the foreground. | Preference persists locally and all windows subscribe/bootstrap. | Same theme across windows, no flash of another theme, and motion stops when requested. |

## Target product and design language

### Principles

1. **Comprehension before atmosphere.** Cinematic character may support content; it may not reduce contrast, obscure hierarchy, or invent an affordance.
2. **Action before decoration.** Home leads with resume/search/privacy status; panels lead with the task they exist to complete.
3. **One concept, one name, one control.** Search, Ask, voice, capture, and privacy must not have competing shortcuts or unexplained duplicate entry points.
4. **Truthful locality.** “On-device,” “ephemeral,” “stored,” and “no network” appear only where the current path actually guarantees them.
5. **Progressive disclosure.** Human-readable memory first; provenance, raw evidence, graph mechanics, scores, and debug payloads on demand.
6. **Accessible by default.** Keyboard, zoom, contrast, visible focus, reduced motion, and useful labels are release criteria, not cleanup.
7. **System feedback is part of the design.** Empty, loading, denied, partial, stale, offline, degraded, long-content, and failure states receive the same attention as populated success.

### Typography

- Use **Inter** for controls, body copy, forms, data, and dense panels.
- Reserve **Cormorant Garamond** for a small number of editorial/display moments, never form labels, placeholders, status, or long operational text.
- Do not keep **EB Garamond** as a competing default body face; retain it only for a deliberately tested long-form editorial use.
- Use **Cutive Mono** only for technical identifiers, shortcuts, timestamps, and evidence labels where monospacing adds meaning.
- Do not use all-caps plus wide tracking below 12 CSS px. Body text starts at 16 CSS px; supporting text starts at 14 CSS px.
- Text reflows at 200% without truncating meaning; truncation has an accessible full-value path.

### Color and themes

- All product UI uses semantic tokens for background, raised surface, primary/secondary text, border, focus, accent, success, warning, and danger.
- Contrast is measured against the *composited* background, including wallpaper and translucency: WCAG AA 4.5:1 for normal text, 3:1 for large text and UI/focus indicators.
- Light and dark modes have equal information and state coverage. Status never depends on color alone.
- Foreground task surfaces receive an opaque or sufficiently strong translucent layer; the wallpaper is ambience, not the text background.
- Motion/grain/parallax are off when `prefers-reduced-motion: reduce` or the preview/native preference requests motion off. No continuously updating WebGL loop remains active in that mode.

### Interaction rules

- Primary icon targets are at least 44 × 44 CSS px; every icon-only action has a stable accessible name and tooltip where useful.
- Dialogs/drawers receive initial focus, contain focus where modal, close on Escape when safe, and restore focus to the invoker.
- Destructive actions name the object and consequence and use an in-product confirmation rather than `window.confirm` in the finished UI.
- Scrollable regions show an affordance and work with wheel, trackpad, keyboard, and assistive technology.
- Cmd/Ctrl+K has one owner: the command palette. Search gets a separate documented shortcut if one is retained.

## Information architecture

### Confirmed current main-window IA

- Startup gates: Onboarding → optional biometric lock → workspace.
- Persistent shell: Home, sidebar, theme, Settings, banners, toasts, command palette.
- Mounted destinations: Memory Vault; Search & Ask FNDR; Daily Summary; Stats (panel heading: Activity Stats); To-dos; FNDR Wrapped; Screen Guide; Engine diagnostics; Privacy Activity.
- Search results render in Timeline; selecting one establishes memory context for the command palette.

### Recommended target IA — human decision required

Keep five user-facing destinations and move developer/secondary views behind them:

1. **Home / Resume** — continue recent work, current capture/privacy state, and universal query.
2. **Search & Ask** — one query surface with explicit Search and Answer outcomes, citations, filters, and recent searches.
3. **Memory Vault** — browse/review human-readable memories; graph becomes an optional insight view.
4. **Daily Brief** — combine useful parts of Daily Summary, To-dos, Stats, and Wrapped into day/week reflection and follow-up workflows.
5. **Trust & Settings** — capture, Privacy Activity, alerts, blocklist, local models, profile, appearance, and explicit developer mode.

Screen Guide remains a distinct, user-invoked assistant. Engine diagnostics moves under developer mode. This recommendation aligns with the master plan's Home/Resume, Vault/Search, **Privacy Proof** (the historical plan label), Intelligence, Approval Queue, and Settings direction without claiming those future features are implemented.

## Surface inventory and disposition

| Class | Surfaces | Current evidence | Disposition |
| --- | --- | --- | --- |
| Startup reachable | Onboarding; biometric lock | Rendered before workspace based on persisted onboarding state. | Redesign and native-test. |
| Shell reachable | HomeHero; SearchBar/Timeline; sidebar; command palette; ControlPanel/PrivacyPanel; model/recording banners; toasts | Mounted from `App.tsx`. | Redesign in place; remove duplicate behavior. |
| Main panels reachable | Ask, Memory Vault, Daily Summary, Stats (panel heading: Activity Stats), To-dos, Wrapped, Screen Guide, Engine diagnostics, Privacy Activity | Mounted by `AppPanels.tsx`; present in sidebar/palette and enumerated by `MOUNTED_PANEL_KEYS`. | Keep until IA decision; then merge or relocate deliberately. |
| Auxiliary reachable | Omnibar (`omnibar.html`), Autofill (`autofill.html`), Screen Guide overlay (`screen-guide.html`) | Separate Vite/Tauri entries; invoked by native events/shortcuts, not main navigation. | Keep only with shared tokens, state tests, and native evidence. |
| Partially reused | 2D KnowledgeGraph components | Embedded graph strip can render in Vault; full graph route is not mounted. | Preserve the used strip while deciding the full graph's role. |
| Compiled, unmounted | AgentPanel, AutomationPanel, MeetingRecorderPanel, FocusModePanel, FocusSessionPanel, TimeTrackingPanel, SearchHistoryPanel UI, ResearchPanel, QuickSkillsPanel, GlassesImportPanel, PipelineInspectorPanel, CompanionDevicesPanel, full Knowledge Graph/3D view | Files compile; no `AppPanels` mount. SearchHistory's append helper is used; Engine diagnostics' optional Inspector callback is not passed. | **Decision:** ship, merge, developer-only, defer, or delete. Do not visually polish before classification. |
| Historical/unmounted destination keys (runtime guarded) | `meeting`, `knowledgeGraph`, `searchHistory`, `focusSession`, `timeTracking`, `focusMode` remain in the broader `PanelKey`/command registry for historical code, but not in the Alpha mounted registry or visible palette. Meeting/graph voice requests are rerouted; focus proactive actions are removed/rerouted. | `isMountedPanelKey` and the typed sidebar prevent them from becoming the foreground. Focused registry/command tests passed. | **Resolved for reachable Alpha controls.** Keep the runtime guard until UX-12 classifies or deletes the historical surfaces; complete browser traversal remains an integration check. |

## Interaction and surface audit ledger

The ledger is both the current coverage map and the template for future controls. The columns encode the five Ws: **Who** is “intended user,” **What** is “control,” **Why** is “purpose/result,” and **Where/When** is “entry/context.”

Evidence codes: **B** = browser fixture can verify; **N** = native macOS evidence required; **B+N** = both. “Pending” means the path exists but has not passed the full matrix in this program.

### Startup, shell, and primary workflows

| Route / entry and foreground/background | Control(s) — what | Intended user — who | Purpose, context/timing — why/where/when | Result and required states | Accessibility requirement | Evidence | Issue / status |
| --- | --- | --- | --- | --- | --- | --- | --- |
| Startup → Welcome; foreground gate | Name field; **Get Started**; **Skip for now** | First-run user | Personalize greeting before any capture starts | Saved/blank name; loading/save error; next step | Labeled field, Enter parity, focus order, 200% reflow | B+N | UX-02 / truth copy + save failure implemented; browser/native pending |
| Startup → Biometrics; foreground + native auth | **Enable Touch ID Lock**; **Skip for now** | Privacy-conscious user | Choose app-open protection before storing memories | waiting, success, cancel, unavailable, failure; skip is explicit | Modal semantics; no focus escape; status announced | B+N | UX-01 / frontend fail-closed; native identity deferred |
| Startup → Privacy promise; foreground | **Continue**; current **Skip for now** | First-run user | Explain actual storage/network/privacy contract before capture | accepted vs declined/deferred must be distinct; no unsupported claims | Headings/list semantics; no emoji-only meaning | B+N | UX-02 / truthful copy implemented; consent distinction pending |
| Startup → Local models; foreground with background download | Model cards; **Download/Use model**; **Skip** | First-run user | Provision required search model and optional richer model | registry loading, disk preflight, progress, resume, checksum/failure, activate, skip/degraded | Progress has name/value; logs scroll; no focus loss | B+N | UX-02 / proposed |
| Startup → Permissions; foreground opens System Settings | Per-permission **Grant**; **Open FNDR**; current **Skip** | First-run user | Request Screen Recording; optionally Accessibility/microphone, at the moment each feature needs it | denied, partial, granted, revoked, returned-from-Settings; required rule consistent | Status not color-only; disabled reason announced | B+N | UX-02 / proposed |
| Startup → Lock; foreground + native auth | **Unlock FNDR / Try again** | Returning protected user | Authenticate before any memory UI is exposed | waiting, cancel, unavailable, failed, success; never bypass to workspace | True modal, busy/alert announcement, focus restore after native sheet | B+N | UX-01 / in progress |
| Shell top-left; foreground | Sidebar **open/close**; scrim **close** | Any user | Navigate when needed without permanent clutter | open/closed; current destination; narrow/zoom layout | 44px target, expanded state, Escape, focus restore | B | UX-04 / implemented + focused-test verified; browser matrix open |
| Sidebar → Home | **Home** | Any user | Return to default resume/search state | query/panel cleared intentionally; no data mutation | Current-page state and keyboard focus | B | UX-04 / implemented + focused-test verified |
| Sidebar → Memory | **Memory Vault**; **Search & Ask FNDR** | User recalling work | Browse or query captured memories | panel opens or returns a visible error; never blank | Buttons named; current destination exposed | B+N | UX-04/06/07 / implemented + focused-test verified; native actions pending |
| Sidebar → Reflect | **Daily Summary**; **Stats**; **To-dos**; **FNDR Wrapped** | User reviewing work | Day/week insight and follow-up | selected panel opens; empty/degraded states explain next action | Same close/nav model; 200% usable | B+N | UX-09 / implemented + focused-test verified; IA/browser/native decisions pending |
| Sidebar → Assist | **Screen Guide**; **Engine diagnostics**; **Privacy Activity** | User seeking live help/trust; developer for diagnostics | Configure guide, inspect runtime, review bounded privacy activity | mounted panel opens; audience is clear | Consistent labels and close action | B+N | UX-04/08/10 / implemented + focused-test verified; native proof pending |
| Sidebar / global Cmd+K; foreground over current context | **Cmd+K Palette**; query; Home, Vault, Search & Ask, Daily Summary, Stats, To-dos, Wrapped, Screen Guide, Engine diagnostics, Privacy Activity, Pause or Resume; confirmation **Cancel/Confirm** | Keyboard user | Navigate or issue a bounded command without leaving context | one shortcut owner; filtered/empty/running/success/error/confirmation; only valid/state-correct commands shown | Combobox/listbox semantics, arrows, Enter, Escape, focus trap/restore | B+N | UX-04 / implemented + focused-test/browser-layering verified; full path matrix open |
| Shell top-right | Theme toggle | Any user | Switch light/dark at any time | all entry points update/persist with no flash; errors do not strand mixed themes | Dynamic accessible name; focus visible; not color-only | B+N | UX-03 / in progress |
| Shell top-right → Settings | Settings toggle, backdrop, **Close** | Any user | Open profile/capture/privacy/model controls | open/close, alert badge, partial-load/error per section | Drawer focus management, Escape, visible scrolling | B+N | UX-03/08 / implemented + focused/browser verified; native controls pending |
| Shell conditional foreground | Model banner **Download/Load** | User in degraded model state | Explain capability loss and repair it | preparing/downloading/finalizing/activating/failure/success | Named progress, readable logs, no overlay clipping | B+N | UX-02/08 / proposed |
| Shell conditional foreground fed by background events | Recording banner; toast card/action/**Close** | User receiving background status | Keep recording/suggestion/privacy work visible and actionable | action target passes the mounted registry; timeout/persistent policy; error; dismiss | `aria-live` without duplicate announcements; focusable action | B+N | UX-04 / mounted-target guard implemented; native event evidence pending |
| Home | Hero search field; voice **Speak/Stop**; submit arrow | Returning user | Recall/resume work immediately | idle/typing/recording/transcribing/denied/error/submitted; time copy consistent | Labels, Enter, visible focus, contrast, reduced motion, 200% | B+N | UX-05 / in progress; voice decision |
| Search results workspace | Search textarea; voice; **Clear**; `@memory` choices; time/app selects | User with a known or fuzzy query | Refine local retrieval after a query | typing/pending/searching/results/none/error/disabled/voice states | Search landmark, listbox semantics, named selects, no Cmd+K conflict | B+N | UX-06 / proposed |
| Timeline | Result card; evidence `<details>`; **Load more** | User comparing results | Inspect a result, provenance, and additional matches | loading/empty/error/populated/selected/long result | Card keyboard activation, proper button semantics, no nested conflicts | B | UX-06 / proposed |
| Ask panel | **Close**; question; **Search & Ask**; example buttons; cited-memory buttons | User asking a natural-language question | Compose an evidence-grounded answer from local memories | idle/asking/answer/partial/refusal/timeout/error; citations open Vault | Dialog focus/restore; live status; readable long answer at 200% | B+N | UX-06 / component implementation verified; browser answer/citation path open |

### Vault, reflection, trust, and assistance

| Route / entry and foreground/background | Control(s) — what | Intended user — who | Purpose, context/timing — why/where/when | Result and required states | Accessibility requirement | Evidence | Issue / status |
| --- | --- | --- | --- | --- | --- | --- | --- |
| Memory Vault list | **Close**; **Excluded captures**; app/time/source filters; memory rows; embedded connected-context nodes | User browsing/reviewing memory | Browse by understandable filters; review excluded captures separately | loading/empty/error/populated/filter-empty/review queue/long content/graph unavailable | Tab/select names reflect purpose; row keyboard parity; visible scroll | B+N | UX-07 / implemented + focused-test verified; browser/native action sweep open |
| Vault expanded memory | Backdrop/**Close**; **Copy for Agent**; **Open source** when available; **Find/Hide similar**; **Delete** + confirmation | User validating or reusing one memory | See human summary first, then evidence/provenance/actions on demand | related/evidence loading, copy success/error, source unavailable, similar empty/error, delete success/error | Focus trap/restore; named destructive action; no raw JSON by default | B+N | UX-07 / implemented + focused-test verified; native clipboard/reopen pending |
| Daily Summary | **Close**; previous/next/**Today**; date picker; **Generate**; **Download PDF**; follow-up expand/done/open memory/add task; PDF **Open/Dismiss** | User ending/reviewing a day | Generate a selected-date brief and carry current global follow-ups forward | no data/loading/error/stale-request/summary/follow-up states/export success/error | Date controls named; checkbox state; export status announced; 200% reflow | B+N | UX-09 / implemented + focused-test verified; browser controls/native PDF pending |
| Activity Stats | **Lay Out All/Stack Cards**; metric cards; **Close** | Curious user or evaluator | Understand activity and signal health, not vanity telemetry | loading/refresh-error/data/zero-data; grid/stack; stacked-card selection | Charts have text alternatives; stacked cards support Enter/Space; grid cards are not false buttons | B+N | UX-09 / implemented + focused-test verified; audience/browser decision pending |
| To-dos | **Close**; title/type/**Add**; To-do/Reminder/Follow-up/All tabs; per-task **Edit/Cancel/Save/Done** | User carrying work forward | Create, classify, edit, and complete local tasks | loading/list-error/action-error/empty/filter-empty/create/edit/save/done; failed edits remain | Tabs expose selected state; fields labeled; errors tied to action | B+N | UX-09 / implemented + focused-test verified; default-tab opinion/browser pass pending |
| Wrapped | **Close**; month scope; week choices; **Start**; **Change week/Update recap/Export**; empty **Choose another week**; PDF **Open/Dismiss** | User reflecting/exporting a week | Review recorded patterns for a chosen bounded period | selection/loading/error/empty/results/exporting/export success/open error | Radio/pressed semantics; dynamic updates announced; data not color-only | B+N | UX-09 / implemented + focused-test verified; merge/history/browser/native PDF decisions pending |
| Settings → Profile/Capture | Profile field/**Save**; **Pause/Resume capture** | Any user | Personalize and take immediate, durable privacy control | unchanged/saving/saved/error; active/paused/degraded; persisted pause feedback | Persistent feedback; no ellipsis-only busy label; status announced | B+N | UX-08 / implemented + focused/browser verified; native relaunch pending |
| Settings → Privacy | Alert **Add to Blocklist/Dismiss**; blocklist per-item **Remove**; new pattern/**Add** | Privacy-conscious user | Respond to detected sensitive context and manage future exclusion | empty/loading/error/alert, deletion consequence, add/remove success/error | Confirm destructive deletion; alert relationships; keyboard-complete | B+N | UX-08 / proposed |
| Settings → Local Models | Readiness list (currently read-only) | User diagnosing capabilities | Explain which local capabilities are available and what is paused | checking/ready/missing/degraded/error | Status text, not color alone; direct repair link if actionable | B+N | UX-08 / proposed |
| Privacy Activity (internal component/IPC names still use `PrivacyProof`) | **Close** | User/evaluator reviewing bounded privacy activity | Inspect current-app-session evaluated/stored/skipped counts and recorded, explicitly incomplete egress activity | loading/error/data/zero counts; reset timing and telemetry omissions stated | Definition text near metric; headings and list semantics | B+N | UX-08 / implemented + focused-test verified; browser/native counter validation pending |
| Engine diagnostics (historically “Engine Metrics”) | **Close**; optional Inspector button is currently not mounted | Developer/evaluator | Diagnose performance without presenting best-effort internals as consumer value | loading/error/data/stale/zero/retry | Charts/tables have text; developer/best-effort label | B+N | UX-08 / implemented + focused-test verified; placement/browser decision pending |
| Screen Guide panel | **Close**; enable; shortcut/**Save**; speak; cursor; question/**Ask**; **Hold/Release to talk** | User needing help with current display | Configure once; read the current main display only when intentionally invoked; never click/type/store the turn | off/ready/listening/transcribing/thinking/answer/denied/busy/listener-error/load-error | Checkboxes labeled; hold works by keyboard; state announced; retry and reduced motion | B+N | UX-10 / implemented + focused-test verified; browser text/native permission paths pending |
| Screen Guide overlay native window | Answer/status and optional point cue; no click action | Same user, while another app stays foreground | Show a concise answer without stealing focus or controlling the target app | listening/transcribing/thinking/answer/error/dismissed; cue absent when ungrounded | Click-through, non-focusable, reduced-motion appearance, text answer always available | N (B for visual states only) | UX-10 / proposed |

### Auxiliary windows and background-triggered paths

| Route / entry and foreground/background | Control(s) — what | Intended user — who | Purpose, context/timing — why/where/when | Result and required states | Accessibility requirement | Evidence | Issue / status |
| --- | --- | --- | --- | --- | --- | --- | --- |
| Global Omnibar native window | Query; Memory/Clips toggle; result/citation buttons; keyboard open/ask/copy/paste/close | Keyboard-first user in another app | Recall or reuse context without opening the main window | searching/empty/results/asking/answer/error/copied/pasted | Labeled toggle, listbox/option correctness, focus on open, theme parity | B+N | UX-11 / proposed |
| Autofill native window | **Dismiss**; query/**Search or Insert**; candidate choices; **Insert Selected**; **Dismiss** | User filling a focused field | Rank memory-derived candidates and insert only after appropriate confidence/confirmation | bootstrap/manual/searching/preview/injecting/done/error; field context changed | Non-modal semantics accurate; keyboard 1–9/arrows/Enter/Escape; no raw fixed theme | B+N | UX-11 / proposed |
| Proactive suggestion event | Toast **Open Daily Summary/Review Tasks/Review the day** or no action for focus drift | User receiving a timely local suggestion | Move from background detection to a relevant foreground workflow | mounted target or deliberately informational-only; dedupe; dismiss; stale event | Non-interruptive live region; action label predicts destination | B+N | UX-04 / dead destination resolved in source; native event evidence pending |
| Voice command from Search | Speak/Stop can search, clear, search meeting memories, or open Vault connected context | Voice user | Hands-free query/navigation after explicit microphone use | heard/unknown/permission denied/transcription error; only mounted destinations | Text feedback and keyboard alternative | B+N | UX-04/06 / dead destinations resolved in source; native microphone/voice decision pending |

Any new control must add or update one ledger row before it can be called complete.

## Browser fixture versus native permission boundary

### Browser fixture may prove

- Real React component composition, CSS, theme tokens, layout, responsive reflow, typography, focus order, keyboard behavior, labels/roles, visible states, and console/runtime errors.
- Deterministic synthetic IPC states: populated, empty, loading, error, long content, denied permission, paused capture, missing model, alerts, partial/refusal, and success.
- Screenshot comparisons with motion disabled and known time/name/data.
- That every mocked foreground action invokes the expected IPC command and that unknown commands fail loudly.

The implemented preview contract is:

```text
/ui-preview.html?theme=<light|dark>&palette=<film|paper|matrix|...>&motion=<off|on>
```

The preview is development-only, contains no real user data, is not a Rollup production entry, and never silently returns a generic value for an unknown command. Its current fixture is deterministic and mutable within a page session; named multi-scenario URL fixtures remain a future expansion, not a completed claim.

### Browser path verification log — integration pass still open

Only interactions actually exercised in the localhost preview belong in the “verified evidence” column. The integration owner should update this table as the remaining paths run; **Pending** is intentionally not inferred from unit tests or fixture coverage.

| Path | Preview configuration / interaction | Verified evidence so far | Remaining browser work | Native boundary |
| --- | --- | --- | --- | --- |
| Shell, sidebar, and Cmd+K | Film dark/light, motion off; desktop, 720 × 600, 640 px, and 360 px samples | Sidebar stayed compact; every mounted destination was reached through the sidebar or palette. Cmd+K order, query filtering, Enter activation, Pause confirmation/cancel/confirm, Resume, Escape, feedback, and launcher-focus return were exercised. The 360 px results pass exposed a real shell/search collision; the final pass measured no menu/search, settings/search, or settings/Clear intersection, and Clear was clickable. | Finish the formal V1–V5 screenshot ledger. | Native minimum-window behavior and global shortcut ownership remain unproven. |
| Home → Search results | Film dark/light, motion off; 640 px and 360 px; query `accessibility` | Purpose copy, submit, populated and empty results, time/app filters, result selection, Clear, long-query scrolling, count grammar, and narrow reflow were exercised; three synthetic results completed synthesis. Final 360 px dark/light runs had no page-level horizontal overflow; a light-theme voice-glyph contrast defect found visually was corrected to inverse ink. | Browser-injected failure and long-result stress remain. | Home Speak/microphone was deliberately not invoked; real transcription and latency are native-only. |
| Settings | Film dark/light, motion off; desktop and 720 × 600 representative runs | Open/close, Escape/focus return, profile save, Pause/Resume and durable-state copy, blocklist add/remove, read-only model readiness, and announced success feedback were exercised with synthetic IPC. | Browser-injected save/blocklist/model failures and final 200% reflow remain. | Real pause/relaunch, block exclusion, model process, and System Settings require Tauri/native evidence. |
| Memory Vault | Film dark, motion off; populated mutable fixture | App/time/activity filters, Excluded captures, expanded detail, connected/related context, related navigation, Find/Hide similar, browser copy feedback, virtual Open source, and both cancel/confirm delete were exercised. | Injected loading/empty/error/long-content states and final light/200% keyboard trace remain. | Native clipboard and source reopening still require Tauri evidence; the preview opened no external app. |
| Ask | Film dark, motion off; grounded preview-permission question | Initial focus and dialog behavior passed; a grounded answer named one source, and its citation opened the corresponding Vault context. | Refusal, timeout/error, long answer, and final light/200% pass remain. | Real local-model latency/failure requires native evidence. |
| Daily Summary | Film dark, motion off; current and previous day | Generate, date navigation, global-follow-up expansion, Done/Undo state, Open related memory, Add to tasks, virtual PDF export, Open, and Dismiss were exercised. | Injected stale/error/empty/long-content and final light/200% passes remain. | The preview stayed on localhost and wrote no PDF; real creation/open requires native evidence. |
| Layered modal and background | Daily Summary + Cmd+K; Vault open | Cmd+K became the topmost modal; first Escape closed only the palette, second closed Daily Summary, focus returned to the sidebar launcher, and Home was inert behind Vault. | Repeat with each mounted panel, Settings, and destructive confirmations. | Native multi-window focus remains separate. |
| Console | Fresh no-reload main-shell final session plus focused auxiliary sessions | Main shell produced only the React development-tool informational line: no errors or warnings while exercising 360 px dark/light Search, Settings modal isolation, and Privacy Activity focus/Escape. Earlier deliberate-reload mock callback warnings and the pre-fix React `inert` warning are retained as historical diagnostics, not counted as final evidence. Omnibar and Autofill focused sessions also reported zero console errors. | Repeat after any future shell/fixture change. | Browser console cannot validate Rust/native logs. |
| Stats | Film dark, motion off; populated fixture | Grid data, Stack Cards, card-to-front selection, and Lay Out All were exercised with text alternatives present for charts. | Injected loading/refresh-error/zero states, final light/200% pass, and the product decision on removing stacked mode remain. | Real counters must be compared with a Tauri session. |
| To-dos | Film dark, motion off; mutable task fixture | Add with type, every task filter, edit/cancel/save, completion, count updates, and persistent feedback were exercised. | Injected list/action failures, empty filters, final light/200% pass, and default-tab decision remain. | Real persistence/restart proof is native. |
| Wrapped | Film dark, motion off; month/week selection | This/last month, week radios, Start, Update, Change week, virtual export/Open/Dismiss, projects, documents, and follow-up totals were exercised. | Empty/error/history depth and final light/200% pass remain. | The preview wrote/opened no file; native PDF behavior remains. |
| Screen Guide | Film dark, motion off; settings plus typed question | Enable off/on, shortcut save, speak/cursor options, typed Ask, and explicit “Preview-only response ready; no screen was captured” status were exercised. | Injected listener/load/retry paths and final reflow remain. Hold-to-talk was deliberately not invoked. | Screen capture, mic, global shortcut, overlay, cue, click-through, and multi-display are native-only. |
| Engine diagnostics and Privacy Activity | Film dark plus final narrow Film light, motion off; populated fixtures | Developer framing, best-effort metrics, current-session privacy counts, skip reasons, and incomplete-egress caveat rendered. Browser audit found missing Privacy Activity dialog semantics; the focused fix added a named modal, 44 px close target, containment, Escape, and restore. The final 360 px pass verified initial close focus, one-control focus loop, no horizontal overflow, inert background, Escape close, and sidebar-launcher restoration. | Injected stale/error/zero/retry states remain. | Validate counters/metrics against a real Tauri session. |
| Omnibar and Autofill | Matrix dark and Film light; 680 × 480, 500 × 430, and 320 px narrow | Omnibar search/listbox/keyboard/answer/edit/copy/open and its browser-safe failures were exercised. Autofill waiting/no-context and responsive review surfaces were exercised. Shared auxiliary appearance followed the main theme; focused sessions reported zero console errors. | Native-only event entry and real-app return paths remain; continue visual regression snapshots if the canonical palette changes. | Global shortcut, placement, clipboard/paste, Accessibility context/injection, changed-target race, and hidden-webview resync are native-only. |

Local evidence artifacts are intentionally ignored by Git. Final main-shell narrow screenshots: `.playwright-cli/page-2026-09-23T01-44-53-330Z.png` (Film dark) and `.playwright-cli/page-2026-09-23T01-46-32-858Z.png` (corrected Film light). Auxiliary examples: `.playwright-cli/page-2026-09-23T01-24-58-030Z.png` (Omnibar native-size dark), `.playwright-cli/page-2026-09-23T01-27-31-843Z.png` (Autofill light), and `.playwright-cli/page-2026-09-23T01-27-48-030Z.png` (Autofill narrow).

### Native evidence is mandatory for

- FNDR-branded LocalAuthentication / password fallback and lock persistence.
- Screen Recording, Accessibility, and microphone grant/deny/revoke/re-grant flows.
- Real background capture, pause/resume, blocked-context exclusion, and event delivery.
- Global shortcuts, multi-window focus, auxiliary window placement, and Screen Guide click-through behavior.
- Audio capture/transcription watchdog behavior, source reopening, clipboard/paste injection, file export/open, model download/activation, and app restart persistence.

Native QA is a short, human-operated pass. Automation must not grant broad OS permissions, capture unrelated user content, or treat a dev binary prompt as release-identity proof.

## Acceptance criteria

The checkboxes below remain whole-program release gates. A checkpoint row marked implemented does not check a box until its complete browser/native evidence and final current-tree integration commands pass.

### Accessibility and readability

- [ ] Normal text has at least 4.5:1 composited contrast; large text, controls, focus rings, and meaningful graphics have at least 3:1.
- [ ] Every reachable control is usable without a pointer, has a meaningful accessible name, and shows a visible focus indicator.
- [ ] Modal/drawer focus is contained and restored; Escape behavior is consistent and never performs a destructive action.
- [ ] At 200% browser zoom, all core workflows remain reachable with no lost content or unintended page-level horizontal scroll; intended graph canvases may pan inside a clearly bounded region.
- [ ] At 720 × 600 CSS px, primary actions remain visible or are reachable through an obvious scroll region.
- [ ] Reduced-motion mode disables wallpaper animation, parallax, repeating pulses, travel animation, and nonessential transitions without removing information.
- [ ] Loading, success, error, denied, empty, paused, stale, and degraded states are announced and not conveyed by color alone.

### Consistency and product truth

- [ ] Light/dark choice, semantic tokens, typography, and focus styling match across main, Omnibar, Autofill, and Screen Guide entries.
- [ ] No visible action targets an unmounted panel; a reachability test covers every sidebar, palette, toast, and voice destination.
- [ ] Home, Search, and Ask have explicit roles and do not compete for the same keyboard shortcut.
- [ ] User-facing copy contains no raw filename/bundle ID/meta-narration unless deliberately expanded as evidence.
- [ ] Counts use the domain term **memory**, not **card**, except internal component documentation.
- [ ] Privacy/locality statements match code and ADR scope, including exceptions for model/update downloads and configured external providers.
- [ ] Background capture, recording, download, and inference states always have a visible foreground status and recovery path.

### Engineering and evidence

- [ ] Focused component tests cover each changed stable boundary; `npm run typecheck`, `npm test`, and `npm run build` pass.
- [ ] `make test` passes at each release-sized integration checkpoint.
- [ ] Browser evidence includes URL/configuration, viewport, theme, screenshot, keyboard trace, console result, and issue/commit.
- [ ] Native evidence includes build identity, macOS version, permission state, exact workflow, expected/actual result, and a redacted screenshot or recording.
- [ ] No fixture or evidence artifact includes a real capture, secret, database blob, or ignored app-data content.

## Scenario, viewport, theme, and state matrix

### Canonical presentation matrix

| ID | Viewport / mode | Themes | Required checks |
| --- | --- | --- | --- |
| V1 | 1280 × 900, 100% | light + dark | Primary desktop composition, long panel, visual baseline |
| V2 | 900 × 700, 100% | light + dark | Typical constrained Tauri window, keyboard-only pass |
| V3 | 720 × 600, 100% | light + dark | Minimum supported layout candidate; scroll and action reachability |
| V4 | 1280 × 900 at 200% zoom | light + dark | Reflow, text clipping, focus visibility, no lost controls |
| V5 | 900 × 700, reduced motion / `motion=off` | light + dark | Static wallpaper, no parallax/continuous motion, stable screenshots |

The product owner must confirm the supported minimum native window size. Until then, V3 is a design target, not a release-support claim.

### Required state coverage (current fixture plus planned scenario expansion)

| Scenario ID | Surfaces | States |
| --- | --- | --- |
| `startup` | Onboarding, lock | welcome; long name; auth waiting/cancel/unavailable/success; model loading/downloading/failure/success; permissions denied/partial/granted |
| `home` | Shell, Home, Settings | capture active/paused; model ready/missing; zero/one/many alerts; recording banner; long greeting; toast with/without action |
| `search` | Home search, SearchBar, Timeline, Ask | idle; typing; loading; empty; results; long result; error; voice denied; answer; partial; refusal; timeout |
| `vault` | Vault, expanded memory | loading; empty; error; populated; needs review; long/raw content; expanded; copy/reopen/similar/delete outcomes; graph empty/error/populated |
| `reflect` | Daily Summary, Stats, To-dos, Wrapped | zero data; loading; error; typical data; long data; edit/export success and failure |
| `trust` | Settings, Privacy Activity, model banner | no alerts; alert; block add/remove; capture degraded; activity zero/populated/error; model progress/failure |
| `guide` | Screen Guide panel/overlay | off; ready; listening; transcribing; thinking; answer with/without cue; permission denied; blocked context; error |
| `aux` | Omnibar, Autofill | no results; results; long clipboard item; answer; copy/paste success/error; autofill manual/preview/injecting/done/error |

Run all core success states through V1–V5. Run failure/destructive/native-only states at V2 plus the relevant native configuration; do not create a meaningless full Cartesian product.

## Larger revamp recommendations and human decisions

These decisions do not block truth, dead-control, contrast, keyboard, or reduced-motion fixes.

| Decision | Recommendation | Why an opinion is needed | Default if unanswered |
| --- | --- | --- | --- |
| D-01 Primary IA | Adopt Home/Resume, Search & Ask, Vault, Daily Brief, Trust & Settings; keep Screen Guide distinct. | Changes sidebar labels and whether four current reflection panels remain top-level. | Implement accessibility within current IA; do not delete panels. |
| D-02 Visual identity | Make warm **Film/Paper** the canonical pair; move cinematic palettes to optional customization. | Current source defaults to Matrix while design docs specify warm film/paper. | Use film dark/light in fixtures and new components. |
| D-03 Wallpaper | Static/subtle by default; motion is opt-in and always behind a readable foreground surface. | The cinematic wallpaper is distinctive but can compete with content and GPU/battery goals. | Keep it, motion-off in tests and for reduced-motion users. |
| D-04 Voice scope | Put voice primarily in Screen Guide; keep search voice only if user testing shows value. | Home, Search, and Screen Guide currently duplicate microphone interactions and permission expectations. | Do not add more voice entry points; repair dead voice commands. |
| D-05 Memory graph role | Make graph an optional Vault insight view, not persistent chrome. | It may be differentiating, but current labels/mechanics are not self-explanatory. | Keep the embedded strip behind progressive disclosure; do not promote full 3D. |
| D-06 Reflection surfaces | Merge Summary + follow-ups + useful Stats into Daily Brief; make Wrapped a period/export view. | A merge changes existing navigation and presentation commitments in the master plan. | Improve current panels separately, then measure usage. |
| D-07 Developer surfaces | Put Engine diagnostics and any approved Pipeline Inspector under explicit Developer Mode. | Their current audience differs from normal recall workflows. | Keep Engine diagnostics reachable but label its audience. |
| D-08 Orphaned features | Classify each as ship/merge/developer-only/defer/delete before visual work; default to deletion only after reachability/import/IPC checks. | Some contain real capability; others are historical prototypes with maintenance cost. | Leave unmounted and stop adding triggers. |
| D-09 Biometrics | Keep as opt-in, fail closed, implemented in-process under FNDR identity. | Product owner must decide whether it is offered during onboarding or later in Trust & Settings. | Keep opt-in onboarding choice; no bypass after enabled. |
| D-10 Minimum window | Support at least 720 × 600 and 200% zoom, or publish a larger enforced minimum with rationale. | Determines panel composition and native window constraints. | Design/test for 720 × 600; do not claim support until native proof. |
| D-11 To-dos landing filter | Open on **All** rather than one task type. | Users otherwise encounter an apparently incomplete list before understanding the type tabs. | **Recommended:** default to All; preserve the user's later choice only after a persistence decision. |
| D-12 Stats card deck | Keep the readable grid and remove the stacked-card mode unless user research validates it. | The deck adds interaction and occlusion without yet proving a decision-making benefit. | Grid remains the default; keep stacked mode experimental until measured. |
| D-13 Daily follow-up scope | Associate follow-ups with dates/memories in the backend, or keep the current “Across your task list” disclosure. | The UI cannot honestly claim historical day-specific follow-ups without a data relationship. | Keep global follow-ups and the explicit label; do not infer history. |
| D-14 Wrapped history | Add explicit older-period navigation only if weekly/monthly reflection is retained as a separate destination. | The current bounded picker works for recent periods but does not define a durable history product. | Do not add another history store; revisit after D-06 decides whether Wrapped remains separate. |
| D-15 Sensitive-context policy | Keep known banking, medical, authentication, private-browser, password-manager, blocklist, self-app, and detected-secret contexts blocked by default; any future allowlist must be high-friction and explicit. | This changes the balance between recall completeness and privacy risk. | Continue fail-closed whole-frame exclusion; expose bounded skip counts, not sensitive context values. |
| D-16 Secret handling granularity | Treat whole-frame exclusion as the current safety baseline; consider span/region redaction only as a separately threat-modeled feature. | Redaction might preserve useful context but creates false-negative and leakage risk across OCR, images, embeddings, logs, and exports. | Keep whole-frame skip. Do not market selective redaction until every downstream artifact is covered and native-tested. |
| D-17 Autofill sensitive values | Mask candidate values until revealed and require confirmation for identifiers or sensitive fields regardless of confidence. | This trades one-click speed for clearer intent at the moment data crosses into another app. | Keep confirmation mandatory; do not auto-inject a sensitive candidate. |
| D-18 Autofill permission recovery | Add an explicit **Open Accessibility Settings** action beside the current explanation. | Browser QA can design this state but only native macOS can complete the permission handoff. | Keep the truthful blocked state; add the native deep link in a bounded follow-up. |
| D-19 Quick Find structure | Keep Memory and Clips in one Tab-cycling Omnibar until task testing shows distinct entry points are faster. | Separate shortcuts/windows increase discoverability but also multiply mental models and global-shortcut conflicts. | Retain one named **FNDR Quick Find** surface with visible mode/context. |
| D-20 Search selection outcome | Open a dedicated memory detail sheet only if users expect card activation to inspect rather than select context for commands. | The current action truthfully selects memory context, but the visual card may imply drill-in navigation. | Keep explicit Select/Selected controls; test the expectation before adding another detail surface. |
| D-21 Voice confirmation | Require transcript review before any voice query or command executes, including the active Search workspace. | Home currently returns focus for review while active Search can auto-submit, creating inconsistent permission and error consequences. | Use confirmation-first everywhere; do not add more voice automation until the policy is unified. |

### ADR candidate

If UX-01 replaces the Swift subprocess as recommended, record **“In-process biometric authentication under FNDR identity”** as an ADR (or an amendment to an existing release/security ADR): chosen LocalAuthentication binding, signed-process identity, device-owner policy, fail-closed behavior, persistence boundary, and native evidence requirements. The visual overhaul itself does not otherwise change persistence or public IPC contracts.

## Ordered vertical slices

Status values are evidence states, not percent-complete estimates. “Owner” names the next-action owner, not code authorship.

| Order / issue | User-visible outcome | Scope and stable boundary | Reuse / delete guidance | Verification and evidence | Dependencies | Owner | Status |
| --- | --- | --- | --- | --- | --- | --- | --- |
| UX-00 Browser evidence harness | The real shell opens with deterministic, non-sensitive scenarios and no OS permissions. | Dev-only `ui-preview.html`, strict mock IPC, theme/palette/motion parser; no production entry. | Reuse official Tauri `mockIPC`; never add catch-all success responses. Ignore generated screenshots. | 13/13 fixture tests passed; unknown command and real-file-path rejection covered; mounted-path success sweep and auxiliary surfaces exercised; formal V1–V5/error ledger remains. | None | Codex | **Foundation and mounted-path sweep verified; stress matrix in progress** |
| UX-01 Lock identity and fail-closed UX | Enabling the lock means failed/cancelled auth never reveals memories, and macOS identifies FNDR. | `BiometricLockScreen` observable state + native LocalAuthentication boundary; no persistence/schema change. | The frontend bypass is removed; delete the remaining subprocess `swift -e` native path and reuse onboarding state. | Component cancel/failure/success tests; native prompt screenshot under FNDR identity; restart/cancel/retry run. | Native implementation review | Codex + native reviewer | **Frontend fail-closed implemented; native identity deferred** |
| UX-02 Truthful onboarding and degraded setup | First run explains exactly what is required, optional, local, downloaded, or unavailable. | Existing onboarding step state and model/permission IPC. | Reuse downloader/progress/permission checks; remove same-result “accept/skip” controls and unsupported copy. | Truth/network-exception and save-error component paths implemented; browser `startup` and native deny/grant/revoke/download/restart remain. | UX-00; privacy copy review | Codex + human operator | **Partially implemented; native/setup matrix pending** |
| UX-03 Semantic appearance foundation | Every entry has readable light/dark tokens, shared type roles, focus, scroll, and motion behavior. | Existing palettes, atoms, global CSS, entry bootstraps; behavior unchanged. | Consolidate raw colors/duplicate primitives; do not add a UI framework. | Main dark/light at 360/640/720+, auxiliary native-like/narrow sizes, reduced-motion code/tests, fresh console, and visual contrast fixes passed; formal 200%/contrast ledger remains. | UX-00; D-02/D-03 can refine later | Codex | **Broad browser parity implemented; formal stress ledger pending** |
| UX-04 Reachable shell and navigation | Every sidebar, palette, toast, and voice action opens a real destination; one Cmd+K behavior exists. | `MOUNTED_PANEL_KEYS`, typed `SIDEBAR_GROUPS`, `AppPanels`, palette allowlist, notification and voice dispatch. | Historical keys remain compiled but are runtime-guarded; visible triggers were deleted or rerouted instead of mounting unapproved panels. | Focused registry/command/Search/modal tests passed; every mounted destination and palette capture action was browser-traversed; layered ownership and narrow shell collision passed. | UX-00 | Codex | **Implemented and browser-verified for mounted Alpha paths; native events pending** |
| UX-05 Actionable Home / Resume | Home is readable and useful: resume/search/status, no false scroll cue or time contradiction. | `HomeHero`, shell status, future Resume component boundary from FEA-03. | Simplify parallax/decorative copy; reuse Search and status data rather than duplicate retrieval. | Home component states; V1–V5 screenshots; long name; active/paused/degraded; keyboard. | UX-03; backend Resume work only for resume card | Codex | **In progress** |
| UX-06 Unified Search & Ask | Query intent, filters, results, cited answer, refusal, and recovery are clear without duplicate shortcuts. | SearchBar, Timeline, AskPanel, `useSearch`, stable retrieval/answer IPC. | Reuse one query model; Cmd+K duplication is removed; D-04 still decides long-term voice placement. | Home/Search filters, results, selection, empty state, narrow dark/light and clear action passed; a grounded Ask answer opened its citation in Vault. Injected refusal/error/long stress and native voice remain. | UX-03/04 | Codex | **Implemented for current success paths; recovery stress/native voice pending** |
| UX-07 Human-readable Memory Vault | Users browse memories, review excluded/low-signal items, and expand evidence without raw internals dominating. | Vault list, MemoryCard, ExpandedMemoryCard, optional embedded graph; storage unchanged. | Reuse canonical MemoryCard; remove dead grid columns, raw debug-by-default, duplicated copy, and machine labels. | Focused shaping/action/modal tests plus browser filters, review queue, detail, related/similar, copy, virtual reopen, and delete cancel/confirm passed. | UX-03/04; D-05 for graph prominence | Codex | **Implemented and browser-verified for synthetic paths; native/error stress pending** |
| UX-08 Trust & Settings | Capture, privacy activity, models, profile, and appearance share one clear control center. | ControlPanel/PrivacyPanel/PrivacyProof internals/model status; event contracts unchanged. | Merge repeated status/blocklist UI; remove absolute “secure” claims and duplicate settings controls. | Focused action/copy tests and representative Settings browser trace passed; native durable pause/block exclusion/model repair and Privacy Activity counter validation remain. | UX-02/03/04; D-01 | Codex + human operator | **Partially implemented and focused/browser verified** |
| UX-09 Reflection workflow | A user can review a day/week and carry a follow-up to completion with less navigation. | Daily Summary, Todo, Stats, Wrapped; first pass keeps panels separate. | Reuse date/task/export logic; do not add a second task store. Merge only after D-06. | Focused batch plus every visible success control across Daily, Stats, To-dos, and Wrapped passed in the browser; virtual export stayed local. Injected failures, native PDF, 200% long-content, and IA decisions remain. | UX-03/04; D-06 | Codex | **Current panels implemented and browser-verified; stress/native/IA work pending** |
| UX-10 Screen Guide coherence | Settings, hold/text request, overlay, permission errors, and reduced motion behave as one bounded workflow. | Existing Screen Guide IPC/events and ADR-014 boundary. | Reuse typed state machine and native overlay; no second capture/voice stack. | 32/32 assistance batch and typed-question browser path passed with explicit no-capture status; microphone/hold was not invoked. | UX-03; human native pass | Codex + human operator | **Panel and text-only browser path verified; native permission path pending** |
| UX-11 Auxiliary-window parity | Omnibar and Autofill look and behave like FNDR and fail safely in another app's context. | Separate Vite entries, native window/events, Autofill confirmation boundary. | Import shared bootstrap/tokens; remove large inline fixed-color CSS; reuse search/result components where behavior matches. | 21 focused tests, production build, Matrix dark/Film light, native-like sizes, 320 px narrow, and browser-safe failures passed with zero console errors. | UX-03/06 | Codex + human operator | **Browser parity implemented; native shortcut/AX/focus/placement evidence pending** |
| UX-12 Orphan disposition | No historical surface silently expands scope or leaves a dead trigger. | Per-surface ship/merge/dev/defer/delete record; imports, IPC and docs traced first. | Runtime guards stop historical keys from becoming visible; delete only after proving no live caller and preserve reused helpers. | Mounted registry and trigger reroutes are tested; per-surface disposition and bounded deletion/build evidence remain. | D-05/D-08; UX-04 | Product owner + Codex | **Reachability guard implemented; disposition deferred** |
| UX-13 Regression and native release gate | Every reachable workflow has repeatable light/dark/a11y evidence and a bounded native sign-off. | State coverage, browser checks, QA runbook/evidence index. | Extend the strict fixture rather than duplicating component tests in pixel tests. | Final typecheck, 297 frontend tests, production build, complete executed Rust suite, diff hygiene, mounted-path browser sweep, and fresh console passed. Formal V1–V5/error ledger and native checklist remain. | UX-00–12 as applicable | Codex + human operator | **Code integration green; stress matrix and native sign-off pending** |

## Definition of done

The program is complete only when:

1. Every reachable interaction in the ledger has a deliberate success, empty, loading, denied/degraded, and error treatment where applicable.
2. Every sidebar/palette/toast/voice destination is mounted and tested or removed.
3. Light/dark, keyboard, contrast, 200% zoom, reduced motion, focus management, and minimum-window checks pass on every production entry.
4. The browser matrix is green with deterministic fixtures and clean console output.
5. Native-only workflows have human-operated evidence; browser simulation is never cited as native proof.
6. Copy and privacy claims are reviewed against current code/ADRs.
7. Orphaned surfaces have an explicit disposition and dead code is removed in bounded, test-backed slices.
8. Focused tests and `make test` pass, with exact commands and evidence linked from the owning issue or merge request.
9. A five-person task-based user study (already anticipated by the master plan) measures completion, errors, confidence, and SUS; aesthetic preference alone is not the success metric.

## Truthful limitations

- The current browser preview is an implemented development harness, not a production mode or native integration test; its complete path/theme/viewport matrix is still in progress.
- Current browser evidence covers only the explicitly logged representative paths and does not establish all-surface accessibility.
- WCAG targets in this document are acceptance criteria; they are not claims that the present UI passes.
- The supported native minimum window size has not yet been confirmed.
- Some current panels depend on optional local models, Python/ffmpeg, native permissions, or experimental data. Their browser states can be designed before their native functionality is proven, but release status must remain explicit.
- The full 262-test frontend run recorded above predates later concurrent edits; `npm run typecheck`, `npm test`, `npm run build`, and `make test` must be rerun on the final working tree before integration is called green.
- Capture-safety and pause-persistence unit tests do not replace a privacy-safe native run proving real permission, exclusion, pause/relaunch, and counter behavior.
- This program deliberately leaves product-opinion decisions visible instead of silently converting them into code.
