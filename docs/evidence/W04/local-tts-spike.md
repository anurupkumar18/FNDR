# Local neural TTS spike (2026-10-09)

Verdict: **NO-GO for now.** The only natural-sounding offline model with a permissive licence (Kokoro-82M, Apache-2.0) needs a phonemizer, and every shipping runtime measured here gets one from espeak-ng, which is GPL-3.0. The speech provider id `local_neural` stays unregistered; `system_enhanced` (best installed Mac voice) is the offline voice.

## Setup

- Apple M1 Pro, 16 GB, macOS 27.2. Measured while other sessions ran Rust builds on the same machine, so the timings are an upper bound, not a clean benchmark.
- sherpa-onnx 1.13.8 (Python wheel, Apache-2.0), `num_threads=2`, models from the k2-fsa `tts-models` release.
- Three FNDR-style lines: 15, 100 and 109 characters. Load is the time to construct the engine from cold files.

## Numbers

| Model | Download | Model on disk | Load | First line (15 chars) | RTF (100 / 109 chars) | Peak RSS |
| --- | --- | --- | --- | --- | --- | --- |
| Kokoro v0.19 int8 | 103 MB | 134 MB + 5.8 MB voices + 18 MB espeak data | 0.81 s | 1.56 s | 0.96 / 1.38 | 379 MB |
| Kokoro v0.19 fp32 | 320 MB | 346 MB + 5.8 MB + 18 MB | 0.81 s | 1.01 s | 0.59 / 0.58 | 630 MB |
| Piper en_US amy medium | 67 MB | 63 MB + espeak data | 0.67 s | 0.11 s | 0.053 / 0.059 | 325 MB |

RTF is synthesis time over audio length; under 1 is faster than real time. The int8 Kokoro was slower than fp32 on this CPU.

## Licences

- Kokoro weights: Apache-2.0 (LICENSE in the release). Redistribution is fine.
- sherpa-onnx: Apache-2.0, but `libsherpa-onnx-c-api.dylib` exports 14 espeak-ng symbols: espeak-ng is compiled in for Kokoro and Piper phonemization. espeak-ng is GPL-3.0 (its COPYING), and its data directory ships with every model.
- Piper US voices: the amy model card says "License: See URL" and was fine-tuned from lessac; lessac's dataset is under the Blizzard 2013 research licence agreement. Not shippable in a commercial product without clearance.

## Reuse in FNDR

`ort = 2.0.0-rc.12` is already in `src-tauri/Cargo.toml` (used by `embedding/onnx.rs`), so Kokoro's ONNX graph could run in-process with no new runtime. The missing piece is text to phonemes.

## Why no-go

1. Licence: shipping espeak-ng inside a closed macOS app is a GPL-3.0 obligation FNDR has not taken on. Piper's natural US voices come from a research-only dataset.
2. Latency: Kokoro fp32 needs about 1 s for a short line and 3.3 s for a 100-character one on two threads, so it misses the registry's 1.5 s start budget unless it streams sentence by sentence.
3. Memory: 380 to 630 MB resident while loaded, beside FNDR's embedding model and LanceDB.

## Path to GO

Kokoro fp32 on the existing `ort`, a permissive grapheme-to-phoneme step ported to Rust (a lexicon such as CMUdict plus rules for unknown words, as misaki does), sentence streaming to the webview, and the model fetched on demand through `downloads.rs` behind a Settings consent. The G2P port is the large part: several days, with its own pronunciation tests.

## Not done

- No sample was heard: the WAVs were written to the session scratchpad and nobody has listened to them, so nothing here says how natural any model sounds.
- No clean-machine run and no 4-thread run.
