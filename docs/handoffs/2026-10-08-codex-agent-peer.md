# Codex peer lane checkpoint, 2026-10-08

This replaces the completed five-hour plan in the earlier version of this file. The ongoing goal is to improve FNDR product quality, architecture, AI behavior and autonomous development through one verified slice at a time. The two Claude sessions still share this checkout.

## Current state

- `main` includes `bada5ac` (reviewed A2A 1.0 Send/Get/Cancel and content-free run ledger), `7778384` (saved peer sends in Privacy Activity), and `80f8fa1` (ignored live interoperability test). Earlier peer and Hermes read-grant commits are listed in [the cross-session checkpoint](2026-10-08-cross-session-closeout.md).
- A person can save an explicitly configured HTTPS Card, preview the exact text and selected current memory summaries, then Send. The backend revalidates Card, DNS, endpoint, source visibility, destination and reviewed text before egress. An uncertain send is recorded and never retried automatically. Peer output stays untrusted and outside memory.
- Get and Cancel require a known remote task ID. Cancel is followed by Get before its state is shown. Bearer-protected peers are refused because FNDR has no peer credential binding yet.
- Privacy Activity shows saved host, time, request size and readable state. The session request counter remains separate from this durable list.
- The checkout is dirty only in another session's `risk_policy.rs`, `mcp/mod.rs` and `storage/lance_store/tests.rs` as of this checkpoint. Inspect current status before editing or staging.

## Verification and limits

- Peer Rust tests: 17 passed; delegation Rust tests: 3 passed; peer-run persistence test: 1 passed. Peer and Privacy Activity frontend tests: 20 passed. Typecheck, production build and native binary check passed. The shared tree has existing compiler warnings.
- Browser preview: synthetic Card, reviewed Send, status check and Privacy Activity row worked; zero browser console errors. This proves mounted frontend behavior against preview IPC only.
- The ignored live Rust test fetched [Emissar's public A2A 1.0 Card](https://emissar.ai/agent), sent a synthetic question through FNDR's transport and received reviewable output. It proves one independent Send path, not native UI, ledger, Get or Cancel.
- Earlier disposable native launch confirmed `FNDR_DATA_DIR` isolation, but no FNDR window appeared in the available automation inventory. Repeating the same launch without a new window-access route would add no UI evidence. See [peer Send evidence](../evidence/W04/agent-peer-send-2026-10-08.md) and [ADR 025](../decisions/025-agent-peer-interoperability.md).

## Next slices, in order

1. Finish AG-04 protocol lifecycle evidence: exercise Get and cancellation against a controlled peer, including working after cancel request, confirmed cancel, unknown task and uncertain Send. Keep synthetic data and exact wire assertions. Recheck source visibility after the network response.
2. Establish a native window-access route, then run Peer UI and Privacy Activity after restart on a disposable profile. Do not use the owner vault. If the route remains unavailable, report native QA pending without another duplicate build.
3. Bind a credential to one configured peer if a real target needs Bearer. Never place it in a Card URL, task body or run ledger. Prove endpoint binding and refusal after credential removal.
4. After AG-04, measure task usefulness, unsupported claims, latency and bytes against two independent peers before expanding to inbound A2A. Keep AG-04 Doing until its native and lifecycle gates pass; AG-05 owns broader native Agent QA.
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
- Testability gaps: native window, Get/Cancel lifecycle and second independent peer.
- Verdict: approve with the stated acceptance work open.
