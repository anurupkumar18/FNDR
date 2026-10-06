# Status and next: Oct 5

Goal: make FNDR a dependable local memory system that captures useful work context, retrieves it for people and agents, restores supported work states, feels easy to use, and connects memories over time. The month plan is canonical: `docs/team/2026-10-month-plan.md`. Detailed verification is in `docs/evidence/W04/2026-10-05-cloud-integration-local.md`.

## Direction agreed with the owner

- October 16 is the teammate delivery checkpoint. It does not cap the owner's scope; work continues through the following two months.
- Optimize for the 8 GB Mac: adaptive local models and resource scheduling, with cloud reasoning explicitly optional. EmbeddingGemma is the selected text-model direction, pending migration and workload gates.
- Deliver typed actions and saved workflows first, then bounded UI automation.
- Start connected memory with cited related work and a timeline; build the richer graph explorer on that foundation.
- Make Home focus on resuming recent work, with prominent search and nearby Vault/Connections.
- Batch manual QA after implementation and automated integration. Ask earlier only for a small native probe that resolves a real implementation decision.
- Use existing portable workflows, parallel implementation/review where independent, and remove obsolete code within the slice being changed.

## Implemented and verified so far

| Area | Result and limit |
|---|---|
| Shared retrieval and MCP | Cloud integration and all three pinned MiniLM synthetic sets passed. Shared ranked retrieval now authorizes current rows before fusion/debug output and removes excluded nested links. The inspector checks graph/knowledge backing memories. Fresh context packs now authorize event/task sources and rebuild project aggregates; agent conversion and typed graph snapshots also check provenance. Direct Vault/image, MCP source/full-context/timeline and code/working-state/delta reads reuse these checks. Saved packs and remaining legacy project/knowledge/audit reads still need separate checks. |
| Retrieval discrepancy | The installed tokenizer differs from the pinned tokenizer in padding/truncation. On the same M1, the installed version reproduces the office-pm lost result; pinned assets restore its ranks. This is asset-dependent behavior, not established platform numerical tolerance. Installed assets remain untouched. |
| Capture quality | Empty/secure-only Accessibility web areas now allow OCR fallback. Merges preserve text-source lineage, and cards/MCP/reporting show bounded source labels. Aggregate reports retain unknown historical provenance. Native Chrome discovery and the 12-app matrix remain unverified. |
| Source-backed context | New text extraction preserves exact source statements with snapshot/line citations instead of generating canonical intent or pending tasks. Storage/review/merge preserve that boundary. Vault details and agent packs expose bounded observations separately from unverified summaries. Legacy and pixel paths remain separate. |
| Resume and Home | Home surfaces three recent work threads and cited next steps through the existing Vault. Resume applies current configurable exclusions, deletion, low-signal, internal and agent-source rules before deriving thread state or citations. Automated and synthetic-browser checks pass; native usefulness remains to be assessed. |
| Model scheduling | Existing callers share real sessions while retaining their preprocessing/fallback policies. Queries receive bounded priority between background chunks; cold loading and query inference run off async workers. Missing-asset initialization remains retryable. Blocking Qwen jobs retain their context across cancellation/engine replacement. Static wrappers and Qwen weights still retain residency. |
| EmbeddingGemma | Inactive v6 retains fp32 reference parity and correct chunk prompts. Bounded scheduling reduced M1 query medians under background embedding from 779–808 ms to 167–201 ms, with similar background completion time. A synthetic Qwen/Gemma run reached 228 ms query median and 2.16 GB process RSS; host pressure, teardown and grounding issues prevent acceptance. Native capture/VLM budgets, terms and migration remain gates. |
| Related memories | Vault and MCP now follow persisted links after restart, resolve consolidated IDs, and honor current exclusions. Stored links and similarity suggestions have distinct labels; notes retain authorship. Full graph traversal and timeline remain pending. |
| Agent notes | Cloud PR 35 and local corrections pass integration: project validation, capture/identity isolation, reopening, provenance, formatting, review and derived-context boundaries. Notes remain off by default. Settings/filter/activity and the native demo remain pending. |

Latest full Rust gate: **1,138 passed, 20 intentionally ignored** across 21 targets. Ten new regressions cover direct Vault/diagnostic/image reads, Resume exclusions, code/working-state/delta sources, MCP current-record projection, page provenance, aliases and newest-first ordering. The existing legacy agent-label Resume behavior is preserved. No frontend behavior changed; native QA remains deferred.

The earlier isolated Qwen run parsed 8/8 cases without repair/cap hits and retained both qualified request and completed/negated quotes in the small positive case. Larger-chat quote recall and generated-summary factuality remain weak; exact quotes do not certify summaries. Model processes still hit the known Metal teardown abort after saving results. Production Gemma remains gated.

## Execution order now

1. Extend the verified retrieval, fresh context-pack and direct-read boundaries to historical saved context/status/audit and remaining legacy MCP project/knowledge/event reads. Then improve constrained source selection and generated-summary quality. Follow with pixel/whole-app resource measurements. Actual unload and the known Metal teardown abort remain unresolved; production Gemma activation remains gated.
2. Coordinate the versioned re-embedding/cutover with Minh's EM-09, including recovery and coverage checks. Use seeded profiles before the owner vault.
3. Build on the verified persisted Related memories action with graph traversal and cited paths, typed reopen outcomes and saved workflows. Improve onboarding and the related-work/timeline journey alongside those slices.
4. Finish trust/privacy and note opt-in/filter/activity work, then run the combined native session and targeted follow-ups. Confirm capture before migrating the owner vault.

The board's 323 nominal hours (157 p0) describe ticket estimates, not an agent-work capacity ceiling. Preserve teammate ownership. VS-42 to VS-51 and VS-58 to VS-61 remain the core foundation; VS-52's image-search spike keeps its go/no-go gate. "No good match" remains dependent on useful chunk-score evidence.

## Manual QA, deferred

The first four checks form one 60-minute session: Case 1 public SimBio rerun (10), normal capture with metrics (30), 12-app Accessibility matrix (10), and five PX-07 native flows (10). Screen Guide (15), real Vault review (5), unresolved product decisions, and two user conversations (40) are separate follow-ups, not hidden inside the hour. No manual run is requested now.

Active text remains MiniLM 384; optional chunk retrieval uses BGE-large 1024; image vectors are CLIP 512. Those spaces are separate. The new EmbeddingGemma model is not yet the production embedder, and the stored image vector is not yet a delivered image-search journey.
