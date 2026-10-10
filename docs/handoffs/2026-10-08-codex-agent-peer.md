# Codex peer lane checkpoint, 2026-10-08

This replaces the completed five-hour plan in the earlier version of this file. The ongoing goal is to improve FNDR product quality, architecture, AI behavior and autonomous development through one verified slice at a time. The two Claude sessions still share this checkout.

## Current state

- `main` includes reviewed A2A 1.0 Send/Get/Cancel (`bada5ac`), Privacy Activity (`7778384`), two independent live Send checks (`80f8fa1`, `87d81ba`), controlled lifecycle tests (`4e1d2dd`), active-run retention (`5e78dc2`) and text-output Card validation (`e68ccd7`). Earlier peer and Hermes read-grant commits are listed in [the cross-session checkpoint](2026-10-08-cross-session-closeout.md).
- A person can save an explicitly configured HTTPS Card, preview the exact text and selected current memory summaries, then Send. The backend revalidates Card, DNS, endpoint, source visibility, destination and reviewed text before egress. An uncertain send is recorded and never retried automatically. Peer output stays untrusted and outside memory.
- Get and Cancel require a known remote task ID. Cancel is followed by Get before its state is shown. Bearer-protected peers are refused because FNDR has no peer credential binding yet.
- Privacy Activity shows saved host, time, request size and readable state. The session request counter remains separate from this durable list.
- The 100-run ledger preserves uncertain and active tasks. When full, it reuses an ended run's slot or refuses a new Send before egress.
- A direct reply without text is recorded as confirmed but unsupported content. Peer UI and Privacy Activity explain that FNDR cannot display it; no task ID is invented.
- Send asks task-capable peers to return immediately, so FNDR can keep the real task ID and check status later. Two live direct-answer peers still pass; long-running live task polling is unverified.
- The checkout is shared with two Claude sessions. Inspect current status before editing or staging; do not stage their files.

## Verification and limits

- Peer Rust tests: 19 passed with two external tests ignored in a clean copy of HEAD plus `peer.rs`; each external test also passed earlier when run separately. Delegation Rust tests: 3 passed earlier; peer-run tests: 2 passed. Peer and Privacy Activity frontend tests: 21 passed; typecheck passed. Production build and native binary check passed before the latest reply-state change. An earlier shared-tree Rust gate had another session's uncommitted `operator/policy.rs` borrow error; no later full shared-tree gate is claimed.
- Browser preview: synthetic Card, reviewed Send, status check and Privacy Activity row worked; zero browser console errors. This proves mounted frontend behavior against preview IPC only.
- Ignored live Rust tests fetched [Emissar's public A2A 1.0 Card](https://emissar.ai/agent) and [AgentNative Data Exchange's Card](https://agentnative.cazimedia.com/.well-known/agent-card.json), then sent synthetic questions through FNDR's transport. Both returned reviewable output; the second returned public sample records with source URLs. They prove two independent Send paths, not native UI, ledger, Get or Cancel.
- Earlier disposable native launch confirmed `FNDR_DATA_DIR` isolation, but no FNDR window appeared in the available automation inventory. Repeating the same launch without a new window-access route would add no UI evidence. See [peer Send evidence](../evidence/W04/agent-peer-send-2026-10-08.md) and [ADR 025](../decisions/025-agent-peer-interoperability.md).

## Next slices, in order

1. Finish AG-04 lifecycle acceptance: controlled Cancel/Get, missing task and one-attempt failed Send now pass. Exercise nonblocking Send, Get/Cancel and an uncertain task through native UI with a real task-capable peer and a disposable profile. Keep source visibility checks before and after network access.
2. Establish a native window-access route, then run Peer UI and Privacy Activity after restart on a disposable profile. Do not use the owner vault. If the route remains unavailable, report native QA pending without another duplicate build.
3. Bind a credential to one configured peer if a real target needs Bearer. Never place it in a Card URL, task body or run ledger. Prove endpoint binding and refusal after credential removal.
4. After AG-04, measure task usefulness, unsupported claims, latency and bytes across the two checked peers before expanding to inbound A2A. Keep AG-04 Doing until its native and lifecycle gates pass; AG-05 owns broader native Agent QA.
5. When the Claude sessions settle, run one serial full gate and reconcile the board with their commits. Continue the core daily-loop QA from the cross-session checkpoint without repeating browser or synthetic evidence as native proof.

## Process doctor

- Worked: explicit-path commits protected the shared checkout; focused tests and one verification gate per slice kept work moving. The public peer test found a real interoperability path without owner data.
- Failed: the earlier five-hour plan and cross-session status remained in handoffs after their work shipped. Repeated native launches had no visible window and did not improve the acceptance claim. Multiple Cargo commands competed for the same artifact lock once; later checks ran serially.
- Correction: AGENTS now permits a scoped ticket to start from its execution skill and existing decision instead of replaying discovery. This file replaces its stale plan with current state. Native GUI QA resumes only when window access changes.

ANTI-BLOAT REVIEW

- Behavior delivered: one reviewed outbound peer flow and durable content-free activity visibility.
- Complexity added: one peer transport module, one run record module, IPC commands and mounted UI state. The separate public run view omits source IDs intentionally.
- Bloat risks: peer state labels are mapped in two mounted views; keep them local until a third caller justifies a shared utility. The browser IPC fixture is synthetic and should not grow into a second backend.
- Simplifications required: reuse the existing StateStore, context source authorization and Privacy Activity. Avoid another delegation framework or duplicate planning document.
- Code to delete or merge: none in this slice. Remove superseded handoff claims rather than retaining a second current state.
- Interface improvements: future Bearer binding should attach to saved peer identity and exact endpoint.
- Testability gaps: native window and live Get/Cancel lifecycle.
- Verdict: approve with the stated acceptance work open.
