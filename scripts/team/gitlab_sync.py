#!/usr/bin/env python3
"""Sync the October tickets in docs/team/tickets/ to GitLab, and move issues around.

  python3 scripts/team/gitlab_sync.py plan                  # parse and validate; no token, no network
  python3 scripts/team/gitlab_sync.py sync                  # read GitLab and print what would change
  python3 scripts/team/gitlab_sync.py sync --apply          # create labels, milestones, issues, lane hubs, boards
  python3 scripts/team/gitlab_sync.py sync --apply --update # also rewrite existing descriptions from the files
  python3 scripts/team/gitlab_sync.py list [--user NAME] [--status doing]
  python3 scripts/team/gitlab_sync.py move VS-05 doing      # ready, doing, review, evidence, or closed
  python3 scripts/team/gitlab_sync.py comment VS-05 "Merged !42; qa-retrieval output attached"

Token: $GITLAB_TOKEN, otherwise the macOS Keychain item "fndr-gitlab"
(create it once with: security add-generic-password -s fndr-gitlab -a "$USER" -w).
The token is never printed. Host and project: $GITLAB_HOST, $GITLAB_PROJECT_PATH.
"""
from __future__ import annotations

import argparse
import json
import os
import re
import subprocess
import sys
import time
import urllib.error
import urllib.parse
import urllib.request
from dataclasses import dataclass, field
from pathlib import Path

REPO = Path(__file__).resolve().parents[2]
TICKET_DIR = REPO / "docs" / "team" / "tickets"
ROSTER = REPO / "docs" / "team" / "roster.json"
HOST = os.environ.get("GITLAB_HOST", "https://capstone.cs.utah.edu")
PROJECT_PATH = os.environ.get("GITLAB_PROJECT_PATH", "fndr/fndr")

STATUSES = ("ready", "doing", "review", "evidence")
MILESTONES = {
    "W02-Measure": ("2026-09-28", "2026-10-04"),
    "W03-Build": ("2026-10-05", "2026-10-11"),
    "W04-Prove": ("2026-10-12", "2026-10-18"),
    "W05-Retro": ("2026-10-19", "2026-10-25"),
}
LABEL_COLORS = {
    "status::ready": "#5BC0DE", "status::doing": "#F0AD4E",
    "status::review": "#A78BFA", "status::evidence": "#34D399",
    "prio::p0": "#B60205", "prio::p1": "#FBCA04", "prio::p2": "#C2E0C6",
    "type::feature": "#0E8A16", "type::bug": "#B60205", "type::chore": "#C5DEF5",
    "type::docs": "#0075CA", "type::spike": "#FBCA04", "type::qa": "#1D76DB",
    "type::decision": "#5319E7", "type::research": "#D4C5F9",
    "area::vault-search": "#17BECF", "area::reopen": "#8C564B", "area::embeddings": "#9467BD",
    "area::command": "#D62728", "area::skills": "#E377C2", "area::local-models": "#7F7F7F",
    "area::voice": "#FF7F0E", "area::onboarding": "#2CA02C", "area::ui-polish": "#BCBD22",
    "area::tests": "#1F77B4", "area::product": "#000000",
    "lane-hub": "#333333",
}
HEADER_RE = re.compile(r"^## ([A-Z]{2,3}-\d{2,3}) (.+?)\s*$")
META_RE = re.compile(r"^- (assignee|labels|milestone|estimate|depends): (.*)$")
TITLE_ID_RE = re.compile(r"^\[([A-Z]{2,3}-\d{2,3})\] ")
HUB_TITLE = "[LANE] {name}: October lane"


@dataclass
class Ticket:
    id: str
    title: str
    assignee: str
    labels: list[str]
    milestone: str
    estimate: str
    depends: list[str]
    body: str
    source: str
    extra: dict = field(default_factory=dict)

    @property
    def hours(self) -> float:
        match = re.match(r"(\d+(?:\.\d+)?)h", self.estimate)
        return float(match.group(1)) if match else 0.0

    @property
    def prio(self) -> str:
        return next((l for l in self.labels if l.startswith("prio::")), "prio::p2")


# ---------------------------------------------------------------- parsing


def parse_tickets(text: str, source: str) -> list[Ticket]:
    tickets: list[Ticket] = []
    lines = text.splitlines()
    i = 0
    while i < len(lines):
        header = HEADER_RE.match(lines[i])
        if not header:
            i += 1
            continue
        meta: dict[str, str] = {}
        i += 1
        while i < len(lines) and META_RE.match(lines[i]):
            key, value = META_RE.match(lines[i]).groups()
            meta[key] = value.strip()
            i += 1
        body_lines = []
        while i < len(lines) and not lines[i].startswith("## "):
            body_lines.append(lines[i])
            i += 1
        depends = [d.strip() for d in meta.get("depends", "none").split(",") if d.strip() and d.strip() != "none"]
        tickets.append(
            Ticket(
                id=header.group(1),
                title=header.group(2),
                assignee=meta.get("assignee", ""),
                labels=[l.strip() for l in meta.get("labels", "").split(",") if l.strip()],
                milestone=meta.get("milestone", ""),
                estimate=meta.get("estimate", ""),
                depends=depends,
                body="\n".join(body_lines).strip(),
                source=source,
            )
        )
    return tickets


def load_roster() -> dict[str, str]:
    """username -> display name"""
    return {username: name for name, username in json.loads(ROSTER.read_text()).items()}


def load_all_tickets() -> list[Ticket]:
    tickets: list[Ticket] = []
    for path in sorted(TICKET_DIR.glob("*.md")):
        if path.name == "README.md":
            continue
        tickets.extend(parse_tickets(path.read_text(), str(path.relative_to(REPO))))
    return tickets


def validate(tickets: list[Ticket], roster: dict[str, str]) -> list[str]:
    errors: list[str] = []
    seen: set[str] = set()
    ids = {t.id for t in tickets}
    for t in tickets:
        if t.id in seen:
            errors.append(f"{t.id}: duplicate id")
        seen.add(t.id)
        if t.assignee not in roster:
            errors.append(f"{t.id}: assignee {t.assignee!r} is not in docs/team/roster.json")
        if t.milestone not in MILESTONES:
            errors.append(f"{t.id}: unknown milestone {t.milestone!r}")
        if not any(l.startswith("prio::") for l in t.labels):
            errors.append(f"{t.id}: needs a prio:: label")
        for label in t.labels:
            if label not in LABEL_COLORS:
                errors.append(f"{t.id}: unknown label {label!r}")
        if not t.hours:
            errors.append(f"{t.id}: estimate must look like '3h'")
        for dep in t.depends:
            if dep not in ids:
                errors.append(f"{t.id}: depends on unknown ticket {dep}")
        if re.search("[\\u2013\\u2014]", t.title + t.body):
            errors.append(f"{t.id}: contains an em or en dash")
        if not t.body:
            errors.append(f"{t.id}: empty body")
    return errors


# ---------------------------------------------------------------- rendering


def owner_label(username: str, roster: dict[str, str]) -> str:
    return f"owner::{roster[username].lower()}"


def issue_title(t: Ticket) -> str:
    return f"[{t.id}] {t.title}"


def create_labels(t: Ticket, roster: dict[str, str]) -> list[str]:
    return [*t.labels, owner_label(t.assignee, roster), "status::ready"]


def render_description(t: Ticket, iids: dict[str, int]) -> str:
    deps = ", ".join(f"#{iids[d]} ({d})" if d in iids else d for d in t.depends) or "none"
    return (
        f"{t.body}\n\n---\n"
        f"Estimate: {t.estimate}. Depends on: {deps}.\n\n"
        f"Source: `{t.source}`. Edit the ticket there and run `make gitlab-sync`; "
        f"status, comments, and merge request links live here."
    )


def render_hub(name: str, tickets: list[Ticket], iids: dict[str, int]) -> str:
    total = sum(t.hours for t in tickets)
    lines = [
        f"All of {name}'s October tickets, grouped by week. Checkboxes follow the issues.",
        f"Total estimate: {total:g} hours "
        f"(p0 {sum(t.hours for t in tickets if t.prio == 'prio::p0'):g}, "
        f"p1 {sum(t.hours for t in tickets if t.prio == 'prio::p1'):g}, "
        f"p2 {sum(t.hours for t in tickets if t.prio == 'prio::p2'):g}).",
        "",
    ]
    for milestone in MILESTONES:
        week = [t for t in tickets if t.milestone == milestone]
        if not week:
            continue
        lines += [f"### {milestone}", ""]
        for t in week:
            ref = f"#{iids[t.id]}" if t.id in iids else f"[{t.id}]"
            lines.append(f"- [ ] {ref} {t.title} ({t.estimate}, {t.prio.split('::')[1]})")
        lines.append("")
    lines.append("Plan: `docs/team/2026-10-month-plan.md`. Tickets: `docs/team/tickets/`.")
    return "\n".join(lines)


def status_change(current: list[str], new_status: str) -> tuple[list[str], list[str]]:
    """Labels to add and remove when moving to new_status."""
    remove = [l for l in current if l.startswith("status::") and l != f"status::{new_status}"]
    add = [] if f"status::{new_status}" in current else [f"status::{new_status}"]
    return add, remove


def board_url(board_id: int, scoped: bool, username: str | None) -> str:
    url = f"{HOST}/{PROJECT_PATH}/-/boards/{board_id}"
    if username and not scoped:
        url += f"?assignee_username={urllib.parse.quote(username)}"
    return url


# ---------------------------------------------------------------- GitLab


def read_token() -> str | None:
    token = os.environ.get("GITLAB_TOKEN")
    if token:
        return token.strip()
    try:
        out = subprocess.run(
            ["security", "find-generic-password", "-s", "fndr-gitlab", "-w"],
            capture_output=True, text=True, check=False,
        )
    except FileNotFoundError:
        return None
    return out.stdout.strip() or None


class GitLab:
    def __init__(self, token: str):
        self.token = token
        self.project = urllib.parse.quote(PROJECT_PATH, safe="")

    def request(self, method: str, path: str, data: dict | None = None, params: dict | None = None):
        url = f"{HOST}/api/v4/{path}"
        if params:
            url += "?" + urllib.parse.urlencode(params, doseq=True)
        body = json.dumps(data).encode() if data is not None else None
        req = urllib.request.Request(url, data=body, method=method)
        req.add_header("PRIVATE-TOKEN", self.token)
        if body is not None:
            req.add_header("Content-Type", "application/json")
        try:
            with urllib.request.urlopen(req, timeout=30) as resp:
                raw = resp.read()
                return resp.status, (json.loads(raw) if raw else None), resp.headers
        except urllib.error.HTTPError as err:
            raw = err.read()
            try:
                payload = json.loads(raw) if raw else None
            except json.JSONDecodeError:
                payload = raw.decode(errors="replace")[:200]
            return err.code, payload, err.headers

    def project_path(self, suffix: str) -> str:
        return f"projects/{self.project}/{suffix}"

    def get_all(self, suffix: str, params: dict | None = None) -> list:
        items, page = [], 1
        while True:
            status, payload, headers = self.request(
                "GET", self.project_path(suffix), params={**(params or {}), "per_page": 100, "page": page}
            )
            if status != 200:
                raise SystemExit(f"GET {suffix} failed with {status}: {payload}")
            items.extend(payload)
            next_page = headers.get("X-Next-Page") if headers else None
            if not next_page:
                return items
            page = int(next_page)

    def post(self, suffix: str, data: dict):
        return self.request("POST", self.project_path(suffix), data=data)

    def put(self, suffix: str, data: dict):
        return self.request("PUT", self.project_path(suffix), data=data)


def issues_by_ticket_id(gl: GitLab) -> dict[str, dict]:
    found = {}
    for issue in gl.get_all("issues", {"state": "all"}):
        match = TITLE_ID_RE.match(issue["title"])
        if match:
            found[match.group(1)] = issue
    return found


def user_ids(gl: GitLab, usernames) -> dict[str, int]:
    ids = {}
    for username in usernames:
        status, payload, _ = gl.request("GET", "users", params={"username": username})
        if status == 200 and payload:
            ids[username] = payload[0]["id"]
        else:
            print(f"warn: GitLab user {username!r} not found; issues stay unassigned", file=sys.stderr)
    return ids


def ensure_boards(gl: GitLab, roster: dict[str, str], uid: dict[str, int], apply: bool) -> list[str]:
    """Team board, one board per person, and a board with one list per person. Returns board links."""
    labels = {l["name"]: l["id"] for l in gl.get_all("labels")}
    boards = {b["name"]: b for b in gl.get_all("boards")}
    wanted = [("FNDR team", None)] + [(name, username) for username, name in roster.items()] + [("People", None)]
    links = []
    for name, username in wanted:
        board = boards.get(name)
        if board is None:
            if not apply:
                links.append(f"would create board {name!r}")
                continue
            status, board, _ = gl.post("boards", {"name": name})
            if status not in (200, 201):
                print(f"FAIL create board {name}: {status} {board}", file=sys.stderr)
                continue
        list_labels = (
            [f"owner::{n.lower()}" for n in roster.values()] if name == "People" else [f"status::{s}" for s in STATUSES]
        )
        existing = {lst.get("label", {}).get("name") for lst in board.get("lists", []) if lst.get("label")}
        for label in list_labels:
            if label not in existing and apply and label in labels:
                gl.post(f"boards/{board['id']}/lists", {"label_id": labels[label]})
        scoped = False
        if username and username in uid and apply:
            status, updated, _ = gl.put(f"boards/{board['id']}", {"assignee_id": uid[username]})
            scoped = status == 200 and bool((updated or {}).get("assignee"))
        links.append(f"{name}: {board_url(board['id'], scoped, username)}")
    return links


def sync(apply: bool, update: bool) -> int:
    roster = load_roster()
    tickets = load_all_tickets()
    errors = validate(tickets, roster)
    if errors:
        print("\n".join(errors), file=sys.stderr)
        return 1
    token = read_token()
    if not token:
        print("No token: set GITLAB_TOKEN or create the Keychain item 'fndr-gitlab'. Showing the offline plan.")
        return plan()
    gl = GitLab(token)
    mode = "APPLY" if apply else "DRY RUN"
    print(f"{mode} against {HOST}/{PROJECT_PATH}")

    labels = {l["name"] for l in gl.get_all("labels")}
    needed = set(LABEL_COLORS) | {f"owner::{n.lower()}" for n in roster.values()}
    for name in sorted(needed - labels):
        print(f"label   + {name}")
        if apply:
            gl.post("labels", {"name": name, "color": LABEL_COLORS.get(name, "#428BCA")})

    milestones = {m["title"]: m["id"] for m in gl.get_all("milestones", {"state": "active"})}
    for title, (start, due) in MILESTONES.items():
        if title not in milestones:
            print(f"milestone + {title}")
            if apply:
                _, created, _ = gl.post("milestones", {"title": title, "start_date": start, "due_date": due})
                milestones[title] = (created or {}).get("id")

    uid = user_ids(gl, roster)
    existing = issues_by_ticket_id(gl)
    iids = {tid: issue["iid"] for tid, issue in existing.items()}
    created = updated = 0
    for t in tickets:
        if t.id in existing:
            if update:
                print(f"issue   ~ {issue_title(t)}")
                if apply:
                    gl.put(f"issues/{existing[t.id]['iid']}", {"description": render_description(t, iids)})
                updated += 1
            continue
        print(f"issue   + {issue_title(t)}  -> {t.assignee}, {t.milestone}")
        created += 1
        if apply:
            payload = {
                "title": issue_title(t),
                "description": render_description(t, iids),
                "labels": ",".join(create_labels(t, roster)),
                "milestone_id": milestones.get(t.milestone),
                "assignee_ids": [uid[t.assignee]] if t.assignee in uid else [],
            }
            status, issue, _ = gl.post("issues", payload)
            if status in (200, 201):
                iids[t.id] = issue["iid"]
            else:
                print(f"FAIL {t.id}: {status} {issue}", file=sys.stderr)
            time.sleep(0.1)

    if apply and created:
        # Second pass so dependency references point at real issue numbers.
        for t in tickets:
            if t.id in iids and t.depends:
                gl.put(f"issues/{iids[t.id]}", {"description": render_description(t, iids)})

    hubs = {i["title"]: i for i in gl.get_all("issues", {"labels": "lane-hub", "state": "all"})}
    for username, name in roster.items():
        mine = [t for t in tickets if t.assignee == username]
        title = HUB_TITLE.format(name=name)
        description = render_hub(name, mine, iids)
        print(f"hub     {'~' if title in hubs else '+'} {title} ({len(mine)} tickets)")
        if not apply:
            continue
        if title in hubs:
            gl.put(f"issues/{hubs[title]['iid']}", {"description": description})
        else:
            gl.post("issues", {
                "title": title, "description": description,
                "labels": f"lane-hub,{owner_label(username, roster)}",
                "assignee_ids": [uid[username]] if username in uid else [],
            })

    for link in ensure_boards(gl, roster, uid, apply):
        print(f"board   {link}")
    print(f"{created} issues to create, {updated} to update" + ("" if apply else " (dry run; add --apply)"))
    return 0


def plan() -> int:
    roster = load_roster()
    tickets = load_all_tickets()
    errors = validate(tickets, roster)
    if errors:
        print("\n".join(errors), file=sys.stderr)
        return 1
    print(f"{len(tickets)} tickets in {TICKET_DIR.relative_to(REPO)}")
    print(f"{'person':8} {'tickets':>7} {'hours':>6} {'p0':>5} {'p1':>5} {'p2':>5}  " + "  ".join(MILESTONES))
    for username, name in roster.items():
        mine = [t for t in tickets if t.assignee == username]
        by_prio = {p: sum(t.hours for t in mine if t.prio == f"prio::{p}") for p in ("p0", "p1", "p2")}
        by_week = [sum(t.hours for t in mine if t.milestone == m) for m in MILESTONES]
        print(
            f"{name:8} {len(mine):7} {sum(t.hours for t in mine):6g} "
            f"{by_prio['p0']:5g} {by_prio['p1']:5g} {by_prio['p2']:5g}  "
            + "  ".join(f"{h:>11g}" for h in by_week)
        )
    return 0


# ---------------------------------------------------------------- agent commands


def find_issue(gl: GitLab, ticket_id: str) -> dict:
    issue = issues_by_ticket_id(gl).get(ticket_id)
    if not issue:
        raise SystemExit(f"No GitLab issue titled [{ticket_id}]")
    return issue


def require_own(gl: GitLab, issue: dict, allow_any: bool) -> None:
    if allow_any:
        return
    _, me, _ = gl.request("GET", "user")
    assignees = {a["username"] for a in issue.get("assignees", [])}
    if me and me.get("username") not in assignees:
        raise SystemExit(f"{issue['title']} is assigned to {sorted(assignees)}; pass --any to act on it anyway")


def cmd_move(args) -> int:
    gl = GitLab(read_token() or sys.exit("No token"))
    issue = find_issue(gl, args.ticket)
    require_own(gl, issue, args.any)
    if args.status == "closed":
        gl.put(f"issues/{issue['iid']}", {"state_event": "close"})
    else:
        add, remove = status_change(issue["labels"], args.status)
        gl.put(f"issues/{issue['iid']}", {"add_labels": ",".join(add), "remove_labels": ",".join(remove)})
    print(f"{issue['title']} -> {args.status}")
    return 0


def cmd_comment(args) -> int:
    gl = GitLab(read_token() or sys.exit("No token"))
    issue = find_issue(gl, args.ticket)
    require_own(gl, issue, args.any)
    status, _, _ = gl.post(f"issues/{issue['iid']}/notes", {"body": args.text})
    print(f"comment on {issue['title']}: {status}")
    return 0 if status in (200, 201) else 1


def cmd_list(args) -> int:
    gl = GitLab(read_token() or sys.exit("No token"))
    params = {"state": "opened"}
    if args.user:
        params["assignee_username"] = args.user
    else:
        _, me, _ = gl.request("GET", "user")
        params["assignee_username"] = me["username"]
    if args.status:
        params["labels"] = f"status::{args.status}"
    for issue in gl.get_all("issues", params):
        status = next((l.split("::")[1] for l in issue["labels"] if l.startswith("status::")), "none")
        milestone = (issue.get("milestone") or {}).get("title", "")
        print(f"{status:9} {milestone:12} {issue['title']}  {issue['web_url']}")
    return 0


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description="FNDR GitLab ticket sync and board moves")
    sub = parser.add_subparsers(dest="cmd", required=True)
    sub.add_parser("plan")
    p_sync = sub.add_parser("sync")
    p_sync.add_argument("--apply", action="store_true")
    p_sync.add_argument("--update", action="store_true")
    p_list = sub.add_parser("list")
    p_list.add_argument("--user")
    p_list.add_argument("--status", choices=STATUSES)
    p_move = sub.add_parser("move")
    p_move.add_argument("ticket")
    p_move.add_argument("status", choices=(*STATUSES, "closed"))
    p_move.add_argument("--any", action="store_true")
    p_comment = sub.add_parser("comment")
    p_comment.add_argument("ticket")
    p_comment.add_argument("text")
    p_comment.add_argument("--any", action="store_true")
    args = parser.parse_args(argv)
    if args.cmd == "plan":
        return plan()
    if args.cmd == "sync":
        return sync(args.apply, args.update)
    return {"list": cmd_list, "move": cmd_move, "comment": cmd_comment}[args.cmd](args)


if __name__ == "__main__":
    sys.exit(main())
