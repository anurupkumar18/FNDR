# Cloud proposals

Tickets the cloud session proposes during the parallel-session campaign (`docs/superpowers/plans/2026-10-04-parallel-sessions/README.md`, Part 2 rule 6). This folder is inert: `scripts/team/gitlab_sync.py` reads only the top level of `docs/team/tickets/`, so nothing here reaches the board on its own. The local session reviews each proposal, moves accepted ones into `docs/team/tickets/anurup-followups-2026-10.md` (or the owning lane's file), runs `make gitlab-plan`, then `make gitlab-sync APPLY=1`, and deletes them from here. Format and IDs follow `docs/team/tickets/README.md`; each ID is the next free one in its lane.

No open proposals. VS-40 and VS-41 were accepted on 2026-10-05 and moved to `docs/team/tickets/anurup-followups-2026-10.md`.
