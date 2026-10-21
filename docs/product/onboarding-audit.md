# OB-01 native onboarding audit

Tested 2026-10-18 on Felipe's Mac from an empty `FNDR_DATA_DIR` with the native Tauri development app at `8f1ea5f`. The app window was 900 by 700 CSS pixels. No name, source import, or private content was entered into the test profile. The light pass used the skip actions; the dark pass selected the Apple on-device model. Screenshots show only the app window and the disposable profile.

## Result by step

| Step | Light | Dark | Action and observed wait |
| --- | --- | --- | --- |
| Welcome | [screen](../evidence/W02/ob-01/welcome-light.png) | [screen](../evidence/W02/ob-01/welcome-dark.png) | Left optional name empty, clicked Get Started. `privacy_promise` was saved in 0.302 s in a timed replay. |
| How FNDR handles your data | [top](../evidence/W02/ob-01/privacy-light-top.png), [footer](../evidence/W02/ob-01/privacy-light-footer.png) | [top](../evidence/W02/ob-01/privacy-dark-top.png), [footer](../evidence/W02/ob-01/privacy-dark-footer.png) | Scrolled several viewports to find Continue and Skip for now; clicked Skip. `biometrics` was saved in 0.303 s in a timed replay. |
| Touch ID | [screen](../evidence/W02/ob-01/biometrics-light.png) | [screen](../evidence/W02/ob-01/biometrics-dark.png) | Clicked Skip for now. The model step appeared without an observed loading stall. Enabling Touch ID was not tested because it needs the Mac owner's biometric input. |
| Model | [initial](../evidence/W02/ob-01/model-light-initial.png), [footer](../evidence/W02/ob-01/model-light-footer.png) | [footer](../evidence/W02/ob-01/model-dark-footer.png) | Required MiniLM downloaded with visible activity and a Completed state during this session. First pass clicked Skip for now. Second pass clicked Continue with Apple on-device model. The Qwen download was not started. |
| Permissions | [screen](../evidence/W02/ob-01/permissions-light-initial.png) | [screen](../evidence/W02/ob-01/permissions-dark.png) | First pass clicked Skip for now; second pass clicked Open FNDR. Both advanced to imports without an observed loading stall. Screen Recording, Accessibility, and Microphone were already granted to this app on this Mac; Speech Recognition was not. |
| Bring your past | [entry position](../evidence/W02/ob-01/bring-past-light.png), [top after scrolling](../evidence/W02/ob-01/bring-past-light-top.png) | [entry position](../evidence/W02/ob-01/bring-past-dark-initial.png) | All source switches were off. Clicked Skip imports and reached Home. Actual imports were not run because they would read the owner's sources. |

The two numbers above time the click to the saved onboarding step, not time spent reading. Other transitions were observed as immediate but were not instrumented; this audit does not claim precise timings for them. The MiniLM download completed during the walkthrough, but its exact duration was not captured. No step displayed an indefinite spinner.

## Findings, ordered by severity

### P1: Skipping the model gives a false completion

**Reproduce:** From a clean profile choose Skip for now on the model step, then skip permissions and imports. Home opens, but the saved state remains `model_download` with `model_downloaded: false`. Quit and relaunch the same profile: the [model step returns](../evidence/W02/ob-01/relaunch-after-model-skip-app-only.png). Choosing the Apple model on the next pass lets `complete` persist. The backend's `normalize_onboarding_state` changes `complete` back to `model_download` when `model_downloaded` is false (`src-tauri/src/ipc/onboarding.rs`).

**Impact:** A visible skip path is a dead end across launches. Either make the skip persist with a clearly limited Home state or explain that model selection is required before completion.

### P2: Moving to imports preserves the previous page's scroll position

**Reproduce:** Scroll to the bottom of the permissions step and advance. The import step opens on Browser history, with its heading and consent explanation above the viewport. Compare the [entry position](../evidence/W02/ob-01/bring-past-light.png) with the [top after scrolling](../evidence/W02/ob-01/bring-past-light-top.png). This also happened on the second pass in dark appearance.

**Impact:** Someone deciding what private sources FNDR may read can miss the introductory scope and 30-day limit. Reset the onboarding scroll container to the top on step changes.

### P2: Dark appearance makes onboarding symbols hard to see

The dark window puts near-black icons on dark tiles while the main card remains light. The Welcome mark and Touch ID fingerprint are particularly faint in the [dark Welcome](../evidence/W02/ob-01/welcome-dark.png) and [dark Touch ID](../evidence/W02/ob-01/biometrics-dark.png) screens. The permissions activity panel also uses muted text on a dark inset. Use a contrasting icon and status-text color in this appearance.

### P2: An optional 134 MB voice download is preselected

The model step's [footer](../evidence/W02/ob-01/model-light-footer.png) has “Also download FNDR's natural voice (134 MB)” checked by default. It explains the source and background download below the checkbox, but the positive choice has already been made. Default this extra download off, or make its inclusion explicit in the primary action. This audit did not select a model with the checkbox enabled to verify the resulting network transfer.

### P2: Privacy explanation buries the decision and leads with diagnostics

At this window size, both privacy actions are several scrolls below the [top](../evidence/W02/ob-01/privacy-light-top.png). The first visible details are frame and network counters, including a recorded host, before a short plain-language account of what FNDR reads and stores. The consent decision needs a concise summary near the top with details available below. This Mac's existing Screen Recording grant allowed session counters to appear before the new data profile had reached the permissions step; that observation does not establish what happens on a newly installed Mac.

### P3: Several onboarding sentences are more technical than conversational

The Welcome promise “Synthesize meetings and track tasks” is broad and uses “synthesize” where “summarize” would be clearer. The privacy copy says “Frames evaluated this app session,” “Child processes, provider tools, and commands,” and “activity history.” The model step says “Search Embedder,” “GGUF,” and “pinned checksums.” Keep those details in an expandable explanation and say what the person gains or permits first. The exact copy is visible in the linked screens above.

## Coverage limits and follow-up

- This was an empty FNDR data directory, not a new macOS account. Existing operating-system grants made the permission request and denial flows untestable here.
- Touch ID activation, Qwen's 1.5 GB download, and real source imports were not exercised. These require biometric input, a large download, or reading personal data.
- A second teammate Mac was not available during this run. Repeat the permission path and a complete first-run sequence there before treating this as a clean-install usability result.
- The first attempted profile was unexpectedly advanced before its initial screenshot. It was excluded from this audit; all evidence linked here comes from the second empty profile. Its data and both disposable profiles were removed after the run.
