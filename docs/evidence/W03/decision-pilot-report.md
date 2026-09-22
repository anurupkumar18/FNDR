# Decision pilot report

Status: blocked on labeled pilot data.

DEC-02 provides the calibration and coverage-report tooling, but this checkout
does not contain either of the inputs named by the plan: MEM-04 merge replay
labels or the WS2 activity-type gold set. No threshold, automated share, or
precision claim is reported here because producing one from invented labels
would misrepresent model evidence.

## Required next input

Provide a JSONL fit split and held-out evaluation split where each line is:

```json
{"logits": [0.0, 0.0], "label": 0}
```

Then run:

```bash
python3 scripts/model/decision_report.py eval.jsonl \
  --fit-jsonl fit.jsonl \
  --name "enrichment gate, current mechanism" \
  --out docs/evidence/W03/decision-pilot-report.md
```

The resulting table must be reviewed before any tier is connected to a capture
or agent runtime path.
