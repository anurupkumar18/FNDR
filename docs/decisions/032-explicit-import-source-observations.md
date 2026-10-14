# ADR 032: Exact source observations for explicit image imports

## Status

Accepted 2026-10-13 under the owner's delegated product and architecture decision.

## Context

The native Quality Lab read three labeled images accurately with Apple Vision OCR. Under memory pressure, the pixel model fell back to a local text model. Its stored summaries omitted the paper's twelve-percent result, changed the chat's PR status and speaker, and shortened the release article. Search still found the memories, but grounded Ask refused the labeled questions. The import stored OCR transiently in `text`, which normal storage compaction clears. Ask's context used the short snippet and had no exact source statements from these imports.

FNDR already has bounded `raw_evidence.source_evidence` statements with a source hash and line number. They are observations of captured text, never instructions, task status, or authority to act.

## Decision

1. When an explicit image import accepts OCR, choose at most six complete, salient source lines from the first 4,000 characters. Store them as `source_evidence` with kind `observation`, the source hash, original line number, and uncut quote. A rejected or empty OCR input yields no statements.
2. Reuse the existing evidence collector's visibility check and bounds. Pass its cited source statements to Ask as labeled, untrusted captured text. Include their memory IDs in the Quality Lab citation receipt.
3. Keep the current embedding document, vectors, snippets, and global match threshold unchanged. This does not reindex or rewrite existing owner records. Earlier imports without observations keep their existing behavior.
4. Judge answer fidelity separately from the Lab's mechanical cited-and-not-refused check. The three labeled questions and unsupported refusals need native evaluation, with any remaining failures reported as failures.

## Options considered

| Option | Reason rejected |
| --- | --- |
| Trust the fallback summary as the whole source | The native gold review found missing numbers, changed status, and wrong speaker attribution. |
| Persist full OCR or image pixels | It widens durable private content and conflicts with the compact memory path for a narrow Ask repair. |
| Change embedding text or lower the match threshold | A vector-space change needs a reindex plan, and weaker matching cannot restore missing facts. |
| Add a separate import evidence table | The existing bounded, source-hashed evidence format already owns this boundary. |

## Limits

Only explicit image imports gain these observations. The first 4,000 OCR characters and six quotes may omit a later fact. An exact quote proves the words appeared, not who authored them or whether an action remains open. The local answer model may still refuse or misstate a fact; the stored quote does not by itself prove answer quality.
