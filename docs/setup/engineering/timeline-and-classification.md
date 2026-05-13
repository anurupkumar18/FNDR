# Timeline and classification engineering

This note covers **search timeline UI** (`src/domains/timeline/Timeline.tsx`), **browse filters** that mirror coarse activity buckets (`MemoryCardsPanel` perspective filters in `src/domains/memory-vault/`), and **Rust-side timeline classification** (`src-tauri/src/timeline/`).

## Anti-patterns

- **Hardcoded thresholds in JSX** with no named constant or doc (for example match-quality cutoffs, dedupe windows). These drift silently when search ranking changes.
- **One-off string rules** duplicated across React and Rust without a single source of truth or tests on both sides where behavior must align.
- **Copy-paste classification** between panels (timeline chips vs memory browse filters) instead of shared config or shared backend fields (`activity_type`, `timeline_action_class`).
- **Magic URL fragments** embedded in long `if` chains without grouping, ownership, or ADR when the rule is product-defining (for example “what counts as a meeting?”).

## Required practices

1. **Frontend numeric thresholds** for the timeline belong in `src/domains/timeline/timelineConfig.ts` (with small unit tests in `timelineConfig.test.ts` alongside). Import from `Timeline.tsx` instead of inlining literals.
2. **Rust URL / path / title heuristics** for `classify_action_class` belong in `src-tauri/src/timeline/classify_rules.rs` as named constant tables plus tiny helpers (`any_substring`, `any_path_suffix`). Keep `classify.rs` as orchestration + `ActionClass` + integration tests.
3. **New product-facing rules** (anything a PM could debate) need a short note in `docs/decisions/` (ADR-style) *or* an explicit comment block pointing to an existing ADR.
4. **When React and Rust must agree** on a definition, prefer persisting structured fields on the memory row and treating UI rules as presentation-only; use cross-language tests only at serialized boundaries if needed.

## How to add a rule (Rust)

1. Add substrings or suffixes to the appropriate `&[&str]` table in `classify_rules.rs`.
2. Wire the check in `classify_action_class` **before** the broad `activity` match arm when the signal is strong (URL/path wins over generic activity labels).
3. Extend `src-tauri/src/timeline/classify.rs` `#[cfg(test)]` with a `SearchResult` fixture proving the new rule and one negative guard (avoid accidental classification from `app_name` alone).

## How to add a threshold (timeline UI)

1. Add a named field to `TIMELINE_STREAM`, `TIMELINE_DEDUPE`, or `TIMELINE_MATCH_LABEL` in `src/domains/timeline/timelineConfig.ts`.
2. Replace the literal in `Timeline.tsx`.
3. If the threshold is non-obvious, add a one-line comment in `timelineConfig.ts` referencing the hybrid scorer or UX intent.

## Related paths

| Area | Path |
| --- | --- |
| Timeline UI | `src/domains/timeline/Timeline.tsx`, `timelineConfig.ts` |
| Rust classification | `src-tauri/src/timeline/classify.rs`, `classify_rules.rs`, `mod.rs` |
| Search result shape | `src-tauri/src/storage/` (`SearchResult`, memory card DTOs) |
| Memory Vault browse | `src/domains/memory-vault/MemoryCardsPanel.tsx` |
