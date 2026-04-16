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
import fcntl
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
_LOCK_PATH = os.path.join(tempfile.gettempdir(), "fndr_minilm_embedder.lock")
_DAEMON_LOG_PATH = os.path.join(tempfile.gettempdir(), "fndr_minilm_embedder.log")
_REQUEST_TIMEOUT_SEC = float(os.environ.get("FNDR_EMBEDDER_REQUEST_TIMEOUT_SEC", "30"))
_IDLE_TIMEOUT_SEC = float(os.environ.get("FNDR_EMBEDDER_IDLE_TIMEOUT_SEC", "120"))


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


def _request_daemon(payload: Dict[str, Any], timeout_sec: float = _REQUEST_TIMEOUT_SEC) -> Dict[str, Any]:
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


def _daemon_running() -> bool:
    lock_fd = os.open(_LOCK_PATH, os.O_CREAT | os.O_RDWR, 0o600)
    try:
        fcntl.flock(lock_fd, fcntl.LOCK_EX | fcntl.LOCK_NB)
        fcntl.flock(lock_fd, fcntl.LOCK_UN)
        return False
    except OSError:
        return True
    finally:
        os.close(lock_fd)


def _spawn_daemon() -> None:
    os.makedirs(os.path.dirname(_DAEMON_LOG_PATH), exist_ok=True)
    with open(_DAEMON_LOG_PATH, "ab", buffering=0) as log_file:
        subprocess.Popen(
            [sys.executable, os.path.abspath(__file__), "--serve-daemon"],
            stdin=subprocess.DEVNULL,
            stdout=log_file,
            stderr=log_file,
            start_new_session=True,
            close_fds=True,
        )


def _spawn_daemon_if_needed() -> None:
    if _daemon_running():
        return
    _spawn_daemon()


def cmd_embed_via_daemon() -> int:
    payload = _read_json_stdin()
    texts = payload.get("texts", [])
    if not isinstance(texts, list):
        raise RuntimeError("Field 'texts' must be an array")
    normalized = [str(item) for item in texts]
    request = {"texts": normalized}

    last_exc: Exception | None = None
    for attempt in range(45):
        if attempt in (0, 1, 10):
            _spawn_daemon_if_needed()
        try:
            response = _request_daemon(request)
            break
        except Exception as exc:  # pragma: no cover - runtime race/retry path
            last_exc = exc
            if attempt in (2, 8, 20):
                _spawn_daemon_if_needed()
            time.sleep(0.1)
    else:
        raise RuntimeError(
            f"Embedding daemon unavailable: {last_exc}. See daemon log: {_DAEMON_LOG_PATH}"
        )

    if "error" in response:
        raise RuntimeError(str(response["error"]))
    embeddings = response.get("embeddings")
    if not isinstance(embeddings, list):
        raise RuntimeError("Invalid embedding daemon response")

    sys.stdout.write(json.dumps({"embeddings": embeddings}))
    return 0


def _acquire_singleton_lock() -> int | None:
    lock_fd = os.open(_LOCK_PATH, os.O_CREAT | os.O_RDWR, 0o600)
    try:
        fcntl.flock(lock_fd, fcntl.LOCK_EX | fcntl.LOCK_NB)
        os.ftruncate(lock_fd, 0)
        os.write(lock_fd, str(os.getpid()).encode("utf-8"))
        return lock_fd
    except OSError:
        os.close(lock_fd)
        return None


def _release_singleton_lock(lock_fd: int) -> None:
    try:
        fcntl.flock(lock_fd, fcntl.LOCK_UN)
    except OSError:
        pass
    os.close(lock_fd)


def cmd_serve_daemon() -> int:
    # Ensure only one daemon can own the socket and model at a time.
    lock_fd = _acquire_singleton_lock()
    if lock_fd is None:
        return 0

    server: socket.socket | None = None
    try:
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
        server.settimeout(1.0)
        last_activity = time.monotonic()

        while True:
            if time.monotonic() - last_activity > _IDLE_TIMEOUT_SEC:
                break

            try:
                conn, _ = server.accept()
            except socket.timeout:
                continue

            with conn:
                try:
                    data = b""
                    while not data.endswith(b"\n"):
                        part = conn.recv(8192)
                        if not part:
                            break
                        data += part

                    payload = json.loads(data.decode("utf-8").strip() or "{}")
                    if payload.get("ping"):
                        response = {"ok": True}
                    else:
                        texts = payload.get("texts", [])
                        if not isinstance(texts, list):
                            raise RuntimeError("Field 'texts' must be an array")
                        normalized = [str(item) for item in texts]
                        embeddings = _embed_texts(normalized)
                        response = {"embeddings": embeddings}
                    last_activity = time.monotonic()
                except Exception as exc:  # pragma: no cover - daemon runtime path
                    response = {"error": str(exc)}
                conn.sendall(json.dumps(response).encode("utf-8") + b"\n")
    finally:
        if server is not None:
            server.close()
        try:
            if os.path.exists(_SOCKET_PATH):
                os.unlink(_SOCKET_PATH)
        except OSError:
            pass
        _release_singleton_lock(lock_fd)
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
