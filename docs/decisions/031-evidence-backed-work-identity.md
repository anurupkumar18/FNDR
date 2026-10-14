# ADR 031: Evidence-backed work identity across apps

## Status

Accepted 2026-10-13. The owner delegated product and architecture decisions for the quality loop.

## Context

Resume, Home, work sets and meeting prep rely on a memory's `project` or its app-specific `session_key`. On the owner's vault, the October 13 aggregate read found 162 memories, 21 with a project, and 66 distinct session keys; the largest key holds 74 memories. Of the 57 records with an exact file or browser reopen target, no target occurs in two apps. Reusing an exact target alone cannot connect the work across apps. These counts contain no captured content.

The synthetic personas already use `project` for a course assignment, hiring work and a launch, as well as software projects. Vault's `sessionLinks` and work-set resolution already link sessions in a short time window when they share a file or distinctive title words. Those are useful relationships, but the current product deliberately shows them as links, not one thread.

## Decision

1. `MemoryRecord.project` is the existing name for a **named piece of work**, including a document task, course, recurring meeting or software project. Keep the field and its current storage contract. `session_key` continues to describe the capture context, not the work identity.
2. A model-proposed project is stored only when every substantive word of the label appears together in the captured window title or one cleaned-text sentence or line as a whole word. The app name and the model's confidence are not evidence. An unsupported label is cleared even at high confidence. This first slice improves precision; it does not claim to solve coverage.
3. Time proximity and partial title similarity remain links between sessions. They do not assign a project or merge Resume threads. A later capture slice may use an explicit document, course or meeting name as an anchor and record its source in the existing `project_evidence` and `project_confidence` fields. Ambiguous cases stay unlabeled.
4. Before wider assignment, add cross-app positive and unrelated negative cases to the synthetic corpus and measure project precision and coverage separately. Check Resume and work-set results, all three retrieval personas, `make qa-vault`, and a native Quality Lab run. Do not rewrite the owner vault without FNDR closed, a backup and the owner's go-ahead.

## Options considered

| Option | Why it was not chosen |
| --- | --- |
| Keep repository-path identity only | It leaves the measured non-code work gap and the 74-memory fallback thread. |
| Merge nearby windows or similar titles into a project | Two apps being open together does not prove one piece of work; the existing link presentation is the honest weaker claim. |
| Group by exact URL or file target | The current vault has no exact target repeated across apps. |
| Add a new work-ID table now | It duplicates `project` before a source-backed assignment rule or labeled evaluation exists. |

## Consequences

Some unsupported high-confidence project labels will disappear from new captures. Existing records are not rewritten. Coverage must be improved with supported anchors and measured separately from this precision gate. Resume and work sets keep their current behavior until the later source-backed assignment slice is verified.

## Addendum, 2026-10-13: a shared name joins Resume threads when they are read

Decided the same day by the owner's other session, after the owner said to design and build the work-identity change. It replaces one clause of decision 3 and leaves the rest standing.

**What changes.** Decision 3 said title similarity never merges Resume threads. Windows that share a name now are one thread: a phrase of two neighbouring words, or an identifier ("Q4", "a4", "3500"), written the same way in the window titles of two or more windows. Time proximity still merges nothing, and nothing is merged on resemblance.

**What does not change.** Nothing is written to `project` or anywhere else (decisions 1 and 2): the grouping is computed each time threads are read, in `resume/work_names.rs`, so it covers every past memory and can be changed without a migration. An explicit `project` outranks a name: one project among the windows that share a name takes them, and two projects are never joined. No model output is read.

**The rule.** From each title, drop the part that names the app, the site name a page title ends with, file extensions and version numbers. A shared phrase that sits in most windows of one app or site and nowhere else names the place (a Slack workspace, "Pull Request") and is ignored. Names that sit on mostly the same windows are one set, and each window joins the set of names it shares most widely. A set with fewer than two windows names nothing. Threads are named over the last 14 days whatever window a surface shows, so Home, work sets, `what_changed_since` and the seen markers use one key (`work:<name>`).

**Evidence (decision 4's measurement).** The three persona corpora file each entry under a project. With that label hidden, the share of same-project pairs the grouping finds, and the share of the pairs it makes that are right:

| Persona | Found before | Found, titles only | Found, with the text pass | Right, with the text pass |
|---|---:|---:|---:|---:|
| knowledge-worker | 0.06 | 0.33 | 0.42 | 1.00 |
| office-pm | 0.05 | 0.69 | 0.84 | 0.98 |
| software-engineer | 0.14 | 0.37 | 0.46 | 0.94 |

**The text pass (VS-97, same day).** A window whose title names nothing, such as a chat channel or a rubric page, joins a thread when its on-screen text uses two of that thread's names and no name of any other thread. A window with its own project never moves. Screen text is searched for names FNDR already took from titles; it is never a source of new names and nothing in it is followed.

`resume::work_names::tests::naming_finds_far_more_of_a_personas_work_and_is_seldom_wrong` holds floors under these numbers.

**Options weighed.** Chaining any two windows that share a rare word reached 0.88 found on one persona but was right only 0.3 to 0.6 of the time, because one stray word joins two large threads. A cap on how common a name may be dropped the largest real project of the week. Attaching a window when its on-screen text mentions one name of a thread found 0.96 on one persona and fell to 0.43 right on another, because an inbox mentions every project; requiring two names of one thread and none of another kept every precision floor, and that is what was built. Showing links without joining (the Vault's `sessionLinks`) leaves a work set at one place.

**Limits.** 86 synthetic entries, and the rule was shaped on them. The owner's vault is too small to judge it (162 memories). A product or repository name used across several pieces of work can still join them: the wrong pairs on the software-engineer persona are of that kind. The thread card shows the shared words ("Grouped by the words its windows share") so a wrong grouping can be seen for what it rests on. Not yet seen in the running app.
