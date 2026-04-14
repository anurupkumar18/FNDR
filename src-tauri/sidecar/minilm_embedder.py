#!/usr/bin/env python3
"""Local all-MiniLM-L6-v2 embedding sidecar.

Usage:
  python3 minilm_embedder.py --ping
  python3 minilm_embedder.py --embed < request.json

Input JSON:
  {"texts": ["...", "..."]}

Output JSON:
  {"embeddings": [[...384 floats...], ...]}
"""

from __future__ import annotations

import argparse
import json
import os
import socket
import sys
import tempfile
import time
import subprocess
from typing import Any, Dict, List

_MODEL = None
_MODEL_NAME = "sentence-transformers/all-MiniLM-L6-v2"
_SOCKET_PATH = os.path.join(tempfile.gettempdir(), "fndr_minilm_embedder.sock")


def _load_model():
    global _MODEL
    if _MODEL is not None:
        return _MODEL

    # Keep CPU deterministic and lightweight for local runtime.
    os.environ.setdefault("TOKENIZERS_PARALLELISM", "false")

    try:
        from sentence_transformers import SentenceTransformer
    except Exception as exc:  # pragma: no cover - runtime dependency path
        raise RuntimeError(
            "sentence-transformers is required for MiniLM embeddings"
        ) from exc

    _MODEL = SentenceTransformer(_MODEL_NAME, device="cpu")
    return _MODEL


def _embed_texts(texts: List[str]) -> List[List[float]]:
    model = _load_model()
    vectors = model.encode(
        texts,
        normalize_embeddings=True,
        convert_to_numpy=True,
        show_progress_bar=False,
    )

    out: List[List[float]] = []
    for row in vectors.tolist():
        if len(row) != 384:
            raise RuntimeError(f"Unexpected embedding dim {len(row)}; expected 384")
        out.append([float(x) for x in row])
    return out


def _read_json_stdin() -> Dict[str, Any]:
    raw = sys.stdin.buffer.read()
    if not raw:
        raise RuntimeError("No JSON payload received on stdin")
    try:
        return json.loads(raw.decode("utf-8"))
    except Exception as exc:
        raise RuntimeError("Invalid JSON payload") from exc


def cmd_ping() -> int:
    _load_model()
    sys.stdout.write("ok\n")
    return 0


def cmd_embed() -> int:
    payload = _read_json_stdin()
    texts = payload.get("texts", [])
    if not isinstance(texts, list):
        raise RuntimeError("Field 'texts' must be an array")

    normalized = [str(item) for item in texts]
    embeddings = _embed_texts(normalized)
    sys.stdout.write(json.dumps({"embeddings": embeddings}))
    return 0


def _request_daemon(payload: Dict[str, Any], timeout_sec: float = 2.5) -> Dict[str, Any]:
    raw = json.dumps(payload).encode("utf-8") + b"\n"
    with socket.socket(socket.AF_UNIX, socket.SOCK_STREAM) as sock:
        sock.settimeout(timeout_sec)
        sock.connect(_SOCKET_PATH)
        sock.sendall(raw)

        chunks = bytearray()
        while True:
            part = sock.recv(8192)
            if not part:
                break
            chunks.extend(part)
            if b"\n" in part:
                break

    line = bytes(chunks).split(b"\n", 1)[0].strip()
    if not line:
        raise RuntimeError("Empty response from embedding daemon")
    return json.loads(line.decode("utf-8"))


def _spawn_daemon() -> None:
    subprocess.Popen(
        [sys.executable, os.path.abspath(__file__), "--serve-daemon"],
        stdin=subprocess.DEVNULL,
        stdout=subprocess.DEVNULL,
        stderr=subprocess.DEVNULL,
        start_new_session=True,
        close_fds=True,
    )


def cmd_embed_via_daemon() -> int:
    payload = _read_json_stdin()
    texts = payload.get("texts", [])
    if not isinstance(texts, list):
        raise RuntimeError("Field 'texts' must be an array")
    normalized = [str(item) for item in texts]
    request = {"texts": normalized}

    # Fast path: daemon already available.
    try:
        response = _request_daemon(request)
    except Exception:
        _spawn_daemon()
        # Retry for cold start.
        last_exc: Exception | None = None
        for _ in range(30):
            try:
                response = _request_daemon(request, timeout_sec=4.0)
                break
            except Exception as exc:  # pragma: no cover - startup race
                last_exc = exc
                time.sleep(0.1)
        else:
            raise RuntimeError(f"Embedding daemon unavailable: {last_exc}")

    if "error" in response:
        raise RuntimeError(str(response["error"]))
    embeddings = response.get("embeddings")
    if not isinstance(embeddings, list):
        raise RuntimeError("Invalid embedding daemon response")

    sys.stdout.write(json.dumps({"embeddings": embeddings}))
    return 0


def cmd_serve_daemon() -> int:
    # Load once; keep model hot for all future requests.
    _load_model()
    try:
        if os.path.exists(_SOCKET_PATH):
            os.unlink(_SOCKET_PATH)
    except OSError:
        pass

    server = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
    server.bind(_SOCKET_PATH)
    os.chmod(_SOCKET_PATH, 0o600)
    server.listen(8)

    while True:
        conn, _ = server.accept()
        with conn:
            try:
                data = b""
                while not data.endswith(b"\n"):
                    part = conn.recv(8192)
                    if not part:
                        break
                    data += part

                payload = json.loads(data.decode("utf-8").strip() or "{}")
                texts = payload.get("texts", [])
                if not isinstance(texts, list):
                    raise RuntimeError("Field 'texts' must be an array")
                normalized = [str(item) for item in texts]
                embeddings = _embed_texts(normalized)
                response = {"embeddings": embeddings}
            except Exception as exc:  # pragma: no cover - daemon runtime path
                response = {"error": str(exc)}
            conn.sendall(json.dumps(response).encode("utf-8") + b"\n")
    return 0


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--ping", action="store_true")
    parser.add_argument("--embed", action="store_true")
    parser.add_argument("--embed-daemon", action="store_true")
    parser.add_argument("--serve-daemon", action="store_true")
    args = parser.parse_args()

    mode_count = sum(
        [bool(args.ping), bool(args.embed), bool(args.embed_daemon), bool(args.serve_daemon)]
    )
    if mode_count != 1:
        raise RuntimeError(
            "Specify exactly one of --ping, --embed, --embed-daemon, --serve-daemon"
        )

    if args.ping:
        return cmd_ping()
    if args.embed:
        return cmd_embed()
    if args.embed_daemon:
        return cmd_embed_via_daemon()
    return cmd_serve_daemon()


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except Exception as exc:  # pragma: no cover - surfaced to Rust stderr
        sys.stderr.write(f"minilm_embedder error: {exc}\n")
        raise SystemExit(1)
