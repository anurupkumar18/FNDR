# PD-05: Friday scoreboard (cloud)

Date: 2026-10-04. Branch `claude/train-e-docs`.

## What was produced

- `scripts/audit/scoreboard.py` (standard library only) prints one Markdown page: a title with the date, a Retrieval table (persona, path, Recall@5, paraphrase Recall@5, MRR@10, p95 ms, top-1 agreement), the month plan Beta targets for those rows with a met or not met status, a Vault health table with its Beta targets, linked voice and user-session evidence when given, and a "Not measured yet" list for everything that has no input.
- `scripts/audit/test_scoreboard.py`: 24 unittest cases, written first and run failing before the script existed.
- `Makefile`, PD-05 block at the end: `make scoreboard`.

## How it reads its inputs

- Retrieval: any number of `retrieval_qa` JSON reports (`--retrieval FILE`, repeatable), schema v1 or v2. The persona is the report's `case_set`, not the file name. Paraphrase Recall@5 is `recall_at_5_by_kind.paraphrase`. The paraphrase target is judged on the Search path, like the Search Recall@5 target; the table shows Ask's paraphrase recall too. "Same top result" is met only when `top1_agreement` is every case.
- Vault health: the Markdown that `vault_health.py` writes (`--vault-health FILE`). It reads the current parent row of the table health table (rows, active days, rows per active day, median clean chars, exact reopen), the chunk row, and the structured fields table for the current parent. Any line it cannot find prints "not measured". A test renders a report with `vault_health.render` and parses it back, so a format change in the producer breaks a test (that test needs numpy and is skipped without it).
- Voice and user sessions: `--voice FILE` and `--sessions FILE` are linked by file name and never parsed.
- Voice command success and the reopen outcome share (RE-13 runtime counters) have no input yet, so they are always listed as not measured.
- A missing, unreadable, or unsupported file prints "not measured" with the file name and the script still exits 0. The page names files only, never directories, and replaces any en or em dash from the inputs with a hyphen.

`make scoreboard` defaults to `scripts/demo/retrieval-reference/*.json` (the accepted references from VS-04 on train A) and `docs/evidence/W02/vault-health-owner.md`. Override with `SCOREBOARD_RETRIEVAL` (space-separated), `SCOREBOARD_VAULT_HEALTH`, `SCOREBOARD_VOICE`, `SCOREBOARD_SESSIONS`, and `SCOREBOARD_DATE`.

## Tests

```
$ python3 -m unittest scripts/audit/test_scoreboard.py
....................s...
----------------------------------------------------------------------
Ran 24 tests in 0.019s

OK (skipped=1)
```

The skipped case is the `vault_health.render` round trip (no numpy in the cloud container). With numpy installed in a scratch virtual environment the same command printed `Ran 24 tests` and `OK`.

## Output on the W02 evidence

```
$ python3 scripts/audit/scoreboard.py --retrieval docs/evidence/W02/retrieval-baseline-seeded.json --vault-health docs/evidence/W02/vault-health-owner.md --date 2026-10-04
# FNDR Friday scoreboard: 2026-10-04

Numbers come from the files named below; nothing is typed by hand. Regenerate with `make scoreboard`.

## Retrieval

- knowledge-worker from retrieval-baseline-seeded.json (schema v1, 22 cases)

| Persona | Path | Recall@5 | Paraphrase Recall@5 | MRR@10 | p95 ms | Top-1 agreement |
|---|---|---:|---:|---:|---:|---:|
| knowledge-worker | search | 0.955 | 0.875 | 0.909 | 764 | 17/22 (0.773) |
| knowledge-worker | ask | 1.000 | 1.000 | 1.000 | 1418 | 17/22 (0.773) |

Beta targets (month plan section 3):

| Persona | Measure | Beta target | Measured | Status |
|---|---|---|---:|---|
| knowledge-worker | Search Recall@5 | 0.90 or higher | 0.955 | met |
| knowledge-worker | Paraphrase Recall@5 (Search) | 0.80 or higher | 0.875 | met |
| knowledge-worker | Search and Ask same top result | always | 17/22 | not met |

## Vault health

From vault-health-owner.md, current parent table.

| Measure | Value | Beta target | Status |
|---|---|---|---|
| Memories per active day | 5.8 (29 memories, 5 active days) | none | n/a |
| Median clean text per memory | 129 characters | 800 or more | not met |
| Memories with a project | 0.0% | 60% or more | not met |
| Memories with next steps | 0.0% | 60% or more | not met |
| Other structured fields | topic 100.0%, outcome 0.0%, decisions 0.0%, errors 0.0% | none | n/a |
| Chunk rows | 0 | every memory | not met |
| Memories that reopen exactly | 10.3% | 90% of a live day | not met |

The Beta target counts memories with both a project and next steps; each field's share is an upper bound for it. Chunk rows count chunks, not memories, so a non-zero count does not prove every memory is covered. Exact reopen is the stored share across the whole vault, not a live day.

## Not measured yet

- Voice latency (first partial text 0.5 s, final text 1 s after release): not measured yet.
- Voice command success (45 of 50 on the utterance script): not measured yet.
- Reopen outcome share (RE-13 runtime counters): not measured yet.
- User sessions (PD-18 interviews and outside users): not measured yet.
```

## Output with the train A references

The two schema v2 references from `claude/train-a-measure` at `aed1315`, copied out of that branch, through the Make target (the Retrieval section; the rest matches the run above):

```
$ make scoreboard SCOREBOARD_RETRIEVAL="<copy>/knowledge-worker.json <copy>/office-pm.json" SCOREBOARD_DATE=2026-10-04
## Retrieval

- knowledge-worker from knowledge-worker.json (schema v2, 39 cases)
- office-pm from office-pm.json (schema v2, 37 cases)

| Persona | Path | Recall@5 | Paraphrase Recall@5 | MRR@10 | p95 ms | Top-1 agreement |
|---|---|---:|---:|---:|---:|---:|
| knowledge-worker | search | 0.955 | 0.875 | 0.909 | 1512 | 28/39 (0.718) |
| knowledge-worker | ask | 1.000 | 1.000 | 1.000 | 2920 | 28/39 (0.718) |
| office-pm | search | 0.700 | 0.333 | 0.589 | 913 | 12/37 (0.324) |
| office-pm | ask | 0.900 | 0.778 | 0.657 | 3197 | 12/37 (0.324) |

Beta targets (month plan section 3):

| Persona | Measure | Beta target | Measured | Status |
|---|---|---|---:|---|
| knowledge-worker | Search Recall@5 | 0.90 or higher | 0.955 | met |
| knowledge-worker | Paraphrase Recall@5 (Search) | 0.80 or higher | 0.875 | met |
| knowledge-worker | Search and Ask same top result | always | 28/39 | not met |
| office-pm | Search Recall@5 | 0.90 or higher | 0.700 | not met |
| office-pm | Paraphrase Recall@5 (Search) | 0.80 or higher | 0.333 | not met |
| office-pm | Search and Ask same top result | always | 12/37 | not met |
```

On this branch, where `scripts/demo/retrieval-reference/` does not exist yet, `make scoreboard` prints "No retrieval report given: not measured." under Retrieval and the same vault health table.

## What a human must do

The ticket is done when two Fridays are posted (Oct 9 and Oct 16). Each Friday, on the Mac:

1. Refresh the inputs: `make vault-health` (owner profile), and `make qa-retrieval-check` plus `make qa-retrieval-check PERSONA=office-pm` once VS-04 has merged.
2. The default reads the accepted references, which change only when a new reference is accepted. To post this week's run instead, pass the fresh reports: `make scoreboard SCOREBOARD_RETRIEVAL="src-tauri/target/qa-retrieval-check/knowledge-worker/current.json src-tauri/target/qa-retrieval-check/office-pm/current.json"`.
3. Add `SCOREBOARD_VOICE=` and `SCOREBOARD_SESSIONS=` with the week's voice and user-session evidence files when they exist.
4. Read the page once (it carries aggregate numbers only), post it in the team chat, and paste the post link on PD-05. After the second Friday, move PD-05 to `status::evidence`.
