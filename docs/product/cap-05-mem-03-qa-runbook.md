# CAP-05 and MEM-03 QA runbook

Use this run only after the owner is ready to start a local dev build. It
creates a metrics-only NDJSON file, not a capture archive. Do not commit the
NDJSON file or any live LanceDB data.

## Before launching

1. Confirm `git status --short --branch` is clean on `main`.
2. Confirm the dev build has Accessibility permission in System Settings,
   Privacy and Security, Accessibility.
3. Check that the machine has several GB free with `df -h /tmp` and avoid a
   cache-clean operation while the app or model build is running.
4. Create a unique temporary directory and retain the printed path until the
   evidence report has been reviewed:

```bash
QA_METRICS_DIR="$(mktemp -d /tmp/fndr-qa.XXXXXX)"
export FNDR_METRICS_DUMP="$QA_METRICS_DIR/cap-05-mem-03.ndjson"
printf '%s\n' "$FNDR_METRICS_DUMP"
```

## Run

From the repository root, build only when instructed, then launch:

```bash
cd src-tauri && cargo build
cd .. && npm run tauri dev
```

Use the app normally for 10 to 15 minutes. Switch among several non-sensitive
apps and browser pages. Do not open secrets, financial sites, medical data, or
password managers during the run. At least two metric lines are required, and
15 minutes gives the latency and outcome counters useful sample depth.

Stop the app normally, then generate the aggregate-only report:

```bash
make capture-baseline-verify METRICS="$FNDR_METRICS_DUMP"
make capture-baseline \
  METRICS="$FNDR_METRICS_DUMP" \
  OUT=docs/evidence/W03/cap-05-context-baseline.md \
  BEFORE_CONTEXT_P95=915 \
  BUILD=debug
```

The comparison baseline of 915 ms comes from
`docs/evidence/W01/tier2-merge-qa.md`. The report records the new
`capture.context_ms` p95, MEM-03 `mem.*` stage rows, and memory outcomes.

## Review and closeout

1. Verify the report has a CAP-05 comparison table, `mem.` rows, and a memory
   outcome table.
2. Confirm it contains aggregates only:

```bash
rg -n -i 'https?://|password|@' docs/evidence/W03/cap-05-context-baseline.md
```

3. Copy the short MEM-03 counters excerpt into
   `docs/evidence/W03/mem-03-post-capture-counters.md`.
4. Review `git diff -- docs/evidence/W03/` before committing either evidence
   file. Do not reset LanceDB as part of this evidence run because that deletes
   actual local memories.
5. After the evidence is committed and backed up, move the exact temporary
   metrics directory printed above to Trash. Do not use a broad cache cleanup
   command for this one file.
