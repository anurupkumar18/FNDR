# ADR 031: Evidence-backed work identity across apps

## Status

Accepted 2026-10-13. The owner delegated product and architecture decisions for the quality loop.

## Context

Resume, Home, work sets and meeting prep rely on a memory's `project` or its app-specific `session_key`. On the owner's vault, the October 13 aggregate read found 162 memories, 21 with a project, and 66 distinct session keys; the largest key holds 74 memories. Of the 57 records with an exact file or browser reopen target, no target occurs in two apps. Reusing an exact target alone cannot connect the work across apps. These counts contain no captured content.

The synthetic personas already use `project` for a course assignment, hiring work and a launch, as well as software projects. Vault's `sessionLinks` and work-set resolution already link sessions in a short time window when they share a file or distinctive title words. Those are useful relationships, but the current product deliberately shows them as links, not one thread.

## Decision

1. `MemoryRecord.project` is the existing name for a **named piece of work**, including a document task, course, recurring meeting or software project. Keep the field and its current storage contract. `session_key` continues to describe the capture context, not the work identity.
2. A model-proposed project is stored only when every substantive word of the label appears together in the captured window title or one cleaned-text sentence or line as a whole word. The app name and the model's confidence are not evidence. An unsupported label is cleared even at high confidence. This first slice improves precision; it does not claim to solve coverage.
3. Time proximity and partial title similarity remain links between sessions. They do not assign a project or merge Resume threads. A later capture slice may use an explicit document, course or meeting name as an anchor and record its source in the existing `project_evidence` and `project_confidence` fields. Ambiguous cases stay unlabeled.
4. Before wider assignment, add cross-app positive and unrelated negative cases to the synthetic corpus and measure project precision and coverage separately. Check Resume and work-set results, all three retrieval personas, `make qa-vault`, and a native Quality Lab run. Do not rewrite the owner vault without FNDR closed, a backup and the owner's go-ahead.

## Options considered

| Option | Why it was not chosen |
| --- | --- |
| Keep repository-path identity only | It leaves the measured non-code work gap and the 74-memory fallback thread. |
| Merge nearby windows or similar titles into a project | Two apps being open together does not prove one piece of work; the existing link presentation is the honest weaker claim. |
| Group by exact URL or file target | The current vault has no exact target repeated across apps. |
| Add a new work-ID table now | It duplicates `project` before a source-backed assignment rule or labeled evaluation exists. |

## Consequences

Some unsupported high-confidence project labels will disappear from new captures. Existing records are not rewritten. Coverage must be improved with supported anchors and measured separately from this precision gate. Resume and work sets keep their current behavior until the later source-backed assignment slice is verified.
