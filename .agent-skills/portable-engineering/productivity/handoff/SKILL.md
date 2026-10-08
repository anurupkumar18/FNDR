# Handoff

## Trigger
Use when work stops, changes hands, or a long session needs a checkpoint.

## Goal
Give the next agent one accurate current state and the shortest path to resume.

## Workflow
1. Verify branch, HEAD, remote and dirty paths. Attribute shared-checkout edits; never claim or stage another session's work.
2. Read prior handoffs and commit history. Resolve dated contradictions in favor of verified current code and evidence.
3. State shipped work with commits, checks and their limits. Separate implemented, measured, native-verified and unverified claims.
4. List in-flight edits with owner and exact blocker. Preserve secrets and private captures outside the handoff.
5. Order remaining work by dependency, acceptance evidence and safe verification. Link existing tickets and deeper reports instead of repeating them.
6. Doctor the process: name what worked, what failed, why, and one concrete correction. Remove or mark stale handoff text rather than appending a conflicting status section.
7. Run `git diff --check`; stage only owned paths. Stop after the requested handoff and board updates.

## Required output
A concise `docs/handoffs/<date>-<topic>.md` with current state, verification scope, ordered next steps, ownership/risks, and process corrections. Include exact commands only where a successor needs them. Link evidence instead of copying long logs.
