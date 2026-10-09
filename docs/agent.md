# FNDR Agent

"Agent" means two different things in this repo. This file covers both, the page first.

1. **The Agent page**: a chat with Hermes, an open-source agent FNDR runs as a child process. This is what a person sees.
2. **The MCP agent tools** (`agent.*`): a deterministic, model-free runner that outside agents call over MCP. It has no page.

Decisions behind the page: [ADR 018](decisions/018-reasoning-tier.md) (when a cloud model may be used) and [ADR 024](decisions/024-agent-surfaces-egress-and-action-policy.md) (what is sent and what Hermes may do). Product requirements: `docs/superpowers/specs/2026-10-07-agent-surfaces-prd.md`.

## The Agent page (Hermes chat)

### What it does

A person picks a provider, sends a message, and gets an answer. They can attach up to 8 memories to a message. FNDR can also add up to 5 related memories on its own.

Hermes answers only. Started by FNDR it has two kinds of tools and no others:

- its own `todo` planning list
- FNDR's read-only memory tools over MCP: `memory.search_full_context`, `memory.get_context_pack`, `memory.timeline`, `memory.source_evidence`

It has no terminal, file, code, browser or schedule tools. FNDR writes that limit into the Hermes config (`hermes_codex.rs`: `HERMES_TOOLS_YAML`, `HERMES_MCP_TOOLS`) and enforces the four memory reads again at the MCP server with a separate bearer token. The config was checked on a live gateway (`docs/evidence/W03/agent-surfaces-phase1.md`); the server grant was checked with a real localhost MCP test (ADR 025). The token is scoped to this embedded Hermes connection, not to every process running as the Mac owner.

### Providers

| Provider | Where the message goes | Related memories sent |
|---|---|---|
| ChatGPT (Codex sign-in) | OpenAI, on the person's ChatGPT plan | only when turned on in setup |
| OpenRouter | OpenRouter, with the person's key | only when turned on in setup |
| Custom endpoint | the base URL the person typed | only when turned on, unless the host is this Mac |
| Ollama | this Mac, or the base URL set for it | always when it is on this Mac |

Memories a person attaches by hand are always sent, except ones from an app excluded since they were picked. Every request to a provider is listed in Privacy Activity with the feature, the host, the bytes and what kinds of content went along, never the content.

### How a message travels

```text
AgentWorkspace.tsx
  -> send_hermes_message (hermes_agent.rs)
       -> attached memories by id, minus excluded apps (agent_chats.rs)
       -> related memories from the shared Search path (operator/memory.rs), if allowed
       -> POST /v1/responses on the local Hermes gateway (127.0.0.1:8742)
            -> Hermes -> the chosen provider
       -> answer, the memories FNDR added, and what Hermes did on the way
  -> stored in agent-chats.json (owner-only file)
```

The gateway is a pinned Hermes (`hermes_codex::HERMES_PINNED_COMMIT`) installed into FNDR's app data on first use. FNDR starts it when a message needs it, restarts it once after a crash, stops it when FNDR quits, and stops one left running by a crash at the next start. There is no manual start, stop or update control.

For ChatGPT, Hermes reuses the Codex sign-in on this Mac. The Codex app-server is the only thing that refreshes the login; Hermes's copy is emptied whenever the login rotates. Signing out on the Agent page signs Codex out on this Mac too.

### Commands the page calls

| Command | File | Purpose |
|---|---|---|
| `get_hermes_bridge_status` | `hermes_agent.rs` | provider, model, install and gateway state |
| `install_hermes_bridge` | `hermes_agent.rs` | install the pinned Hermes |
| `save_hermes_setup` | `hermes_agent.rs` | save provider, model, key and the related-memories choice |
| `send_hermes_message` | `hermes_agent.rs` | send one message, return the answer |
| `cancel_hermes_message` | `hermes_agent.rs` | Stop: drop the message in flight and hand it back |
| `list_agent_chats`, `get_agent_chat`, `delete_agent_chat` | `agent_chats.rs` | chat history |
| `search_memory_cards`, `list_memory_cards` | `search.rs` | the memory picker |
| `reopen_memory` | `memory.rs` | open a cited or attached memory's source |
| `codex_account_status`, `codex_login_start`, `codex_login_cancel`, `codex_logout` | `codex_account.rs` | the ChatGPT account card |

Front end: `src/domains/workspace/AgentWorkspace.tsx`, `AgentReply.tsx` (renders answers: lists, code, emphasis, http links, and `[2]` citations that open the cited memory), `CodexAccountCard.tsx`.

### Instructions Hermes is given

All in `src-tauri/src/inference/prompts.rs`, fingerprinted: `HERMES_CHAT_INSTRUCTIONS`, `HERMES_IDENTITY`, `HERMES_OPERATING_NOTES`, `HERMES_MEMORY_PREAMBLE`, `HERMES_ATTACHED_MEMORIES_HEADER`. Memory text is framed as evidence, not instructions.

### Not built

- Hermes cannot act on the Mac. Acting is Notch Do's job, under its own per-call policy (`operator/policy.rs`).
- Chat history keeps the newest 200 chats and 400 messages in each. Nothing expires by age.
- No streaming: an answer arrives whole.

## The MCP agent tools

Outside agents that connect to FNDR over MCP can call `agent.build_context_pack`, `agent.run`, `agent.privacy_status`, `agent.explain_retrieval`, `agent.rate_result`, `agent.list_prompts` and `agent.get_prompt`. These use the `src-tauri/src/agent/` module and load no model.

```text
memory records / graph / search
  -> context_runtime::ContextPack
  -> agent::AgentContextPack
  -> Ask / Plan / Act / Learn response
  -> policy / persisted audit / feedback / skill and eval drafts
```

- `agent/context.rs`: typed context pack and the deterministic Ask, Plan, Act, Learn runner.
- `agent/policy.rs`, `agent/risk_policy.rs`: mode-based permission and risk policy.
- `agent/audit.rs`: append-only JSONL audit and retrieval feedback ledger.
- `agent/skills.rs`, `agent/evals.rs`: draft shapes a person reviews before anything is activated.
- `agent/prompts.rs`: the MCP prompt registry.
- `ipc/commands/agent.rs`: the same functions as Tauri commands. **No page calls them today**; they are reachable only over MCP.

### Modes

- Ask: read-only memory and context. No file writes, commands or outside messages.
- Plan: reads memory and project context and suggests next steps. No execution.
- Act: builds context; every action needs approval first, including opening a link or a file.
- Learn: describes skill and eval candidates; activation needs a person's review.

### Safety and audit

Tool policy sits outside any model. File writes, mutating commands, outside messages and credential access are denied or held for approval in code.

Every `agent.run` call tries to append an audit row under the app support `agent/` directory: run id, mode, goal, context pack id, memories used, policy decisions, approvals required, dropped or redacted context, confidence, output summary, status and any error. A failed audit write does not fail the response; the response carries a warning instead.

### Feedback

Callers can rate a retrieval result as `useful`, `irrelevant`, `wrong`, `stale` or `missing_context`. Ratings are stored locally and attached to the run. They do not change ranking yet.

Defaults that keep this path light on an 8 GB Mac: no model is loaded to build a context pack, raw evidence is off, the Ask budget is 900 tokens, and no visual model is used.
