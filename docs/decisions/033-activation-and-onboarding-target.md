# ADR 033: Activation and the onboarding target

## Status

Proposed, 2026-10-18. PD-14. Team review is pending; this is not an accepted product decision.

## Context

The October plan measures time to correctly recover prior work: find it, reopen it, and know the next step. The value scorecard says the first useful moment is returning to a task with supporting memories. OB-01 found that a person can reach Home after skipping model selection yet return to setup on relaunch (MR !57). Reaching Home alone therefore cannot be the activation measure. OB-06 needs a first useful moment to guide, OB-07 needs a local measurement contract, and PD-13 supplies outside-user observations.

The current evidence is one native onboarding audit, not a baseline conversion rate. The target below is a proposed gate for five outside-user sessions, not a claim about the wider user population.

## Proposed decision

Call a participant **activated** when, without facilitator hints, they complete this sequence within ten elapsed minutes of Home first appearing after onboarding:

1. Submit a search for a known, permitted work item.
2. Choose a result that actually refers to that item.
3. Open its original source with a successful reopen outcome and correctly identify the work context or next step from the source and its cited memory.

The study observer judges relevance and understanding. A click or a model answer alone does not prove them. The clock continues through errors and restarts. Every person who starts the onboarding trial stays in the denominator, including people who skip a model, deny a permission, or cannot reach Home. A participant who cannot reach Home within the session fails the activation target; the onboarding failure is recorded separately.

**Target:** at least four of five outside participants in PD-13 meet this definition within ten minutes. Report the exact count and each time, not a percentage alone. OB-06's first two sessions are an early usability check; two successes do not establish the five-person target. A clean-profile smoke run must also capture a deliberate public example, find it, and reopen it; report that separately from the seeded-profile study, because the two starting conditions are different.

For OB-07, store only local event metadata: onboarding start and completion, skipped steps and errors, first search submission, result selection, reopen outcome, and monotonic elapsed time. Do not log query text, result titles, source paths, captured content, or identities. A local sequence that ends in a successful reopen is an **activation proxy**, not proof of relevance or understanding. Report the proxy separately from PD-13's observed activation result. Provide a count of failed and incomplete trials so the denominator is visible. Nothing is sent from the Mac.

## Options considered

| Option | Why it was not selected |
| --- | --- |
| Onboarding complete or Home opened | Measures setup, not recovered work; OB-01 reproduced a false completion after model skip. |
| First search submitted or result clicked | Easy to count locally, but the result can be wrong and no source may open. Keep this as funnel evidence only. |
| Search, relevant result, and verified source reopen within ten minutes | Proposed. It matches the product promise and has a human-observable outcome. |
| Return to the app on a later day | Useful for retention, but later work and scheduling confound whether onboarding delivered a first useful moment. Measure separately after the initial study. |

## Consequences and verification

- OB-06 should guide a person toward a safe first capture, search, and source reopen, and show a clear next step when capture or a permission is unavailable. Its two-user check reports the observed sequence and elapsed time.
- OB-07 should emit content-free local funnel events and summarize the activation proxy with the full denominator. It must not label that proxy a successful recovery.
- PD-13 should use its consented, seeded public profile for the five outside-user trials and record relevance, reopen success, understanding, time, and where each person hesitated. The study reports outcomes without putting private screens or source text in Git.
- Review the target after those five sessions. If fewer than four people activate, treat the missed step as product work rather than changing the denominator or relaxing the clock after seeing results.

## Agreement path

Post this draft for team review under the decision-log process. After the two-day comment window and any written resolution, add an accepted row to `docs/team/decision-log.md` with the actual reviewers or the dated silence record. Until then, the decision log lists this as open.
