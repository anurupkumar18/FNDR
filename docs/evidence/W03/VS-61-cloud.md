# VS-61 every MCP route and method, checked for auth gaps (cloud)

## Routes and methods

The HTTP server (`src-tauri/src/mcp/mod.rs`, `start`) has four routes. Every POST carries JSON-RPC. Results are for the default Local mode (loopback bind, token required, loopback handshake exemption on). Each row is covered by a test.

| Route | Request without a valid token | Result | Test |
|---|---|---|---|
| `GET /` | any | 200, server facts only (name, endpoints, mode, auth mode); no memory data | `localhost_handshake_bypasses_auth_but_tools_call_requires_token` checks the exact key set |
| `GET /mcp`, `GET /mcp/sse` | no token | 401 | same test |
| `POST /mcp`, `POST /mcp/messages` | `initialize`, `tools/list`, or a batch of only those, from a loopback peer | 200 (the documented handshake exemption, ADR-017) | same test |
| same | `tools/call`, alone or in a batch after handshakes | 401 | same test (VS-41's case and a longer one) |
| same | nested batch, notification (no `id`), unknown method, `Tools/List` in another case, empty batch | 401 | same test |
| same | wrong token, token without `Bearer `, `bearer` in lower case | 401 | same test; `the_token_must_match_exactly` adds more forms |
| same | `Content-Type: text/plain` | 415 before the handler runs; no data | same test |
| same | a web page's `Origin`, even with the token | 403 | same test |
| any | a peer that is not loopback (`192.168.1.20`, `10.0.0.7`, `0.0.0.0`, `[2001:db8::1]`) asking for a handshake method | no exemption, so the token is required | `only_loopback_peers_get_the_handshake_exemption` |
| any | a payload shaped to hide a call behind the exemption (13 shapes) | no exemption | `no_payload_shape_carries_a_call_past_the_handshake_exemption` |

Every JSON-RPC method other than `initialize` and `tools/list` needs the token. That includes `tools/call` for every tool, every `memory.*` tool, and unknown methods. There is no route that returns memory data without it.

## Gaps found and fixed

1. **An environment variable could turn auth off on the network.**
   - Before: `FNDR_MCP_REQUIRE_AUTH=0` disabled the token check in every mode. That includes Public mode, which binds to a network address, so anyone who could reach the port could call every tool. `FNDR_MCP_ALLOW_LOOPBACK_AUTH_BYPASS=1` turned on the handshake exemption in Tunnel mode, where a tunnel delivers internet traffic from loopback.
   - `docs/mcp.md` already said remote modes "must use bearer auth ... regardless"; the code did not enforce it.
   - Now `auth_settings` honors the two overrides only in Local mode. In Tunnel and Public mode, auth is always required, with no exemption, and a loosening override is logged and ignored. Tightening is always honored.
   - Test: `auth_overrides_only_loosen_local_mode`; it did not compile before.
2. **The token comparison stopped at the first wrong byte and accepted an empty token.**
   - `check_auth` used `==`, whose time depends on the length of the matching prefix, and an empty expected token matched the header `Bearer `. The token is never empty in practice (an empty file is regenerated), but the check should not rely on that.
   - Now `tokens_match` compares every byte, and an empty token never matches.
   - Test: `the_token_must_match_exactly`; its empty-token case failed before.
3. **The token file was briefly readable by every local user.**
   - `~/.fndr/mcp_token` was written with default permissions (0644 under the usual umask) and only then changed to 0600. A file left 0644 (by a failed change, or by an older build) stayed that way.
   - Now the file is created 0600 from the start, and a token file that others can read is tightened when it is loaded.
   - Tests: `a_new_token_file_is_readable_by_the_owner_only`, `a_token_file_others_can_read_is_tightened_on_load`, `an_empty_token_file_gets_a_new_token`.

## Not changed

- **The root probe answers without a token.** It is the documented discovery endpoint and returns no memory data; its keys are now pinned by the test.
- **The 415 for a wrong content type comes from the JSON extractor**, before the auth check, so a wrong-type request gets 415 rather than 401. It returns no data. Moving auth into a middleware layer would change that order; it is not needed for safety.
- **Stdio:** there is no stdio transport in this module.

## Results

- `cargo test --lib`: 925 passed, 0 failed, 10 ignored.
- `mcp::` tests: 15 passed, including the HTTP test with 13 new refused requests.
- Retrieval gate: see below.

The change does not touch retrieval. The gate ran after it on fresh seeds (America/Denver, 2026-10-05), against the committed references on main: knowledge-worker and office-pm both PASS, "No per-query rank changed".
