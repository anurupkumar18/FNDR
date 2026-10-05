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
| Shared retrieval and MCP | Cloud retrieval/security work integrated, with local corrections for token-file permissions, parser filtering and strict retrieval comparisons. Search, Ask and MCP share retrieval. All three pinned MiniLM synthetic retrieval sets match their references. |
| Retrieval discrepancy | The installed tokenizer differs from the pinned tokenizer in padding/truncation. On the same M1, the installed version reproduces the office-pm lost result; pinned assets restore its ranks. This is asset-dependent behavior, not established platform numerical tolerance. Installed assets remain untouched. |
| Capture quality | Empty/secure-only Accessibility web areas now allow OCR fallback. Merges preserve text-source lineage, and cards/MCP/reporting show bounded source labels. Aggregate reports retain unknown historical provenance. Native Chrome discovery and the 12-app matrix remain unverified. |
| Resume and Home | Home surfaces three recent work threads and cited next steps through the existing Vault. Resume excludes hidden/low-signal and agent-source rows. Automated and synthetic-browser checks pass; native usefulness remains to be assessed. |
| EmbeddingGemma | Inactive v6 now prompts every query/document chunk correctly and retains fp32 reference parity. Repeated M1 fp32/256 runs measured 26.12 ms median individual queries and about 21 seconds for 104 long-document chunks; none exceeded the token limit. Concurrent capture, term/notice decisions and versioned migration remain gates. |
| Agent notes | Cloud PR 35 and local corrections pass integration: project validation, capture/identity isolation, reopening, provenance, formatting, review and derived-context boundaries. Notes remain off by default. Settings/filter/activity and the native demo remain pending. |

Last full gate (agent-note integration): **534 frontend tests and 1,066 Rust tests passed**, with 17 Rust tests intentionally ignored. The subsequent inactive embedding slice passed 34 focused embedding tests, four measurement tests and real fp32 parity at both dimensions. Native QA is still pending.

## Execution order now

1. Complete the concurrent capture/model budget and shared-model integration using the corrected EmbeddingGemma boundary. Long-chunk and isolated individual-query checks now pass; keep production unchanged until migration gates pass.
2. Coordinate the versioned re-embedding/cutover with Minh's EM-09, including recovery and coverage checks. Use seeded profiles before the owner vault.
3. Make the existing Related memories action resolve persisted links after restart, then integrate graph traversal with cited paths, typed reopen outcomes and saved workflows. Improve onboarding and the related-work/timeline journey alongside those slices.
4. Finish trust/privacy and note opt-in/filter/activity work, then run the combined native session and targeted follow-ups. Confirm capture before migrating the owner vault.

The board's 323 nominal hours (157 p0) describe ticket estimates, not an agent-work capacity ceiling. Preserve teammate ownership. VS-42 to VS-51 and VS-58 to VS-61 remain the core foundation; VS-52's image-search spike keeps its go/no-go gate. "No good match" remains dependent on useful chunk-score evidence.

## Manual QA, deferred

The first four checks form one 60-minute session: Case 1 public SimBio rerun (10), normal capture with metrics (30), 12-app Accessibility matrix (10), and five PX-07 native flows (10). Screen Guide (15), real Vault review (5), unresolved product decisions, and two user conversations (40) are separate follow-ups, not hidden inside the hour. No manual run is requested now.

Active text remains MiniLM 384; optional chunk retrieval uses BGE-large 1024; image vectors are CLIP 512. Those spaces are separate. The new EmbeddingGemma model is not yet the production embedder, and the stored image vector is not yet a delivered image-search journey.
