#!/usr/bin/env python3
"""How far along are we? Progress by phase, workstream, and executor, plus what each person can start now.

The manifest in the master plan says what exists. The GitLab board says what is closed. This joins them:
a ticket is closed when its GitLab issue (title starts with `[ID]`) is in the closed state.

  scripts/team/phase_progress.py --manifest docs/superpowers/plans/2026-09-21-beta-final-master-plan.md --api
  scripts/team/phase_progress.py --manifest ... --closed CAP-01 CAP-02        # manual, no network
  scripts/team/phase_progress.py --manifest ... --issues-json issues.json      # a saved API response

`--api` needs GITLAB_TOKEN (read_api scope is enough). GITLAB_HOST and GITLAB_PROJECT override the defaults.
"""
import argparse
import json
import os
import re
import sys
import urllib.request
from collections import OrderedDict
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parent))
import plan_to_csv as p

TITLE_ID = re.compile(r"^\[([A-Z]{3}-\d{2})\]")


def closed_ids_from_issues(issues):
    closed = set()
    for issue in issues:
        m = TITLE_ID.match(issue.get("title", ""))
        if m and issue.get("state") == "closed":
            closed.add(m.group(1))
    return closed


def fetch_issues(host, project, token, pages=10):
    issues = []
    for page in range(1, pages + 1):
        url = f"{host}/api/v4/projects/{project}/issues?state=all&per_page=100&page={page}"
        request = urllib.request.Request(url, headers={"PRIVATE-TOKEN": token})
        with urllib.request.urlopen(request, timeout=30) as response:
            batch = json.load(response)
        issues.extend(batch)
        if len(batch) < 100:
            break
    return issues


def _bucket():
    return {"tickets": 0, "closed_tickets": 0, "hours": 0, "closed_hours": 0}


def add(bucket, ticket, closed):
    bucket["tickets"] += 1
    bucket["hours"] += ticket["hours"]
    if ticket["id"] in closed:
        bucket["closed_tickets"] += 1
        bucket["closed_hours"] += ticket["hours"]


def summarize(tickets, closed):
    by_phase, by_ws, by_executor = OrderedDict(), OrderedDict(), OrderedDict()
    for t in tickets:
        for table, key in ((by_phase, p.phase_of(t) + " " + t["prio"]), (by_ws, p.prefix_of(t)), (by_executor, p.executor_name(t))):
            add(table.setdefault(key, _bucket()), t, closed)
    return by_phase, by_ws, by_executor


def is_ready(ticket, tickets, closed):
    """A ticket can start when every dependency is closed. 'all ...' means every other P0 ticket of its phase."""
    for dep in p.deps_of(ticket):
        if dep.lower().startswith("all"):
            others = [t for t in tickets if p.phase_of(t) == p.phase_of(ticket) and t["prio"] == "P0" and t["id"] != ticket["id"]]
            if any(t["id"] not in closed for t in others):
                return False
        elif dep not in closed:
            return False
    return True


def next_up(tickets, closed, limit=3):
    result = OrderedDict()
    for person in sorted({p.executor_name(t) for t in tickets}):
        open_ready = [t for t in tickets if p.executor_name(t) == person and t["id"] not in closed and is_ready(t, tickets, closed)]
        open_ready.sort(key=lambda t: (t["prio"] != "P0", p.week_number(t["week"]), t["id"]))
        result[person] = open_ready[:limit]
    return result


def pct(part, whole):
    return f"{100 * part / whole:.0f}%" if whole else "0%"


def render(tickets, closed):
    by_phase, by_ws, by_executor = summarize(tickets, closed)
    lines = ["# Phase progress", ""]
    for title, table in (("By phase and priority", by_phase), ("By workstream", by_ws), ("By executor", by_executor)):
        lines += [f"## {title}", "", "| Group | Tickets closed | Hours closed | Share of hours |", "|---|---|---|---|"]
        for key, b in table.items():
            lines.append(f"| {key} | {b['closed_tickets']} of {b['tickets']} | {b['closed_hours']} of {b['hours']} | {pct(b['closed_hours'], b['hours'])} |")
        lines.append("")
    lines += ["## Next up (ready to start, P0 first)", ""]
    for person, items in next_up(tickets, closed).items():
        lines.append(f"- {person}: " + (", ".join(f"{t['id']} ({t['prio']}, {t['week']}, {t['hours']}h)" for t in items) or "nothing ready, ask on Monday"))
    lines.append("")
    return "\n".join(lines)


def main(argv):
    ap = argparse.ArgumentParser()
    ap.add_argument("--manifest", required=True)
    ap.add_argument("--closed", nargs="*")
    ap.add_argument("--issues-json")
    ap.add_argument("--api", action="store_true")
    ap.add_argument("--out")
    args = ap.parse_args(argv)
    tickets = p.parse_manifest(Path(args.manifest).read_text())
    closed = set(args.closed or [])
    if args.issues_json:
        closed |= closed_ids_from_issues(json.loads(Path(args.issues_json).read_text()))
    if args.api:
        token = os.environ.get("GITLAB_TOKEN")
        if not token:
            print("Set GITLAB_TOKEN (read_api scope is enough).", file=sys.stderr)
            return 1
        host = os.environ.get("GITLAB_HOST", "https://capstone.cs.utah.edu")
        project = os.environ.get("GITLAB_PROJECT", "fndr%2Ffndr")
        closed |= closed_ids_from_issues(fetch_issues(host, project, token))
    unknown = closed - {t["id"] for t in tickets}
    if unknown:
        print("ignoring closed ids not in the manifest: " + ", ".join(sorted(unknown)), file=sys.stderr)
    report = render(tickets, closed & {t["id"] for t in tickets})
    if args.out:
        Path(args.out).parent.mkdir(parents=True, exist_ok=True)
        Path(args.out).write_text(report)
        print(f"wrote {args.out}")
    else:
        print(report)
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
