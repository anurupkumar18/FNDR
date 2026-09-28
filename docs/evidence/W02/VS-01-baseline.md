# VS-01 retrieval and vault baseline

Recorded on 2026-09-28 before retrieval tuning. The retrieval fixture is a synthetic 20-record knowledge-worker week; 19 records are surfaceable and one is an intentional low-signal control. The owner-vault report contains aggregates only.

## Retrieval result

`Recall@5` is case-level: a query passes when at least one accepted relevant ID appears in its top five results.

| Path | Recall@5 | MRR@10 | keyword Recall@5 | paraphrase Recall@5 | p50 | p95 |
|---|---:|---:|---:|---:|---:|---:|
| Search ranked retrieval | 0.955 | 0.909 | 1.000 | 0.875 | 595 ms | 764 ms |
| Ask `context_runtime` cards | 1.000 | 1.000 | 1.000 | 1.000 | 1234 ms | 1418 ms |

The paths agree at top 1 on 17 of 22 cases. Search misses one paraphrase case: “which plotting library am I allowed to use”; Ask ranks an accepted record first. This is baseline evidence, not a tuning result.

Scope: Search is measured at its behavior-preserving ranked retrieval boundary before card synthesis. Ask is measured through `context_runtime` in card mode. This does not claim coverage of final Search card ordering or every legacy MCP tool.

## Owner vault result

- Current parent table: 29 rows across 5 active local days, 5.8 rows per active day.
- Primary vectors: 0.0% zero or missing; median clean text is 129 characters.
- Future parent table: present with 0 rows. Chunk table: present with 0 rows.
- Structured coverage: topic 100.0%; project, outcome, next steps, decisions, and errors 0.0%.
- Summary provenance: 17 fallback, 11 visual capture, and 1 tracker.
- Exact page/file/deep-link reopen coverage: 10.3%.

The empty future-parent and chunk tables show that this profile's baseline still relies on the current 384-dimensional parent index rather than the future parent-plus-chunk path.

## Safety and evidence

- `make qa-seed` refuses the real profile, its descendants, its ancestors, and unresolved symlink aliases before reset or directory creation.
- `make qa-retrieval` evaluates a disposable copy, so opening stores cannot migrate or add tables to the seeded source profile.
- `make vault-health` projects only fields needed for aggregate calculations and never renders memory text, titles, URLs, or file paths.
- The committed JSON has schema version 1, nullable per-query ranks, and no absolute path or generated timestamp.

Artifacts:

- [Owner vault health](vault-health-owner.md)
- [Seeded retrieval report](retrieval-baseline-seeded.md)
- [Seeded retrieval JSON](retrieval-baseline-seeded.json)

## Verification

- `cargo test --example seed_demo`: 6 passed.
- `cargo test --example retrieval_qa`: 7 passed.
- `cargo test --test search_flow`: 1 passed.
- `cargo test --lib search`: 48 passed.
- `python3 scripts/audit/test_vault_health.py -v`: 6 passed.
- `make qa-seed`: 20 stored, 19 surfaceable, 1 low-signal.
- `make qa-retrieval`: 22 cases evaluated through both scoped paths.
- Real-profile seeding and evaluation refusal checks: both refused before opening the profile.
- `CARGO_BUILD_JOBS=1 make test`: TypeScript passed; 63 Vitest files / 350 tests passed; the production build passed; all Rust library, integration, and documentation tests passed (773 library tests passed, 10 ignored).
- `git diff --check`: passed.
