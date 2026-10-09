# Peer Send/Get/Cancel verification, 2026-10-08

## Implemented boundary

- Only a person-configured HTTPS Agent Card with a same-origin A2A 1.0 JSONRPC interface can be used. DNS is checked and pinned for each request; redirects, proxies, local and reserved addresses are refused.
- Send rebuilds the current authorized preview after Card inspection, compares the reviewed destination and text, and records a content-free local run before egress. No automatic resend follows an uncertain delivery.
- Get and Cancel require a known remote task ID and recheck attached memory visibility. Cancel is followed by Get before the UI reports its state.
- Bearer protected peers cannot receive a task until a credential binding is implemented. Peer artifacts are bounded untrusted text for review; the local run record contains no task or artifact body.

## Checks

The mock JSONRPC test verifies the exact request body and `A2A-Version` header on a local fixture. Focused parser tests cover mismatched IDs, direct replies, task artifacts, unknown states and visible output truncation. The Peer UI test covers preview and send. A browser preview fixture exercised Add peer → Preview → Send → Check status with synthetic data and no console errors. Typecheck and the native binary compile passed during this slice.

Privacy Activity now reads the durable peer run metadata and shows destination host, time, reviewed request size and current state. A synthetic browser preview exercised Add peer → Preview → Send → Privacy Activity; the saved run appeared with no console errors. This fixture is not a real network send or native persistence check.

## Acceptance still open

- A real independent A2A peer receiving the exact reviewed text, including its authentication setup.
- Native UI and network behavior on a disposable profile, including a canceled and an uncertain task.
- Native verification that Privacy Activity presents the saved egress record after restart, plus measured task usefulness/latency.

This file contains no private task text, real capture or credential.
