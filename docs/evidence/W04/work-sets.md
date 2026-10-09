# Work sets on a copy of the owner's vault (2026-10-09)

The copy holds 20 memories, all from the last seven days, and only one of them reopens to a real place: 19 reopen to an app alone (Dia, ChatGPT, Spotify, Finder, Terminal, Preview) and 1 to a file (a Markdown file in VS Code). No memory carries a browser URL, a PDF page or a Google Doc. So this run checks which thread a request picks; it cannot check the case the feature is for, a Canvas page, a PDF at its page and a doc opened together. That path is covered only by fixture tests (`workset/rank.rs`, `the_request_opens_every_place_of_one_stretch_of_work`).

## How it was run

Read-only, on a copy: `cp -R ~/Library/Application\ Support/com.fndr.app/lancedb <scratch>/lancedb`, then

```text
FNDR_WORKSET_EVAL_DIR=<scratch> FNDR_WORKSET_QUERIES="q1|q2|..." \
  cargo test --lib work_set_eval -- --ignored --nocapture
```

The test refuses the real profile. Config is the default (empty blocklist), not the owner's. Hybrid search ran with the local embedder. Nothing was opened: `resolve` only.

## Results after tuning

Kind of check: real data, on a copy. "Right" is my judgment from the window titles and projects in the copy; where I cannot know what the owner meant, it says so.

| # | Request | Chosen | Items | Right? |
|---|---|---|---|---|
| 1 | pull up everything related to the assignment I was working on | Dia thread, project "FNDR integration and Hermes agent setup" | Dia (app only) | Probably a miss. A Finder window named for a programming assignment submission exists; "assignment" matched a task title linked to the Dia thread instead. Cannot judge which the owner meant. |
| 2 | pull up everything for PA2 | Finder thread with the PA2 submission folder | Finder (app only) | Right thread. Only the app opens: the Finder memory has no folder path, so the folder itself does not open. |
| 3 | open everything related to looped transformers | Dia thread titled with the topic | Dia (app only) | Right thread; the page itself cannot open (no URL captured). |
| 4 | bring up all the FNDR stuff | Chooser: the Markdown file in VS Code, or a ChatGPT project thread | file; ChatGPT | Reasonable. Both are FNDR work. |
| 5 | pull up what I was working on | Chooser of three recent threads | Dia, ChatGPT, Preview | Honest: with no topic, three threads from the same evening scored within 0.002. |
| 6 | set up my workspace for the recruiting app | Chooser: ChatGPT recruiting project, a Spotify thread, a Dia recruiting thread | apps | Half right. The two recruiting threads are right; Spotify is there because a task linked to that Spotify memory mentions the app. |
| 7 | get out everything related to the memory journey baseline | The Markdown file | file (rank 6) | Right. |
| 8 | reopen everything from the mechanical page | Dia thread about the mechanical page | Dia (app only) | Right thread. |
| 9 | pull up the Claude Code stuff | Dia thread titled Claude Code | Dia (app only) | Right thread. |
| 10 | pull up everything related to the tax return (nothing like it exists) | None: "Nothing FNDR remembers matches tax return" | none | Right. |

Thread picked as I would pick it: 6 of 10 clearly (2, 3, 7, 8, 9, 10), 2 acceptable choosers (4, 5), 1 half right (6), 1 probable miss (1). Places opened as a person would want: only item 7 opens the place itself; every other item opens an app.

Latency: 50 to 600 ms per request on the copy, debug build.

## What the first run showed, and what changed

The first run (same ten requests) had three faults, each now a test in `workset/rank.rs`:

- Request 10 returned a thread because one common word ("return") appeared in a linked task. A match now needs every topic word, two of them, or one with a search hit behind it (`one_shared_common_word_is_not_a_match`).
- Request 6 chose the Spotify thread on a search hit that shared no word with the request. Such a hit now counts half, and alone it matches only at 0.6 or more (`a_search_hit_that_shares_no_word_with_the_request_loses_to_one_that_names_it`).
- Request 7 opened the file plus ChatGPT and Dia as bare apps. A bare app now gives way once the set has a real place (`an_app_alone_gives_way_when_the_set_has_a_real_place`).

## Not judged

- Opening anything: no item was opened from this copy. Opening is covered by a store-backed test with a fake opener (`workset/open.rs`) and the existing reopen tests, not by a run on this Mac.
- A live Notch Do run of a work-set request (voice, plan card, chooser, narration). Not done.
- Cross-app linking (page, PDF, doc together): the copy has no such stretch of work.
- The owner's real blocklist: the run used the default config.

## Open question

Most of the owner's memories reopen only to an app, because Dia captures carry no URL and Finder captures no folder path. Work sets will mostly open apps until capture records those targets.
