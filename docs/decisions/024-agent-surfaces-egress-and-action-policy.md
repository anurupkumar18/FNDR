# ADR 024: One egress rule and one stop for the agent surfaces

**Accepted, 2026-10-07, by the owner**, with every recommendation in the
table at the end taken as proposed. The lane owner (Kunj) agreed with the
findings the same day. It closes the questions ADR-018's 2026-10-06 amendment
and ADR-022's amendment left open.

- Requirements: [agent surfaces PRD](../superpowers/specs/2026-10-07-agent-surfaces-prd.md)
- Evidence and work parts: [work breakdown](../superpowers/plans/2026-10-07-agent-surfaces-work-breakdown.md). Finding numbers N1 to N14 and part ids such as C6.3 refer to that file.

Revised 2026-10-07 after a second pass through the code and a first round of
checks. Items 7 to 13 and the release gate are new. Findings were read from `main` at `81b6cbd`; the app was
not run. "(to confirm)" marks a finding that needs a run before it is treated
as fact.

## Context

Hermes Agent, Screen Guide's ChatGPT answers, and Notch Do each reach a cloud
model, and Notch Do and Hermes can act. Each was built with its own answer to
consent, logging and action control.

### What leaves the Mac, and what records it

| | Hermes Agent | Screen Guide (ChatGPT) | Notch Do |
| --- | --- | --- | --- |
| Payload | Message, attached memories, five auto-retrieved snippets on every message, anything the gateway pulls through FNDR's MCP server | Question, OCR text, recent conversation, screenshot if consented | Transcript, accessibility text of the operated app, five snippets when the request refers to the past |
| Consent | Choosing a provider | Model choice plus screenshot switch | Operate switch |
| Sent before the person can review | No | No | Yes: the transcript goes to plan as soon as the utterance ends |
| Logged in Privacy Activity | One row per message; the gateway's own calls are not counted | No | Yes, per request |
| Person can see what was sent | Attached memories only; auto-retrieved ones are not shown or saved (N11) | Yes | Count of memories only |
| Pixels on disk | Not applicable | Screenshot staged as a temp file before sending (N8) | Not applicable |
| Covered by an accepted ADR | Amendment by lane owner | No | Amendment by lane owner |

The activity log is in memory, holds 200 rows, and is empty after a restart.
ADR-018 itself is still Proposed.

### What controls an action

The Hermes column below records the 2026-10-07 pre-fix audit. The later config restrictions and server-enforced four-tool read grant are recorded in ADR 025; the row claiming access to every MCP tool is no longer current.

| | Hermes Agent | Notch Do |
| --- | --- | --- |
| Where the limit lives | Sentences in `SOUL.md`, `.hermes.md` and the request instructions. FNDR writes no tool restrictions into Hermes's config (default tool list to confirm) | `operator/policy.rs`, per call, from the tool, its arguments and the observed element |
| FNDR tools it can reach | Every MCP tool, including ones that write, through FNDR's own bearer token | None |
| Kill switch | Not read | Not read |
| Private Mode | Not checked on send | Checked once, when planning starts (N3) |
| App blocklist | Not applicable | Never checked (N3) |
| Acts before review | Unknown until the tool list is inventoried | The planning turn has the tools attached and run-tier calls are approved in it (N1, to confirm) |
| Approval | None in FNDR | Tap, or a spoken "yes", "ok" or "sure" with the microphone open (N2) |
| Runs without asking | Unknown | Any http or https link (N5). In browsers and media apps: a click on any label outside two short English word lists, and every key except Return and a few delete chords (N4) |
| Stops on quit | No; the gateway is not in the exit handler (N6; orphan to confirm) | No (N6) |

Notch Do's design is sound in its core: Codex runs read-only, every
computer-use call arrives as an approval request, the model's words are not an
input to the level, and unknown tools are refused. The gaps are at the edges
of that design, not in it.

### Shared state outside FNDR

FNDR signs in through the person's real `~/.codex` and forces file credential
storage there. "Sign out" on the account card is Codex's own logout, so it
likely signs the person out of Codex for the whole Mac (N7, to confirm).

## Decision (proposed)

### Egress

1. **Egress rule.** A cloud model request carries only the person's words,
   memories the person attached, and context named in that feature's consent
   copy. Automatic memory retrieval to a provider that is not local is a
   separate switch, default off. "Local" means a loopback address.
2. **Consent is per feature and names the payload.** Agent setup shows the
   payload and host before saving a cloud provider. Screen Guide's ChatGPT
   path is brought under ADR-018's amendment explicitly, including the recent
   conversation it sends.
3. **Every cloud model request is logged, durably.** Feature, host, bytes,
   and flags for memories, screen text and screenshot. Stored locally with a
   bounded retention. No content. A Hermes row states that the gateway may
   make further calls unless those can be counted.
4. **The person can see what was sent.** Memories FNDR adds on its own are
   shown with the message and kept in chat history, the same as attached ones.
5. **Pixels on disk are an exception that is written down.** Staging the
   Screen Guide screenshot for Codex is allowed only in a private directory
   (`0700`, file `0600`) that is swept at startup. ADR-004 and Screen Guide
   FR7 are amended to say so.

### Action control

6. **The kill switch covers every acting surface.** `operator::policy` keeps
   its own rule table, because UI control needs rules the registry tools do
   not, but a run starts and continues only through a check of
   `actions_kill_switch`. A Hermes send is refused too while Hermes has any
   tool that can act.
7. **Nothing acts or reads the screen before the plan is shown.** A planning
   turn is answered with a refusal for every tool request.
8. **Approval is a tap or a key, never speech.** Voice may stop a run and
   decline an action. This restores the command-surface contract's rule for
   Notch Do.
9. **Privacy gates hold for the whole run.** Private Mode stops a run in
   progress. A blocklisted app is never read, clicked or typed into, and a
   plan step naming one is refused. FNDR's own windows and settings are in
   the never tier.
10. **Browsers confirm by default.** A short run list (follow a link, switch
    tab, scroll, play, pause, next, search-field typing and Return) runs
    without asking. Every other click and key waits for a tap. An empty,
    icon-only or non-Latin label counts as unknown. A link whose host the
    person did not say, or that carries a query the person did not say,
    waits for a tap.
11. **Auto-start is limited to plans that need no confirmation.** A Notch Do
    plan containing a confirm-tier step waits for a tap.
12. **A third-party agent runtime is bounded in code, not in prose.** Hermes
    gets an explicit tool allowlist and an MCP token limited to read tools.
    FNDR starts only the pinned Hermes; a different one on the system path is
    not used.
    Instructions in `SOUL.md` remain as a second layer.

### Lifecycle and shared state

13. **FNDR's child processes end with FNDR.** The exit handler stops the
    Hermes gateway and any Notch Do run; a stale gateway found at start is
    killed. FNDR keeps using the shared Codex home, and the Sign out control
    says it signs Codex out on this Mac.

### Release gate for acting features

An acting feature leaves Labs only when all of these hold, each with evidence:

- Stop works in every phase, with no action after it.
- The kill switch, Private Mode and the blocklist are honored.
- The injection fixtures cause no unasked action.
- Every cloud request it makes is in the persisted log.
- No process survives quit.

## Options considered

**A. Leave as is and document it.** Cheapest. Keeps behavior the product copy
and the command-surface contract contradict: memories sent that the person did
not choose, spoken approval, and no blocklist check on an acting surface.
Rejected.

**B. This ADR.** Changes at existing boundaries; no new layer. Two visible
regressions: Hermes on a cloud provider answers questions about the past less
well until the person attaches memories or turns the switch on, and Notch Do
asks more often in browsers.

**C. Route Hermes, Screen Guide and Notch Do through one gateway module that
owns consent, logging and policy.** Cleanest end state, but a refactor across
about 10,000 lines of working code in one step, against the anti-bloat rule.
Revisit if a fourth cloud caller appears.

**D. Merge `operator/policy.rs` into `agent/risk_policy.rs`.** One table would
be simpler to audit, but the inputs differ (a registry tool and caller versus
a tool call plus an observed UI element). Sharing the entry point gets the
safety property without the merge.

**E. Grow the word lists instead of flipping browsers to confirm by default.**
Keeps Notch Do fast. Rejected as the main fix because a deny list cannot keep
up with the web and fails for every other language; a longer list is still
useful inside item 10's never tier.

**F. Give FNDR its own Codex home.** Removes the shared sign-out, but the
person signs in twice and Hermes's import path was read against the shared
home. Not chosen for now; revisit if item 13's copy confuses people.

## Consequences

- Privacy Activity becomes a complete record for these features, which is
  what the consent copy already promises.
- Agent answers on cloud providers depend on explicit attachment by default.
- Notch Do becomes slower in browsers and needs a hand on the trackpad to
  approve. The task set in the breakdown (F1) measures the cost.
- The lane keeps two policy tables; a new action surface must state which one
  it uses and must pass through the kill switch.
- `docs/agent.md`, `screen-guide.md`, `command-surface.md`, ADR-004 and
  ADR-020's amendment need the edits listed in the PRD and breakdown.
- ADR-018 still needs its own accept or reject. This ADR assumes option B
  stays scoped to these three features until then.
- Items 7 to 10 and 13 are safety fixes. They should not wait on the product
  questions below.

## Confirmation status

Phase 1 checked the open findings on 2026-10-07. Tables and line references:
[phase 1 evidence](../evidence/W03/agent-surfaces-phase1.md).

| Finding | Status | Left to do |
| --- | --- | --- |
| N1 planning turn can act | FNDR approves run-tier calls in a planning turn; confirmed by code | One live run to see whether the model asks |
| N2 spoken approval | Confirmed by an executed probe | None |
| N4, N5 browser clicks, keys, links | Confirmed by an executed probe | None |
| N6 gateway survives quit | Confirmed by code | `ps` after a real quit |
| N6 Notch Do run survives quit | Open | Quit during a run |
| N7 Sign out is Mac-wide | Confirmed by configuration | Not executed on purpose |
| N14 typing follows a cached focus | Confirmed by an executed probe; a fresh screen read does not correct it | Live run with the Tab fixture |
| Hermes default tools | Terminal, file write, code execution, browser control and cron are on in the Hermes this Mac would run | Same check at the pinned commit; part A9.2 |
| Stale element index | Half confirmed | Live run with the reordering fixture |

Phase 1 also showed three things that change the items above:

- FNDR runs whatever Hermes is on the system path when its pinned copy is
  missing, so item 12 must also require the pinned runtime or refuse to start.
- A spoken Stop is recognized only when the sentence begins with the stop
  word. Item 8 keeps voice as a way to stop, so that matching must be widened.
- The forced file credential store changes nothing on a default Codex setup;
  it is no longer a concern for item 13.

Walk N1 to N14 with the lane owner (D7.2); some may be known or have context
the code does not show.

## Decisions taken

Ids match the breakdown's Track E. "Built" means the code is on `main` with
tests; see the breakdown's progress tables.

| Id | Decision | State |
| --- | --- | --- |
| E1 | ADR-018's amendment stands and covers Screen Guide's ChatGPT answers; ADR-018 is accepted for these three features only | Recorded in ADR-018 |
| E2 | Related memories go to a provider that is not on this Mac only when the person turns it on; Hermes gets memory search only then too | Built |
| E3 | A plan starts by itself only when no step can need a yes | Built |
| E4 | Hermes is an answering surface for Beta: a planning list and read-only memory search, no terminal, files, code, browser or schedules | Built against Hermes 0.13's config; to recheck at the pinned commit |
| E5 | Approval is a tap or a key; speech can stop and decline | Built |
| E6 | A transcript should not reach the cloud before the person sees it | Built: what was heard shows for 1.2 s before it is sent |
| E7 | Browsers confirm by default with a short run list; a link the person's words do not account for waits for a tap | Built |
| E8 | FNDR keeps sharing the Codex sign-in and says so beside Sign out | Built |
| E9 | No updating Hermes past the pin until a version is reviewed | Built: the control and its commands are removed |
| E10 | OpenClicky bridge moves to Labs or goes | Built: tagged Labs |
| E11 | Notch Do writes to the SK-01 journal | Waits for SK-01 |
| E12 | Agent and Notch Do sit in Labs for Beta until the release gate passes | Built: a Labs group in the sidebar; the full regroup is PX-01 |
| E13 | Which account QA spends | Owner's account, when the live checks run |
| E14 | Provider keys stay in a file readable by the owner only; Keychain later | Built |
| E15 | Chat history is the person's own writing, so it never expires by age; it is bounded by size: the newest 200 chats, 400 messages in each, oldest dropped (2026-10-08, was waiting on PD-09) | Built |
| E16 | Which helper Notch Do acts through | Decided in [ADR 026](026-notch-do-acts-with-fndrs-own-hands.md) |

One departure from item 10 as written: "next" is not on the browser run
list, because a Next button on a web page often submits a step of a form. A
link labelled "Next page" still runs, as a link.
