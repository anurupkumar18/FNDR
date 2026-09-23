# FNDR documentation

Use this index to find the right document quickly. **Authoritative agent vocabulary** lives in [`CONTEXT.md`](../CONTEXT.md) together with [`AGENTS.md`](../AGENTS.md) at the repository root.

## Start here

| Document | Purpose |
| --- | --- |
| [`team/TEAM.md`](team/TEAM.md) | Team guide: what we build, what to do now, definition of done |
| [`CONTEXT.md`](../CONTEXT.md) | Product terms, where truth lives, default quality bar |
| [`architecture/ARCHITECTURE.md`](architecture/ARCHITECTURE.md) | Capture → search → UI pipeline and core Rust modules |
| [`product/DESIGN_DIRECTION.md`](product/DESIGN_DIRECTION.md) | UX and visual direction |
| [`product/UI-UX-OVERHAUL-PROGRAM.md`](product/UI-UX-OVERHAUL-PROGRAM.md) | Evidence-backed UI/UX program, complete surface ledger, accessibility matrix, and ordered delivery slices |
| [`product/screen-guide.md`](product/screen-guide.md) | Local, ephemeral Screen Guide product contract |
| [`mcp.md`](mcp.md) | MCP tools, modes, privacy model, and agent-facing additions |
| [`agent.md`](agent.md) | FNDR Agent architecture, modes, provider strategy, and safety |
| [`agent-context-pack.md`](agent-context-pack.md) | Typed context pack schema, ranking, redaction, and provenance |
| [`skills-and-evals.md`](skills-and-evals.md) | Skill lifecycle, eval case shape, and approval requirements |

## By topic

| Folder | Contents |
| --- | --- |
| [`decisions/`](decisions/) | Architecture decision records (ADRs), numbered filenames; ADR-015: v1 is the product, v2 is a knowledge source |
| [`architecture/`](architecture/) | Long-form architecture + insight graph schema (`graph-schema.md`) |
| [`setup/engineering/`](setup/engineering/) | Implementation guides (timeline rules, repo layout, refactoring notes, agent tooling) |
| [`product/`](product/) | Product-level technical notes, incl. the intelligence engine (`intelligence-engine.md`) |
| [`agents/`](agents/) | Reserved for agent/MCP-oriented runbooks (add as needed) |
| Frontend source layout | [`../src/domains/README.md`](../src/domains/README.md) |

## Root files (not under `docs/`)

`README.md`, `AGENTS.md`, `CLAUDE.md`, and a short root `CONTEXT.md` pointer exist for tooling and first-time orientation. All substantive prose should live under **`docs/`** as above.
