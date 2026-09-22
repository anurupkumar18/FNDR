# ADR-017: MCP requires auth by default in every deployment mode, including Local

Status: Accepted 2026-09-21
Decider: project owner (Anurup). Team review happens on the merge request.

## Context

Verified in code on 2026-09-21 (master plan section 2, "MCP security"): in the default `Local` deployment mode, `default_require_auth` returned `false`, so `should_bypass_http_auth` exempted every request from a loopback peer regardless of JSON-RPC method, not just the `initialize`/`tools/list` handshake. `is_origin_allowed` returned `true` unconditionally for `Local` mode, so the `Origin` header was never checked. The CORS layer allowed any origin, method, and header. Together, the MCP server bound to `127.0.0.1` on a random port but exposed all 51 tools, including `memory.search_raw` and `agent.run`, to any local process or web page that could discover the port, with no token and no origin check. This matches the equivalent finding already fixed in FNDR v2.

## Decision

MCP requires a bearer token (`Authorization: Bearer <token>`, read from `~/.fndr/mcp_token`) by default in every mode, including `Local`. Only the `initialize` and `tools/list` handshake from a loopback peer is exempt, so a client can discover the server before authenticating; every other method, including every `tools/call`, requires the token. A request carrying an `Origin` header is refused unless that origin is explicitly listed in `FNDR_MCP_ALLOWED_ORIGINS`; requests with no `Origin` header (CLI clients such as Claude Code, which never send one) are unaffected. The CORS layer only emits `Access-Control-Allow-Origin` for origins on that same allow-list, instead of a wildcard.

`FNDR_MCP_REQUIRE_AUTH=0` remains available to opt back into the old no-auth-on-localhost behavior for local development, but it is off by default and not recommended.

## Consequences

- Any existing local integration that called MCP tools without a token will start receiving `401 Unauthorized` and must read the token from `~/.fndr/mcp_token` and send it as a Bearer token.
- Two adversarial tests (`mcp_rejects_unauthenticated_tool_call_in_default_local_mode`, `mcp_rejects_web_origin_in_local_mode`) plus an updated integration test (`localhost_handshake_bypasses_auth_but_tools_call_requires_token`) in `src-tauri/src/mcp/mod.rs` lock this contract in.
- `docs/mcp.md` and `README.md` no longer describe bearer auth as something only remote/tunnel/public modes need.
