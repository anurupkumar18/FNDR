# VS-41 check MCP auth for every item of a JSON-RPC batch (cloud, proposed ticket)

Found by the VS-35 spec work and confirmed in code: in the default Local deployment mode, loopback requests may skip the bearer token for the handshake methods `initialize` and `tools/list` (`should_bypass_http_auth`, `src-tauri/src/mcp/mod.rs`). The method that decides the exemption came from `jsonrpc_method_hint`, which for a batch returned the first item's method. A batch that starts with `initialize` therefore ran every later item, `tools/call` included, without the token. ADR-017 says `tools/call` needs the token. Browser pages are still stopped by the Origin check; the gap was for any local process.

Fix: for a batch, the hint is the first item that is not a handshake method (or `None` for an item without a method), so a batch is exempt only when every item is a handshake method.

Test (written first): the existing HTTP test `localhost_handshake_bypasses_auth_but_tools_call_requires_token` now also posts `[initialize, tools/call]` without a token and expects 401, and a batch of two `tools/list` without a token and expects 200. The added assertion failed on the old code (`left: 200, right: 401`) and passes now. The assertions live in the existing test because both tests would start and stop the same process-wide MCP server and race each other when run in parallel.

`cargo test --lib` (Linux, cloud build shim): 883 passed, 0 failed, 10 ignored, three runs in a row.

Not changed: the separate SSE endpoint passes no method and is unaffected; the loopback handshake exemption itself stays as ADR-017 describes.
