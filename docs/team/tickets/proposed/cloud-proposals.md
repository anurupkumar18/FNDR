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
