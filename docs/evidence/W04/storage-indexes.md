# MEM-08 storage index inventory and 10000 row timing

Scaled down from the ticket's original 100,000 rows: this machine hit
critical memory pressure earlier in the same session, and 10k rows
across several 384-dim vector columns is a safer working set. Rerun
at full 100k scale on a machine with more headroom before trusting
this as final evidence.

Seed time: 8273.3 ms. Index build time: 39725.6 ms.

## Memories table indexes

The memories table has seven BM25 full-text indexes, one each on
`window_title`, `clean_text`, `snippet`, `memory_context`, `lexical_shadow`,
`url`, and `app_name`. Writes create these lazily, and the next write checks
for any missing indexes. They serve keyword search. They do not accelerate
the leading-wildcard `LIKE` path used by the legacy keyword searcher.

The storage-scale benchmark explicitly adds three more indexes: BTree on
`id`, BTree on `timestamp`, and IVF_PQ on `embedding`. The production code
defines this method but has no caller that creates these scale indexes. The
benchmark's pre-index vector query therefore uses a flat scan. After index
creation, it uses IVF_PQ; LanceDB still scans rows written after an index was
built until those rows are folded into the index. At this 10000 row size, the
measured IVF_PQ query was only 1.1x faster. A 100000 row run is still needed
before deciding whether to enable scale index creation in the live pipeline.

| Query | Before scale indexes | After (BTree id/timestamp, IVF_PQ embedding) | Speedup |
|---|---|---|---|
| Point lookup by id | 24.23 ms | 17.30 ms | 1.4x |
| Time-range scan (60s window) | 20.12 ms | 18.80 ms | 1.1x |
| Vector top-10 | 56.72 ms | 49.53 ms | 1.1x |

Dataset versions: 18 before, 21 after (index creation itself commits
new versions; compact_and_prune, tested separately, collapses this).

## Decision against the plan's own 3x margin

- Point lookup: 1.4x (below the 3x margin, not proven to help yet)
- Time-range scan: 1.1x (below the 3x margin, not proven to help yet)
- Vector top-10: 1.1x (below the 3x margin, not proven to help yet)

None of the three query classes cleared the 3x bar at 10k rows on this
machine, and building the indexes cost 39726ms against a
10k-row seed that itself only took 8273ms. v2's T-208 spike saw
a real win (about 4.4x on vector search) at 100k rows, so this may be a
scale effect rather than a real no-benefit result. Recommendation:
re-run at 100k rows before deciding whether to keep these indexes in
the live pipeline; do not wire index creation into production based on
this 10k-row evidence alone.
