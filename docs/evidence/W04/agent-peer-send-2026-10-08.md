# Peer Send/Get/Cancel verification, 2026-10-08

## Implemented boundary

- Only a person-configured HTTPS Agent Card with a same-origin A2A 1.0 JSONRPC interface can be used. DNS is checked and pinned for each request; redirects, proxies, local and reserved addresses are refused.
- Send rebuilds the current authorized preview after Card inspection, compares the reviewed destination and text, and records a content-free local run before egress. No automatic resend follows an uncertain delivery.
- Get and Cancel require a known remote task ID and recheck attached memory visibility. Cancel is followed by Get before the UI reports its state.
- Bearer protected peers cannot receive a task until a credential binding is implemented. Peer artifacts are bounded untrusted text for review; the local run record contains no task or artifact body.

## Checks

The mock JSONRPC test verifies the exact request body and `A2A-Version` header on a local fixture. Focused parser tests cover mismatched IDs, direct replies, task artifacts, unknown states and visible output truncation. The Peer UI test covers preview and send. A browser preview fixture exercised Add peer → Preview → Send → Check status with synthetic data and no console errors. Typecheck and the native binary compile passed during this slice.

Privacy Activity now reads the durable peer run metadata and shows destination host, time, reviewed request size and current state. A synthetic browser preview exercised Add peer → Preview → Send → Privacy Activity; the saved run appeared with no console errors. This fixture is not a real network send or native persistence check.

The ignored live Rust test `agent::peer::tests::live_independent_text_peer_returns_a_reviewable_result` passed against [Emissar's public A2A 1.0 text agent](https://emissar.ai/agent). It fetched the current Card, validated its HTTPS JSONRPC interface, sent a synthetic question about Emissar's published modules through FNDR's pinned-address transport, and received nonempty reviewable output. No FNDR memory or real task text was used. This verifies one independent Send path; it does not exercise FNDR's native UI, ledger, Get or Cancel.

Controlled lifecycle tests now exercise Cancel followed by Get with contradictory states in both directions. FNDR reports only the verified Get state. A missing task response is refused; an HTTP 503 Send makes one request with no transport retry. The final isolated peer test binary passed 17 local tests with two external tests ignored by default. The shared checkout's direct Cargo test could not compile earlier because another session's uncommitted `operator/policy.rs` test had a borrow error (`E0502`); no claim of a clean shared-tree gate is made.

A second ignored live Rust test fetched [AgentNative Data Exchange's public Card](https://agentnative.cazimedia.com/.well-known/agent-card.json), selected its unauthenticated JSONRPC 1.0 interface, and sent a synthetic Federal Register question through FNDR's transport. It returned a direct message containing a public sample, Federal Register document URLs, and source provenance. This is a second independent Send implementation; it does not prove Get or Cancel. The free sample represented three records from an aggregate of 25, so it must not be described as a full 25-record result.

Ledger capacity check: a focused regression first showed that the 101st Send discarded the oldest run even when it was still working. The ledger now retains uncertain and active runs, reuses a completed run's slot when available, and refuses a new Send before egress when all 100 slots are unfinished. The test checks both preservation and reuse at the real StateStore boundary. The isolated `agent::peer_runs::tests::` run passed 2 tests after the fix.

Card output compatibility: FNDR requests and displays `text/plain`, but the validator previously accepted a Card offering only `application/json` output. The regression failed on that Card before the change. The validator now requires default `text/plain` output. The final isolated peer binary passed 18 local tests with 2 external tests ignored. Current Emissar and AgentNative Cards both advertise text output; this read-only check does not prove a native UI flow.

## Acceptance still open

- Native reviewed-text and ledger verification for the real peer path. The two live transport tests check synthetic questions and relevant replies; the local fixture checks the exact JSONRPC request body. Bearer credential binding is still absent.
- Native UI and network behavior on a disposable profile, including a canceled and an uncertain task. The controlled lifecycle tests are not native UI proof.
- Native verification that Privacy Activity presents the saved egress record after restart, plus measured task usefulness/latency.

This file contains no private task text, real capture or credential.
