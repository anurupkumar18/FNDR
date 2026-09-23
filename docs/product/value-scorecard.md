# FNDR value scorecard

**Status:** product direction for the Beta. This is not a user-study result.
Claims below are marked as implemented, planned, or unverified so a demo does
not outrun the evidence.

## The promise

FNDR helps a person resume interrupted knowledge work by finding local,
cited context from work they have already chosen to capture.

It is not a screen recorder, a surveillance product, an autonomous employee,
or a generic second brain. The first useful moment is returning to a task and
seeing what was happening, what changed, and where the supporting memories
are.

### Primary user

A privacy-conscious knowledge worker who switches among code, documents,
browsers, terminals, and meetings, then loses time reconstructing a task after
an interruption. Developers are the first useful test group because their
work makes citations, error recovery, and project context easy to inspect.

### Thirty-second explanation

FNDR is a local work-memory layer for macOS. It turns permitted screen context
into searchable memories, then helps you return to a project with citations
instead of asking you to trust a generated summary. It treats privacy controls
as a product surface: a user can see what was skipped and why.

## Feature map

| Surface | Role in the product | Status | What we may say today | Proof still needed |
| --- | --- | --- | --- | --- |
| Resume Work | **Front door.** A short, cited context pack to restart a task. | Planned in FEA-02 and FEA-03. | It is the Beta direction, not a shipped workflow. | Pack latency, cited-claim correctness, and an unaided resume task. |
| Search and Memory Vault | Supporting retrieval layer for finding prior work. | Implemented foundation. | FNDR stores structured local memory records and supports hybrid retrieval, browsing, and grounded answers. | Native dogfood evidence that retrieval helps with real resumption. |
| Deja vu | Quiet, cited nudge from an on-screen error to a prior fix. | Planned in FEA-04. | It is a proposed assist, not a current capability. | Zero false triggers on the fixture corpus and a seeded correct-match scenario. |
| Privacy Activity | **Trust proof.** Current-session explanation of evaluated, stored, and skipped context. | Foreground UI implemented; native proof incomplete. | The UI presents bounded session activity and does not claim a full network audit. | Native blocklist, absence-from-retrieval, and permission-state evidence. |
| Screen Guide | Explicit, ephemeral help with the current display. | Partially implemented and browser verified. | It is user-invoked, local/read-only, and does not click, type, or store the turn. | Native permissions, shortcut, overlay, and multi-display evidence. |
| Daily Summary, To-dos, Wrapped, Stats | Reflection and follow-up support. | Implemented surfaces with remaining native and IA work. | They are supporting workflows, not the product pitch. | Real-use evidence before promotion. |
| Knowledge graph and Engine diagnostics | Inspection and developer tooling. | Present, but not a hero workflow. | They explain or diagnose the system. | A demonstrated user need before promotion. |

## Product decisions

1. Lead every demo, pitch, and Home design with **Resume Work**, even while it
   remains planned. Search and Vault supply its evidence rather than competing
   as separate front doors.
2. Treat **Deja vu** as a rate-limited cited assist. It stays silent when the
   evidence is weak, and it does not ship after a fixture false trigger.
3. Treat **Privacy Activity** as a condition of trust, not a decorative
   dashboard. Its scope remains current-session counters and recorded egress
   activity, not a complete audit claim.
4. Do not promote the graph, Engine diagnostics, broad autonomous actions, or
   reflection surfaces as the reason an average person should adopt FNDR.

## Measures that decide whether the promise is real

| Question | Initial measure | Evidence owner | Decision rule |
| --- | --- | --- | --- |
| Does Resume Work reduce reconstruction effort? | Four of five study users complete a resume task unaided; cited packs return at p95 of two seconds or less. | FEA-02 and FEA-03 | Do not market it as the front door until citations and task completion are demonstrated. |
| Is Deja vu helpful rather than distracting? | Zero false triggers on the fixture corpus and one correct seeded match. | FEA-04 | Do not ship after a false trigger. |
| Is privacy understandable and credible? | A safe blocklisted scenario reports a content-free skip reason and is absent from Search, Resume Work, and Vault. | OPS-19 and OPS-20 | Do not call it proof until native evidence exists. |
| Does FNDR help real work? | Three working days of diary entries, including helpful, unhelpful, and missing-context moments, plus one non-team review. | OPS-16 | Cut, merge, or explain a surface with no observed use. |

## Current limitations

- A draft label set and synthetic fixtures are not user-value evidence.
- Browser and fixture tests do not prove macOS permissions, capture behavior,
  biometric authentication, native counters, or external-file behavior.
- FNDR is local-first, but onboarding must continue to state model-download
  and optional-integration network exceptions plainly.
- No external user research result is claimed in this document.

## Next actions

1. OPS-16 records the dogfood diary and an external review without storing
   sensitive source content in git.
2. OPS-17 turns those observations into a keep, defer, or cut decision.
3. OPS-19 and OPS-20 provide the native privacy evidence needed to make the
   trust proof part of a demo.
