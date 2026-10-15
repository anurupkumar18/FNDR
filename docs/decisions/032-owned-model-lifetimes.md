# ADR 032: Owned model lifetimes and native shutdown

## Status

Accepted for implementation, 2026-10-15, under the owner's delegation of product and architecture decisions in the active quality goal. F9 remains open until real text and vision teardown checks pass.

## Context

The native QA app repeatedly aborts on quit at ggml's Metal resource assertion. The [F9 ownership comparison](../evidence/W01/LRN-01.md#f9-ownership-comparison-october-15-2026) isolated a real-model case without generation: a directly owned model and context exited 0; FNDR's engine exited 134 after its Rust drops completed. Both used an owned Tokio runtime and the same local model and context parameters.

`InferenceEngine` leaks its model to obtain a static reference for its context. The lazy `MtmdVlmRuntime` independently does the same. Both move contexts between blocking jobs using unsafe Send/Sync wrappers, while the installed binding does not implement Send for `LlamaContext`. The app's exit handler stops helpers but does not stop either model runtime. Cancelled waiters can leave real blocking work running.

## Decision

1. Each existing text or vision runtime owns its model, borrowing contexts and backend handle on a worker thread. Create, format prompts, decode and destroy those resources on that thread. Public handles carry owned requests and replies instead of FFI references. Use the existing standard-library and Tokio channel primitives; add no dependency or generic actor framework.
2. Preserve the public inference entry points, prompts, sampling, model selection, context sizes, trace task/version metadata and persistence. Text and vision remain separate runtimes in this slice. Weight sharing, admission scheduling and idle eviction need separate measurements.
3. Shutdown stops new capture/inference admission, rejects new or queued model work, cancels active generation through the existing token-loop controls, and joins the resource owners before native exit. Reuse the existing inference initialization gate and vision registry when closing; an in-flight load must not publish another engine after shutdown begins.
4. A cancelled waiting future does not release resources that its real work still uses. Worker ownership lasts through actual completion or cancellation. Cloneable handles cannot format a prompt or decode against a freed model.
5. Normal Quit remains responsive while draining. Repeated Quit requests must not bypass a drain already in progress. Release contexts and projector resources before their model and backend. No forced process exit or leaked-pointer reconstruction substitutes for this ordering.
6. Keep runtime ownership explicit for the app loop and shutdown procedure. Remove the unnecessary process-lifetime leaks as the owned paths replace them, including the existing unsafe cross-thread context wrappers. Add no dormant unload timer or alternate model path.

## Options considered

| Option | Assessment |
| --- | --- |
| Owned workers | Chosen. A context can borrow its model within one thread's scope, respecting the binding's thread traits and providing an explicit stop/join boundary. Existing generation already serializes each runtime. |
| Self-referential owner under the existing mutex | It can express the borrow lifetime, but still needs a cross-thread safety argument, shutdown coordination and potentially a dependency. It does not address the context's missing Send implementation. |
| Recover and free leaked model pointers at exit | Rejected. Current clones can format prompts outside the context mutex and cancelled waiters can retain real work. Pointer reconstruction would require a fragile global proof. |
| Put inference in another process and kill it on Quit | Rejected for this defect. It adds IPC and another lifecycle while leaving destructor ownership unresolved inside the child. |
| Force process exit before destructors | Rejected. It conceals the assertion and leaves unloading and active work unresolved. |

## Delivery and verification

- Replace the text runtime's leaked model/context boundary first, keeping the model work and trace logic together. Verify real-model idle drop, an active completion and a cancelled waiter before relying on the new lifetime.
- The idle process gate is `CARGO_BUILD_JOBS=2 FNDR_INFERENCE_TEST_APP_DIR="$HOME/Library/Application Support/com.fndr.app.quality-lab/office-pm" cargo test --test model_lifecycle -- --ignored --nocapture` from `src-tauri`. It requires installed weights and asserts a normal child-process exit after engine and runtime drops.
- Apply the owned lifetime to the existing vision runtime and projector. Verify a real synthetic-image request with matching model assets. Missing or mismatched assets remain a failed or unverified gate; do not substitute a fake for this check.
- Integrate native Quit with the existing initialization gate and helper shutdown. Test Quit while idle, while loading, after text generation and after vision use. Record exit codes and request-to-exit time. Every tested normal Quit must exit without the Metal assertion; do not invent a fixed timeout to report success.
- Run relevant inference/cancellation tests and native model checks serially with the repository's resource limits. Run the vault and retrieval gates if output or retrieval behavior changes; preserve their exact results.
- Use the paused synthetic profile for native checks. Do not rewrite the owner vault or install through the Lab profile's shared model symlink.

## Consequences

The runtime handles become message boundaries, and thread startup/shutdown becomes observable work. This is more explicit than the current mutex plus static references, but it gives models a real owner and makes destruction order enforceable. The slice is complete only with real teardown evidence for both runtimes and native Quit, not with compilation or a fake alone.
