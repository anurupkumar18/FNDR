# Command surface

Updated 2026-10-15. Notch Do is FNDR's one action layer (decision log, 2026-10-14). The command bar (Alt+Space), the notch's typed field, its voice and Home's work sets all reach the Mac through it. The earlier contract, a router over a typed tool registry with its own risk function (GS-03, GS-06, GS-11), was built only as backend code no screen called; it was removed, and this page describes what runs.

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
       paste                      FNDR pastes the exact text into the focused field, after a tap
  -> plan::verify                 FNDR checks each step landed; one retry
  -> journal                      operator/journal.jsonl, one line per action, typed text hashed
```

- **Command bar.** A query that names a saved skill, or its first three letters or more ("mon", "run my Monday setup"), gets a first row, "Run skill: …", with the skill's steps. A query that starts with an action verb ("open", "look up", "play", "pull up") gets "Do it: …". Choosing either calls `notch_do_request`, which hides the command bar, opens the notch in Do and plans the text there, so the plan card, Stop, approvals and journal are the notch's. Refused while "Operate my Mac" is off. Every other query is a memory search, as before.
- **Local plans** need no Codex process and no request on the ChatGPT plan. An app is planned only when it is installed. A link or a web search may name the browser it opens in ("open github.com in Safari", "look up looped transformers in Chrome"), when that is a browser and installed; FNDR opens it there and checks that browser is in front. A request that names any other place ("open Notes in Safari"), refers to the past, has to operate an app or asks for something Notch Do never does goes to the planner.
- **The app in front** is the one the person was in when the notch or the command bar took the keyboard, since FNDR itself is in front while they type (`native::remember_app_in_front`). Never FNDR, never a blocklisted app or a password manager.
- **The planner** is given the request, the name of the app in front, and up to five memory snippets only when the request points at the past. Its answer is parsed and validated (`plan::parse_plan`), and `plan::settled` drops steps that change nothing. Prompts live in `inference/prompts.rs`.

## Paste

A `paste` step puts its `text` (at most 500 characters) into the focused field of its app, or of the app in front when it names none. The planner may paste only words the request gives, or a link, address or name from the memory snippets, never a message it composed. A paste with no text puts in what an earlier step copied ("copy the text of this article into Notes"); a copy a password manager marked secret is refused. The approval card shows the exact text (copied text: its start and its length), and nothing is pasted without the tap. FNDR then pastes it the way the operator types (the app brought to the front and checked, never into a secure field), reads the field back and calls the step checked only when the field shows the text; a field that shows FNDR no text is reported as pasted, not as checked. A paste is never retried, since a second try would put the text in twice. Terminal, script tools and password managers refuse it. Text the person had copied is put back on the clipboard once the field has been read (an image or a secret copy is not); FNDR's own pastes are not added to its clipboard history.

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

**Settings > AI > Notch Do** lists the skills, each with its steps, Run (the same hand-off as the command bar) and Delete, which asks once more. It also lists the newest 20 runs, with how each was planned and how it ended, so a run can be rated, or saved as a skill, after the notch has closed. A run that reopened memories is not offered as a skill: a work set opens again by its name. Whether a run can be repeated is decided once, by `RunRecord::repeatable`, and read from each step's action, since a step's resolved memory is never stored.

## Measured

`scripts/operator/notch-do-cases.json` labels requests with their route, memory snippets, left-out note and plan; `scripts/operator/notch-do-policy-cases.json` labels computer-use calls with their risk. `cargo test --lib operator::eval` scores both, local plans included, and rescores every recorded planner run under `scripts/operator/recordings/` against its floor. `notch_do_live_planner_eval` (ignored, opt-in) records a new run from the real planner with no computer-use tools. A case may carry synthetic `snippets`, made up and never copied from a vault, which the live run sends as the memory snippets FNDR would find; a recording is rescored only on the cases that still go to the planner.

## Not built

The October tool list in [actions policy](actions-policy.md) beyond opening apps, links, web searches, operating an app, reopening memories and pasting: reminders, Shortcuts, timers and revealing a file. Each would be a new step type in `operator::plan`, with a policy case and suite cases, not a separate registry.
