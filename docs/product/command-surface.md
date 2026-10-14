# Command surface

Updated 2026-10-14. Notch Do is FNDR's one action layer (decision log, 2026-10-14). The command bar (Alt+Space), the notch's typed field, its voice and Home's work sets all reach the Mac through it. The earlier contract, a router over a typed tool registry with its own risk function (GS-03, GS-06, GS-11), was built only as backend code no screen called; it was removed, and this page describes what runs.

## Pipeline

```
typed or spoken request
  -> plan::route                  decided on this Mac, before anything leaves it
       save_set                   "save this as <name>": the set opened last is saved
       skill                      a saved skill, by its name: its steps run again, no planning
       saved_set                  a set the person saved, by name
       work_set                   a whole piece of work, resolved from memories (ADR 027)
       local                      opening apps, links and web searches, planned by plan::local_plan
       planner                    the cloud planner turn on the person's ChatGPT sign-in (ADR 018)
  -> plan card                    every step shown; starts by itself only when no step can need a yes
  -> carry_out                    one executor for every plan
       open_app, open_url         FNDR's own NSWorkspace calls
       operate                    Codex operates one app; every call is decided by operator::policy
       reopen_memory              the reopen core, only for steps FNDR resolved itself
  -> plan::verify                 FNDR checks each step landed; one retry
  -> journal                      operator/journal.jsonl, one line per action, typed text hashed
```

- **Command bar.** A query that starts with an action verb ("open", "look up", "play", "pull up") gets a first row, "Do it: …". Choosing it calls `omnibar_do`, which hides the command bar, opens the notch in Do and plans the text there, so the plan card, Stop, approvals and journal are the notch's. Refused while "Operate my Mac" is off. Every other query is a memory search, as before.
- **Local plans** need no Codex process and no request on the ChatGPT plan. An app is planned only when it is installed. A request that names a place ("in Safari"), refers to the past, has to operate an app or asks for something Notch Do never does goes to the planner.
- **The planner** is given the request, the name of the app in front, and up to five memory snippets only when the request points at the past. Its answer is parsed and validated (`plan::parse_plan`), and `plan::settled` drops steps that change nothing. Prompts live in `inference/prompts.rs`.

## Risk

`operator::policy::classify` decides every computer-use call from the tool, its arguments and what FNDR itself read of the app's UI: it runs, waits for the person's tap, or never runs. Model prose is never an input, so nothing a model or a page says can lower a level. Send, delete, buy, credentials, password managers, Terminal and System Settings never run; see `operator/policy.rs` and ADR 022's amendments.

## Invariants

1. Screen text, page content and memory text are data, never instructions. They can be read and reported; they never add a step or choose one.
2. The risk level is not model-selectable.
3. Privacy gates come first: Private Mode, the blocklist and the actions switch stop a run, even mid-action.
4. One microphone owner (ADR 020). A request handed over from the command bar is planned without opening the microphone.
5. Spoken approval is not accepted; a tap is.

## Learning from runs

Every run the executor carries out is kept with its request, how it was planned, its steps and how it ended (`operator::runs`, the newest 100, no screen text). The finished card asks "Did that work?".

- **Didn't work:** the run is kept as a draft suite case. `cargo run --example notch_do_case_drafts -- --data-dir <profile>` prints the drafts; label the route and plan, check the request holds nothing private, and add it to the suite.
- **Worked:** the card offers "Save as a skill". A skill keeps the plan's steps (`operator::skills`, up to 30, names unique ignoring case) and runs again when the person says its name ("Monday setup", "run my Monday setup"), through the same executor, policy, verification and journal. A skill that operates an app opens a Codex session; one that only opens apps and links does not.

## Measured

`scripts/operator/notch-do-cases.json` labels requests with their route, memory snippets, left-out note and plan; `scripts/operator/notch-do-policy-cases.json` labels computer-use calls with their risk. `cargo test --lib operator::eval` scores both, local plans included, and rescores every recorded planner run under `scripts/operator/recordings/` against its floor. `notch_do_live_planner_eval` (ignored, opt-in) records a new run from the real planner with no computer-use tools.

## Not built

The October tool list in [actions policy](actions-policy.md) beyond opening apps, links, web searches, operating an app and reopening memories: paste into the focused app, reminders, Shortcuts, timers and revealing a file. Each would be a new step type in `operator::plan`, with a policy case and suite cases, not a separate registry.
