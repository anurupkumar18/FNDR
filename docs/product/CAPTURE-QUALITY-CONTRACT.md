# FNDR capture-quality contract

FNDR should turn visible, reviewable work into useful local memories. It must
not turn a filename, an unreadable screen, or generic visual metadata into a
confident claim.

## The quality path

1. **Capture admission.** The capture pipeline skips FNDR itself, blocklisted
   apps and sites, duplicate frames, and privacy-protected content before it
   becomes a memory candidate.
2. **Grounded synthesis.** OCR text, window context, and local model output
   produce a structured record: what happened, why it mattered, topic, source
   context, and confidence. A visual fallback is explicitly labelled instead
   of being treated as evidence.
3. **Storage outcome.** Records retain their lifecycle and storage outcome.
   `low_quality_evidence` is reviewable; it is not silently promoted to a
   useful memory.
4. **One shared surfacing rule.** `partition_surfaceable` rejects image-only,
   filename-only, unreadable, and ungrounded visual-fallback records. Search,
   Memory Vault, Search & Ask FNDR, and Daily Summary use this policy. The
   Vault's **Needs more signal** queue keeps the reason visible without
   exposing a misleading filename as a memory.
5. **Grounded answers.** Search & Ask FNDR retrieves only local surfaceable
   memories, cites the records it used, and says it lacks enough evidence when
   the local index cannot support an answer.

## Quality harness

The evaluator uses only synthetic fixtures; it does not read a person's live
capture history. From `src-tauri/`, with the local memory model available:

```bash
CARGO_BUILD_JOBS=1 cargo test --lib eval_memory_quality -- --ignored --nocapture
```

It runs `tests/fixtures/synthetic_captures/captures.json` through deterministic
OCR-to-insight processing, then asks the local model to judge informativeness.
The gate flags template text, timestamped image filenames, redundant app-name
fluff, and weak answers; the current mean-score threshold is `0.55`. Inspect
the generated `target/memory_quality_evals.jsonl` before changing a prompt,
capture heuristic, or model setup.

Run the lightweight regression check after changing summary surfacing:

```bash
CARGO_BUILD_JOBS=1 cargo test --lib daily_summary_excludes_low_signal_visual_fallbacks
```

## Release checklist

- Use only non-sensitive, reviewable desktop content for a live rehearsal.
- Confirm a new high-signal capture appears in Memory Vault and exact and
  paraphrase search.
- Confirm filename-only or visual-fallback items stay in **Needs more signal**
  and out of Search, Ask, and Daily Summary.
- Ask one grounded question and one unsupported question; verify cited sources
  for the first and an honest refusal for the second.
- Review the generated evaluator output and preserve the fixture-based score
  floor before claiming a quality improvement.

## Honest demo claim

"I improved FNDR's memory quality by adding a shared evidence gate rather than
just changing the model prompt. Weak visual and filename-only captures remain
available for review but are excluded from retrieval, answers, and summaries.
The local model is evaluated against synthetic cross-domain captures, while
answers remain local, cited, and able to refuse unsupported questions."
