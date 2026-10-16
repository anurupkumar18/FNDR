#!/usr/bin/env python3
"""A scripted stand-in for `codex app-server` (JSONL over stdio).

The planner turn (input starting with "Request:") answers with a three-step
plan; with "PROBE" in it, the planner first asks to read the screen. A turn
containing "REWORDED_APPROVAL" asks with approval wording FNDR cannot parse. Any other turn asks for three computer-use approvals in order: reading
Spotify, clicking its "Buy Premium" button, and typing into Notes, then
reports done. Every approval answer is appended to the log file passed as
FAKE_CODEX_LOG (one JSON line: request id and the answer).

Realtime voice (`thread/realtime/*`, Codex 0.162 shapes) answers only after
`initialize` opted into `experimentalApi`, like the real server. Arguments
after `app-server` steer it: `fake:signed_out`, `fake:usage=<percent>`,
`fake:no_realtime` (an older Codex), `fake:no_sdp` (the answer never comes),
`fake:drop_after_speak` (exits after the first appendSpeech) and
`fake:log=<path>` (one JSON line per realtime request: method and params).
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


FLAGS = [arg[len("fake:"):] for arg in sys.argv[1:] if arg.startswith("fake:")]


def flag_value(name):
    for item in FLAGS:
        if item.startswith(name + "="):
            return item[len(name) + 1:]
    return None


REALTIME_LOG = flag_value("log")
VOICES = {"v1": ["juniper", "cove"], "v2": ["alloy", "marin"], "defaultV1": "cove", "defaultV2": "marin"}


def log_realtime(method, params):
    if REALTIME_LOG:
        with open(REALTIME_LOG, "a") as log:
            log.write(json.dumps({"method": method, "params": params}) + "\n")


def realtime(request_id, method, params):
    """One thread/realtime/* request, answered the way Codex 0.162 does."""
    log_realtime(method, params)
    if "no_realtime" in FLAGS or not experimental:
        send({"id": request_id, "error": {"code": -32601, "message": "unknown method " + method}})
        return
    thread = params.get("threadId", "")
    if method == "thread/realtime/listVoices":
        send({"id": request_id, "result": {"voices": VOICES}})
    elif method == "thread/realtime/start":
        send({"id": request_id, "result": {}})
        send({"method": "thread/realtime/started", "params": {"threadId": thread, "realtimeSessionId": thread, "version": "v3"}})
        if "no_sdp" not in FLAGS:
            send({"method": "thread/realtime/sdp", "params": {"threadId": thread, "sdp": "v=0\r\nfake-answer\r\n"}})
    elif method == "thread/realtime/appendSpeech":
        send({"id": request_id, "result": {}})
        if "drop_after_speak" in FLAGS:
            send({"method": "thread/realtime/closed", "params": {"threadId": thread, "reason": "transport closed"}})
            sys.exit(0)
    elif method == "thread/realtime/stop":
        send({"id": request_id, "result": {}})
        send({"method": "thread/realtime/closed", "params": {"threadId": thread, "reason": "stopped"}})
    else:
        send({"id": request_id, "result": {}})


threads = 0
turns = 0
experimental = False
while True:
    message = read()
    method, request_id, params = message.get("method"), message.get("id"), message.get("params") or {}
    if request_id is None:
        continue
    if method == "initialize":
        experimental = bool((params.get("capabilities") or {}).get("experimentalApi"))
        send({"id": request_id, "result": {"userAgent": "fake"}})
    elif method and method.startswith("thread/realtime/"):
        realtime(request_id, method, params)
    elif method == "account/read":
        if "signed_out" in FLAGS:
            send({"id": request_id, "result": {"account": None, "requiresOpenaiAuth": True}})
        else:
            send({"id": request_id, "result": {"account": {"type": "chatgpt", "email": "test@example.com"}}})
    elif method == "account/rateLimits/read":
        used = float(flag_value("usage") or 20)
        send({"id": request_id, "result": {"rateLimits": {
            "primary": {"usedPercent": used, "windowDurationMins": 300, "resetsAt": 1791599030},
            "secondary": {"usedPercent": 10, "windowDurationMins": 10080, "resetsAt": 1791907651},
            "rateLimitReachedType": "primary" if used >= 100 else None}}})
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
        if "PERSISTS_AFTER_NO" in text:
            # After a no, the model tries other routes to the same end
            # (seen live on 2026-10-08: letter keys, then set_value).
            allow = 'Allow the fndr_computer MCP server to run tool "%s"?'
            ask(980, thread, turn, "type_text", {"app": "Notes", "text": "hello"}, allow % "type_text")
            ask(981, thread, turn, "press_key", {"app": "Notes", "key": "h"}, allow % "press_key")
            ask(982, thread, turn, "set_value", {"app": "Notes", "element_index": "4", "value": "hello"}, allow % "set_value")
            complete(thread, turn, json.dumps({"done": False, "detail": "declined"}))
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
