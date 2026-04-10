#!/usr/bin/env python3
"""FNDR Whisper GGUF sidecar.

Usage:
    python3 whisper_gguf_runner.py <model_path> <audio_path>
"""

from __future__ import annotations

import os
import sys


def _extract_text(result: object) -> str:
    """Best-effort text extraction across whisper_cpp_python result formats."""
    if result is None:
        return ""

    if isinstance(result, str):
        return " ".join(result.split()).strip()

    if isinstance(result, dict):
        direct = str(result.get("text") or "").strip()
        if direct:
            return " ".join(direct.split()).strip()

        # Some variants only populate per-segment text.
        segments = result.get("segments")
        if isinstance(segments, list):
            parts: list[str] = []
            for segment in segments:
                if isinstance(segment, dict):
                    seg_text = str(segment.get("text") or "").strip()
                    if seg_text:
                        parts.append(seg_text)
            joined = " ".join(parts).strip()
            if joined:
                return " ".join(joined.split()).strip()

    # Final fallback for custom object wrappers.
    text_attr = getattr(result, "text", None)
    if text_attr:
        return " ".join(str(text_attr).split()).strip()

    return ""


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
        import subprocess
        import tempfile
        wav_path = tempfile.mktemp(suffix=".wav")
        try:
            subprocess.run(
                ["ffmpeg", "-y", "-i", audio_path, "-ar", "16000", "-ac", "1", "-c:a", "pcm_s16le", wav_path],
                check=True,
                stdout=subprocess.DEVNULL,
                stderr=subprocess.DEVNULL,
            )
            whisper = Whisper(model_path=model_path)
            result = None
            # Try a few modes; some builds return empty text for one format
            # but valid text for another.
            for mode in ("verbose_json", "json", "text"):
                with open(wav_path, "rb") as audio_file:
                    kwargs = {"response_format": mode} if mode != "text" else {}
                    result = whisper.transcribe(audio_file, **kwargs)
                text = _extract_text(result)
                if text:
                    print(text, end="")
                    return 0
        finally:
            if os.path.exists(wav_path):
                os.remove(wav_path)
    except Exception as exc:
        print(f"Whisper transcription failed: {exc}", file=sys.stderr)
        return 3

    if not _extract_text(result):
        print("Whisper returned an empty transcript", file=sys.stderr)
        return 4
    return 4


if __name__ == "__main__":
    raise SystemExit(main())
