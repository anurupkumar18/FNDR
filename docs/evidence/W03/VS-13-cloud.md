# VS-13 parse time and app phrases into filters (cloud)

## What changed

- `src-tauri/src/context_runtime/query_filters.rs`: a pure parser, `parse_query_filters(query, now, known_apps)`. It recognizes today, earlier today, yesterday, this morning (05:00 to 12:00), this afternoon (12:00 to 17:00), this evening or tonight (17:00 to 24:00), last night (18:00 to 05:00), last week (the seven days before today), a week ago, N days ago (digits or words up to fourteen), weekday names ("Thursday" is the most recent Thursday including today, "last Thursday" excludes today), and dates ("on Sep 30", "September 29th"; a future date means last year). Apps are recognized only after in, on, from, or using, and only when they name an app stored in the vault, through aliases ("Excel" for Microsoft Excel, "Chrome" for Google Chrome, "Zoom" for zoom.us, "VS Code" for Visual Studio Code). Look-alikes are not filters: "Monday.com", "weekday vs weekend", "you may want", "the today show", "in Sheets" (a web page inside Chrome), "in the PDF".
- `retrieve` reads the phrases unless the request already carries that filter (explicit Search dropdowns win), passes a `range:<start_ms>:<end_ms>` time filter and the app to every route, and reports them in a new `RetrieveResult.filters`. If a filter read from a phrase finds nothing, it searches the whole vault with the query as typed instead of returning an empty page.
- The store accepts `range:<start>:<end>` time filters (`time_filter_to_sql`).
- `Store::get_app_names` now reads only the `app_name` column instead of every column, vectors included; the app-name cache moved out of the IPC command into `cached_app_names` so retrieval shares it.

## The ticket said "strip the phrase from the text query"; the data said no

Measured on `retrieve`, one seed per persona (America/Denver local time, so the fixtures' today and yesterday hold):

| Persona | Variant | Time Recall@5 | Time MRR@10 | App Recall@5 | App MRR@10 | kw+para Recall@5 | kw+para MRR@10 |
|---|---|---:|---:|---:|---:|---:|---:|
| knowledge-worker | before (VS-09) | 1.000 | 0.938 | 1.000 | 0.900 | 1.000 | 0.966 |
| knowledge-worker | phrases stripped from text | 1.000 | 1.000 | 1.000 | 1.000 | 1.000 | 0.966 |
| knowledge-worker | filters, full text kept (shipped) | 1.000 | 1.000 | 1.000 | 1.000 | 1.000 | 0.966 |
| office-pm | before (VS-09) | 1.000 | 0.917 | 1.000 | 1.000 | 0.900 | 0.661 |
| office-pm | phrases stripped from text | 1.000 | 0.906 | 1.000 | 1.000 | 0.900 | 0.661 |
| office-pm | filters, full text kept (shipped) | 1.000 | 1.000 | 1.000 | 1.000 | 0.900 | 0.661 |

Stripping lost "the product requirements doc I drafted last week" (rank 1 to 4): without "last week" in the text the planner drops its time intent and the temporal route, and "last week" is a weak filter on a one-week corpus. Keeping the full text while applying the filters is best on every row, so the shipped version searches the full query; the phrase-free text is used only to report which words matched, so "yesterday" or "Slack" never shows as a matched word.

## Final report (both personas reseeded, America/Denver)

| Persona | Path | Time Recall@5 / MRR@10 | App Recall@5 / MRR@10 |
|---|---|---|---|
| knowledge-worker | Search | 1.000 / 1.000 | 1.000 / 1.000 |
| knowledge-worker | Ask | 1.000 / 0.938 | 1.000 / 0.900 |
| knowledge-worker | Retrieve | 1.000 / 1.000 | 1.000 / 1.000 |
| office-PM | Search | 0.875 / 0.747 | 1.000 / 0.900 |
| office-PM | Ask | 1.000 / 0.917 | 1.000 / 1.000 |
| office-PM | Retrieve | 1.000 / 1.000 | 1.000 / 1.000 |

Done when "time and app rows reach Recall@5 of 0.9": met on `retrieve` (1.000 on both personas). Search and Ask reach it when VS-10 and VS-11 point them at `retrieve`. The discriminating queries from VS-03 now rank first on `retrieve`: "the LL-1482 spam discussion from four days ago" (3 to 1), "the weekday vs weekend chart from two days ago" (2 to 1), "the pandas error in Terminal" (2 to 1). Keyword and paraphrase rows are unchanged; the references are ratcheted to this run.

## Tests

- `query_filters` unit tests (written first; five of six failed against a stub that returned no filters): day phrases, parts of a day and weeks, app phrases with aliases, time plus app together, ten look-alikes, and a query that is only a filter. They pass in America/Denver, America/Los_Angeles, Asia/Kolkata, and Pacific/Kiritimati.
- `tests/retrieve.rs` new cases: a time phrase and an app phrase narrow the hits and show in `filters`; a parsed filter that finds nothing falls back to the whole vault; explicit request filters win. 7 passed in total.
- `keyword_search_accepts_an_explicit_time_range` (store): failed before the `range:` filter existed.
- `cargo test --lib` 901 passed; `cargo test --lib context_runtime` 58 passed.
