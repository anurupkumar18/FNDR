# Test audit

Measured 2026-10-18 on the current `origin/main` worktree. The ticket says 55 frontend files; the repository now has 116, including the new QT-03 journey file. There are 23 top-level Rust integration files. Frontend runtime is elapsed time per file from one passing Vitest JSON run (907 tests, 115 files), plus a focused six-test run for the new journey file. Parallel file times do not sum to wall time. Rust runtime is elapsed time from cached integration binaries built against this code line, excluding compilation, with two test threads. `capture_fixtures.rs` required an unsandboxed rerun because macOS CoreVideo could not create a pixel buffer in the sandbox. Runtimes vary with machine load.

Mount labels: **Main** is reachable in the primary app, **Aux** in a separate app window, **Conditional** behind a feature toggle or dev view, **Dev** in preview or engineering checks, **Shared** used by mounted surfaces, and **No** has no production mount found. A mounted label is reachability, not proof that the test itself drives the full surface.

## Frontend files

| Test file | User outcome or implementation detail | Mount | Time |
| --- | --- | --- | ---: |
| `src/app/App.onboarding.test.tsx` | Onboarding gates access to the workspace and completed users can search | Main | 401 ms |
| `src/app/AppShell.test.tsx` | Implementation detail: root shell mounts the workspace and wallpaper | Main | 16 ms |
| `src/app/BiometricLockScreen.test.tsx` | Biometric lock prevents access until unlock | Main | 156 ms |
| `src/app/HomeDeadlines.test.tsx` | Home shows relevant deadlines | Main | 206 ms |
| `src/app/HomeHero.test.tsx` | Home search and voice draft accept user input | Main | 183 ms |
| `src/app/HomeSets.test.tsx` | People can reopen saved work sets | Main | 200 ms |
| `src/app/McpApprovalPrompt.test.tsx` | MCP actions require visible approval | Main | 81 ms |
| `src/app/PanelErrorBoundary.test.tsx` | A failed panel leaves a recoverable workspace | Main | 106 ms |
| `src/app/ResumeWork.test.tsx` | Home resumes recent work from its sources | Main | 398 ms |
| `src/app/ThreadChanges.test.tsx` | A thread shows what changed since last visit | Main | 120 ms |
| `src/app/WorkSetOpener.test.tsx` | A saved work set reopens its items | Main | 241 ms |
| `src/app/auxiliaryAppearance.test.ts` | Implementation detail: auxiliary windows inherit appearance | Aux | 9 ms |
| `src/app/homeWorkSet.test.ts` | Implementation detail: one-tap work-set selection rule | Main | 18 ms |
| `src/app/panels.test.ts` | Implementation detail: only allowed foreground panels mount | Main | 1 ms |
| `src/dev/VoicePreviewGallery.test.tsx` | Dev preview shows every shared voice state | Dev | 121 ms |
| `src/dev/docsDrift.test.ts` | Implementation detail: instruction links resolve | Dev | 5 ms |
| `src/dev/memoryReadDrift.test.ts` | Memory readers keep the visibility gate | Dev | 3 ms |
| `src/dev/previewIpc.test.ts` | Dev preview models product IPC without personal data | Dev | 33 ms |
| `src/dev/releaseDrift.test.ts` | Implementation detail: release instructions match files | Dev | 4 ms |
| `src/domains/actions/ActionApprovalPanel.test.tsx` | People can inspect and resolve proposed actions | Main | 90 ms |
| `src/domains/ask/AskPanel.test.tsx` | People can ask over permitted memories and see evidence | Main | 228 ms |
| `src/domains/ask/AskPanelActivityBoundaries.test.tsx` | Ask activity reports observed steps without leaking memory text | Main | 170 ms |
| `src/domains/command-palette/CommandPalette.interaction.test.tsx` | Keyboard and pointer commands reach the intended action | Main | 219 ms |
| `src/domains/command-palette/CommandPalette.test.ts` | Implementation detail: reduced palette exposes allowed commands | Main | 1 ms |
| `src/domains/companion/__tests__/CompanionDevicesPanel.test.tsx` | Companion pairing controls display and update | No | 123 ms |
| `src/domains/handoff/HandOffPanel.test.tsx` | People can hand off a task and inspect its checked result | Main | 353 ms |
| `src/domains/handoff/handOffTrace.test.ts` | Implementation detail: hand-off trace reflects run state | Main | 1 ms |
| `src/domains/memory-vault/CopyForAgentButton.test.tsx` | People can copy a safe memory context for an agent | Main | 85 ms |
| `src/domains/memory-vault/ExpandedMemoryCard.test.tsx` | People can inspect a memory and its source details | Main | 275 ms |
| `src/domains/memory-vault/MemoryCardsPanel.test.tsx` | People can browse, filter, and open visible memories | Main | 1,868 ms |
| `src/domains/memory-vault/__tests__/KnowledgeGraph.hover.test.tsx` | Graph neighbors are highlighted when explored | Conditional | 157 ms |
| `src/domains/memory-vault/__tests__/MemoryCard.compact.test.tsx` | Compact memory cards remain usable in narrow space | Main | 150 ms |
| `src/domains/memory-vault/__tests__/MemoryCard.lifecycle.test.tsx` | Memory lifecycle states and actions stay accurate | Main | 234 ms |
| `src/domains/memory-vault/__tests__/SurfacingReason.test.tsx` | Memory cards explain why they appeared | Main | 35 ms |
| `src/domains/memory-vault/__tests__/perspectiveFilter.test.ts` | Implementation detail: perspective filter matches selected view | Main | 1 ms |
| `src/domains/memory-vault/__tests__/vaultGrouping.test.ts` | Implementation detail: vault memories group consistently | Main | 20 ms |
| `src/domains/memory-vault/graph/__tests__/activeUseClock.test.ts` | Implementation detail: graph active-use timer is bounded | Conditional | 1 ms |
| `src/domains/memory-vault/graph/__tests__/graphCache.test.ts` | Implementation detail: graph cache avoids stale data | Conditional | 2 ms |
| `src/domains/memory-vault/graph/__tests__/graphDataBuilder.test.ts` | Implementation detail: graph nodes and edges are built from evidence | Conditional | 5 ms |
| `src/domains/memory-vault/graph/__tests__/graphFilterOptions.test.ts` | Implementation detail: graph filter choices follow available data | Conditional | 1 ms |
| `src/domains/memory-vault/graph/__tests__/graphFilters.test.ts` | Implementation detail: graph filters exclude nonmatches | Conditional | 1 ms |
| `src/domains/memory-vault/graph/__tests__/graphLegendBuilder.test.ts` | Implementation detail: graph legend matches visible relationships | Conditional | 3 ms |
| `src/domains/memory-vault/graph/__tests__/graphPalette.test.ts` | Implementation detail: graph colors remain distinguishable | Conditional | 2 ms |
| `src/domains/memory-vault/graph/__tests__/graphRelationshipResolver.test.ts` | Implementation detail: graph edges receive correct relationship types | Conditional | 4 ms |
| `src/domains/memory-vault/sessionDigest.test.ts` | Implementation detail: session digest groups related memories | Main | 3 ms |
| `src/domains/memory-vault/sessionLinks.test.ts` | Implementation detail: session links connect related captures | Main | 3 ms |
| `src/domains/notch/NotchDoSettings.test.tsx` | People can configure Notch Do safely | Aux | 248 ms |
| `src/domains/notch/NotchHud.test.tsx` | Notch Ask and Do show their active states | Aux | 31,348 ms |
| `src/domains/notch/doNarration.test.ts` | Spoken action updates match the actual operation | Aux | 13 ms |
| `src/domains/notch/doRun.test.ts` | Stop words interrupt Notch Do | Aux | 5 ms |
| `src/domains/notch/notchConversation.test.ts` | Implementation detail: notch conversation state remains coherent | Aux | 5 ms |
| `src/domains/notch/notchMetrics.test.ts` | Implementation detail: notch timing metrics are recorded | Aux | 3 ms |
| `src/domains/notch/notchShape.test.ts` | Implementation detail: notch silhouette matches window size | Aux | 6 ms |
| `src/domains/notch/notchSpeech.test.ts` | Notch speech uses the intended spoken line | Aux | 416 ms |
| `src/domains/omnibar/OmnibarApp.test.tsx` | Quick Find searches and opens selected results | Aux | 2,392 ms |
| `src/domains/outputs/OutputsPanel.test.tsx` | People can create and inspect cited daily outputs | Main | 274 ms |
| `src/domains/privacy-proof/PrivacyProof.test.tsx` | People can inspect capture skips and network activity | Main | 278 ms |
| `src/domains/screen-guide/__tests__/OperatorPermissions.test.tsx` | Computer operation follows the permission gate | Main | 81 ms |
| `src/domains/screen-guide/__tests__/ScreenGuideActivityBoundaries.test.ts` | Screen Guide activity reflects observed operations | Main | 11 ms |
| `src/domains/screen-guide/__tests__/ScreenGuideOverlay.test.tsx` | Screen Guide voice and overlay controls work | Main | 647 ms |
| `src/domains/screen-guide/__tests__/ScreenGuidePanel.test.tsx` | People can ask about the screen and inspect answers | Main | 896 ms |
| `src/domains/screen-guide/__tests__/screenGuideState.test.ts` | Implementation detail: Screen Guide state transitions stay valid | Main | 3 ms |
| `src/domains/search/SearchBar.test.tsx` | People can enter, filter, and submit a memory search | Main | 266 ms |
| `src/domains/search/SearchLayout.test.ts` | Implementation detail: search layout stays inside its container | Main | 1 ms |
| `src/domains/setup/CalendarMeetingPrep.test.tsx` | People can opt into meeting preparation | Main | 168 ms |
| `src/domains/setup/ImportSources.test.tsx` | People can consent to and review imported sources | Main | 395 ms |
| `src/domains/setup/SetupCenter.test.tsx` | People can manage setup and model availability | Main | 235 ms |
| `src/domains/timeline/Timeline.test.tsx` | Search results can be inspected in time order | Main | 250 ms |
| `src/domains/timeline/timelineConfig.test.ts` | Implementation detail: timeline display parameters stay valid | Main | 1 ms |
| `src/domains/workspace/AgentReply.test.tsx` | Agent answers render with usable citations | Main | 86 ms |
| `src/domains/workspace/AgentWorkspace.test.tsx` | People can connect an agent and use a conversation | Main | 626 ms |
| `src/domains/workspace/AutofillOverlay.test.tsx` | Autofill asks before acting and owns its requests | Aux | 413 ms |
| `src/domains/workspace/CodexAccountCard.test.tsx` | People can see account connection state | Main | 297 ms |
| `src/domains/workspace/ControlPanel.test.tsx` | Settings controls persist and show current state | Main | 1,498 ms |
| `src/domains/workspace/DailySummaryPanel.test.tsx` | People can open a daily summary and its sources | Main | 206 ms |
| `src/domains/workspace/EngineMetricsCard.test.tsx` | People can inspect engine health metrics | Main | 258 ms |
| `src/domains/workspace/EngineMetricsPanel.test.tsx` | People can open engine diagnostics | Main | 61 ms |
| `src/domains/workspace/FndrWrappedPanel.test.tsx` | People can inspect a weekly recap | Main | 294 ms |
| `src/domains/workspace/MemoryJourneyInspector.test.tsx` | Developers can inspect a memory journey | Dev | 363 ms |
| `src/domains/workspace/ModelDownloadBanner.test.tsx` | People can see model download status | Main | 179 ms |
| `src/domains/workspace/Onboarding.test.tsx` | New users can progress through setup and model choices | Main | 349 ms |
| `src/domains/workspace/PeerDirectory.test.tsx` | People can manage peers in the agent workspace | Main | 403 ms |
| `src/domains/workspace/PrivacyPanel.test.tsx` | People can manage privacy controls | Main | 164 ms |
| `src/domains/workspace/StatsPanel.test.tsx` | People can inspect capture and usage statistics | Main | 295 ms |
| `src/domains/workspace/TodoPanel.test.tsx` | People can inspect and manage tasks | Main | 385 ms |
| `src/features/graph/__tests__/graph.test.ts` | Implementation detail: 3D graph geometry and layout remain valid | Conditional | 4 ms |
| `src/journeys.test.tsx` | Six journeys across onboarding, search, capture, agent setup, voice, and briefing | Main | 633 ms |
| `src/shared/activity/activityTrace.test.ts` | Implementation detail: activity trace state transitions stay valid | Shared | 2 ms |
| `src/shared/activity/activityTraceBoundaries.test.ts` | Activity traces report real producer events | Shared | 9 ms |
| `src/shared/activity/voiceActivityTrace.test.ts` | Implementation detail: voice trace state transitions stay valid | Shared | 3 ms |
| `src/shared/activity/voiceActivityTraceBoundaries.test.ts` | Voice traces report real capture events | Shared | 9 ms |
| `src/shared/components/ActivityTrace.test.tsx` | People can read activity progress and failures | Shared | 134 ms |
| `src/shared/components/MotionWallpaper.test.tsx` | Implementation detail: wallpaper mounts without hiding the UI | Shared | 13 ms |
| `src/shared/home/homeSections.test.ts` | Implementation detail: Home section ordering stays valid | Shared | 9 ms |
| `src/shared/hooks/__tests__/useActiveCinematicPalette.test.ts` | Implementation detail: active wallpaper colors resolve | Shared | 1 ms |
| `src/shared/hooks/useModalFocus.test.tsx` | Keyboard focus stays inside open dialogs and returns afterward | Shared | 69 ms |
| `src/shared/hooks/useModelDownloadStatus.test.ts` | Model download status stays current | Shared | 1 ms |
| `src/shared/hooks/usePolling.test.tsx` | Implementation detail: polling stops and restarts correctly | Shared | 36 ms |
| `src/shared/hooks/useSearch.test.tsx` | A memory query returns results or a clear failure | Shared | 119 ms |
| `src/shared/hooks/useSearchActivityBoundaries.test.tsx` | Search activity reflects actual retrieval steps | Shared | 34 ms |
| `src/shared/ipc/agentNotes.test.ts` | Assistant note consent persists correctly | Shared | 2 ms |
| `src/shared/ipc/imports.test.ts` | Import requests follow the IPC contract | Shared | 4 ms |
| `src/shared/ipc/ipcDrift.test.ts` | Implementation detail: frontend and Rust IPC commands stay in sync | Shared | 110 ms |
| `src/shared/motion/gesture.test.ts` | Implementation detail: gesture projection is stable | Shared | 1 ms |
| `src/shared/reopenOutcome.test.ts` | Reopen actions report the actual result | Shared | 2 ms |
| `src/shared/theme/__tests__/cinematic-palettes.test.ts` | Implementation detail: theme palettes apply consistently | Shared | 10 ms |
| `src/shared/theme/__tests__/wallpaper-field-colors.test.ts` | Implementation detail: wallpaper colors meet token rules | Shared | 3 ms |
| `src/shared/voice/VoiceControl.test.tsx` | Voice status announces listening, final text, and errors | Shared | 125 ms |
| `src/shared/voice/VoiceOutputSection.test.tsx` | People can choose spoken-output settings | Shared | 230 ms |
| `src/shared/voice/localNeural.test.ts` | Implementation detail: local neural speech splits sentences | Shared | 18 ms |
| `src/shared/voice/realtimeOut.test.ts` | ChatGPT realtime speech sends and receives valid events | Shared | 42 ms |
| `src/shared/voice/speechRegistry.test.ts` | Available speech providers and voices are listed correctly | Shared | 27 ms |
| `src/shared/voice/systemEnhanced.test.ts` | Enhanced system voices rank predictably | Shared | 14 ms |
| `src/shared/voice/useVoice.test.tsx` | A voice session forwards matching partial and final text | Shared | 108 ms |
| `src/shared/voice/webviewBasic.test.ts` | Basic webview speech handles its lifecycle | Shared | 4 ms |
| `src/shared/wallpaper/__tests__/wallpaper-registry.test.ts` | Implementation detail: available wallpaper IDs resolve | Shared | 2 ms |

## Rust integration files

| Test file | User outcome | Mount | Time |
| --- | --- | --- | ---: |
| `src-tauri/tests/action_injection.rs` | Captured text cannot turn into an unsafe action | Backend | 55 ms |
| `src-tauri/tests/agent_regression.rs` | Agent tasks retain expected outcomes on synthetic cases | Backend | 1,199 ms |
| `src-tauri/tests/anti_overfitting.rs` | Retrieval generalizes beyond fixture-specific aliases | Backend | 6 ms |
| `src-tauri/tests/capture_fixtures.rs` | Capture OCR and cleanup stay within fixture accuracy budgets | Backend | 1,948 ms |
| `src-tauri/tests/chunk_retrieval_quality.rs` | Relevant chunks survive retrieval and ranking | Backend | 498 ms |
| `src-tauri/tests/daily_briefing_inputs.rs` | Daily briefing uses only checked source fields | Backend | 126 ms |
| `src-tauri/tests/embedding_audit.rs` | Embedding text preserves searchable meaning | Backend | 62 ms |
| `src-tauri/tests/embeddinggemma_reference.rs` | Local embeddings match reference vectors | Backend | 43 ms |
| `src-tauri/tests/end_to_end_fndr_query.rs` | A query traverses retrieval and answer generation | Backend | 546 ms |
| `src-tauri/tests/footprint_replay.rs` | Capture replay stays within expected storage footprint | Backend | 21,874 ms |
| `src-tauri/tests/low_signal_surface.rs` | Low-signal and hidden records stay out of read surfaces | Backend | 432 ms |
| `src-tauri/tests/memory_browse.rs` | Large vault browsing remains bounded and visible | Backend | 1,722 ms |
| `src-tauri/tests/merge_replay.rs` | Repeated captures merge into the right memories | Backend | 564 ms |
| `src-tauri/tests/operator_mcp.rs` | The real operator MCP binary accepts and guards tool requests | Aux | 96 ms |
| `src-tauri/tests/query_plan_rules.rs` | Query plans choose valid retrieval routes | Backend | 92 ms |
| `src-tauri/tests/resume_work.rs` | Recent memories group into resumable threads | Backend | 1,541 ms |
| `src-tauri/tests/retrieval_routes.rs` | All read surfaces use the shared retrieval visibility gate | Backend | 618 ms |
| `src-tauri/tests/retrieve.rs` | Memory retrieval obeys ranking and permission rules | Backend | 7,365 ms |
| `src-tauri/tests/search_flow.rs` | Seeded memories can be found through keyword and vector search | Backend | 490 ms |
| `src-tauri/tests/search_relevance_eval.rs` | Search relevance meets synthetic query expectations | Backend | 2,731 ms |
| `src-tauri/tests/storage_scale.rs` | Storage indexes and compaction handle a synthetic corpus | Backend | 1,519 ms |
| `src-tauri/tests/store_graph_regressions.rs` | Stored graph relationships stay consistent | Backend | 772 ms |
| `src-tauri/tests/task_suggestions.rs` | Tasks require evidence from the captured screen | Backend | 49 ms |

## Consolidation candidates

- `src/domains/notch/NotchHud.test.tsx` took 31.3 s, almost the full 32.6 s suite wall time. Inspect its timers and redundant waits before deleting any user behavior coverage.
- The eight `src/domains/memory-vault/graph/__tests__/` files total only 19 ms. The graph is still reachable through conditional Vault controls, so deleting them for speed would not help; consolidate only true duplicates after checking the mounted graph path.
- `src/domains/companion/__tests__/CompanionDevicesPanel.test.tsx` tests a panel with no production mount found. Candidate to retire or mount deliberately in QT-02.
- `src/app/AppShell.test.tsx` and `src/shared/components/MotionWallpaper.test.tsx` mostly assert composition or markup; consider a single user journey through the real shell instead.
- `src/domains/command-palette/CommandPalette.test.ts` checks only the reduced registry while `CommandPalette.interaction.test.tsx` checks user behavior; merge only if the registry guard remains protected.
- The many small `shared/activity/*` and graph helper tests protect contracts but are not the source of suite wall time. Keep their behavior assertions unless a true duplicate is demonstrated.

## Gaps that drive QT-03

The baseline suite relied heavily on component-level mocks. QT-03 adds six outcome-named journeys through the real `AppShell` and preview IPC, crossing search, memory detail, settings persistence, and voice event wiring. The complete suite now passes 913 tests in 116 files.

## QT-02 consolidation, 2026-10-18

On the same M1, a complete `npm test -- --reporter=dot` run took **33.18 s** (915 tests, 116 files) before consolidation and **10.28 s** (910 tests, 114 files) afterward. The longest test file, `NotchHud.test.tsx`, now runs in 2.02 s in a focused run. Its state-transition tests use short delays while `doRun.test.ts` checks the production endpoint, heard-request, and auto-start timings (1,200, 1,200, and 1,500 ms). This preserves behavior coverage without making the whole suite wait for the real countdowns. Timings are single wall-clock samples and will vary with machine load.

Deleted `CompanionDevicesPanel.test.tsx` because the panel has no production mount, and `AppShell.test.tsx` because it only asserted that two mocked elements rendered. The real `AppShell` is exercised by `journeys.test.tsx`. The conditional graph tests and `MotionWallpaper.test.tsx` remain: the graph is reachable and the wallpaper test is the only reduced-motion check. The command registry test also remains because the interaction test does not assert its complete ID contract.

Mounted destinations retain direct coverage: Home (`HomeHero.test.tsx`), Memory Vault (`MemoryCardsPanel.test.tsx`), Search and Ask (`SearchBar.test.tsx`, `AskPanel.test.tsx`), To-dos (`TodoPanel.test.tsx`), Settings (`ControlPanel.test.tsx`), and auxiliary Notch and Quick Find (`NotchHud.test.tsx`, `OmnibarApp.test.tsx`). The six real-shell journeys also cross the main destinations.
