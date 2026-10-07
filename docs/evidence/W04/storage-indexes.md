# MEM-08 storage indexes: before/after on 10000 synthetic rows

Scaled down from the ticket's original 100,000 rows: this machine hit
critical memory pressure earlier in the same session, and 10k rows
across several 384-dim vector columns is a safer working set. Rerun
at full 100k scale on a machine with more headroom before trusting
this as final evidence.

Seed time: 8580.7 ms. Index build time: 46286.9 ms.

| Query | Before (no index) | After (BTree id/timestamp, IVF_PQ embedding) | Speedup |
|---|---|---|---|
| Point lookup by id | 141.18 ms | 55.55 ms | 2.5x |
| Time-range scan (60s window) | 25.26 ms | 22.08 ms | 1.1x |
| Vector top-10 | 127.37 ms | 77.88 ms | 1.6x |

Dataset versions: 18 before, 21 after (index creation itself commits
new versions; compact_and_prune, tested separately, collapses this).

## Decision against the plan's own 3x margin

- Point lookup: 2.5x (below the 3x margin, not proven to help yet)
- Time-range scan: 1.1x (below the 3x margin, not proven to help yet)
- Vector top-10: 1.6x (below the 3x margin, not proven to help yet)

None of the three query classes cleared the 3x bar at 10k rows on this
machine, and building the indexes cost 46287ms against a
10k-row seed that itself only took 8581ms. v2's T-208 spike saw
a real win (about 4.4x on vector search) at 100k rows, so this may be a
scale effect rather than a real no-benefit result. Recommendation:
re-run at 100k rows before deciding whether to keep these indexes in
the live pipeline; do not wire index creation into production based on
this 10k-row evidence alone.
