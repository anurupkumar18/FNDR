# VS-66 the before and after recall chart, rebuilt by one command (cloud)

## The command

```
make recall-chart          # python3 scripts/audit/recall_chart.py
```

- **Reads** only committed files: the VS-03 baselines (`docs/evidence/W03/retrieval-baseline-{knowledge-worker,office-pm}.json`, recorded at aed1315 before any train B change) and the accepted references (`scripts/demo/retrieval-reference/{knowledge-worker,office-pm}.json`).
- **Writes:**
  - `docs/evidence/W03/beta-demo-recall.csv`: one row per persona, path (Search, Ask), and stage, with Recall@5, MRR@10, p95, and Recall@5 by kind.
  - `docs/evidence/W03/beta-demo-recall.png`: two panels, Recall@5 and paraphrase-only Recall@5, before and after.
- **Byte-identical reruns:** the PNG carries no timestamp or version (`metadata={"Software": None}`), so an unchanged chart is byte-identical across runs; the test checks this. `--no-png` writes the CSV without matplotlib.
- **The software-engineer persona is left out:** it has no "before" baseline (it was written after the retrieval changes).

## Tests

`scripts/audit/test_recall_chart.py`, 3 passed:

- every persona, path, and stage has a row;
- the committed CSV equals what the command produces from the committed reports, so a stale CSV fails the test instead of reaching a slide;
- the PNG is a valid PNG and identical on a second run (skipped when matplotlib is missing).

## What the chart shows

The numbers and what the stage may claim are in `beta-demo-recall-cloud.md`. In short:

- office-PM Search Recall@5 rises 0.70 to 0.90, and paraphrase-only from 0.33 to 0.78;
- knowledge-worker Search rises 0.955 to 1.000;
- Ask is unchanged on both.

The ticket's dependency on VS-63 does not change the chart: VS-63 changes how the gate judges a run, not the references the chart reads.
