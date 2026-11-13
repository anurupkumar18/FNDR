# FNDR vault health

Aggregate counts only; no memory text, titles, URLs, or file paths.

## Table health

| table | role | present | rows | active days | date range | rows/active day | zero/missing vectors | median clean chars | exact reopen |
|---|---|:---:|---:|---:|---|---:|---:|---:|---:|
| `memories_v4_minilm_384` | current parent | yes | 11 | 3 | 2026-10-01 to 2026-10-07 | 3.7 | 0.0% | 45 | 90.9% |
| `memories_v5_bge_1024` | future parent | yes | 0 | 0 | n/a | 0.0 | n/a | n/a | n/a |
| `memory_chunks_v1_bge_1024` | chunk | yes | 0 | 0 | n/a | 0.0 | n/a | n/a | n/a |

## Structured fields filled: `memories_v4_minilm_384`

| field | filled |
|---|---:|
| project | 0.0% |
| topic | 100.0% |
| outcome | 36.4% |
| next_steps | 54.5% |
| decisions | 0.0% |
| errors | 27.3% |

## Summary source: `memories_v4_minilm_384`

| source | rows |
|---|---:|
| llm | 9 |
| tracker | 2 |

## Text source: `memories_v4_minilm_384`

Known observation lineage, not per-character attribution; older merges may be incomplete.

| source | rows | share | median clean chars | under 200 chars |
|---|---:|---:|---:|---:|
| ocr | 9 | 81.8% | 45 | 100.0% |
| unknown | 2 | 18.2% | 95 | 100.0% |

## Reopen targets: `memories_v4_minilm_384`

Stored `reopen_kind` only; counts are aggregate labels, never URLs or paths.

Reopen specific share (browser URL, file path, or app deep link): 90.9%.

| kind | rows | share |
|---|---:|---:|
| app_bundle | 1 | 9.1% |
| browser_url | 2 | 18.2% |
| file_path | 8 | 72.7% |
