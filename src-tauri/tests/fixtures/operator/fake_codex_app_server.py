#!/usr/bin/env python3
"""A scripted stand-in for `codex app-server` (JSONL over stdio).

The planner turn (input starting with "Request:") answers with a three-step
plan; with "PROBE" in it, the planner first asks to read the screen. A turn
containing "REWORDED_APPROVAL" asks with approval wording FNDR cannot parse. Any other turn asks for three computer-use approvals in order: reading
Spotify, clicking its "Buy Premium" button, and typing into Notes, then
reports done. Every approval answer is appended to the log file passed as
FAKE_CODEX_LOG (one JSON line: request id and the answer).
"""
import json
import os
import sys

LOG = os.environ.get("FAKE_CODEX_LOG")
TREE = "App=com.spotify.client (pid 1)\nWindow: \"Spotify\", App: Spotify.\n0 standard window Spotify\n\t1 search text field What do you want to play?\n\t2 button Play\n\t3 button Buy Premium\n"
PLAN = {"steps": [
    {"action": "open_app", "label": "Open Spotify", "app": "Spotify", "url": "", "goal": "", "check": "frontmost"},
    {"action": "operate", "label": "Play the song", "app": "Spotify", "url": "", "goal": "play Blinding Lights", "check": "media_playing"},
    {"action": "open_url", "label": "Search the web", "app": "", "url": "https://www.google.com/search?q=looped+transformers", "goal": "", "check": "page_loaded"},
]}
APPROVALS = [
    (900, "get_app_state", {"app": "Spotify"}),
    (901, "click", {"app": "Spotify", "element_index": "3"}),
    (902, "type_text", {"app": "Notes", "text": "hello"}),
]


def send(message):
    sys.stdout.write(json.dumps(message) + "\n")
    sys.stdout.flush()


def read():
    line = sys.stdin.readline()
    if not line:
        sys.exit(0)
    return json.loads(line)


def wait_for_answer(request_id):
    while True:
        message = read()
        if message.get("id") == request_id and "method" not in message:
            if LOG:
                with open(LOG, "a") as log:
                    log.write(json.dumps({"id": request_id, "answer": message.get("result", {}).get("action")}) + "\n")
            return message


def complete(thread, turn, text):
    send({"method": "item/completed", "params": {"threadId": thread, "item": {"type": "agentMessage", "id": "m-" + turn, "text": text, "phase": "final_answer"}}})
    send({"method": "turn/completed", "params": {"threadId": thread, "turn": {"id": turn, "status": "completed", "error": None}}})


def ask(approval_id, thread, turn, tool, args, message):
    """One approval request; the tool runs only when FNDR accepts."""
    send({"id": approval_id, "method": "mcpServer/elicitation/request", "params": {
        "threadId": thread, "turnId": turn, "serverName": "fndr_computer", "mode": "form",
        "_meta": {"codex_approval_kind": "mcp_tool_call", "tool_params": args},
        "message": message,
        "requestedSchema": {"type": "object", "properties": {}}}})
    answer = wait_for_answer(approval_id)
    if answer.get("result", {}).get("action") == "accept":
        item = {"type": "mcpToolCall", "id": "call-%d" % approval_id, "server": "fndr_computer", "tool": tool, "arguments": args}
        send({"method": "item/started", "params": {"threadId": thread, "item": dict(item, status="inProgress")}})
        result = {"content": [{"type": "text", "text": TREE if tool == "get_app_state" else "ok"}], "isError": False}
        send({"method": "item/completed", "params": {"threadId": thread, "item": dict(item, status="completed", result=result, error=None)}})


threads = 0
turns = 0
while True:
    message = read()
    method, request_id, params = message.get("method"), message.get("id"), message.get("params") or {}
    if request_id is None:
        continue
    if method == "initialize":
        send({"id": request_id, "result": {"userAgent": "fake"}})
    elif method == "account/read":
        send({"id": request_id, "result": {"account": {"type": "chatgpt", "email": "test@example.com"}}})
    elif method == "thread/start":
        threads += 1
        send({"id": request_id, "result": {"thread": {"id": "thread-%d" % threads}}})
    elif method == "turn/start":
        turns += 1
        turn, thread = "turn-%d" % turns, params["threadId"]
        send({"id": request_id, "result": {"turn": {"id": turn, "status": "inProgress"}}})
        text = params["input"][0]["text"]
        if text.startswith("Request:"):
            if "PROBE" in text:
                # A planner that tries to look at the screen before planning.
                ask(960, thread, turn, "get_app_state", {"app": "Spotify"},
                    'Allow the fndr_computer MCP server to run tool "get_app_state"?')
            complete(thread, turn, json.dumps(PLAN))
            continue
        if "REWORDED_APPROVAL" in text:
            # A Codex release whose approval wording FNDR does not know.
            ask(970, thread, turn, "get_app_state", {"app": "Spotify"}, "May fndr_computer proceed?")
            complete(thread, turn, json.dumps({"done": False, "detail": "refused"}))
            continue
        if "AUTOMATION_DENIED" in text:
            # The bundled Computer Use without Automation permission.
            send({"id": 950, "method": "mcpServer/elicitation/request", "params": {
                "threadId": thread, "turnId": turn, "serverName": "fndr_computer", "mode": "form",
                "_meta": {"codex_approval_kind": "mcp_tool_call", "tool_params": {"app": "Spotify"}},
                "message": 'Allow the fndr_computer MCP server to run tool "get_app_state"?',
                "requestedSchema": {"type": "object", "properties": {}}}})
            wait_for_answer(950)
            item = {"type": "mcpToolCall", "id": "call-950", "server": "fndr_computer", "tool": "get_app_state", "arguments": {"app": "Spotify"}}
            send({"method": "item/started", "params": {"threadId": thread, "item": dict(item, status="inProgress")}})
            result = {"content": [{"type": "text", "text": "Computer Use server error -1743: Unknown error"}], "isError": True}
            send({"method": "item/completed", "params": {"threadId": thread, "item": dict(item, status="completed", result=result, error=None)}})
            continue  # never finishes the turn on its own, like the real retry loop
        for approval_id, tool, args in APPROVALS:
            send({"id": approval_id, "method": "mcpServer/elicitation/request", "params": {
                "threadId": thread, "turnId": turn, "serverName": "fndr_computer", "mode": "form",
                "_meta": {"codex_approval_kind": "mcp_tool_call", "tool_params": args},
                "message": 'Allow the fndr_computer MCP server to run tool "%s"?' % tool,
                "requestedSchema": {"type": "object", "properties": {}}}})
            answer = wait_for_answer(approval_id)
            if answer.get("result", {}).get("action") == "accept":
                item = {"type": "mcpToolCall", "id": "call-%d" % approval_id, "server": "fndr_computer", "tool": tool, "arguments": args}
                send({"method": "item/started", "params": {"threadId": thread, "item": dict(item, status="inProgress")}})
                result = {"content": [{"type": "text", "text": TREE if tool == "get_app_state" else "ok"}], "isError": False}
                send({"method": "item/completed", "params": {"threadId": thread, "item": dict(item, status="completed", result=result, error=None)}})
        complete(thread, turn, json.dumps({"done": True, "detail": "Playing"}))
    else:
        send({"id": request_id, "result": {}})
