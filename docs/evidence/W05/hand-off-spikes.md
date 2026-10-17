# Hand-off spikes S1 to S6, 2026-10-16

Part 0 of the [Hand-off work breakdown](../../superpowers/plans/2026-10-16-hand-off-work-breakdown.md). Machine: Apple M1, 8 GB, macOS 26 (Darwin 25.6). Codex CLI 0.151.0. Everything ran from a Claude Code session, not from FNDR, so the parent app for macOS privacy prompts was Claude, not FNDR.

Inputs were synthetic: decoy files with fake text (`~/fndr-handoff-canary/canary.txt`, `/private/tmp/fndr-canary-tmp.txt`) and a job folder with no real data. No sandbox probe read a real file's content into a model; reads of sensitive paths went to `/dev/null` and only exit codes were kept.

## Summary

| Spike | Result | Changes the plan |
| --- | --- | --- |
| S1 sandbox through app-server | Holds for the home folder, Codex sign-in, FNDR's vault and MCP token, symlink escapes, clipboard, AppleScript, app launch, network. **`/private/tmp` stays readable and writable** in every profile tried | Yes: FR5, FR6, a known gap, the self-test |
| S2 ephemeral thread leaves nothing | **No.** No rollout file, but the prompt and commands are written to `~/.codex/logs_2.sqlite`. `sqlite_home` moves Codex's stores to a folder FNDR chooses | Yes: per-job Codex state folder, deleted after the job |
| S3 skills from the job folder | Discovered (scope `repo`) without git. The person's own 38 Codex skills are also offered unless each is disabled; disabling each works | Yes: disable the person's skills per job |
| S4 Documents folder | Writes under `~/Documents/FNDR Hand-offs/` worked with no blocking prompt with Claude as the parent app. Not yet checked with FNDR as the parent | Moves to Part 13 |
| S5 toolchain | 199 MB installed (Python 3.12.12 managed by uv, 51 MB; packages 148 MB), 45 s, plus a 162 MB download cache that can be deleted | Yes: size estimate |
| S6 full job | Ran at 15:51 with the 5-hour window at 0 percent. A one-page readout with a table and a chart in 148 s; 4 of 4 claims with checked sources; 7 of 7 gold facts; nothing left in `~/.codex` | Yes: how a turn ends, which message is the answer, how reads show up, the matplotlib cache, image egress, table styling |

## S1: the sandbox

### Configuration that reaches the worker

`codex app-server` started with `-c` overrides; `config/read` read back afterwards.

| Override | Read back as asked |
| --- | --- |
| `default_permissions="fndr_hand_off"` and `permissions.fndr_hand_off.filesystem={":minimal"="read", "<toolchain>"="read", "<job>"="write"}`, `network={enabled=false}` | Yes |
| `web_search="cached"` | Yes |
| `--disable` for `computer_use`, `browser_use`, `browser_use_external`, `in_app_browser`, `apps`, `plugins`, `memories`, `hooks`, `image_generation`, `multi_agent` | Yes, all `false` |
| `mcp_servers={}` | **No.** Merged, not replaced: all five servers in the person's `~/.codex/config.toml` stayed enabled (`computer-use`, `evidence-engine`, `node_repl`, `notion`, `openaiDeveloperDocs`) |
| `mcp_servers.<name>.enabled=false`, one per server | Yes, all five `false` |
| `features.plugins=false` alone | `config/read` still lists all 20 configured plugins (including `browser`, `computer-use`, `unified-computer-use`) as `enabled: true`. In S3, with the feature off, none of their skills appeared in `skills/list`, so the switch probably does detach them; `config/read` cannot prove it either way |
| `plugins.<name>.enabled=false`, one per plugin | Yes, all 20 `false` |

Codex's `-c` key parser keeps TOML quotes as part of the name: `mcp_servers."computer-use".enabled=false` created a new server literally named `"computer-use"` and Codex refused to start ("invalid transport"); `plugins."x@y".enabled=false` added 20 new entries instead of changing the existing ones. Bare keys work, including names with `-` and `@`. A name containing a dot cannot be expressed this way.

`thread/start` reported `sandbox: workspaceWrite, networkAccess: false, excludeSlashTmp: true, excludeTmpdirEnvVar: true` for the profile. The `/tmp` part of that report was not what the probes found (below), so a reported policy is not proof.

### Probes, with no model (`command/exec`)

The app-server's `command/exec` runs a command under the configured profile without a model turn and without using the plan. 17 probes per run, about 1.4 s in total.

| Probe | Expected | Job folder in `/private/tmp` | Job folder in `~/Documents`, `/private/tmp` denied |
| --- | --- | --- | --- |
| read a file in the home folder | deny | deny | deny |
| list the home folder | deny | deny | deny |
| write in the home folder | deny | deny | deny |
| read a file in `/private/tmp` | deny | **allow** | **allow** |
| write in `/private/tmp` | deny | **allow** | **allow** |
| write in `$TMPDIR` (`/var/folders/...`) | deny | deny | deny |
| read `~/.codex/auth.json` | deny | deny | deny |
| read `~/.fndr/mcp_token` | deny | deny | deny |
| list FNDR's app data | deny | deny | deny |
| read through a symlink inside the job folder pointing out | deny | deny | deny |
| read the clipboard (`pbpaste`) | deny | deny | deny |
| AppleScript to Finder | deny | deny | deny |
| open an app (`open -a Calculator`) | deny | deny | deny |
| network (`curl`) | deny | deny | deny |
| write in the job folder's `out/` | allow | allow | allow |
| run the toolchain's Python | allow | allow | allow |
| read `/etc/hosts` | allow | allow | allow |

`/private/tmp`: a `"/private/tmp"="deny"`, `"/tmp"="deny"` or `":tmpdir"="deny"` entry did not close it while `:minimal` was granted. With an explicit list of system folders in place of `:minimal`, `/private/tmp` was still readable. It appears to come from Codex's base Seatbelt policy, which a profile cannot override. FNDR's own runtime writes temporary files to `$TMPDIR`, which is denied; only tests name `/tmp`. Other programs' files under `/private/tmp` (Claude Code's scratch folders among them) are readable by a worker.

### One model turn (the first of the two approved calls)

Account default model from `model/list` (`gpt-5.6-sol`; the person's `config.toml` names `gpt-6.1-sol`, which the ChatGPT plan refuses, so the first attempt failed with a 400 before any work). The prompt asked for eight commands and one web search, with the developer instruction "work only inside the current folder".

- The model ran the five commands inside the folder or in `/tmp`, and **declined on its own the three that touched the home folder**. That is a useful second layer, and the reason the sandbox must be tested with `command/exec`, not through a model.
- It confirmed: `/tmp` read and write allowed, `curl` failed to resolve, the job-folder write and Python ran.
- It produced an empty structured answer early, then stalled mid-answer for 180 s with no `turn/completed`; the run was cut by the idle limit. The plan's 5-hour window read 100 percent from the first rate-limit event.
- No web search ran.
- Two things in the item stream the job runner must handle: the first `agentMessage` (the empty answer) had `phase: "commentary"`, so the manifest is read only from a final-phase message; and commands ran as `/bin/zsh -lc`, a login shell that tries to read the person's dotfiles (`allow_login_shell` is on by default). The worker profile now sets `allow_login_shell=false`, and Codex reads it back as false.
- 114,798 tokens in total for a 34-second turn (113,805 input, 97,664 of them cached; 993 output): every step resends the context.
- Peak memory of the app-server process group: 138 MB. First event 1.9 s after `turn/start`.

## S2: what an ephemeral thread leaves

- No rollout file under `~/.codex/sessions` mentions the thread id.
- `~/.codex/logs_2.sqlite` (108 MB) holds the thread id 80 times, the prompt text, the decoy file name and the commands. A hand-off prompt holds memory text, so this would keep it outside FNDR's control.
- `command/exec` writes nothing to that log.
- With `-c sqlite_home="<folder>"` and `-c log_dir="<folder>"`, Codex created its stores (`logs_2`, `state_5`, `memories_1`, `goals_1`, `queue_1`) in that folder. S6 confirmed it on a full turn: the job's marker string is in the job's own Codex state folder and in none of `~/.codex`'s databases, logs, today's sessions or history.

## S3: skills

With the job folder holding `.agents/skills/readout-docx/SKILL.md` and no git repository, `skills/list` returned it with scope `repo`. It also returned 38 of the person's own skills under `~/.codex/skills` (deploys, Notion, Sentry and others) plus 6 system skills, all enabled, whose names and descriptions would reach the worker's prompt. One `skills.config=[{path=..., enabled=false}, ...]` override listing those 39 paths left only `readout-docx` enabled.

## S5: toolchain

`uv venv --python 3.12 --python-preference only-managed` with `UV_PYTHON_INSTALL_DIR` inside the toolchain folder, then `pandas`, `matplotlib`, `python-docx`, `openpyxl`, `python-pptx`, `pypdf`:

| Measure | Value |
| --- | --- |
| Python 3.12.12 (managed by uv) | 51 MB, 14 s |
| Packages (20 in the freeze; pandas 3.0.6, matplotlib 3.11.2, openpyxl 3.1.5) | 148 MB, 31 s |
| Download cache | 162 MB, deletable after install |
| All imports | ok |

No Homebrew is needed. The worker's profile must make the toolchain folder readable.

## S6: one full job (the second approved model call)

Ran by itself at 15:51 MDT once `account/rateLimits/read` showed the 5-hour window at 0 percent (weekly 78 percent); the driver would not have started above 100 percent and would have interrupted the turn if a window reached it. Inputs were synthetic: a 400-row survey CSV with known answers, and two made-up memories (a mail from Dana asking for the readout, notes guessing that pricing is the top driver). The job folder was `~/Documents/FNDR Hand-offs/2026-10-16 Q4 onboarding readout/`, with the worker profile from S1 to S3 plus `allow_login_shell=false`, Codex's state in a per-job folder, and the `readout-docx` skill in `.agents/skills/`.

| Measure | Value |
| --- | --- |
| First event after `turn/start` | 1.6 s |
| Final answer | 148 s |
| Tokens | 220,840 (214,526 input, 171,008 of them cached; 6,314 output) |
| Plan usage | 5-hour window 0 to 2 percent |
| Peak memory, app-server process group | 157 MB |
| Bytes FNDR wrote to the app-server | 3,983 |
| Command output the model read back | 10,690 bytes, plus one image of its own chart |
| Items | 8 commands, 3 file changes, 1 image view, 13 reasoning, 3 agent messages |
| Deliverables | `out/q4-onboarding-survey-readout.docx` (69 KB), `out/q4-onboarding-survey-readout-chart.png` (35 KB); both pass the verifier's type checks |
| Claims | 4, each with a source the verifier accepts (the input file with a locator, or an attached memory id) |
| Gold facts | 7 of 7: 400 respondents, 78 churned, setup 24, Salesforce 21, pricing 18 in that order, SMB 28.2 percent, and the notes' pricing guess called wrong. The checker first scored "setup ranked above pricing" false because the summary says "not pricing" before it names setup; the table is right. Part 12 checks rank from the table, not word order |
| `not_done` | empty; one assumption: the optional benchmark was skipped because the data answered the question |

What the worker did, in order: read the skill, profiled the CSV with pandas, computed the drivers and segments, read the renderer, wrote `work/readout-spec.json`, rendered the docx, previewed it with Quick Look, viewed its own chart, adjusted the spec twice, re-rendered, and re-derived every number with `assert` statements before answering. It wrote nothing outside the job folder.

What the job runner must handle (Part 5):

- **The end of a turn.** `turn/completed` never arrived (in S1 either). The turn ended with an `agentMessage` whose `phase` is `final_answer`, followed by `thread/status/changed` to `idle`. The driver waited 180 more seconds on its idle limit; the job was done at 148 s.
- **Commentary looks like an answer.** Two earlier agent messages were JSON-shaped because of `outputSchema`, with `phase: commentary`. The manifest is the `final_answer` message only.
- **Reads are not labelled.** The CSV was read by Python, so `commandActions` said `unknown`; only `sed` on the renderer came back as `read`. A trace line such as "Read the survey" comes from the command naming `inputs/<file>`.
- **Images leave too.** `imageView` sent the worker's own chart to the model. Egress counts the bytes of viewed images with command output.
- **A cache outside the folder.** matplotlib tried its cache under the home folder (denied) and the worker worked around it with `MPLCONFIGDIR=work/matplotlib`. The worker's environment sets `MPLCONFIGDIR` and similar caches inside `work/`.
- **Login shell off.** Commands ran as `/bin/zsh -c`, confirming the S1 fix.

What the document needs: Quick Look shows the table without borders or a bold header and with cramped columns; the renderer sets explicit borders, header weight and widths (Part 7).

## Not done

- S4 with FNDR as the parent app: Part 13.
- Web search with shell network off: not exercised; the task made it optional and the worker skipped it.
- Whether `turn/interrupt` stops a running command: not exercised; the guard never had to interrupt.

## Side note

`scripts/dev/publish-merged.sh` fetched only GitLab, so a commit pushed to GitHub alone (Kunj's, 2026-10-15) made GitHub reject the publish after GitLab had taken it, leaving the remotes split. It did exit non-zero; an earlier note here said it exited 0, which was the exit code of a pipe around it. The commit was published afterwards by hand with the same checks, and the script now merges against GitHub's head when GitHub is ahead and stops before any check when the two have diverged (`scripts/dev/test_publish_merged.py`).
