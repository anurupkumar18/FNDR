# Apple Foundation Models text backend, 2026-10-15

## Decision

On a Mac where the system model is available and enabled, FNDR can complete onboarding without downloading Qwen. Apple handles extraction, memory snippets and answers from supplied memory snippets when no GGUF is installed. With Qwen installed, the measured default for those three tasks is Qwen. Other catalog tasks retain the GGUF route until they have comparable quality evidence. The vision path remains GGUF only.

The Apple switch defaults on only when the helper reports `available`. An explicit off setting persists. The Intelligence panel shows the last text backend, and `llm_traces.jsonl` records the backend without adding prompt text.

## Synthetic comparison

The benchmark uses the same cases from `src-tauri/tests/fixtures/gold/v0/extraction.jsonl` and `questions.jsonl` for both backends. It calls the production `InferenceEngine` extraction, snippet and answer methods. Each process performs one cold call, then measures three calls after warmup. Cold time includes model initialization and the first call. A p95 from three samples is only the largest sample, so these numbers support routing but are not a stable tail-latency estimate. The fixture labels are draft and await owner review.

The Apple schema descriptions are centralized in `inference/prompts.rs` and counted in the prompt fingerprint guard. Traces use `v10` for snippet and answer, and `source_refs_v6` for typed extraction fields.

| Task | Backend | Format valid | Method accepted | Evidence clean | Cold ms | Warm p50 ms | Warm p95 ms |
| --- | --- | ---: | ---: | ---: | ---: | ---: | ---: |
| Extraction | Apple | 3/3 | 3/3 | 2/3 | 13815 | 10433 | 11156 |
| Extraction | Qwen | 3/3 | 3/3 | 0/3 | 3015 | 3270 | 3483 |
| Snippet | Apple | 3/3 | 2/3 | n/a | 4806 | 2981 | 3193 |
| Snippet | Qwen | 3/3 | 3/3 | n/a | 2180 | 564 | 587 |
| Answer | Apple | 3/3 | 3/3 | n/a | 5391 | 1971 | 3216 |
| Answer | Qwen | 3/3 | 3/3 | n/a | 1371 | 365 | 867 |

Format valid means the traced backend output parsed as an object for extraction, or was nonempty for the two text tasks. Apple's typed extraction fields are normalized into FNDR's JSON shape before the trace is written. Method accepted is the production method's nonempty result after its checks. The benchmark emits this as `validator_pass`. Evidence clean applies only to extraction and means `finalize_extraction` recorded no source evidence issues. The host filters or removes unsupported references even when the method accepts a draft. These checks do not prove semantic accuracy or factual correctness of an answer.

The initial free-form JSON string wrapper produced zero accepted Apple extraction drafts in a later rerun. The typed extraction field schema fixed format validity and produced three accepted drafts. An earlier exploratory field run matched numbered source lines on only one of three calls; the final production run had two of three drafts with no evidence issues. This small sample does not establish a stable quality advantage. Qwen remains the installed default because it is substantially faster, while Apple enables extraction without a GGUF download.

Run from `src-tauri/`:

```sh
export SDKROOT=/Library/Developer/CommandLineTools/SDKs/MacOSX27.0.sdk
export CXXFLAGS="-isystem $SDKROOT/usr/include/c++/v1"
export CARGO_BUILD_JOBS=2 CARGO_TARGET_DIR=~/FNDR/src-tauri/target
cargo run --example foundation_benchmark --no-default-features -- apple extraction
$CARGO_TARGET_DIR/debug/examples/foundation_benchmark apple snippet
$CARGO_TARGET_DIR/debug/examples/foundation_benchmark apple answer
$CARGO_TARGET_DIR/debug/examples/foundation_benchmark qwen extraction /path/to/Qwen3VL-2B-Instruct-Q4_K_M.gguf
$CARGO_TARGET_DIR/debug/examples/foundation_benchmark qwen snippet /path/to/Qwen3VL-2B-Instruct-Q4_K_M.gguf
$CARGO_TARGET_DIR/debug/examples/foundation_benchmark qwen answer /path/to/Qwen3VL-2B-Instruct-Q4_K_M.gguf
```

The direct binary invocations used the already built example to avoid repeated Cargo locking. All three final Qwen runs printed their metrics, then failed with exit 134 at llama.cpp Metal teardown: `GGML_ASSERT([rsets->data count] == 0) failed`. The benchmark commands therefore did not pass as processes, even though the calls yielded measurable outputs. The example uses installed model weights only and synthetic fixture text. It creates and removes its own temporary app directory.

## Route table

| Task IDs | With Qwen installed | No Qwen, Apple ready and enabled | No usable Apple model |
| --- | --- | --- | --- |
| `memory_extraction`, `memory_snippet`, `answer` | Qwen | Apple | Existing deterministic fallback |
| `daily_summary_narrative` | Apple first, Qwen when Apple fails the checks | Apple | Basic per-theme summary composed in code |
| `memory_extraction_repair`, `memory_review`, `memory_review_repair`, `card_synthesis`, `query_expansion`, `screen_guide`, `todo_extraction`, `task_suggestion`, `meeting_breakdown`, `eval_judge`, `query_plan` | Qwen | Existing fallback | Existing fallback |
| `vision_description` and the pixel runtime | Qwen | Existing fallback | Existing fallback |

The current measured routes choose Qwen first whenever it is installed. The completion and validation paths also have a GGUF retry if a later task route chooses Apple first with GGUF present. With no GGUF, each caller's existing deterministic fallback applies after an Apple failure. The table in `src-tauri/src/inference/foundation.rs` is the route source of truth.

## Platform and packaging

The app's minimum is macOS 13. The helper compiles with that deployment target and reports `unsupported_os` before calling Foundation Models on systems below macOS 26. Foundation Models also requires an eligible device and enabled Apple Intelligence. Probe states distinguish an unavailable model, Apple Intelligence off, unsupported device, unsupported OS and helper failure. No private cloud model API is used.

The macOS 27 SDK exposes `Attachment<ImageAttachmentContent>` initializers for `CGImage`, `CIImage`, `CVPixelBuffer` and image URLs, plus transcript image attachments. API presence does not establish FNDR screen-image quality, so the vision path has not moved.

The Swift helper is built by `src-tauri/build.rs` as an external Tauri binary. The development build is ad hoc linker signed, like the existing helper path. Release signing and notarization of a full app bundle were not run here. Cold process starts were measured above. A second fresh Apple answer process had a 6145 ms cold call and 915 ms warm p50 while other Cargo builds were active. This shows restart warmup varies; waking after sleep was not measured. The real protocol test covered two sequential generations, active cancellation, invalid requests, probe and clean exit. A fake helper test covers restart after process death and the Rust answer path.

## Verification limits

`make eval` is not present in this checkout, so the benchmark example uses the existing draft gold fixtures and production methods. Tasks outside extraction, snippet and answer were not benchmarked on Apple. No full Tauri bundle was built because the shared disk is nearly full and other agents are building Rust in the shared target. No real user captures, vault data or personal files were used.
