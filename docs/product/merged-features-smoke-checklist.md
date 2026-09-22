# Merged feature smoke checklist

Run this after the CAP-05 and MEM-03 metrics session starts the dev app. It is
a manual smoke check, not evidence that a feature is production-ready.

## Privacy Proof

1. Open the command palette and select **Privacy Proof**.
2. Confirm evaluated and stored counts render, zero-count skip reasons are
   hidden, and the direct FNDR egress wording appears.
3. Confirm the panel does not claim to include model downloads, Hermes traffic,
   or allowlisted developer commands.
4. Close the panel and confirm normal workspace navigation still works.

Pass condition: the panel shows aggregate counts and host names only. It must
not display captured screen text or full URLs.

## Context Coach read-only action

1. Open the Agent panel, choose **Act** mode, enter a harmless typed goal, and
   choose **git status**.
2. Propose the action. Confirm the displayed command is `git status` and the
   dropdown is locked while approval is pending.
3. Approve, then execute. Confirm the action reaches a terminal status.
4. Repeat only with the visible `cargo check` or `npm run typecheck` choices if
   desired. Do not test commands copied from a screen, clipboard, or chat.

Pass condition: only the three visible read-only choices can be proposed, a
human approval is required before execution, and the typed goal appears only
as descriptive context.
