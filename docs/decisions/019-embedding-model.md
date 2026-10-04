# ADR 019: Choose the text embedding model by measurement

## Status

Proposed, 2026-10-04 (VS-17). Cloud quality numbers are in. Milliseconds per chunk and peak memory on the reference M1 8 GB are to be measured by local before this is accepted; the license question for EmbeddingGemma needs the owner's call. Implementation (contract, table, reindex) belongs to Minh's embedding tickets.

## Context

- Live search embeds one short composed summary per memory with all-MiniLM-L6-v2 (2019, 384 dimensions, contract v4 in ADR 002). BGE-large (1024 dimensions) is wired for an explicit v5 reindex but unused, and the chunk table from ADR 008 is empty on real profiles.
- The October plan (section 6, target 4) asks for one embedding model chosen by measurement on the M1 8 GB, judged on Recall@5 over the labeled set, milliseconds per chunk, and memory. VS-18 will embed every roughly 300-token chunk of screen text at capture time, so cost per chunk matters more than it did for one summary per memory.
- History: the 2026-05-15 model-stack plan moved to EmbeddingGemma at 256 dimensions but never finished its migration and was deleted. FNDR v2 picked Qwen3-Embedding-0.6B without a v1 ablation and excluded EmbeddingGemma for its terms.

## Candidates

Parameter counts are from the weight files (fp32 for MiniLM, bge-small, and EmbeddingGemma; bf16 for Qwen3).

| Candidate | Params | Dim | Max tokens | Pooling | Prompts | License | ONNX build for the app's `ort` runtime |
|---|---:|---:|---:|---|---|---|---|
| sentence-transformers/all-MiniLM-L6-v2 (today) | 23 M | 384 | 256 (app allows 512) | mean | none | Apache-2.0 | In use: Xenova build, 90.4 MB fp32, sha256 pinned in `model_config.rs` |
| BAAI/bge-small-en-v1.5 | 33 M | 384 | 512 | CLS | query: "Represent this sentence for searching relevant passages: " | MIT | Official repo 133 MB fp32; Xenova int8 34 MB. App must pool by CLS |
| google/embeddinggemma-300m | 308 M | 768 (Matryoshka 512, 256, 128) | 2048 | mean, two dense layers | query "task: search result \| query: ", document "title: none \| text: " | Gemma Terms of Use; Hugging Face repo gated | onnx-community/embeddinggemma-300m-ONNX (ungated): fp32 1235 MB, q8 309 MB, q4 197 MB; outputs a pooled `sentence_embedding`; no fp16 |
| Qwen/Qwen3-Embedding-0.6B | 596 M | 1024 | 32768 | last token | query: "Instruct: Given a web search query, retrieve relevant passages that answer the query\nQuery:" | Apache-2.0 | onnx-community build: fp32 2400 MB, int8 614 MB (license field unset there; upstream Apache-2.0). App must pool by last token |

The gated Google repo could not be read without a token, so EmbeddingGemma was loaded from `unsloth/embeddinggemma-300m` at a pinned revision whose weight and config files have the same sha256 and git blob ids as `google/embeddinggemma-300m` at `57c266a7`. A fifth row, `embeddinggemma-256`, is the same model with vectors cut to the first 256 dimensions and re-normalized, the size the May plan targeted.

## Method

`scripts/audit/embedding_bakeoff.py` (tests in `scripts/audit/test_embedding_bakeoff.py`, pins in `scripts/audit/requirements-bakeoff.txt`) runs pure vector retrieval, cosine over normalized vectors, top 10, ties by memory id, over both labeled personas: knowledge-worker (19 searchable memories, 39 queries) and office-PM (39 memories, 37 queries), with the low-signal control excluded as the app hides it. Each model runs with its model-card pooling and prompts through sentence-transformers on CPU, in its own process. Metrics follow `retrieval_qa.rs`: case-level Recall@5 and MRR@10 over keyword and paraphrase cases (the headline), per-kind Recall@5, and negative queries reported as top scores only.

Two document modes:

- **record**: one vector per memory from a Python mirror of the app's primary text (`compose_insight_embedding_text` over the record `seed_demo.rs` builds, after `derive_insight_for_record`): project, topic, context (summary), what_happened, why_mattered, what_changed, context_thread, decisions, errors, todos, urls. Not mirrored: intent, workflow, and aliases (Rust heuristics at insert time), and why_mattered for four office-PM memories where the app falls back to a salient OCR span. The seeded profile that `make qa-retrieval` measures embeds `title + summary + OCR` instead (`seed_demo.rs`), so these rows are not comparable to that baseline.
- **chunks**: window title plus OCR text in windows of about 300 tokens (4 characters per token, 60 overlap), memory score = best chunk plus 0.01 for each other chunk within 0.05 of it (at most 3). Every demo memory fits in one chunk, so this mode compares raw screen text against the structured summary; it does not exercise the multi-chunk roll-up.

This isolates the embedder. It is not the app's hybrid path (no BM25, no time or app filters, no reranker), and it is not the app's ONNX runtime.

## Cloud quality results

Both personas pooled (42 headline queries). Per-persona rows, per-query ranks, and the cost table are in `docs/evidence/W03/VS-17-bakeoff-cloud.md`. AUC is the chance that a positive query's top score beats a negative query's (8 negative queries), which compares models with different cosine ranges fairly.

| Model | Dim | record Recall@5 | record MRR@10 | chunks Recall@5 | chunks MRR@10 | AUC record / chunks | Cloud ms per chunk / peak RSS MB (relative) | Download MB | M1 ms per chunk | M1 peak memory |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---|---|
| minilm | 384 | 0.929 (39/42) | 0.771 | 0.952 (40/42) | 0.751 | 0.969 / 0.991 | 8.7 / 636 | 92 | to be measured by local | to be measured by local |
| bge-small | 384 | 0.929 (39/42) | 0.807 | 0.929 (39/42) | 0.795 | 0.952 / 0.943 | 15.8 / 678 | 135 | to be measured by local | to be measured by local |
| embeddinggemma | 768 | 0.976 (41/42) | 0.913 | 1.000 (42/42) | 0.889 | 0.982 / 0.993 | 73.9 / 1517 | 1270 | to be measured by local | to be measured by local |
| embeddinggemma-256 | 256 | 0.976 (41/42) | 0.907 | 0.976 (41/42) | 0.875 | 0.991 / 0.991 | 76.5 / 1521 | 1270 | to be measured by local | to be measured by local |
| qwen3 | 1024 | 0.881 (37/42) | 0.789 | 0.952 (40/42) | 0.744 | 0.950 / 0.954 | 1086.7 / 4629 (noisy) | 1207 | to be measured by local | to be measured by local |

Download MB is the Hugging Face snapshot the harness fetched (PyTorch weights plus tokenizer), not the ONNX file the app would ship; see the candidates table for those.

The cloud cost column is PyTorch fp32 on a shared 4-vCPU Linux container (fastest of 3 passes, batches of 16, one-chunk memories of at most 159 MiniLM tokens), and peak RSS includes about 434 MB of PyTorch runtime. Only the ratios mean anything: per chunk, bge-small costs about 1.8 times MiniLM and EmbeddingGemma about 8.5 times; Qwen3's passes varied up to 6 times because of other load, so its row is only "far slower". Cutting EmbeddingGemma to 256 dimensions saves storage and search work, not embedding time. The app would run ONNX through `ort` (q8 for EmbeddingGemma), so the M1 columns, not these, decide.

## How much of this is noise

- Recall@5 is near its ceiling: MiniLM already recalls 39 or 40 of 42 headline cases. EmbeddingGemma recalls 2 more than MiniLM and loses none, in both modes; a two-sided exact sign test on 2 to 0 gives p = 0.5. Recall@5 cannot separate these models on these sets.
- MRR@10 separates them more. Per query, EmbeddingGemma ranks the first relevant memory higher than MiniLM on 10 cases and lower on 2 (record, p = 0.039), and higher on 14 and lower on 3 (chunks, p = 0.013). The 256-dimension cut keeps most of that (11 to 3, p = 0.057; 14 to 5, p = 0.064). bge-small (8 to 4, 10 to 6) and Qwen3 (6 to 6, 7 to 9) are not distinguishable from MiniLM. Most of those gains (17 of 24) move the first relevant memory up to rank 1 from ranks 2 to 5.
- These are 24 paired comparisons (4 models against MiniLM, 2 modes, 3 scopes) on 42 queries over 58 short synthetic memories written alongside their labels, so a p near 0.04 is suggestive, not proof. The office-PM persona carries most of the difference (EmbeddingGemma 9 better, 0 worse in chunk mode); on knowledge-worker no model gains or loses more than one headline case at Recall@5 against MiniLM, and per-query rank changes are mixed (EmbeddingGemma 2 better and 1 worse in record mode, 5 and 3 in chunk mode).
- Separation of no-match queries rests on 8 negatives and every model's AUC is between 0.943 and 0.993; it does not rank the models.
- Qwen3 used the default web-search instruction from its sentence-transformers config. The model card recommends task-specific instructions; a tuned instruction was not tried.

## License notes

- MiniLM (Apache-2.0), bge-small (MIT), and Qwen3-Embedding (Apache-2.0) carry no special terms.
- EmbeddingGemma is under the Gemma Terms of Use, not an open-source license. In summary (read https://ai.google.dev/gemma/terms before deciding; this is not legal advice): commercial use and redistribution are allowed, but a redistributor must pass on the use restrictions of the Gemma Prohibited Use Policy and a copy of the terms or a notice, and Google reserves the right to restrict use it believes violates the terms. The Hugging Face repo is gated behind accepting the terms, so the app could not download it without a token; the ungated `onnx-community` ONNX build carries the same terms. FNDR v2 excluded EmbeddingGemma for these terms. The owner should decide this before any other work on it.

## Provisional recommendation

1. **EmbeddingGemma-300m at 256 dimensions**, as a q8 ONNX build, if the M1 numbers fit and the owner accepts the Gemma terms. It is the only candidate that beats MiniLM with any support (MRR@10 +0.12 to +0.14 against MiniLM in both modes, sign test p 0.013 to 0.064), and the 256-dimension cut gives up almost none of that while storing fewer numbers per vector than MiniLM's 384. The cost is a model about 13 times MiniLM's size: a 309 MB q8 download plus a 33 MB tokenizer instead of 90 MB, and about 8.5 times MiniLM's milliseconds per chunk and about 880 MB more peak memory under PyTorch fp32 on this CPU.
2. **If the M1 cost or the license rules it out, keep MiniLM.** bge-small is not measurably better on these sets (no Recall@5 change, MRR differences inside noise), so switching to it would cost a reindex and a pooling change for no demonstrated gain.
3. **Reject Qwen3-Embedding-0.6B for the 8 GB target.** It does not beat MiniLM here, it lost 2 headline cases in record mode, and it peaked at 4629 MB of process memory at fp32 on this host.

Before accepting: rerun the harness on the M1 (instructions in the evidence file) and fill the two M1 columns; confirm the q8 ONNX build reproduces the PyTorch rankings (quantization can move near ties); and rerun with a larger labeled set if one exists by then, since 42 queries are too few to settle a close call.

## Consequences if accepted

- A new embedding contract per ADR 002 (model id, ONNX file and sha256 pins, dimension, table name), a reindex of parents and chunks, and an amendment to ADRs 002 and 008. ADR 008's choice of BGE-large for the chunk table would be superseded.
- The embedder must apply the model's prompts ("task: search result | query: " for queries, "title: none | text: " for documents) and read the ONNX graph's pooled `sentence_embedding` output, then cut to 256 dimensions and re-normalize. Today `embedding/onnx.rs` mean-pools a hidden state, which is correct only for MiniLM.
- Two findings regardless of the choice: the app's BGE prefixes in `embedding/prefixes.rs` ("Represent this sentence: " for documents, "Represent this question for searching relevant passages: " for queries) do not match the BGE v1.5 model card (no document prefix; "Represent this sentence for searching relevant passages: " for queries), which affects the v5 BGE-large path; and on number-dense OCR a 4-characters-per-token estimate undercounts real tokens by up to 1.78 times, so a "300-token" VS-18 chunk can reach about 530 real tokens, past MiniLM's 256-token training length. VS-18 should size chunks with the model's tokenizer.
