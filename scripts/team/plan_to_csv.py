#!/usr/bin/env python3
"""Turn the ticket manifest in the master plan into a GitLab issue CSV.

Reads every table row whose first cell looks like a ticket id (three capitals, a dash, two digits) and writes
`title,description`. The description carries what a teammate or an AI agent needs: why, what happened earlier,
what comes next, phase position, time, owner, executor, pull request shape, verification, evidence, and how to
switch between Claude Code and Codex. Quick actions set labels, milestone, estimate, and the assignee.

Labels and milestones must already exist in the project (run gitlab_bootstrap.sh first).

Manifest columns:
ID | Title | WS | Owner | Executor | Week | Hours | Prio | Deps | Why | Earlier | PR shape | Verify | Evidence
Owner is the accountable person. Executor is who does the work; "+agent" means an AI coding agent is expected.
"""
import argparse
import csv
import json
import re
import sys
from pathlib import Path

ROW_RE = re.compile(r"^\|\s*([A-Z]{3}-\d{2})\s*\|")
COLUMNS = ["id", "title", "ws", "owner", "executor", "week", "hours", "prio", "deps", "why", "earlier", "pr", "verify", "evidence"]
PEOPLE = {"Anurup", "Kunj", "Minh", "Felipe"}

MILESTONES = {
    "W1": "W01-Baseline", "W2": "W02-Measure", "W3": "W03-Build", "W4": "W04-Prove",
    "W5": "W05-Retro", "W6": "W06-Foundations", "W7": "W07-FineTune", "W8": "W08-Preference",
    "W9": "W09-Agent", "W10": "W10-Harden", "W11": "W11-Freeze", "W12": "W12-Submit",
}
WS_LABELS = {
    "OPS": "ws::ops", "CAP": "ws::capture", "MOD": "ws::model", "FEA": "ws::features",
    "SEC": "ws::security", "LRN": "ws::learning", "MEM": "ws::memory", "RET": "ws::retrieval", "DEC": "ws::decisions",
}
TYPE_LABELS = {
    "OPS": "type::chore", "CAP": "type::feature", "MOD": "type::feature", "FEA": "type::feature",
    "SEC": "type::bug", "LRN": "type::learning", "MEM": "type::feature", "RET": "type::spike", "DEC": "type::spike",
}
BRANCH_PREFIX = {
    "OPS": "chore", "CAP": "feat", "MOD": "feat", "FEA": "feat", "SEC": "fix",
    "LRN": "docs", "MEM": "feat", "RET": "docs", "DEC": "docs",
}
PLAN_FILES = {
    "OPS": "2026-09-21-ws4-team-operating-system.md",
    "CAP": "2026-09-21-ws1-capture-pipeline-teardown.md",
    "MOD": "2026-09-21-ws2-local-model-harness.md",
    "LRN": "2026-09-21-ws2-local-model-harness.md",
    "FEA": "2026-09-21-ws3-feature-thesis-and-research.md",
    "SEC": "2026-09-21-beta-final-master-plan.md",
    "MEM": "2026-09-21-ws5-post-capture-memory-pipeline.md",
    "RET": "2026-09-21-ws5-post-capture-memory-pipeline.md",
    "DEC": "2026-09-21-ws6-bounded-decisions-and-fast-local-models.md",
}


def week_number(week):
    return int(week[1:])


def phase_of(ticket):
    return "Beta" if week_number(ticket["week"]) <= 4 else "Final"


def executor_name(ticket):
    return ticket["executor"].split("+")[0].strip()


def uses_agent(ticket):
    return "+agent" in ticket["executor"]


def prefix_of(ticket):
    return ticket["id"].split("-")[0]


def deps_of(ticket):
    raw = ticket["deps"]
    if raw.lower() == "none":
        return []
    return [d.strip() for d in raw.split(",") if d.strip()]


def parse_manifest(text):
    tickets = []
    for line in text.splitlines():
        if not ROW_RE.match(line):
            continue
        cells = [c.strip() for c in line.strip().strip("|").split("|")]
        if len(cells) != len(COLUMNS):
            raise ValueError(f"expected {len(COLUMNS)} columns, got {len(cells)}: {line[:80]}")
        ticket = dict(zip(COLUMNS, cells))
        ticket["hours"] = int(ticket["hours"])
        tickets.append(ticket)
    ids = [t["id"] for t in tickets]
    dupes = {i for i in ids if ids.count(i) > 1}
    if dupes:
        raise ValueError(f"duplicate ticket ids: {sorted(dupes)}")
    known = set(ids)
    for t in tickets:
        for dep in deps_of(t):
            if dep not in known and not dep.lower().startswith("all"):
                raise ValueError(f"{t['id']} depends on unknown ticket {dep}")
    return tickets


def slug(title, words=4):
    tokens = re.findall(r"[a-z0-9]+", title.lower())
    return "-".join(tokens[:words]) or "work"


def branch_name(ticket):
    return f"{BRANCH_PREFIX[prefix_of(ticket)]}/{ticket['id'].lower()}-{slug(ticket['title'])}"


def size_class(hours):
    if hours <= 3:
        return "S (about 100 lines changed or fewer)"
    if hours <= 8:
        return "M (about 100 to 400 lines changed)"
    return "L (split into two merge requests so each is reviewable in 30 minutes)"


def ordered_in_phase(tickets, ticket):
    phase = phase_of(ticket)
    same = [t for t in tickets if phase_of(t) == phase]
    return sorted(same, key=lambda t: (week_number(t["week"]), t["prio"] != "P0", t["id"]))


def position_in_phase(tickets, ticket):
    ordered = ordered_in_phase(tickets, ticket)
    index = [t["id"] for t in ordered].index(ticket["id"])
    hours_before = sum(t["hours"] for t in ordered[:index] if t["prio"] == "P0")
    return index + 1, len(ordered), hours_before


def unblocks(tickets, ticket):
    return [t["id"] for t in tickets if ticket["id"] in deps_of(t)]


def build_description(ticket, roster, tickets=None):
    tickets = tickets if tickets is not None else [ticket]
    tid = ticket["id"]
    prefix = prefix_of(ticket)
    plan = PLAN_FILES[prefix]
    executor = executor_name(ticket)
    username = roster.get(executor.lower())
    milestone = MILESTONES[ticket["week"]]
    phase = phase_of(ticket)
    k, n, hours_before = position_in_phase(tickets, ticket)
    nxt = unblocks(tickets, ticket)
    agent_note = " (with an AI coding agent)" if uses_agent(ticket) else " (human work, no agent expected)"
    lines = [
        f"**Owner (accountable):** {ticket['owner']}",
        f"**Executor:** {executor}{agent_note}",
        f"**Due:** end of {milestone}",
        f"**Time expected:** {ticket['hours']} hours (effort for one competent person, before any agent help)",
        f"**Priority:** {ticket['prio']}",
        f"**Phase:** {phase}, ticket {k} of {n} in schedule order, with {hours_before} P0 hours scheduled before it",
        "",
        "## Why",
        ticket["why"],
        "",
        "## Where this fits",
        f"- Earlier: {ticket['earlier']}",
        f"- Must be closed before you start: {ticket['deps']}",
        f"- Next, unblocked by this ticket: {', '.join(nxt) if nxt else 'nothing else depends on it'}",
        "- Live progress for the phase: run `make phase-progress`",
        "",
        "## What to do",
        f"Follow the steps for `{tid}` in `docs/superpowers/plans/{plan}` (search for the ticket id).",
        "",
        "## Pull request shape",
        f"- Branch: `{branch_name(ticket)}`",
        f"- Scope: {ticket['pr']}",
        f"- Size: {size_class(ticket['hours'])}",
        "- Commits: one per plan step, message `type(scope): what`; a human commits and pushes under their own name, with no AI co-author trailer",
        f"- Merge request: use the template, write `Closes #<this issue number>`, paste `make test` and the verify output, link the evidence, one reviewer who is not the author",
        "",
        "## How to verify",
        ticket["verify"],
        "",
        "## Evidence to attach before Done",
        ticket["evidence"],
        f"Save files under `docs/evidence/W{int(ticket['week'][1:]):02d}/` and link them here.",
        "",
        "## Definition of done",
        "See `docs/team/TEAM.md`, section Definition of done. Merged, reviewed, `make test` output attached, evidence attached, handoff note written if work continues.",
        "",
        "## Agent brief and switching between tools",
        f"- Read first: `AGENTS.md` and the `{tid}` steps in `docs/superpowers/plans/{plan}`. Claude Code and Codex both reach `AGENTS.md`.",
        "- Constraints: strictly local models, no real captures in git, branch and merge request only, no em dashes.",
        "- Checkpoint: commit after every plan step so either tool can resume from git alone.",
        "- If a tool's 5-hour limit or your credits run low: commit, write `docs/handoffs/<date>-" + tid.lower() + ".md`, then paste the resume prompt from `docs/team/agent-switch.md` into the other tool.",
        "- Stop and ask the owner if a step contradicts the code you find.",
        "",
    ]
    labels = [WS_LABELS[prefix], TYPE_LABELS[prefix], f"prio::{ticket['prio'].lower()}", f"phase::{phase.lower()}",
              "status::ready", "evidence::needed", "agent-ok" if uses_agent(ticket) else "needs-human"]
    if uses_agent(ticket):
        labels.append("agent::either")
    quick = [
        "/label " + " ".join(f'~"{label}"' for label in labels),
        f"/milestone %{milestone}",
        f"/estimate {ticket['hours']}h",
    ]
    if username:
        quick.append(f"/assign @{username}")
    return "\n".join(lines + quick) + "\n"


def to_rows(tickets, roster, selected=None):
    chosen = selected if selected is not None else tickets
    return [(f"[{t['id']}] {t['title']}", build_description(t, roster, tickets)) for t in chosen]


def unassigned(tickets, roster):
    return sorted({executor_name(t) for t in tickets if executor_name(t).lower() not in roster})


def load_by_executor(tickets):
    """Nominal hours per executor, split by priority. Agent leverage is upside, not counted."""
    load = {}
    for t in tickets:
        row = load.setdefault(executor_name(t), {"P0": 0, "P1": 0})
        row[t["prio"]] += t["hours"]
    return load


def main(argv):
    ap = argparse.ArgumentParser()
    ap.add_argument("plan", help="path to the master plan markdown")
    ap.add_argument("--out", required=True)
    ap.add_argument("--weeks", nargs="*", help="only these weeks, for example W1 W2")
    ap.add_argument("--roster", help="JSON file mapping first names to GitLab usernames (optional)")
    args = ap.parse_args(argv)
    tickets = parse_manifest(Path(args.plan).read_text())
    selected = [t for t in tickets if t["week"] in set(args.weeks)] if args.weeks else tickets
    roster = {}
    if args.roster:
        roster = {k.lower(): v for k, v in json.loads(Path(args.roster).read_text()).items()}
    rows = to_rows(tickets, roster, selected)
    Path(args.out).parent.mkdir(parents=True, exist_ok=True)
    with open(args.out, "w", newline="") as f:
        writer = csv.writer(f, quoting=csv.QUOTE_ALL)
        writer.writerow(["title", "description"])
        writer.writerows(rows)
    print(f"wrote {len(rows)} issues to {args.out}")
    missing = unassigned(selected, roster)
    if missing:
        print("no GitLab username for: " + ", ".join(missing) + " (assign these on the board by hand)")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
