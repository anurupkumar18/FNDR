# ADR 025: Separate peer tasks from FNDR tool access

## Status

Accepted 2026-10-08. The direction is decided; A2A transport and peer tasks are not yet implemented.

## User outcome

A person can ask FNDR to hand a bounded piece of work to a configured peer agent, see what was sent, follow or cancel the task, and inspect the resulting artifact and its evidence. The peer receives only the text and memory attachments the person approved. Its reply cannot operate the Mac or become trusted memory merely because it arrived over a protocol.

## Evidence and current boundary

- FNDR already serves memory and action tools over MCP (`src-tauri/src/mcp/mod.rs`). Its bearer token authenticates the HTTP caller, while the MCP session name is self-reported and is currently used for attribution, not identity. `AgentContextPack` and `AgentAuditRecord` describe local runs; they are not durable peer-task records.
- Hermes is configured with four MCP read tools in `hermes_codex.rs`. Before the first slice of this ADR, its config contained FNDR's full bearer token, and the four-tool list was enforced by the client. A server-issued read grant closes that model/tool-call path. The full token remains in the local discovery file and Tauri status, so this does not isolate a malicious process running as the same macOS user.
- FNDR's cloud egress proof currently counts hosts and requests in memory (`privacy_proof.rs`). It does not durably record the payload class, peer, source IDs, byte count, or outcome required for a reviewable delegation.
- The [A2A 1.0 specification](https://a2a-protocol.org/v1.0.0/specification/) defines an Agent Card, stateful Tasks, Messages, Artifacts, task retrieval and cancellation, and authorization at the transport layer. Its [MCP comparison](https://a2a-protocol.org/v1.0.0/topics/a2a-and-mcp/) treats MCP as access to tools/data and A2A as collaboration between peer agents. The [MCP specification](https://modelcontextprotocol.io/specification/2026-07-28) continues to define the tool/data plane; version negotiation needs separate compatibility work before claiming FNDR supports that revision.

## Decision

1. **Use MCP for capabilities and A2A for peer work.** Do not disguise a long-running peer task as an MCP tool call or advertise FNDR's 51 tools as 51 A2A skills. FNDR remains a local MCP server. The first A2A role is an outbound client to explicitly configured peers. An inbound A2A server waits until FNDR has a per-peer principal, authorization, durable task store, cancellation and source-scoped reads.
2. **Configure peers explicitly.** The first release accepts a person-entered HTTPS Agent Card URL and pins the resulting origin and selected interface. Card text, examples and remote task replies are untrusted data. Do not discover peers from captured text, follow arbitrary redirects, accept an IP in a private/link-local range for a remote peer, or place credentials in a Card or A2A Message. Transport authentication is configured out of band. A local loopback fixture is allowed in tests only.
3. **Create a delegation draft before egress.** The draft contains the peer, task text, output goal, deadline/budget, and an explicit list of attached memory IDs. No automatic recent-memory expansion, screenshot, transcript, raw OCR, file body, or graph pack is added. Resolve every attachment against today's durable record and privacy policy at send time, then show the exact rendered payload and destination. A removed, excluded, or missing source blocks send until the draft is revised.
4. **Keep remote identities and lifecycle distinct.** FNDR assigns a local draft/run ID. It never fabricates an A2A `taskId` or `contextId`; it records the peer's IDs when returned. Reuse a client `messageId` on retry, but do not assume that makes Send idempotent. After an uncertain send, query a known peer task before resending; if no task ID was returned, mark the state uncertain for review. Cancellation is requested and then verified through Get Task; a timeout is not a canceled task.
5. **Treat Artifacts as the result.** Show Messages as conversation/status, and Artifacts as outputs. A remote artifact is untrusted external evidence: never execute embedded instructions, import it into Memory, or authorize a tool from it automatically. A person may explicitly save a reviewed artifact with peer provenance and its local attachment references.
6. **Recheck and record every crossing.** Persist a bounded local egress event before Send: peer ID/origin, payload classes, byte count, attachment IDs and current policy result, without body text or credentials. Record the response/task ID or uncertain failure afterward. On Get/Subscribe/Cancel, authorize the local task owner and peer before network access or revealing task existence. Use the existing FNDR kill switch for any action a peer later proposes, with the same approval path as native and MCP actions.

## Smallest parts and acceptance gates

| Order | Part | Existing seam | Proof before moving on |
| --- | --- | --- | --- |
| 1 | Server-issued Hermes read grant | `mcp` HTTP auth and `hermes_codex` config | Real localhost MCP test: only four tools listed/callable, raw opt-in refused, write/execute/resource methods refused, full token unchanged, grant inaccessible on stop or after consent is turned off. |
| 2 | Configured peer and Card validation | Existing bounded HTTP client in `http_util` | Local fixture and negative tests for HTTPS/origin/redirect/IP, Card version/interface/auth/size, no network on invalid input. |
| 3 | Delegation draft and source check | `context_runtime::context_source_memories` | Hidden, deleted, alias, stale and missing sources block or refresh before bytes leave; payload preview equals sent bytes. |
| 4 | Outbound Send/Get/Cancel | New small `agent/peer` module, existing `AgentAuditRecord` links | Mock A2A 1.0 peer exercises completed, input-required, auth-required, failed, uncertain send, cancel pending and cancel confirmed. No invented IDs. |
| 5 | Durable egress/task ledger and UI | Privacy Activity, Agent surface | Restart retains state and content-free egress record; UI shows destination, current status, attachment list, artifact and refusal. |
| 6 | Interoperability and quality | A2A conformance fixtures plus FNDR QA | At least two independent peer implementations; measure success, useful artifact rate, unsupported claims, cancellation, privacy negatives, p50/p95 time and bytes on named hardware. |

Each part must be usable or verifiable before the next. Part 1 is implemented: the localhost MCP boundary test and 39 MCP server tests passed on 2026-10-08; the other rows remain open.

## Rejected shortcuts

- **Give every peer the current MCP master token.** The token grants the entire tool surface and cannot identify or revoke a peer independently.
- **Trust an MCP `clientInfo.name` or A2A Message field as the principal.** Both are caller-supplied content. Identity comes from transport credentials bound to the configured peer.
- **Wrap `agent.run` as an A2A server now.** The local deterministic run lacks durable peer-task semantics, per-peer read scope, artifact lifecycle and reliable cancel behavior.
- **Send an `AgentContextPack` wholesale.** Its derived text and historical fields need complete source provenance. Explicit current attachments are easier to inspect and enforce.
- **Accept remote push webhooks initially.** Polling a known task avoids an inbound listener, webhook credential store, duplicate delivery and SSRF path until those are justified by measured latency needs.

## Consequences and compatibility

Existing MCP clients keep the full token and current API. Hermes receives a separate process-lifetime token for exactly four tools; client-side allowlisting remains a second layer. A2A adds no new network call until the person configures and sends to a peer. The first A2A feature cannot claim a general multi-agent runtime, autonomous delegation, inbound interoperability or complete privacy accounting before the gates above pass.
