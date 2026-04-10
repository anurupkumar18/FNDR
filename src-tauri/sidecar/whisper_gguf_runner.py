#!/usr/bin/env python3
"""FNDR Whisper GGUF sidecar.

Usage:
    python3 whisper_gguf_runner.py <model_path> <audio_path>
"""

from __future__ import annotations

import os
import sys


def main() -> int:
    if len(sys.argv) < 3:
        print("usage: whisper_gguf_runner.py <model_path> <audio_path>", file=sys.stderr)
        return 1

    model_path = sys.argv[1]
    audio_path = sys.argv[2]

    if not os.path.isfile(model_path):
        print(f"Whisper model not found: {model_path}", file=sys.stderr)
        return 1
    if not os.path.isfile(audio_path):
        print(f"Audio input not found: {audio_path}", file=sys.stderr)
        return 1

    try:
        from whisper_cpp_python import Whisper  # type: ignore
    except Exception as exc:
        print(f"Failed importing whisper_cpp_python: {exc}", file=sys.stderr)
        return 2

    try:
        whisper = Whisper(model_path=model_path)
        with open(audio_path, "rb") as audio_file:
            result = whisper.transcribe(audio_file, response_format="verbose_json")
    except Exception as exc:
        print(f"Whisper transcription failed: {exc}", file=sys.stderr)
        return 3

    text = ""
    if isinstance(result, dict):
        text = str(result.get("text") or "").strip()
    elif result is not None:
        text = str(result).strip()

    if not text:
        print("Whisper returned an empty transcript", file=sys.stderr)
        return 4

    print(" ".join(text.split()), end="")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
