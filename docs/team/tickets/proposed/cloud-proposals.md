# Cloud proposals

Tickets the cloud session proposes during the parallel-session campaign (`docs/superpowers/plans/2026-10-04-parallel-sessions/README.md`, Part 2 rule 6). This folder is inert: `scripts/team/gitlab_sync.py` reads only the top level of `docs/team/tickets/`, so nothing here reaches the board on its own. The local session reviews each proposal, moves accepted ones into `docs/team/tickets/anurup-followups-2026-10.md` (or the owning lane's file), runs `make gitlab-plan`, then `make gitlab-sync APPLY=1`, and deletes them from here. Format and IDs follow `docs/team/tickets/README.md`; each ID is the next free one in its lane.

VS-40 and VS-41 were accepted on 2026-10-05 and moved to `docs/team/tickets/anurup-followups-2026-10.md`.

## VS-69 Feed the persisted insight graph to the entity route, or remove it
- assignee: anurupkumar
- labels: area::vault-search, type::spike, prio::p2
- milestone: W04-Prove
- estimate: 4h
- depends: VS-33

**Why.** VS-33 found that the insight graph is persisted (`graph_nodes`, `graph_edges` in LanceDB, written by the capture flush and idle `commit_graph_updates`), but `retrieve` never loads it. The entity route matches query entities against graph nodes, so it returns nothing on every real query, while the planner still schedules it for project and entity queries. The graph route was taken out of planning for the same reason.

**Do.**
1. Add graph rows to one seeded persona (`seed_demo` writes nodes for its projects and people), so the effect can be measured.
2. Behind a flag, load project and entity nodes once per query (or cache them per store version) and pass them to the entity route.
3. Measure with `make qa-retrieval-check` on all personas, and time the load at 10,000 nodes.
4. If neither recall nor MRR improves, remove the entity route from planning, as VS-33 did for the graph route (anti-bloat gate).

**Done when.** Either the entity route returns graph-backed hits with no Recall@5 drop on any path, or it is out of the planner with the reason recorded.

**Evidence.** Gate output before and after, and the load timing.

**Result, 2026-10-13: measured, not shipped.** Two things were checked before and after on the same seeded profiles.

The graph cannot feed the route. On the owner's vault (162 memories, read only, counts only) all 659 graph nodes have exactly one source memory, and each of the 10 project nodes has one. A node that matches a query can add at most the one memory it came from.

A version that needs no graph was built and measured instead: the planner recognized a project from words only that project's name has ("essay rubric weights" to "HIST 2100 essay: Reconstruction"), and the entity route returned the memories whose `project` field is that project. Planner and route unit tests passed. On the three personas, search before and after: knowledge-worker Recall@5 1.000 and 1.000, MRR@10 0.909 and 0.909; office-pm 0.950 and 0.950, 0.647 and 0.647; software-engineer 1.000 and 1.000, 0.883 and 0.883. One query moved, from rank 1 to 2. Under step 4 the change was dropped, about 290 lines.

Why it cannot help yet: the entity weight in fusion is 0.10, the personas are at or near the recall ceiling, and on the owner's vault only 21 of 162 memories carry a project, 9 of the 11 projects with a single memory. `detect_project_for_record` recognizes a project only from a code repository path, so a document, a course page or a chat never gets one.

Still open, in this order: decide what a piece of work is for a person who does not work in a repository (it also decides Resume threads, which today are one window title each: 54 of 71 threads on the owner's vault hold one or two memories and one holds 64); then take the entity route out of the planner or feed it from that.

## VS-70 Weight the vector route's two branches explicitly, and recalibrate the score bars
- assignee: anurupkumar
- labels: area::vault-search, type::spike, prio::p2
- milestone: W04-Prove
- estimate: 4h
- depends: VS-12

**Why.** The vector route reports a memory once per branch (full-text embedding and snippet embedding), and fusion adds the vector weight for each, so agreement between the branches counts double. That is load-bearing: counting the route once lost Recall@5 on two of three personas (1.000 to 0.955, 0.900 to 0.850) while lifting MRR@10 (office-PM 0.661 to 0.740), and every fused score fell by about a quarter, which put real queries under VS-12's bar and under Ask's verifier floor (`docs/evidence/W03/property-tests-cloud.md`).

**Do.**
1. Give the snippet branch its own fusion weight, so the agreement bonus is a named number instead of a side effect.
2. Sweep the snippet weight on the three personas; keep Recall@5 within 0.05 on every path.
3. Recalibrate the no-match bars (0.25, 0.45) and the verifier floor (0.3) on the new scale, with "positives under the bar" at 0 on every persona.
4. Update `fusion::tests::both_vector_branches_add_to_a_memorys_score`.

**Done when.** The gate passes on three personas with the explicit weight, and no real query falls under a bar.

**Evidence.** The sweep table and the gate output.
