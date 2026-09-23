# FEA-06: MCP Resume Work contract

`memory.resume_work` is an explicit read-only MCP tool for the existing
Resume Work builder. It does not change `memory.get_context_pack`, whose
richer context-runtime response has separate callers and semantics.

## Contract

- Optional `hours` is clamped to 1 through 168, defaulting to 24.
- Optional `budget_tokens` is clamped to 256 through 4000, defaulting to
  2000.
- The response contains those effective bounds and `threads`, serialized from
  the existing `ResumeThread` type. Each thread retains its cited evidence and
  token-budgeted pack items.
- The tool reads from the local store only. It does not expose raw-memory
  fields or add an action surface.

## Verification

```text
CARGO_BUILD_JOBS=1 cargo test resume_work --lib
  Resume Work unit tests plus MCP discovery and bounded-dispatch tests pass.

CARGO_BUILD_JOBS=1 cargo test --test resume_work
  2 passed: cited thread grouping with stale-memory exclusion, and p95 latency.
```

Existing repository warnings are outside this slice. No native capture or real
personal memory was used for this verification.
