# Agent and output quality slices, filed 2026-10-08

These tickets reconcile merged work with the remaining native and real-use evidence. A merged implementation goes to Evidence for reviewer closure; a partial slice stays Ready. Details live in `docs/handoffs/2026-10-08-cross-session-closeout.md`.

## AG-01 Enforce the Hermes memory-read boundary
- assignee: anurupkumar
- labels: area::command, type::bug, prio::p0
- milestone: W03-Build
- estimate: 4h
- depends: VS-61

**Why.** A token intended for Hermes must not read unrelated MCP tools or raw screen evidence.

**Do.** Limit the server token to four approved read tools, current sharing consent, and safe derived text. Reuse the MCP authorization path.

**Done when.** Loopback tests prove tool discovery and calls refuse other tools, methods, side effects and raw evidence, including after restart.

**Evidence.** Commits `aeb8a05`, `4dbcc95`; focused MCP tests 39 + 15 and token tests. The running pinned Hermes gateway remains a separate native check.

## AG-02 Make Agent and Notch Do safe and understandable
- assignee: anurupkumar
- labels: area::command, type::feature, prio::p1
- milestone: W03-Build
- estimate: 8h
- depends: none

**Why.** The mounted agent surfaces need explicit approvals, clear result state, and visible privacy choices.

**Do.** Reuse existing risk policy and journal, improve Notch Do/Hermes responses, stop behavior, Labs and Operate settings, and remove unmounted panels.

**Done when.** Frontend tests, Rust checks, and a native twenty-task Notch Do set plus pinned Hermes tool/refusal check pass.

**Evidence.** Commits `5b98a86`, `ed175dd`, `f45faa0`, `7065266`, `f424ba5`, `95913c6`, `1b5d48a`; `docs/handoffs/2026-10-07-agent-surfaces.md`. Native acceptance is pending.

## AG-03 Save verified peers and preview exact task drafts
- assignee: anurupkumar
- labels: area::command, type::feature, prio::p1
- milestone: W03-Build
- estimate: 6h
- depends: none

**Why.** A person needs to know the destination and exact shared text before a peer task can leave the Mac.

**Do.** Reuse StateStore and current context-source authorization to save bounded HTTPS Agent Cards and preview a task, output goal and selected memory display summaries. No transport in this slice.

**Done when.** Invalid Cards, blocked or missing memories and raw-snippet fallback are refused; mounted UI previews the exact draft and has no Send control.

**Evidence.** `d1a575f`, `adb2891`, `80340a9`, `bf7e126`; 7 peer, 1 store, 2 delegation and 23 frontend tests; browser preview. Native UI/IPC remains unverified.

## AG-04 Send one reviewed task to an independent A2A peer
- assignee: anurupkumar
- labels: area::command, type::feature, prio::p1
- milestone: W04-Prove
- estimate: 8h
- depends: AG-03

**Why.** The current draft never sends; a useful peer workflow needs a safe and accountable egress path.

**Do.** Revalidate Card and credentials, rebuild current authorized preview at Send, compare reviewed bytes, persist a local run and content-free egress record, then send through the existing agent domain. Treat remote messages as untrusted.

**Done when.** A real independent peer receives the exact reviewed text; stale/hidden/deleted source, auth failure, retry and cancellation tests pass; Privacy Activity records the send without storing secrets.

**Evidence.** Sanitized transport trace, tests and native UI run. Do not add a second delegation framework.

## AG-05 Prove native Agent and peer workflows on a disposable profile
- assignee: anurupkumar
- labels: area::tests, type::qa, prio::p1
- milestone: W04-Prove
- estimate: 4h
- depends: AG-02, AG-04

**Why.** Browser fixtures and a successful native build do not prove macOS UI, permission or peer behavior.

**Do.** Run mounted Agent, Notch Do, Hermes and peer scenarios on a disposable `FNDR_DATA_DIR`; record outcomes without capture text.

**Done when.** A visible native window is exercised; twenty-task set, Stop/halt, tool refusal, permission prompt and peer send each have an explicit result.

**Evidence.** Sanitized native QA table and logs under `docs/evidence/W04/`.

## OQ-01 Ground task suggestions in current screen evidence
- assignee: anurupkumar
- labels: area::vault-search, type::bug, prio::p0
- milestone: W03-Build
- estimate: 6h
- depends: none

**Why.** Hundreds of old inferred tasks polluted To-dos and summaries.

**Do.** Reuse task records and current memory sources; require a supporting sentence, suppress AI-chat and repeated suggestions, and separate suggestions from accepted tasks.

**Done when.** To-dos does not create tasks on open; the direct finder and suggestion gate pass labeled cases; old unsupported suggestions are retired with an owner-vault backup.

**Evidence.** `9803137`, `f051193`, `3dbb602`, `7b02579`, `f3be537`; `docs/evidence/W04/task-audit.md` and 30-screen synthetic eval. Real mail/chat recall remains a separate ticket.

## OQ-02 Keep briefings and memory copy factual
- assignee: anurupkumar
- labels: area::local-models, type::bug, prio::p1
- milestone: W03-Build
- estimate: 6h
- depends: none

**Why.** Model-written briefings fabricated advice or copied inputs, while summaries narrated the person and some search matches overstated confidence.

**Do.** Reuse stored summaries to compose the briefing without a model; clean voice and title echoes; test strong-match negatives and preserve retrieval rank.

**Done when.** Briefings report only supported facts; cards use neutral past-tense text; time words alone cannot mark an unrelated hit strong.

**Evidence.** `98fe16e`, `145a0eb`, `371d550`, `2bb7243`, `527bc6e`, `9b5f0b5`; `docs/evidence/W04/voice-and-fallback.md` and `strong-match.md`. A later owner-vault repair is separate.

## OQ-03 Verify task recall and To-dos in real use
- assignee: anurupkumar
- labels: area::tests, type::qa, prio::p1
- milestone: W04-Prove
- estimate: 4h
- depends: OQ-01

**Why.** Synthetic cases all passed, but recall on a day of real mail/chat captures and the empty native To-dos sentence are not confirmed.

**Do.** Obtain owner-reviewed task labels for one or two days of relevant captures; replay without exposing text in Git; check the empty panel in native UI.

**Done when.** Recall and false suggestions have denominators and counts, and the native empty-state sentence is visibly confirmed or fixed.

**Evidence.** Content-safe score table and native check in `docs/evidence/W04/`.

## AG-06 Measure Notch Do with a labeled suite before changing it
- assignee: anurupkumar
- labels: area::command, type::feature, prio::p1
- milestone: W04-Prove
- estimate: 6h
- depends: AG-02

**Why.** AG-02 and AG-05 ask for a "twenty-task Notch Do set" and none existed, so a change to the planner prompt, the routing words or the policy had nothing to be scored against.

**Result, 2026-10-14.** `scripts/operator/notch-do-cases.json` (30 cases) and `operator::eval`: routing scored on every test run through `plan::route`, which the run now uses; plan labels scored on recorded planner answers. It found five misroutes from never-do words used as nouns, fixed. The live planner test exists and has not been run.

**Done when.** A live recording exists and its floor holds, and the suite has cases for the operate step's tool policy.

**Evidence.** Test output; the first recording under `scripts/operator/recordings/`.

## AG-07 One action layer: Notch Do, reached from the command bar too
- assignee: anurupkumar
- labels: area::command, type::chore, prio::p1
- milestone: W04-Prove
- estimate: 6h
- depends: AG-06

**Why.** The command bar's own stack (typed tool registry, run/confirm/refuse policy, action approvals, skill and eval drafts, about 3,600 lines) had no caller in the app, while Notch Do carried out every action under a separate policy. Owner decision, 2026-10-14.

**Result, 2026-10-14.** The unreached stack and its 15 IPC commands are removed; MCP keeps context packs, `agent.run`, its audit, prompts and the write gate. The command bar shows "Do it: ..." for an action request and hands it to the notch (`omnibar_do`), which plans it without listening first. `docs/product/command-surface.md` describes the pipeline as it runs.

**Done when.** The command bar hands a request to Notch Do in the running app, and the step types the October tool list still lacks (paste, reminders, Shortcuts, timers, reveal file) are ticketed one by one as Notch Do steps.

**Evidence.** Component tests for the hand-off; a native run.

## AG-08 Notch Do learns from its runs: ratings, cases from wrong runs, skills from good ones
- assignee: anurupkumar
- labels: area::command, type::feature, prio::p1
- milestone: W04-Prove
- estimate: 6h
- depends: AG-06, AG-07

**Why.** The journal recorded every action and nothing read it back beyond Privacy Activity, so a run that went wrong could not become a test and one that went well could not be repeated. Owner decision, 2026-10-14.

**Result, 2026-10-14.** `operator::runs` keeps each run's request, plan and outcome and the person's rating; a run rated as not working is a draft suite case (`notch_do_case_drafts`). `operator::skills` saves a run that worked under a name and `plan::route` runs it again by that name through `carry_out`. The notch's finished card asks "Did that work?" and offers "Save as a skill".

**Done when.** A skill is saved and run again by voice in the running app, and a draft case from a real run is labeled into the suite.

**Not built.** A place to list, rename and delete skills; saving a work set or a run with an operate step as a skill is allowed but untested natively.
