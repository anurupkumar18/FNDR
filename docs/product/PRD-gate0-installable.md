# PRD: Gate 0 — Installable FNDR

Status: Draft (2026-07-06). Produced via `to-prd` from the Seed-readiness catalog. Scope rulings in effect: internal automations only, no mobile, strictly local models.

## Problem

FNDR can only be run by developers: clone, `npm install`, manual `./scripts/bootstrap/download-minilm.sh`, `npm run tauri dev`. There is no signed installable artifact, no auto-update, no CI running the test suite (only CodeQL and clippy workflows exist), and no LICENSE file. If the embedder model is missing at runtime, capture silently writes zero-vector embeddings and search quality degrades with no user-visible cause. Until strangers can install and trust FNDR, no beta cohort, retention data, or fundraising conversation is possible.

## Goal

A non-developer on macOS 13+ installs FNDR from a URL, completes onboarding (permissions + model download) inside the app, captures and searches real memories with real embeddings the same day, and receives subsequent releases automatically.

## Users / actors

- **End user** — non-developer, macOS 13+ (Apple Silicon primary), no dev tools or Python installed.
- **Maintainer** — cuts releases by pushing a tag.
- **CI** — GitHub Actions: test gate on PRs, release pipeline on tags.

## Current behavior (repo evidence)

- `src-tauri/tauri.conf.json`: bundle targets `dmg` + `app`, entitlements/Info.plist present; **no signing identity pipeline, no notarization, no updater plugin** (no updater dependency in `Cargo.toml` or `package.json`).
- `.github/workflows/`: only `codeql.yml` and `rust-clippy.yml`. `make test` (typecheck + vitest + `cargo test`; 563 Rust tests, 26 TS test files) never runs in CI.
- Embedding contract (`src-tauri/src/inference/model_config.rs`): `all-MiniLM-L6-v2.onnx`, 384-d, table `memories_v4_minilm_384`. Fetched today by a pre-run shell script; if absent, the runtime degrades to mock embeddings (documented in `docs/product/intelligence-engine.md` §5) and capture proceeds with zero vectors.
- An in-app model downloader **already exists** (`src-tauri/src/ipc/onboarding.rs`, `src/domains/workspace/ModelDownloadBanner.tsx`): progress events, states (preparing/downloading/finalizing/failed), HuggingFace source, activation refresh — currently wired for the Qwen VLM only.
- Python sidecars (`parakeet_runner.py`, `whisper_gguf_runner.py`, `orpheus_tts_runner.py`) are bundled as raw `.py` resources and invoked via system Python (`speech.rs`, `ipc/commands/hermes_agent.rs`); `requirements.txt` and `ffmpeg` are runtime expectations. The core capture → OCR → embed → search path is pure Rust/ONNX and does not need Python.
- The 2026-05 memory-storage drop bug (stacked critical-extraction issues filter) is **resolved**; the surviving defect class is silent embedding degradation.
- No `LICENSE` file at repo root.

## Proposed behavior

A user downloads a signed, notarized DMG. First launch runs onboarding: privacy-framed permissions (screen recording), then a required-model download step with progress, checksum verification, and resume. Capture does not write zero-vector embeddings — if the embedder is unavailable, capture is blocked or paused with a visible status and a one-click fix. Meeting/TTS/agent features that need Python or ffmpeg detect their absence and present as cleanly unavailable rather than erroring. Releases are produced by CI from a tag and delivered in-app via the Tauri updater.

## Non-goals

- Gate 1+ product work (substrate, dossiers, daily brief, demo-data-first-run polish).
- Compiling Python sidecars to standalone binaries (Gate 0 ships graceful degradation; bundling is a follow-up).
- Windows/Linux builds, Homebrew cask, website (post-Gate 0 distribution).
- Telemetry or crash reporting of any kind.
- External automations, mobile companion, cloud models (standing scope rulings).

## User workflows

1. **Install:** download DMG → drag to Applications → open with no Gatekeeper warning → onboarding → grant screen recording → embedder downloads (~90 MB) → capture starts → search returns real results the same day.
2. **Update:** new release published → app prompts on next launch (plus manual "Check for updates") → one-click update.
3. **Release (maintainer):** `git tag v0.x.y` + push → CI builds, signs, notarizes, staples, publishes GitHub Release + updater manifest.
4. **Degraded environment:** user without Python/ffmpeg opens Meetings → sees "requires additional components" state, not a crash; core capture/search unaffected.

## Functional requirements

- **FR1 — LICENSE:** a LICENSE file exists at repo root and is referenced in README. (Blocked on the licensing decision — see Open questions.)
- **FR2 — CI test gate:** a workflow runs `npm run typecheck`, `npm test`, and `cargo test` on every PR and push to main; failures block merge.
- **FR3 — Release pipeline:** tag push produces an ad-hoc-signed DMG attached to a GitHub Release, fully automated. *(Amended 2026-07-06: Apple Developer notarization descoped — no paid developer account for the college-project phase. First-launch requires right-click → Open; this is documented in the README and release notes. Notarization can be re-added later without pipeline redesign.)*
- **FR4 — Auto-update:** `tauri-plugin-updater` with signed manifests (Tauri's own minisign keys — independent of Apple signing); check on launch + manual trigger; update from vN to vN+1 works in-place.
- **FR5 — First-run model provisioning:** the existing downloader registry is extended to all required models — MiniLM embedder (mandatory), Qwen VLM (recommended, optional), CLIP (optional) — with pinned URLs, SHA-256 verification, disk-space preflight, and interrupted-download resume/retry. Onboarding includes this as a step.
- **FR6 — No silent zero-embedding:** when the embedder is unavailable, capture is blocked/paused with a prominent status (workspace chip + onboarding gate). Zero-vector memory rows are never written. The existing startup smoke-embed and runtime status payload are the wiring points.
- **FR7 — Python/ffmpeg graceful degradation:** speech/TTS/agent paths preflight their runtime dependencies and surface a typed "feature unavailable" state to the UI; core install has zero Python dependency.
- **FR8 — Onboarding sequence:** permissions (with the privacy story stated plainly) → required model download → capture live. Existing `Onboarding.tsx` flow extended, not replaced.

## Non-functional requirements

- **Performance:** install → first captured memory with real embedding in < 10 minutes on a 50 Mbps connection (embedder-only path; Qwen optional keeps this achievable).
- **Reliability:** downloads resume after interruption; a corrupt/partial model is never activated (checksum enforced); release artifacts verified in CI (`spctl --assess`).
- **Security/privacy:** models over HTTPS from pinned URLs with SHA-256; signing keys and Apple credentials only in CI secrets; updater manifests signed; no new network egress besides model download and update check (both user-visible).
- **Maintainability:** a release is one tag push; no manual signing steps.

## Domain language

| Term | Meaning | Existing code/docs |
|---|---|---|
| Embedding contract | Model name/file/dimension/table binding | `src-tauri/src/inference/model_config.rs` (v4 MiniLM 384-d) |
| Model registry | Downloadable local models + metadata exposed to onboarding | `src-tauri/src/ipc/onboarding.rs` (`listAvailableModels`) |
| Degrade-to-mock | Runtime fallback when real embedding fails | `docs/product/intelligence-engine.md` §5 — **behavior changes under FR6** |
| Sidecar | Python helpers for transcription/agent/TTS | `docs/CONTEXT.md`, `src-tauri/sidecars/` |

## Affected modules and interfaces

| Module | Change | Interface impact | Tests |
|---|---|---|---|
| `.github/workflows/` | Add `test.yml`, `release.yml` | None | The workflows are the test |
| `src-tauri/tauri.conf.json` + new plugin dep | Updater config, signing/notarization inputs | None public | Update-path QA |
| `src-tauri/src/ipc/onboarding.rs` | Registry entries for embedder/CLIP; SHA-256 verify; resume | Additive IPC | Unit: checksum, registry |
| `src/domains/workspace/Onboarding.tsx`, `ModelDownloadBanner.tsx` | Required-model step; generalize banner beyond Qwen | UI only | Component tests |
| Capture/embedding status boundary | FR6 gate: block capture instead of zero-vector writes | Behavior change | Boundary test: missing embedder → paused + no rows |
| `speech.rs`, `ipc/commands/hermes_agent.rs` | Dependency preflight → typed unavailable state | Additive | Unit: preflight |
| Repo root | LICENSE | None | — |

## Data flow

- **Release:** tag → GitHub Actions (build aarch64 DMG → codesign → notarize → staple → publish Release + `latest.json` updater manifest) → installed app polls manifest → signed update download → in-place install.
- **First run:** registry (pinned URL + SHA-256) → existing downloader (progress events) → models dir under `com.fndr.app` → `refreshAiModels` → embedder smoke-embed passes → capture enabled. Smoke-embed failure ⇒ FR6 gate, not mock fallback.

## Acceptance criteria

- [ ] On a clean macOS 13+ machine with no dev tools and no Python: install from a URL, complete onboarding, first memory captured **with a non-zero embedding**, within 10 minutes of network time.
- [ ] The shipped DMG mounts and the app launches via the documented right-click → Open flow (Gatekeeper unidentified-developer path; notarization descoped).
- [ ] Quitting mid-download and relaunching resumes or restarts cleanly; a checksum-failing artifact is rejected and never activated.
- [ ] With the embedder absent, capture is visibly blocked/paused and **no zero-vector rows are written** (asserted by a test at the capture boundary).
- [ ] With Python/ffmpeg absent, Meetings/TTS/agent surfaces show an unavailable state; no crashes; core search unaffected.
- [ ] A PR failing typecheck, vitest, or `cargo test` cannot merge.
- [ ] A tag push yields a GitHub Release with DMG + signed updater manifest, and an installed vN app updates in-place to vN+1.
- [ ] LICENSE exists and README references it.

## Test plan

- **CI:** `make test` parity on a macOS runner. Risk: Apple Vision / ONNX-dependent tests may need feature-gating or fixtures on runners — tier into per-PR unit/typecheck and a nightly full-suite job if needed.
- **Unit:** checksum verification, registry entries, dependency preflight.
- **Boundary:** capture with missing embedder (FR6) — must not regress the resolved 2026-05 stacked-issues drop fix; both concern silent capture loss.
- **Manual QA matrix (documented checklist):** clean-VM install, permission grant/deny paths, download interrupt/resume, update vN→vN+1, degraded-Python environment.

## Rollout / migration plan

Order of shipping (each independently mergeable): LICENSE → CI test gate → FR6 zero-embedding gate → model registry + onboarding step → signing/notarization pipeline → updater → first public tag `v0.3.0`. Existing dev installs are unaffected (updater only applies to release artifacts). No data migration; models directory layout unchanged.

## Risks

- ~~**Apple Developer enrollment lead time**~~ *(resolved 2026-07-06: notarization descoped for the college-project phase; ad-hoc signing + right-click → Open instead.)*
- **CI macOS runners vs. native deps:** Vision OCR and ONNX paths may not run headless — mitigate with test tiers; do not let this stall the PR gate for pure-Rust/TS tests.
- **Architecture target:** universal binaries may fight ONNX/llama build complexity — recommend aarch64-only for beta and revisit x86_64 on demand.
- **HuggingFace URL stability** for pinned model downloads — mirror critical artifacts to GitHub Releases if flakiness appears.
- **FR6 is a behavior change on the capture hot path** — the last change in this area caused silent frame drops; guard with the boundary test before shipping.

## Open questions

1. ~~**License**~~ — resolved 2026-07-06: **open core; repository under Apache-2.0** (LICENSE added; `Cargo.toml` corrected from an unbacked `MIT` claim).
2. ~~**Distribution identity**~~ — resolved 2026-07-06: no Apple Developer account for the college-project phase; ad-hoc signing.
3. **Qwen in onboarding:** mandatory or recommended-optional? (Recommendation: optional — protects the 10-minute target; the banner already handles post-hoc download.)
4. **aarch64-only acceptable for the beta cohort?** (Recommendation: yes.)

## ADR candidates

- **ADR 011 — Signed release and auto-update channel:** GitHub Actions as the sole release path; Tauri updater with signed manifests; aarch64-first.
- **ADR 012 — Required-model gating and graceful degradation:** embedder is a hard capture prerequisite (supersedes degrade-to-mock for the durable write path); optional features declare runtime dependencies and degrade visibly.

## Suggested issues (vertical slices, in shipping order)

1. Add LICENSE + README reference *(blocked on decision)*
2. CI: test workflow gating PRs (`test.yml`)
3. Capture gate: block zero-embedding writes + surface embedder status (FR6)
4. Model registry: add MiniLM embedder entry with SHA-256 + resume (FR5a)
5. Onboarding: required-model step wired into existing flow (FR5b/FR8)
6. Python/ffmpeg preflight + typed feature-unavailable states (FR7)
7. Release workflow: build → sign → notarize → staple → GitHub Release (FR3)
8. Updater integration + update UX (FR4)
9. Clean-VM QA checklist under `docs/product/`
