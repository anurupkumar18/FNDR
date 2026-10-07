# Beta Q&A prep

Status: draft for PD-04, 2026-10-04. Beta is Wed Oct 21 (assumed; master plan D-4). Every number below comes from a committed evidence file named next to it. If a number is not in evidence, the answer says "not measured".

The dry run needs a person outside the team and a human presenter. This file prepares it; it does not record one.

## 1. What the panel and the course reward

The course rubric is not in the repo (searched for "rubric" on 2026-10-04; only seeded demo text and internal scoring rubrics turned up). What the repo does say:

- "Instructors inspect three things: the live demo with slides, a written evidence packet, and the GitLab board plus commit history." (master plan, Global Constraints)
- The Alpha plan listed "rubric artifacts (wiki, issues, slides, CI)" as owned outside product work (`docs/superpowers/plans/2026-09-18-alpha-demo-hardening.md`, D7).

Assumptions about what a capstone demo panel rewards, to check against the real rubric when the instructors share it:

| Assumed criterion | What earns it | Where we stand |
|---|---|---|
| A real problem and a clear user | One sentence, one person, one pain | `docs/product/positioning.md` (draft) |
| A live demo that works end to end | Real data path, no mocks, inside five minutes | Section 2 below: several beats are not built yet |
| Evidence and measurement | Before and after numbers with method and sample size | Baseline only (`docs/evidence/W02/VS-01-baseline.md`) |
| Technical depth | Why each design choice, what was measured | ADRs in `docs/decisions/`, month plan section 6 |
| Honesty about limits | Saying what does not work and why | Section 3 below |
| Team process | Board, commits by everyone, reviews, decisions written down | GitLab board, `docs/team/decision-log.md` |
| Privacy and safety thinking | What is stored, what leaves, what can go wrong | ADR-004, ADR-017, ADR-018 draft |
| Handling questions | Short, specific, no bluffing | Section 3 below |

## 2. Each demo beat and the evidence that proves it today

Beats from month plan section 9. Rule from section 10: a beat we cannot demo end to end by the freeze (Oct 16) is cut, not simulated.

| Time | Beat | Evidence today | Status | Ticket that produces the missing part |
|---|---|---|---|---|
| 0:00 | Work is scattered; assistants forget between chats | `docs/product/positioning.md` | Draft words; no slide | PD-02 (words), slide not ticketed |
| 0:30 | "That reading about labor contracts" returns the passage | The keyword query "labor contracts passage page 112" ranks first on Search and Ask (`docs/evidence/W02/retrieval-baseline-seeded.md`). The demo's looser wording is not in the labeled set | Proven for the keyword form on the seeded profile | Add the demo wording to the query set (VS-02 or VS-03) |
| 0:30 | One click opens the PDF on page 112 | `docs/product/reopen-qa-matrix.md`, "RE-04 follow-up": the page is stored and shown; Preview opens the file but has no page-jump API; browser PDFs get `#page=N` (unit test only) | Partial | RE-14 (live rerun, including a browser PDF). Preview itself cannot jump to a page |
| 0:30 | A downloaded file is found by a phrase inside it | None | Missing | EM-08, RE-08 |
| 1:30 | Voice command with visible stages and partial text | `docs/evidence/W02/voice-baseline.md`: no streaming; Home 6.0 s median over 3 trials | Missing | VO-03, VO-04, VO-06, VO-11 |
| 1:30 | One tap approves the reminder | Registry and risk-policy tests pass (`docs/evidence/W02/gs-03-tool-registry.md`, `gs-11-risk-policy.md`); no reminder executor or approval card evidence | Partial | GS-05, GS-06, GS-09, GS-13 |
| 2:20 | Claude Code pulls the thread over MCP | `docs/evidence/W03/resume-work-mcp.md`, `resume-work-latency.md` (fixture tests; p95 17.27 ms over 200 synthetic records) | Proven at the MCP boundary; no recorded live session | Record one live run before the freeze |
| 2:20 | The assistant writes a note back that appears in the Vault | None | Missing | VS-35 (spec only; no build ticket) |
| 3:10 | "Save that as my Monday setup," then run it by name | None | Missing | SK-01, SK-02, SK-03 |
| 3:40 | A blocklisted site is absent everywhere | `docs/evidence/W03/privacy-activity-native.md` is a runbook; no recorded run | Missing | Run the runbook; VS-37 |
| 3:40 | Privacy Activity shows what Claude read and any cloud requests | `src-tauri/src/privacy_proof.rs` counts requests and hosts only, in memory; MCP reads and the opt-in cloud paths are not counted (ADR-018 draft) | Missing | VS-37, ADR-018 follow-ups |
| 4:10 | Recall@5 before and after | Synthetic personas, `docs/evidence/W03/beta-demo-recall-cloud.md` and `beta-demo-recall.csv`: office-PM Search 0.700 to 0.900 (14 to 18 of 20), knowledge-worker Search 0.955 to 1.000; Ask unchanged at 0.900 and 1.000; Search and Ask agree on every top result | Proven on synthetic data | Third persona; real-vault spot check (VS-01) |
| 4:10 | Voice latency | Partial baseline only (`voice-baseline.md`) | Half | VO-11 |
| 4:10 | Reopen rate | 10.3% exact reopen on the owner vault (`docs/evidence/W02/vault-health-owner.md`) | Before only | RE-13, RE-14 |
| 4:10 | Time to recover work with and without FNDR | None | Missing | PD-13 |
| 4:45 | What is next | Slide | Not started | None |

## 3. Ten hard questions

Short answers first. Say "not measured" rather than guess.

1. **How is this different from Microsoft Recall?** Recall is Windows-only, needs a Copilot+ PC with 16 GB, and keeps encrypted screenshots. FNDR runs on an 8 GB M1, keeps text and metadata but no screenshots (ADR-004), and gives the context to your AI assistant over MCP. Where Recall is ahead: it is built into the OS with hardware-backed encryption; FNDR's store has no app-level encryption and relies on FileVault. (`docs/product/positioning.md`)
2. **Screenpipe already does this. Why you?** Screenpipe is ahead on platforms, audio, and accessibility-first capture. We keep no screenshots, we are Apache-2.0, and we focus on one job: getting a knowledge worker back into interrupted work, then reopening the exact source. We have not shown we do that better yet; PD-13 tests it with five people.
3. **Why not just use ChatGPT or Claude memory?** They remember your conversations. FNDR remembers what you saw in every app, and any MCP client can use it, including Claude. They are better at reasoning; FNDR is built to feed them, not replace them.
4. **What leaves the Mac?** Capture, OCR, storage, and embeddings never leave. Model downloads and update checks reach the network. Two opt-in Labs paths can send text to a cloud model: Screen Guide with ChatGPT (the question and the screen's text; a screenshot only with a second opt-in) and Hermes Agent with a cloud provider. Whatever your MCP client reads goes to that client's model. Today Privacy Activity does not count those cloud calls; the ADR-018 draft proposes closing that gap before any new cloud use.
5. **How do you know search is good?** On 22 labeled queries over a seeded week, Search finds an accepted memory in the top five 21 times (Recall@5 0.955), Ask 22 times, and paraphrases 0.875 on Search (`VS-01-baseline.md`). That set is small and synthetic, which is why VS-02 adds an office persona and VS-04 makes the report a merge gate. Search and Ask agree on the top result only 17 of 22 times; one retrieval path (VS-09) is the fix, not built yet.
6. **What happens when it does not know?** Ask is built to cite its sources and to answer "not enough evidence" instead of guessing (Alpha decision D4 in `docs/superpowers/plans/2026-09-18-alpha-demo-hardening.md`). The measured refusal case (Memory Journey case 6, `docs/evidence/W04/memory-journey-baseline.md`) is still pending, and Search's "no good match" is VS-12.
7. **Can another app or a website read my memory?** The MCP server needs a bearer token in every mode and refuses web origins unless you allow them (ADR-017, with adversarial tests). Tools with side effects always ask for confirmation over MCP (`gs-11-risk-policy.md`). A client with the token can call the read tools, and those reads are not logged yet (VS-37).
8. **What about passwords and banking?** App, title, and URL checks and your blocklist run before pixels are read (ADR-004). Password managers, a fixed list of banking and patient-portal sites, and sign-in or two-factor pages are skipped, and after OCR, a frame whose text matches a secret pattern such as `api_key` or `password:` is not stored at all (`src-tauri/src/privacy/safety_gate.rs` and its tests). Patterns can miss things. We have not yet recorded the native run that proves a blocklisted site is absent everywhere; that is the 3:40 beat.
9. **Does it slow my Mac down?** Always-on CPU and memory are not measured yet (month plan section 3; target 3% CPU and 700 MB). The only resource numbers in evidence are during voice transcription: 1,905 to 2,048 MB resident on 3 trials (`voice-baseline.md`). VS-32 sets a memory budget.
10. **Who has used it besides your team?** No outside user session is recorded yet. PD-13 runs five outside sessions by Oct 16 and measures time to recover work with and without FNDR.

If there is time for more: "Why a small local model?" (8 GB machine; the structured fields it should fill are 0% on the owner vault today, see `VS-01-baseline.md`; ADR-018 draft proposes an opt-in cloud tier with a measured difference, not yet decided.) "What if you shut down like Rewind?" (Apache-2.0, local files, no account; nothing to switch off.)

## 4. Dry-run checklist

Needs: a presenter, one person outside the team as the audience, a timer, and someone taking notes. No real captures on screen.

Before:

- [ ] Build from the commit you will demo; write its short hash in the notes.
- [ ] Use a seeded profile (`make qa-seed`), never the owner's real profile.
- [ ] Screen Recording and Accessibility granted to that build; MCP token present; Claude Code connected.
- [ ] A synthetic blocklist entry ready for the 3:40 beat.
- [ ] Each beat marked "live", "cut", or "recorded clip" from section 2. Only beats with evidence are live.
- [ ] Slides for 0:00, 4:10, and 4:45, with every number traced to an evidence file.
- [ ] Do Not Disturb on; notifications and other apps closed.

During:

- [ ] Note the clock at each beat.
- [ ] Note every stall, wrong result, or "let me just" moment.
- [ ] After the demo, the outside person asks any three questions from section 3, plus one of their own.

After:

- [ ] Total time under five minutes?
- [ ] Cut or fix each beat that failed; do not script around it.
- [ ] Commit the filled notes as `docs/evidence/W04/PD-04-dry-run-<date>.md` (no names, no captured content) and link it on PD-04.

## 5. Dry-run notes template

```
Date:
Build (short hash):
Profile: seeded (name of seed), not real
Audience: outside the team (role only, no name)
Total time:

| Beat | Planned | Actual start | Worked? | What went wrong |
|---|---|---|---|---|
| 0:00 Problem | 0:00 | | | |
| 0:30 Find and reopen | 0:30 | | | |
| 1:30 Voice | 1:30 | | | |
| 2:20 Resume with an assistant | 2:20 | | | |
| 3:10 Skill | 3:10 | | | |
| 3:40 Trust | 3:40 | | | |
| 4:10 Numbers | 4:10 | | | |
| 4:45 What is next | 4:45 | | | |

Questions asked, and how well we answered (good, weak, wrong):
1.
2.
3.
4.

Cut before Beta:
Fix before Beta (ticket IDs):
```
