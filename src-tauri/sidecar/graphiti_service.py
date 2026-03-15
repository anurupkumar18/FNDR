#!/usr/bin/env python3
"""
Graphiti Knowledge Graph Service for FNDR.

Long-lived sidecar process that wraps Graphiti's API.
Communicates via JSON-RPC over stdin/stdout.

Requires:
  pip install graphiti-core[falkordb,anthropic]

Start FalkorDB:
  docker run -p 6379:6379 -p 3000:3000 -it --rm falkordb/falkordb:latest
"""

import sys
import json
import asyncio
import os
import signal
from datetime import datetime, timezone
from typing import Optional


async def create_graphiti_client():
    """Create and initialize a Graphiti client with FalkorDB."""
    try:
        from graphiti_core import Graphiti
        from graphiti_core.llm_client import AnthropicClient
    except ImportError:
        return None, "graphiti-core not installed. Run: pip install graphiti-core[falkordb,anthropic]"

    # FalkorDB connection (default Docker port)
    falkordb_host = os.environ.get("FALKORDB_HOST", "localhost")
    falkordb_port = int(os.environ.get("FALKORDB_PORT", "6379"))

    anthropic_key = os.environ.get("ANTHROPIC_API_KEY", "")
    if not anthropic_key:
        return None, "ANTHROPIC_API_KEY not set"

    try:
        llm_client = AnthropicClient(api_key=anthropic_key)

        graphiti = Graphiti(
            f"bolt://{falkordb_host}:{falkordb_port}",
            llm_client=llm_client,
        )

        await graphiti.build_indices_and_constraints()
        return graphiti, None
    except Exception as e:
        return None, f"Graphiti init failed: {e}"


def send_response(request_id: str, result=None, error=None):
    """Send a JSON-RPC response to stdout."""
    response = {"jsonrpc": "2.0", "id": request_id}
    if error:
        response["error"] = {"code": -1, "message": str(error)}
    else:
        response["result"] = result
    print(json.dumps(response), flush=True)


async def handle_ingest(graphiti, params: dict) -> dict:
    """Ingest a memory record as a Graphiti episode."""
    from graphiti_core.nodes import EpisodeType

    text = params.get("text", "")
    app_name = params.get("app_name", "unknown")
    window_title = params.get("window_title", "")
    timestamp = params.get("timestamp", None)
    record_id = params.get("id", "")

    episode_body = f"[{app_name}: {window_title}] {text}"

    ts = datetime.now(timezone.utc)
    if timestamp:
        ts = datetime.fromtimestamp(timestamp / 1000.0, tz=timezone.utc)

    await graphiti.add_episode(
        name=f"screen_capture_{record_id}",
        episode_body=episode_body,
        source=EpisodeType.text,
        source_description=f"Screen capture from {app_name}",
        reference_time=ts,
    )

    return {"status": "ok", "id": record_id}


async def handle_search(graphiti, params: dict) -> dict:
    """Search the knowledge graph."""
    query = params.get("query", "")
    limit = params.get("limit", 10)

    results = await graphiti.search(query, num_results=limit)

    formatted = []
    for edge in results:
        formatted.append({
            "fact": edge.fact if hasattr(edge, "fact") else str(edge),
            "source_name": edge.source_node.name if hasattr(edge, "source_node") and edge.source_node else "",
            "target_name": edge.target_node.name if hasattr(edge, "target_node") and edge.target_node else "",
            "created_at": edge.created_at.isoformat() if hasattr(edge, "created_at") and edge.created_at else "",
            "uuid": str(edge.uuid) if hasattr(edge, "uuid") else "",
        })

    return {"results": formatted, "count": len(formatted)}


async def handle_get_graph_data(graphiti, params: dict) -> dict:
    """
    Return all nodes and edges for graph visualization.
    Falls back to the local JSON graph if Graphiti is unavailable.
    """
    try:
        # Try to get recent edges which include node references
        results = await graphiti.search("*", num_results=200)

        nodes = {}
        edges = []

        for edge in results:
            if hasattr(edge, "source_node") and edge.source_node:
                src = edge.source_node
                node_id = str(src.uuid) if hasattr(src, "uuid") else src.name
                if node_id not in nodes:
                    nodes[node_id] = {
                        "id": node_id,
                        "label": src.name if hasattr(src, "name") else node_id,
                        "type": "Entity",
                    }

            if hasattr(edge, "target_node") and edge.target_node:
                tgt = edge.target_node
                node_id = str(tgt.uuid) if hasattr(tgt, "uuid") else tgt.name
                if node_id not in nodes:
                    nodes[node_id] = {
                        "id": node_id,
                        "label": tgt.name if hasattr(tgt, "name") else node_id,
                        "type": "Entity",
                    }

            if hasattr(edge, "source_node") and hasattr(edge, "target_node"):
                src_id = str(edge.source_node.uuid) if hasattr(edge.source_node, "uuid") else edge.source_node.name
                tgt_id = str(edge.target_node.uuid) if hasattr(edge.target_node, "uuid") else edge.target_node.name
                edges.append({
                    "id": str(edge.uuid) if hasattr(edge, "uuid") else f"{src_id}-{tgt_id}",
                    "source": src_id,
                    "target": tgt_id,
                    "label": edge.fact if hasattr(edge, "fact") else "",
                    "type": "RELATES_TO",
                })

        return {
            "nodes": list(nodes.values()),
            "edges": edges,
        }
    except Exception as e:
        return {"nodes": [], "edges": [], "error": str(e)}


async def main():
    """Main event loop: read JSON-RPC requests from stdin, dispatch, respond."""
    graphiti = None
    init_error = None

    # Try to initialize Graphiti
    graphiti, init_error = await create_graphiti_client()

    if init_error:
        # Send initialization status but don't exit — we can still serve
        # the local graph fallback
        print(json.dumps({
            "jsonrpc": "2.0",
            "id": "init",
            "result": {"status": "degraded", "message": init_error}
        }), flush=True)
    else:
        print(json.dumps({
            "jsonrpc": "2.0",
            "id": "init",
            "result": {"status": "ok", "message": "Graphiti service initialized"}
        }), flush=True)

    # Read commands from stdin
    loop = asyncio.get_event_loop()

    for line in sys.stdin:
        line = line.strip()
        if not line:
            continue

        try:
            request = json.loads(line)
        except json.JSONDecodeError:
            continue

        req_id = request.get("id", "unknown")
        method = request.get("method", "")
        params = request.get("params", {})

        if method == "ping":
            send_response(req_id, {"status": "pong"})

        elif method == "ingest":
            if graphiti is None:
                send_response(req_id, error=f"Graphiti unavailable: {init_error}")
            else:
                try:
                    result = await handle_ingest(graphiti, params)
                    send_response(req_id, result)
                except Exception as e:
                    send_response(req_id, error=str(e))

        elif method == "search":
            if graphiti is None:
                send_response(req_id, error=f"Graphiti unavailable: {init_error}")
            else:
                try:
                    result = await handle_search(graphiti, params)
                    send_response(req_id, result)
                except Exception as e:
                    send_response(req_id, error=str(e))

        elif method == "get_graph_data":
            if graphiti is None:
                send_response(req_id, {"nodes": [], "edges": [], "fallback": True})
            else:
                try:
                    result = await handle_get_graph_data(graphiti, params)
                    send_response(req_id, result)
                except Exception as e:
                    send_response(req_id, error=str(e))

        elif method == "shutdown":
            if graphiti:
                await graphiti.close()
            send_response(req_id, {"status": "shutdown"})
            break

        else:
            send_response(req_id, error=f"Unknown method: {method}")


if __name__ == "__main__":
    try:
        asyncio.run(main())
    except KeyboardInterrupt:
        sys.exit(0)
