# ADR 030: An offline neural voice with Kokoro-82M and no espeak

## Status

Accepted 2026-10-09. The owner chose Kokoro-82M (https://huggingface.co/hexgrad/Kokoro-82M) in conversation on 2026-10-09; the shape below was decided under the delegation rule in `AGENTS.md`. Built 2026-10-10: `src-tauri/src/tts/`, `src-tauri/src/ipc/commands/tts.rs`, `src/shared/voice/localNeural.ts`, the neural voice row in `src/shared/voice/VoiceOutputSection.tsx`.

Supersedes the NO-GO verdict of `docs/evidence/W04/local-tts-spike.md`.

## Context

The spike of 2026-10-09 ruled Kokoro out because every shipping runtime it measured (sherpa-onnx) compiles in espeak-ng, which is GPL-3.0, to turn text into phonemes. Two facts change that:

- Kokoro-82M's weights are Apache-2.0 (model card metadata of hexgrad/Kokoro-82M at `f3ff357`; the ONNX export onnx-community/Kokoro-82M-v1.0-ONNX at `1939ad2` is Apache-2.0 too). 82M parameters, StyleTTS 2 with an ISTFTNet decoder, 54 voices in v1.0.
- Kokoro's own phonemizer, misaki (https://github.com/hexgrad/misaki, Apache-2.0), reads English from gold and silver pronunciation dictionaries. espeak-ng is only an optional fallback for words the dictionaries miss; misaki 0.9.4 on PyPI defaults to `fallback=None`.

So a path with no copyleft exists: the dictionaries plus misaki's rules, ported to Rust, feeding Kokoro's ONNX graph through the `ort` crate FNDR already ships for embeddings.

## Decision

1. **Runtime.** Kokoro's ONNX export runs in-process through `ort` (ONNX Runtime 1.24.2, the prebuilt `ort 2.0.0-rc.12` bundles) on the CPU with up to four threads. No CoreML: measured on this Mac it loaded in 5.5 s instead of 0.4 s, used more memory, and was no faster (table below). `Cargo.toml` is unchanged.
2. **Model file.** `model_uint8f16.onnx` (114 MB). On ONNX Runtime 1.24 the 8-bit `model_quantized.onnx` (92 MB) runs at a real-time factor of 0.52, the fp32 model at 0.22 and `uint8f16` at 0.19. (On ONNX Runtime 1.31 the 8-bit model is the fastest; if `ort` moves to a newer runtime, measure again.)
3. **Pronunciation.** `tts/lexicon.rs` and `tts/g2p.rs` port misaki 0.9.4's `en.py` rule for rule: gold then silver dictionaries, the capitalised and lowercased forms, stress marks, function words that follow the next sound ("the", "to", "a", "an", "in", "I", "used to"), the `-s`, `-ed` and `-ing` stems, possessives, hyphenated words and the stress demotion inside them, US and GB. spaCy is replaced by a small neighbour-rule tagger that covers what the dictionaries distinguish by part of speech (read, record, live, used, that).
4. **Out-of-vocabulary policy.** A word neither dictionary has is never sent to espeak, a network or another model. `tts/lts.rs` spells it from deterministic letter rules into misaki's phoneme set; a word without a vowel letter is spelled out letter by letter; digits inside a word are read as numbers; a run of capitals before a capitalised word ("VSCode") is spelled and the word looked up. In a word made of pieces ("PostgreSQL"), only the missing piece is guessed. The guess aims to be intelligible, not right.
5. **Normalisation.** `tts/normalize.rs` turns numbers, ordinals, money, percentages, ISO and written dates, times, ranges, units, version numbers, common abbreviations, links (by host), email addresses and symbols into words first. What should never be read aloud (secrets, long tokens) is still dropped by the caller (`speakable()` in Notch Do).
6. **Download on demand, with consent and checksums.** Nothing is bundled. Settings > Voice shows "Install (134 MB)"; the person reads what is fetched and from where and presses "Download". `tts_install` refuses without `consent: true`. Eighteen files (the model, 13 voices graded C or better on the model card, and misaki's four dictionaries) are pinned to a commit and a SHA-256 in `tts/install.rs`; each download resumes from a `.partial` file, is checked, then moved into place. `install.json` records the consent time. "Remove" deletes the folder, consent included.
7. **Privacy.** After the download the voice uses no network. Text goes from the webview to Rust and audio comes back as WAV bytes; nothing is written to disk. The two download hosts are recorded in Privacy Activity through `privacy_proof::record_egress`.
8. **Order.** The registry tries `codex_realtime`, then `local_neural`, then `system_enhanced`, then `webview_basic`. Until the voice is installed `available()` says why ("not installed", "downloading (42%)", the install error) and the registry falls through.
9. **Playback.** The webview splits a line into sentences (keeping "Dr." and "e.g." whole, cutting a sentence over 220 characters at a comma), synthesizes the next sentence while the current one plays, and schedules the clips back to back on one AudioContext. Cancel stops every scheduled clip at once and tells Rust to stop between model-sized pieces.
10. **Locking.** Synthesis is serialized by the voice's own mutex, not `AppState::model_pipeline_lock`: a spoken line must not wait behind a screenshot's language-model call. The model loads on first use and is dropped after five idle minutes.

## Options considered

| Option | Why not |
|---|---|
| A. Kokoro through `ort` with a Rust port of misaki's dictionary G2P (chosen) | Several days of porting and its own tests; unknown words are guessed by rules. |
| B. sherpa-onnx with its bundled espeak-ng | espeak-ng is GPL-3.0 and compiled into the library (14 exported symbols, spike of 2026-10-09). Shipping it in a closed app is an obligation FNDR has not taken on. |
| C. Piper voices | The natural US voices are fine-tuned from lessac, whose dataset is under the Blizzard 2013 research licence; Piper also phonemizes with espeak-ng. |
| D. Cloud text-to-speech (OpenAI, ElevenLabs, Azure) | Per-character API billing to a key, and every spoken line leaves the Mac. The ChatGPT plan voice (ADR 028) already covers the cloud case without a key. |
| E. Keep the Mac's system voices only | No download, no licence work, but the compact voices on most Macs are what prompted ADR 028. Stays the fallback. |
| F. misaki's espeak fallback for unknown words | GPL-3.0 again, for the words that matter most (names). |
| G. CoreML execution provider | Measured: 5.5 s load, about 1.1 GB resident, first line 0.9 s, no faster than the CPU. |

## Measurements (Apple M1 Pro, 16 GB, macOS 27.2, other builds running)

ONNX Runtime 1.31 (Python), 4 threads, "Hello there." and a 4.2 s sentence:

| Variant | Size | Load | Short line | RTF | Peak RSS |
|---|---|---|---|---|---|
| fp32 `model.onnx`, CPU | 326 MB | 0.73 s | 0.37 s | 0.24 | 603 MB |
| fp32, CoreML | 326 MB | 5.56 s | 0.93 s first, 0.37 s after | 0.24 | 1096 MB |
| 8-bit `model_quantized.onnx`, CPU | 92 MB | 0.41 s | 0.19 s | 0.12 | 420 MB |
| 8-bit, CoreML | 92 MB | 5.43 s | 0.84 s first | 0.18 | 764 MB |

ONNX Runtime 1.24.4 (Python, the runtime family FNDR's `ort` bundles): 8-bit RTF 0.52, `q8f16` 0.52, fp16 0.22, fp32 0.22, `uint8f16` 0.19.

FNDR's Rust path (`measure_and_write_samples`, `uint8f16`, optimised test build): model load 0.74 s, US dictionaries 0.13 s, first short line 0.35 s after load, RTF 0.19 to 0.20 over five samples (2.8 s to 18.3 s of audio), G2P under 3 ms per sentence, peak RSS of the test process 807 MB. The same run with fp32 gave RTF 0.22 and 924 MB.

## Consequences

- Repository growth is code only (about 5,100 lines including tests and the vocabulary table). The dictionaries (12.6 MB) are downloaded with the model; a 68-entry sample of `us_gold.json` sits in a test.
- Every new heteronym rule is a guess at what spaCy would tag. The golden test lists the cases checked.
- The 0.3 s of silence Kokoro puts before each clip becomes the pause between sentences.
- Kokoro's training data includes synthetic audio from closed TTS providers (model card); the licence stated for the weights is Apache-2.0. Recorded, not resolved.

## Verification

- Unit (43 tests, `cargo test --lib tts`): stress rules, dictionary forms, function words, heteronyms, stems, possessives, initialisms, the subtoken scanner, hyphenated words, letter rules within the accent's symbol set, normalisation, vocabulary ids against the model card's token list, the 510-token split, voice pack rows, the WAV header, pinned artifacts, consent.
- Real dictionaries (`golden_sentences_match_misaki_with_the_full_dictionaries`, ignored by default): 15 US and 4 GB sentences come out exactly as misaki 0.9.4 with spaCy `en_core_web_sm` produced them, including contractions, possessives, hyphens and heteronyms.
- Real download (`real_install_downloads_and_verifies_every_file`, ignored): every file fetched from Hugging Face and GitHub and checked.
- Real model (`real_model_speaks_a_sentence_with_plausible_audio`, `measure_and_write_samples`, ignored): non-silent audio (RMS 0.06 to 0.07), no clipped samples, plausible durations.
- Webview, fakes: sentence pipelining, voice ids, cancel under 150 ms, cancel before the first clip, a failure falling back, registry order, the Settings consent flow, choosing, previewing and removing.

## Unverified

- How it sounds. Nobody has listened; the samples are in the session scratchpad for the owner.
- Pronunciation of names, URLs and product names beyond the cases in the tests.
- Audio through FNDR's own webview (WKWebView `AudioContext` and `decodeAudioData` in the packaged app).
- Battery and thermal cost of long answers.
- That the 13-voice choice and grades match what people prefer.
