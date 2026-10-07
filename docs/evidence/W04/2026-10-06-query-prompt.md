# Query prompt on the live search path, 2026-10-06

## Finding

The live vector route embedded every search query as

```text
Represent this sentence for searching relevant passages: <query> <query terms>
```

That instruction is the BGE v1.5 query prompt. The live index is MiniLM v4 (`all-MiniLM-L6-v2`), which is a symmetric model with no instruction, and its documents are stored without one. The string was added in `search/hybrid.rs` (`embedding_query_with_extras`) on 2026-05-06 and applied whatever the embedding contract was. The focus-task comparison in `ipc/commands/stats.rs` embeds its text for the same index without it, so the same vectors were being compared against two differently worded queries.

## Change

- `QueryProfile::embedding_query_with_extras` returns the query text only.
- The vector route asks the embedding contract for the prompt: `prefixes::query_text_for(embedder.contract(), text)`. For MiniLM v4 that is no prompt.
- `Embedder::contract()` exposes the contract so call sites stop guessing.

The BGE v5 and EmbeddingGemma v6 prompts in `embedding/prefixes.rs` are unchanged. The BGE prefixes still differ from the BGE model card (see ADR 019); the owner chose on 2026-10-06 to leave them until they can be measured with the BGE model installed.

## Method

Three seeded synthetic personas, each in its own scratch profile (`com.fndr.app.qa-prefix-eval-<persona>`), seeded once and reused for both runs so only the query side differs. No real profile data was read. Assets: the installed MiniLM ONNX (matches the pinned hash) and the installed tokenizer, which differs from the pinned tokenizer in padding and truncation (see `2026-10-05-cloud-integration-local.md`).

- Hybrid, as a person experiences it: `make qa-retrieval-check QA_SKIP_SEED=1 PERSONA=<persona> QA_PROFILE=<scratch profile>`, before and after, compared with `scripts/audit/retrieval_check.py`.
- Vector branch alone: a temporary ignored test embedded each query three ways with the real model and ranked `Store::vector_search` results, with the hidden low-signal control record excluded. The probe was removed after the run.

## Hybrid results (Search path; Ask and shared retrieve gave the same numbers)

Headline metrics use the keyword and paraphrase cases.

| Persona | Recall@5 | MRR@10 | Median top score, positive | Median top score, negative | Negatives under the 0.25 bar |
|---|---|---|---|---|---|
| knowledge-worker | 1.000 to 1.000 | 0.966 to 0.909 | 0.464 to 0.524 | 0.249 to 0.188 | 2 of 4 to 3 of 4 |
| office-pm | 0.900 to 0.950 | 0.613 to 0.647 | 0.458 to 0.545 | 0.297 to 0.283 | 1 of 4 to 1 of 4 |
| software-engineer | 1.000 to 1.000 | 0.879 to 0.859 | 0.438 to 0.526 | 0.146 to 0.138 | 3 of 4 to 3 of 4 |

Per query, over all 98 positive cases: 9 moved up, 5 moved down, and the relevant memory was first for 78 instead of 80.

| Persona | Positive cases | Moved up | Moved down | Relevant memory ranked first |
|---|---|---|---|---|
| knowledge-worker | 35 | 1 | 3 | 34 to 31 |
| office-pm | 33 | 7 | 1 | 20 to 22 |
| software-engineer | 30 | 1 | 1 | 26 to 25 |

The gate passed for all three personas after the change, against both the before run and the committed references. Before the change, office-pm failed against its committed reference on this machine ("would clients recommend us, and did that improve" fell from rank 9 to a miss, the known installed-tokenizer discrepancy). After the change that query is at rank 8 and office-pm passes.

## Vector branch alone

| Persona | Query text | Recall@1 | Recall@5 | MRR@10 | Mean score of the relevant hit | Mean top score, negatives |
|---|---|---|---|---|---|---|
| knowledge-worker | old: instruction + query | 0.800 | 0.971 | 0.879 | 0.398 | 0.254 |
| knowledge-worker | new: query | 0.771 | 0.943 | 0.865 | 0.469 | 0.180 |
| office-pm | old: instruction + query | 0.424 | 0.909 | 0.608 | 0.321 | 0.226 |
| office-pm | new: query | 0.455 | 0.939 | 0.674 | 0.442 | 0.203 |
| software-engineer | old: instruction + query | 0.667 | 0.933 | 0.766 | 0.311 | 0.218 |
| software-engineer | new: query | 0.667 | 1.000 | 0.796 | 0.431 | 0.191 |

## Reading

- **Score separation improved on every persona.** A relevant memory now scores 0.07 to 0.12 higher in the vector branch and a no-match query scores lower, so the gap between a real match and no match roughly doubles (0.09 to 0.14 before, 0.24 to 0.29 after). On the hybrid path one more no-match query correctly falls under the "No strong matches" bar and no positive falls under it.
- **Recall held or improved.** Recall@5 is unchanged on two personas and up 0.05 on office-pm.
- **Top-rank precision is mixed.** MRR@10 rose on office-pm and fell on the other two, by 0.057 on knowledge-worker, where three queries went from first to second. With 30 to 35 positive cases per persona, each of these is one to three queries, so the ranking differences are within what this corpus can resolve.
- The fusion weights and the strong-match bar were tuned while vector scores carried the instruction. Vector scores are now higher, so the vector branch has more say in the fused rank. That is the likely source of the first-to-second moves and is worth a deliberate retune rather than a tweak against these three sets.

## Not done

- The committed references in `scripts/demo/retrieval-reference/` were not re-recorded. They were produced with the pinned tokenizer; these runs used the installed one. CI runs the gate with pinned assets.
- A query that is doubled in the embedding text (`<query> <query terms>`) was left alone. The probe's third variant, the raw query once, scored between -0.019 and +0.018 MRR@10 against the new text depending on the persona, which is not a clear win.
- BGE prefixes, as noted above.
