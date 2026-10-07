# Property tests for fusion and the phrase parser (cloud)

Part 9 stretch item. The repo has no property-testing crate, and adding one would touch `Cargo.toml`. So the tests use a small seeded generator inside the existing unit-test modules: every failure reproduces from its case number.

## Tests

- **`fusion::tests::fusion_holds_its_invariants_on_random_route_hits`** builds 500 random sets of route hits. They are shaped like the real routes: four routes, two vector branches, scores from a small set so ties are common, and some outside 0..1. It checks that:
  - fusion is deterministic;
  - every input memory appears once, best score first;
  - no score exceeds the weights of the routes that found the memory;
  - the order of hits inside a route does not matter.
- **`fusion::tests::both_vector_branches_add_to_a_memorys_score`** pins the double count below.
- **`query_filters::tests::parsing_holds_its_invariants_on_random_queries`** builds 3,000 random queries from 44 tricky words: filter words, weekday and month names, look-alikes, `Monday.com`, numbers, punctuation, and non-ASCII text. App names include ones whose short name is a time word. It checks that:
  - parsing never panics;
  - text without a filter is unchanged;
  - the text only loses words;
  - every time range is non-empty, at most a week, and starts by now.
- **`query_filters::tests::no_time_phrase_after_a_deadline_word_is_a_filter`** covers 8 deadline words (in mixed case) times 10 time phrases.
- **`query_filters::tests::an_app_whose_name_is_a_time_word_does_not_break_parsing`** is the minimal case of the first finding.

All run in under 4 seconds together.

## Found 1: a crash. Fixed.

With the monday.com app installed, "notes on Monday" read as both an app phrase (alias "monday") and a weekday, over the same bytes. `parse_query_filters` cut both spans out of the text, and the second cut ran past the end of the shortened string:

```
panicked at library/core/src/slice/index.rs:1020:51:
range end index 15 out of range for slice of length 7
```

`retrieve` parses every Search and Ask query with the stored app names, so any query like that would crash the search for that user. Now, when an app phrase overlaps a time phrase, the words are read as the time, and no span is cut twice. Before the fix the random test panicked on such a query too ("range end index 15 out of range for slice of length 5"); after it, all 3,000 queries pass.

## Found 2: slow parsing. Fixed.

The random test first took 167 seconds for 3,000 queries, about 56 ms per parse in a debug build. `find_app` compiled a new regex for every alias of every stored app on every query. Each alias pattern is now compiled once per process (`app_phrase_pattern`), with the same matching rules:

- 3,000 random parses: 167 s to 3.7 s.
- The 80 deadline cases: 3.66 s to 0.16 s.
- End to end, `retrieve` p50 on the seeded profiles (about ten stored apps, one run each, Linux debug):
  - knowledge-worker 189 to 142 ms;
  - office-PM 188 to 157 ms;
  - software-engineer 191 to 158 ms.
  - Search moved by a similar amount. A real profile with more apps saves more.

The gate on all three personas, with both fixes: PASS, "No per-query rank changed" on every persona.

## Found 3: the vector route counts twice. Measured, kept, and pinned by a test.

The vector route returns hits from two branches, the full-text embedding and the snippet embedding. Fusion adds `score x weight` for every hit, so a memory that both branches find gets the vector weight (0.45) twice, while `signals.vector` keeps only the larger score. The first version of the random test flagged it: "m01 scored 0.63 from [Vector]", which is 0.45 x 0.8 + 0.45 x 0.6.

Counting the vector route once per memory (best branch only) was measured on one seed per persona, America/Denver, 2026-10-05:

| Persona | Recall@5 | MRR@10 | Paraphrase Recall@5 | Positives under the no-match bar |
|---|---|---|---|---|
| knowledge-worker | 1.000 to 0.955 | 0.966 to 0.960 | 1.000 to 0.875 | 0 to 3 |
| office-pm | 0.900 to 0.850 | 0.661 to 0.740 | 0.778 to 0.667 | 0 to 2 |
| software-engineer | 1.000 to 1.000 | 0.831 to 0.855 | 1.000 to 1.000 | 0 to 2 |

- The double count is load-bearing. Agreement between the two branches acts as a vote that paraphrase queries rely on: "which plotting library am I allowed to use" falls from rank 4 to 8, and "how much money are we losing to customers leaving" from 5 to 10.
- Counting once lifts MRR@10 on two personas (office-PM +0.079), but loses Recall@5 on two.
- Every fused score drops by about a quarter (median positive top score 0.464 to 0.344), so three calibrated thresholds would all need to move:
  - VS-12's no-match bars, 0.25 and 0.45;
  - Ask's verifier floor, 0.3. `tests/end_to_end_fndr_query.rs` failed with "all hits below confidence threshold".
- **Decision: not shipped.** The behavior stays and is now pinned by `both_vector_branches_add_to_a_memorys_score`, whose comment points here.
- A cleaner design gives the snippet branch its own weight, so the agreement bonus is explicit, and recalibrates the bars in the same change. Proposed as VS-70 in `docs/team/tickets/proposed/cloud-proposals.md`.
