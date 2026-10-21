# Zero-install runtime audit (2026-10-15)

FNDR's core capture, OCR, embedding and search run in the packaged Rust app. A new installation still downloads the required MiniLM embedding model before capture starts. Voice transcription, agent features and some optional models have further runtime dependencies. This audit distinguishes source inspection from commands run on one already provisioned Apple M1 Pro Mac with 16 GB RAM and macOS 27.2. No clean Mac or real user data was used. The first-run failure rate is **unmeasured**; one provisioned machine cannot establish a rate. The failure modes below come from the code paths.

## First-run inventory

Sizes marked "measured" are file or disk sizes on this Mac or in the October 12 app bundle. Other sizes are constants, earlier evidence or estimates. "Terminal" means the person must arrange a tool outside FNDR, even if FNDR can start the install itself. A blank missing feature means core FNDR still opens.

| Component and first-run path | Size and machine needs | Missing or failed behavior | Terminal or outside setup | Evidence |
| --- | --- | --- | --- | --- |
| MiniLM embedding ONNX plus tokenizer, required download in onboarding | Model 90,387,606 bytes in `model_config.rs`, plus tokenizer; network and storage. URL is revision pinned and SHA-256 checked. | Capture waits until install completes. The downloader resumes with HTTP Range. | No | Read `src-tauri/src/inference/model_config.rs`, `src-tauri/src/ipc/commands/onboarding.rs`, ADR 012. |
| Qwen3-VL-2B Q4_K_M, optional recommended model | 1,107,409,952 byte file measured in the prior spike; code budgets 1.5 GB disk and 3.5 GB RAM. Network. | Text extraction uses the existing fallback when unavailable. URL uses `resolve/main` and has no SHA-256. The pixel path needs an mmproj that FNDR does not download, so fresh-install photo import cannot use that path. | No for main model; mmproj has only a bootstrap script | Read `model_config.rs`, model download flow and `scripts/bootstrap/download-qwen3-vl-mmproj.sh`; prior file measurement in checkpoint J. |
| Kokoro local TTS model, consent-gated download | 18 files, about 134 MB in prior measurement. Pinned commit and SHA-256, resumable. In-process ONNX Runtime. | Local neural voice is unavailable until installed; other app functions continue. | No | Read `src-tauri/src/tts/install.rs`, ADR 030; size from prior spike. |
| Whisper small model and `whisper-cli`, used for voice input and transcripts | Model about 466 MB, estimate only; `whisper-cli` is external, size not measured. Model URL uses `resolve/main` without checksum. | If CLI is missing or fails, FNDR tries the Python path. If both fail, transcription returns an error. | Yes today: `whisper-cli` is found in PATH or Homebrew paths, but not shipped. | Read `src-tauri/src/speech.rs`; focused tests cover candidate selection and fallback with fakes. |
| `ffmpeg`, for browser-recorded non-WAV audio | External binary, size not measured; Homebrew or another installed path. | Without it, compressed audio cannot be normalized for `whisper-cli`; meeting Start is disabled when absent. Other transcription paths may fail. | Yes today | Read `speech.rs`, `src/domains/workspace/MeetingRecorderPanel.tsx`; no missing-program run on this provisioned Mac. |
| `whisper_gguf_runner.py` and owned Python venv | Sidecar 10,312 bytes measured; venv and `whisper-cpp-python` size not measured. Python 3.13 or older, pip, network and Command Line Tools for compilation. | FNDR creates the venv under private app storage and attempts pip install on fallback. Missing Python or failed install surfaces a transcription error; one error suggests `brew install python@3.13`. | Yes on a clean Mac | Read `speech.rs`, `src-tauri/sidecars/whisper_gguf_runner.py`; this Mac has Python and CLT, so no clean-install success is claimed. |
| `parakeet_runner.py` | 2,845 bytes measured in Resources; its requirements name `nemo_toolkit[asr]` and `openai-whisper`, potentially multi-GB if installed. | No normal Rust caller; only an override command can use it. | Yes if manually used | Read bundle config, sidecar and `requirements.txt`; no runtime run. |
| `orpheus_tts_runner.py` and Orpheus Python venv | Sidecar 5,963 bytes measured; 3B GGUF and venv sizes not measured. Python, pip and network if reached. | No caller outside `speech.rs` for `synthesize_speech`; Kokoro supersedes it. | Yes if manually reached | Read `speech.rs`, sidecar and ADR 030; no runtime run. |
| Laya runner and venv | Current lane checkout still has a 733 byte `laya_decide_runner.py`; old path makes a venv in `~/Documents/FNDR Laya`, then unpinned `pip install laya`. Size not measured. | Old failure returned a zero-confidence decision. The owner checkout has since removed the bridge and sidecar; this is an integration note, not a dependency to replace. | Old path needed Python, pip, network and possibly CLT; removed path needs none | Read current lane checkout; removal reported in `FNDR-exchange/inbox/J.md`. Prior machine probe found no Laya venv. |
| Native `fndr-speech` and `fndr-auth` Swift helpers | 164 KB and 68 KB measured in October bundle; speech helper app is a second 172 KB copy. Built with macOS 26 SDK; speech assets may download through `AssetInventory`. | Live mic path has no file-transcribe mode. macOS 13 to 25 helper behavior has not been verified, despite app minimum 13. | No runtime terminal | Read `build.rs`, helper Swift source and bundle; sizes measured with `du`. |
| Hermes agent, optional | Not present in October bundle. Existing local Hermes runtime was 540 MB measured in prior probe. Install pulls `uv`, pinned Hermes commit, Python 3.11 and five locked extras. | Missing Hermes leaves its agent features unavailable. `curl` installs uv, `/usr/bin/git` fetches source, then `uv venv` and `uv sync` run. A clean Mac may show a Command Line Tools prompt on `/usr/bin/git`. | FNDR offers Install, but it invokes shell tools and network | Read `hermes_agent.rs`, `hermes_codex.rs`, `build.rs`, Setup center and October bundle; no clean install run. |
| Codex binary and ChatGPT sign-in, optional | Not bundled; installed binary size not measured. Installer fetches the official HTTPS shell script; app-server sign-in opens browser OAuth. | Setup row can install. Agent calls return a missing executable error until installed. | No terminal UI step, but FNDR runs `sh`; network and an account are needed | Read `setup_center.rs` and Codex detection; this Mac already has Codex, so missing-state runtime was not observed. |
| Computer Use package, optional | `open-computer-use@0.3.6` installed globally by npm; package and Node sizes not measured. | Without npm, Setup center sends the person to Node.js first. Tool remains unavailable until Node and package install. | Yes, Node must be arranged outside FNDR | Read `setup_center.rs` and computer-use detection. |
| Ollama, optional Hermes provider | External app and model size depend on selection; FNDR only detects it. | Its provider is unavailable; core FNDR does not need it. | Outside install if chosen | Read Hermes setup path. |
| Native core and build-time tools | Main executable 146 MB in October bundle; app 147 MB, DMG 54 MB measured. Rust, ONNX Runtime and Metal are packaged. CI needs macOS 26 SDK and OpenSSL 3 at build time. | A downloaded app does not need Rust, Python or Xcode for core capture and search. | No at runtime | Read `Cargo.toml`, `build.rs`, release workflow; bundle measured with `du`. |

The machine probe in the prior checkpoint found `ffmpeg`, `whisper-cli`, `uv`, Node, npm, Ollama, Codex and ChatGPT.app installed. `/usr/bin/python3` reported 3.9.6. That confirms only presence on this machine, not that each feature works after a clean install. The new Setup center voice row probes file paths without executing Python or triggering a Command Line Tools dialog. Its Rust tests passed, including missing-combination messages; the frontend renders Setup components generically. It does not install voice tools.

## Removal order by first-run friction

Rank is based on the number of outside installs or prompts before a person can use the feature, then the likelihood of reaching the path. It is a proposed sequence, not changes made in this lane.

| Rank | Friction now | Replacement and release gate |
| --- | --- | --- |
| 1 | Voice search and meeting transcripts need `whisper-cli`, `ffmpeg`, and sometimes a Python venv with a compiled pip package. | Bundle a pinned, signed `whisper-cli` or a native binding; pin and verify its model; decode browser audio in Rust. Evaluate a file-transcribe mode in the existing Swift helper on macOS 26+, but first prove it reads MediaRecorder output and meets quality targets. Keep the Setup row until the replacement is shipped. |
| 2 | Computer Use needs Node before FNDR can run npm. | Replace with a native Rust or Swift path, or remove this optional setup row until it can be zero-install. |
| 3 | Hermes invokes `curl | sh`, Git and `uv` to fetch Python and extras; `/usr/bin/git` may trigger CLT installation. | Package signed `uv`, pinned Hermes source and a permitted Python runtime, or leave Hermes out of first run. Check redistribution rights and artifact size first. |
| 4 | Codex is a separate binary and sign-in. | Keep the official install path, but add in-app progress and verify the downloaded installer; consider bundling only after license review. |
| 5 | Qwen's unpinned main URL and absent mmproj can cause a later photo-import surprise. | Pin revision and SHA-256, and add a matching 2B mmproj to the normal downloader. Verify the vector and model contract before changing embedding or inference behavior. |
| 6 | Unused Parakeet and Orpheus sidecars occupy the bundle and suggest runtimes FNDR does not use. | Delete their resources, dead Rust entry points, Python requirements and documentation together after the integrator confirms no hidden caller. This removes complexity but little first-run friction. |
| Removed in owner checkout | Laya once installed unpinned Python into Documents. | Keep it removed. Do not reintroduce it as a first-run install. Verify the decision fallback in the integrated tree. |

Keep the MiniLM downloader and Kokoro installer. Their pinned, checked and resumable downloads are useful patterns for model-dependent features.

## LM-08: Apple Foundation Models spike

**Run in the prior session on this Mac:** `SystemLanguageModel.default.availability` was `available`, and the locale was supported. Swift was compiled with `SDKROOT=/Library/Developer/CommandLineTools/SDKs/MacOSX27.0.sdk xcrun swiftc -O`. The benchmark files live only in the prior session's scratchpad and are not product code. The inputs were synthetic: four screen texts mapped to `{project, topic, outcome, nextSteps}` and five commands mapped to `{tool, argument}` using `DynamicGenerationSchema`. `@Generable` and `@Guide` failed to compile with Command Line Tools alone because the macro plugin was missing. Full Xcode is needed for that form; the dynamic schema worked here.

| Case | Measured latency in ms | Observation |
| --- | --- | --- |
| Four structured screen extractions | 4,437; 2,169; 1,440; 980 | First includes warmup. All four returned schema-conforming JSON. |
| Five tool routes | 1,003; 750; 980; 904; 905 | All returned schema-conforming JSON. One "what's the weather like" case incorrectly chose `openApp` instead of no tool. |
| Four 3,823-character extractions | 4,985; 3,905; 3,399; 3,236 | Near the current 4,000-character extraction cap; same JSON each run. |
| Session creation and `prewarm()` call | 22; 54 | Call return time, not proof that model loading completed in 54 ms. |

Nine of nine responses conformed to their schemas, but the route error and a screen case that treated `storage::lance_store` as a project show that schema validity is not semantic accuracy. Four screen samples and five commands are not an accuracy estimate. The spike process RSS rose from 9 to 19 MB. The system inference service showed 14 to 37 MB idle and 102 MB during a run. `footprint` could not attach without root, so wired memory, ANE use and total model residency were not measured.

The current Qwen file was measured at about 1.1 GB, with a 3.5 GB RAM target. Earlier local evidence reports up to 44 tokens/s decode and 10.8 s first initialization, 1.36 s later. The Apple runs took roughly 3 to 5 s at production-sized input and required no FNDR-managed model download. These are different workloads, so they are directional, not an equal-input speed comparison. Apple Foundation Models requires macOS 26+ and Apple Intelligence availability; FNDR's minimum is macOS 13. The 4,096-token context and `deviceNotEligible`, `appleIntelligenceNotEnabled` and `modelNotReady` states require a fallback. A Tauri integration would use a Swift helper like the existing speech and auth helpers, with line-delimited JSON and dynamic schema. Keep this as an optional local tier only after LM-07 labeled quality scoring. Never select `PrivateCloudComputeLanguageModel` for FNDR's local-only path. No helper was packaged in this lane.

## Signing and notarization checklist

The checklist was run against the **existing October 12 v0.3.0 artifact**, not a new build with this lane's Setup row. The machine had 17 GiB free before checks, and the inbox instructed this lane not to start a full Tauri build. The existing app is 147 MB and its DMG is 54 MB by `du`.

| Check | Result and evidence |
| --- | --- |
| Verify code seal | **Run, pass:** `codesign --verify --deep --strict FNDR.app` exited 0. |
| Inspect main signature | **Run:** `codesign -dvv FNDR.app` reported ad hoc signing, hardened runtime, no Team ID. |
| Inspect nested speech helper | **Run:** `codesign -dvv FNDR.app/Contents/Resources/FNDR Speech Helper.app` reported ad hoc signing and no hardened runtime. Its own microphone entitlement was not established. |
| Gatekeeper assessment | **Run, fail:** `spctl --assess --type execute --verbose=4 FNDR.app` returned `rejected`, exit 3. |
| Stapled ticket | **Run, fail:** `xcrun stapler validate FNDR.app` said no ticket, exit 65. No notarization submission was made. |
| Version and usage strings | **Read:** `Info.plist` still says 0.2.11 for both bundle versions while Tauri and Cargo say 0.3.0. It has microphone, speech, screen, accessibility, calendar and folder strings. It lacks `NSAppleEventsUsageDescription` even though the app has the automation entitlement and uses `osascript`. |
| CI signing path | **Read:** `.github/workflows/release.yml` uses Developer ID and notarization only when Apple secrets exist. `docs/setup/releasing.md` records a 45-minute build that stopped because `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` did not open the updater key. |
| Clean install and update | **Pending:** no signed and notarized artifact, clean-machine run, update from vN to vN+1, or post-update TCC prompt check was possible here. |

Before release, provide the Developer ID certificate and seven Apple secrets listed in `docs/setup/releasing.md`; repair the updater key password without replacing the installed public key; sign every nested executable and helper with hardened runtime, secure timestamp and the helper's needed audio-input entitlement; synchronize `Info.plist` versions and add the Apple Events usage string. Confirm whether tauri-action re-signs the nested helper. Then run the release workflow by hand, verify the signature and stapled ticket, install on a clean Mac without Homebrew, Python or ffmpeg, and test an update with Screen Recording and microphone prompts. The October artifact proves that the current ad hoc build is sealed; it does not prove Developer ID signing or notarization works.

## Open measurements

- Measure a clean-machine success and failure rate per feature, with a stated sample and OS mix. This audit has no denominator.
- Test WKWebView MediaRecorder output with `AVAudioFile`, and score SpeechAnalyzer file transcription against Whisper on labeled synthetic or consented audio.
- Measure Foundation Models wired and ANE residency with Instruments, then run LM-07 and GS-06 quality cases before choosing it as a route.
- Confirm distribution rights and shipped sizes for Codex, uv, Python and a bundled Whisper binary.
