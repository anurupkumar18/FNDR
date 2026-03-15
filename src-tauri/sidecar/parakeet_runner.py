#!/usr/bin/env python3
"""Parakeet-style local transcription runner for FNDR.

Strategy:
1) faster-whisper (small, CPU int8)
2) openai-whisper fallback (small)
"""

import sys


def main() -> int:
    if len(sys.argv) < 2:
        print("", end="")
        return 1

    audio_path = sys.argv[1]

    # Primary backend
    try:
        from faster_whisper import WhisperModel  # type: ignore

        model = WhisperModel("small", device="cpu", compute_type="int8")
        segments, _ = model.transcribe(audio_path, beam_size=5, vad_filter=True)
        text = " ".join(s.text.strip() for s in segments if getattr(s, "text", "").strip())
        if text.strip():
            print(text.strip(), end="")
            return 0
    except Exception:
        pass

    # Fallback backend
    try:
        import whisper  # type: ignore

        model = whisper.load_model("small")
        result = model.transcribe(audio_path, fp16=False)
        text = (result.get("text", "") or "").strip()
        if text:
            print(text, end="")
            return 0
    except Exception as exc:
        print(f"[parakeet-runner unavailable: {exc}]", end="")
        return 2

    print("[parakeet-runner produced empty output]", end="")
    return 3


if __name__ == "__main__":
    raise SystemExit(main())
