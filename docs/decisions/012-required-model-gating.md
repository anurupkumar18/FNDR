# Decision 012: Required-Model Gating and Graceful Feature Degradation

## Status

Accepted (Gate 0, PRD-gate0-installable)

## Context

The MiniLM text embedder is the foundation of the durable memory write path (384-d vectors, `memories_v4_minilm_384`). Previously, when the model file was missing, the capture loop degraded silently: frames were still stored with all-zero embedding vectors, corrupting search quality with no user-visible cause. Optional features (meetings, TTS, agent) additionally depend on system Python/ffmpeg that a non-developer machine may not have.

## Decision

- **The embedder is a hard capture prerequisite.** When no real embedder is available, the capture loop blocks frames (`embedder_gate_action` in `capture/mod.rs`) instead of writing zero-vector rows. Blocked ticks are counted under the `embedder_unavailable` skip reason and surfaced in the workspace UI. This supersedes the earlier degrade-to-mock behavior for the durable write path (mock embeddings remain available behind `FNDR_ALLOW_MOCK_EMBEDDER` for development).
- **Self-healing:** while blocked, the loop retries embedder initialization every 30 seconds, so capture resumes without a restart once the model lands on disk.
- **Required vs. optional models:** `ModelDefinition` carries `required` and pinned `sha256` fields plus `extra_files` (the embedder needs its tokenizer). Onboarding auto-installs required models rather than offering them as choices; availability checks demand all files. Downloads are verified against pinned digests before promotion into the models directory; a checksum mismatch deletes the artifact and fails loudly.
- **Optional features degrade visibly:** meeting recording is disabled when ffmpeg is missing; speech/TTS paths return typed errors with install instructions when Python is missing. Core capture/search has zero Python dependency.

## Consequences

Positive:

- Zero-vector memory rows can no longer be written; search quality failures are visible instead of silent.
- A fresh install reaches working semantic search entirely through in-app downloads.
- Model artifacts are revision-pinned and checksum-verified, so upstream drift fails loudly.

Negative / accepted risks:

- Capture produces nothing until the ~90 MB embedder is installed (deliberate: visible pause beats silent corruption).
- Runtime embed failures after successful initialization still fall back per the pre-existing behavior; tightening that path is follow-up work.
