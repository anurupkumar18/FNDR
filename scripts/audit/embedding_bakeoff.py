#!/usr/bin/env python3
"""Embedding model bake-off on the labeled demo personas (VS-17), with an
optional cross-encoder rerank spike (VS-19, --rerank).

Pure vector retrieval (cosine, top 10) over the two seeded personas, so the
numbers isolate the embedder. Each model is scored in two document modes:

  record  one vector per memory from a Python mirror of the app's primary
          embedding text (compose_insight_embedding_text over the record that
          seed_demo.rs builds; see app_primary_text for the fields and gaps)
  chunks  window title plus OCR text split into windows of about 300 tokens
          with overlap; a memory scores as its best chunk plus a small stated
          bonus for several close chunks (a preview of VS-18)

Metrics follow src-tauri/examples/retrieval_qa.rs: Recall@5 and MRR@10 over
keyword and paraphrase cases (the headline), case-level (a case is recalled
when any relevant id is in the top five), per-kind Recall@5 for keyword,
paraphrase, time, and app, and negative cases reported only as median top
scores against the positive median. Each model runs in its own subprocess so
its peak RSS is its own. Timings are on the host CPU through PyTorch and are
only comparable with each other, not with the app's ONNX Runtime path.

--rerank (VS-19) rescores each embedder's top 30 chunk-mode candidates with
cross-encoder/ms-marco-MiniLM-L-6-v2 on (query, the memory's best chunk) and
reports MRR@10 and Recall@5 before and after, milliseconds per query, and the
keep rule's verdict (rejected, or needs-M1 when only latency is left to check).

Usage:
  python3 scripts/audit/embedding_bakeoff.py --models all --personas all \
      --out-json report.json --out-md report.md
  python3 scripts/audit/embedding_bakeoff.py --models minilm,bge-small
  python3 scripts/audit/embedding_bakeoff.py --models all --rerank --out-md rerank.md

Install: pip install -r scripts/audit/requirements-bakeoff.txt
Tests:   python3 -m unittest scripts/audit/test_embedding_bakeoff.py
"""

from __future__ import annotations

import re
import argparse
import datetime as dt
import json
import math
import os
import platform
import subprocess
import sys
import tempfile
import time
import unicodedata
from dataclasses import dataclass, field
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parents[2]
DEMO_DIR = REPO_ROOT / "scripts" / "demo"
PERSONAS = ("knowledge-worker", "office-pm")
POOLED = "both"
CASE_KINDS = ("keyword", "paraphrase", "time", "app", "negative")
CORE_KINDS = ("keyword", "paraphrase")
POSITIVE_KINDS = ("keyword", "paraphrase", "time", "app")
NEGATIVE_KIND = "negative"
MODES = ("record", "chunks")
BASELINE_MODEL = "minilm"

TOP_K = 10
# Candidates kept per query in the worker output (the VS-19 rerank depth).
CANDIDATE_DEPTH = 30
# VS-19 (--rerank): a cross-encoder rescores the top 30 chunk-mode candidates,
# each on (query, the memory's best chunk for that query). Keep it only if
# MRR@10 improves by at least 0.03 and it adds under 150 ms at p95; latency is
# decided on the M1, so a cloud run can only reject or defer.
RERANK_MODEL = ("cross-encoder/ms-marco-MiniLM-L-6-v2", "233902d25c440f23af6f7d6e94d2946bac0bee0a")
RERANK_MODE = "chunks"
RERANK_MIN_MRR_GAIN = 0.03
RERANK_MAX_P95_MS = 150.0
# Chunking: about 300 tokens with overlap, using the app's 4 chars per token
# estimate (DEFAULT_CHARS_PER_TOKEN in src-tauri/src/config.rs).
CHUNK_MAX_TOKENS = 300
CHUNK_OVERLAP_TOKENS = 60
CHARS_PER_TOKEN = 4
# Max-over-chunks roll-up: a memory scores as its best chunk, plus
# MULTI_CHUNK_BONUS for each other chunk within MULTI_CHUNK_WINDOW of the best,
# counting at most MULTI_CHUNK_BONUS_CAP such chunks.
MULTI_CHUNK_BONUS = 0.01
MULTI_CHUNK_BONUS_CAP = 3
MULTI_CHUNK_WINDOW = 0.05
# Documents are embedded in batches of 16 (EMBEDDING_MAX_BATCH_SIZE in
# model_config.rs); queries one at a time, as at search time.
DOC_BATCH_SIZE = 16
# Each timing is taken this many times. Documents report the fastest pass (the
# one least disturbed by other load on a shared host); queries pool every
# sample into p50 and p95.
TIMING_REPEATS = 3
SEED = 0

# Field caps from src-tauri/src/memory_insight/derive.rs and
# memory_embedding_document.rs.
MAX_WHAT_CHARS = 280
MAX_WHY_CHARS = 320
MAX_CHANGED_CHARS = 400
PRIMARY_TEXT_MAX_CHARS = 2_000


@dataclass(frozen=True)
class ModelSpec:
    key: str
    model_id: str
    # (repo, pinned revision) tried in order; the first that loads is used.
    sources: tuple[tuple[str, str], ...]
    query_prompt: str
    document_prompt: str
    pooling: str
    license: str
    onnx: str
    tokenizer_kwargs: dict = field(default_factory=dict)
    # Matryoshka truncation (first n dimensions, then re-normalized); None keeps all.
    truncate_dim: int | None = None


MODELS: dict[str, ModelSpec] = {
    spec.key: spec
    for spec in (
        ModelSpec(
            key="minilm",
            model_id="sentence-transformers/all-MiniLM-L6-v2",
            sources=(
                ("sentence-transformers/all-MiniLM-L6-v2", "1110a243fdf4706b3f48f1d95db1a4f5529b4d41"),
            ),
            query_prompt="",
            document_prompt="",
            pooling="mean, L2 normalized",
            license="Apache-2.0",
            onnx=(
                "In use today: Xenova/all-MiniLM-L6-v2 onnx/model.onnx, 90.4 MB fp32 "
                "(pinned in model_config.rs); int8 builds about 23 MB in the "
                "sentence-transformers repo. App pools by mean (matches)."
            ),
        ),
        ModelSpec(
            key="bge-small",
            model_id="BAAI/bge-small-en-v1.5",
            sources=(("BAAI/bge-small-en-v1.5", "5c38ec7c405ec4b44b94cc5a9bb96e735b38267a"),),
            query_prompt="Represent this sentence for searching relevant passages: ",
            document_prompt="",
            pooling="CLS token, L2 normalized",
            license="MIT",
            onnx=(
                "Official BAAI/bge-small-en-v1.5 onnx/model.onnx, 133.1 MB fp32; "
                "Xenova/bge-small-en-v1.5 onnx/model_quantized.onnx, 34.0 MB int8. "
                "App must pool by CLS (it pools by mean today)."
            ),
        ),
        ModelSpec(
            key="embeddinggemma",
            model_id="google/embeddinggemma-300m",
            sources=(
                ("google/embeddinggemma-300m", "57c266a740f537b4dc058e1b0cda161fd15afa75"),
                # Ungated mirror; every weight and config file has the same
                # sha256 / git blob id as the gated original at the pin above.
                ("unsloth/embeddinggemma-300m", "bfa3c846ac738e62aa61806ef9112d34acb1dc5a"),
            ),
            query_prompt="task: search result | query: ",
            document_prompt="title: none | text: ",
            pooling="mean, two dense layers, L2 normalized",
            license="Gemma Terms of Use (google repo is gated; mirrors carry the same terms)",
            onnx=(
                "onnx-community/embeddinggemma-300m-ONNX (ungated): fp32 1235 MB, "
                "q8 309 MB, q4 197 MB; graph outputs a pooled sentence_embedding. "
                "No fp16 (activations overflow, per the model card)."
            ),
        ),
        ModelSpec(
            # Same weights and prompts, vectors cut to 256 dimensions (the
            # model card's Matryoshka sizes are 768, 512, 256, 128). The
            # 2026-05-15 model-stack plan targeted this size.
            key="embeddinggemma-256",
            model_id="google/embeddinggemma-300m (Matryoshka 256)",
            sources=(
                ("google/embeddinggemma-300m", "57c266a740f537b4dc058e1b0cda161fd15afa75"),
                ("unsloth/embeddinggemma-300m", "bfa3c846ac738e62aa61806ef9112d34acb1dc5a"),
            ),
            query_prompt="task: search result | query: ",
            document_prompt="title: none | text: ",
            pooling="as embeddinggemma, first 256 dims, L2 normalized",
            license="Gemma Terms of Use",
            onnx="Same ONNX builds as embeddinggemma; the app slices and re-normalizes.",
            truncate_dim=256,
        ),
        ModelSpec(
            key="qwen3",
            model_id="Qwen/Qwen3-Embedding-0.6B",
            sources=(("Qwen/Qwen3-Embedding-0.6B", "97b0c614be4d77ee51c0cef4e5f07c00f9eb65b3"),),
            query_prompt=(
                "Instruct: Given a web search query, retrieve relevant passages that "
                "answer the query\nQuery:"
            ),
            document_prompt="",
            pooling="last token, L2 normalized",
            license="Apache-2.0",
            onnx=(
                "onnx-community/Qwen3-Embedding-0.6B-ONNX (license field unset; "
                "upstream Apache-2.0): fp32 2400 MB, int8 614 MB, fp16 1200 MB. "
                "App must pool by last token."
            ),
            tokenizer_kwargs={"padding_side": "left"},
        ),
    )
}


# --------------------------------------------------------------------------
# Corpus and the app's embedding text


def load_persona(demo_dir: Path, persona: str) -> tuple[list[dict], list[dict]]:
    """Searchable memories (the low-signal control excluded) and validated cases."""
    entries = json.loads((Path(demo_dir) / f"{persona}-week.json").read_text())
    cases = json.loads((Path(demo_dir) / f"{persona}-queries.json").read_text())
    memories = [entry for entry in entries if not entry.get("low_signal")]
    searchable = {memory["id"] for memory in memories}
    seen = set()
    for case in cases:
        query = case.get("query", "")
        if not query.strip():
            raise ValueError(f"{persona}: every case needs a query")
        if case.get("kind") not in CASE_KINDS:
            raise ValueError(f"{persona}: case {query!r} has unknown kind {case.get('kind')!r}")
        relevant = case.get("relevant_ids", [])
        if (case["kind"] == NEGATIVE_KIND) != (not relevant):
            raise ValueError(f"{persona}: negative cases need empty relevant_ids: {query!r}")
        hidden = [memory_id for memory_id in relevant if memory_id not in searchable]
        if hidden:
            raise ValueError(f"{persona}: case {query!r} points at unsearchable ids {hidden}")
        if query in seen:
            raise ValueError(f"{persona}: duplicate query {query!r}")
        seen.add(query)
    return memories, cases


def normalize_document_text(raw: str) -> str:
    """normalize_document_text in memory_embedding_document.rs."""
    without_controls = "".join(" " if unicodedata.category(ch) == "Cc" else ch for ch in raw)
    return " ".join(without_controls.split())


def _clip_chars(value: str, max_chars: int) -> str:
    """clip_chars in memory_insight/derive.rs."""
    if len(value) <= max_chars:
        return value
    trimmed = value[: max_chars - 1].rstrip(",;\u2014- ")
    return f"{trimmed}\u2026"


def _is_template_summary(value: str) -> bool:
    """is_template_summary in memory_insight/derive.rs."""
    lower = value.strip().lower()
    if not lower:
        return True
    if ".png" in lower and sum(ch.isdigit() for ch in lower) >= 6:
        return True
    return (
        lower.startswith("screen capture (visual)")
        or lower.startswith("captured recent")
        or lower.startswith("viewed content on")
        or lower.startswith("url-only surface capture")
        or (lower.startswith("viewed ") and " at " in lower)
    )


def _derived_insight(entry: dict) -> tuple[str, str, str, str]:
    """what_happened, why_mattered, what_changed, context_thread as
    derive_insight_for_record fills them for a seeded record."""
    summary = entry.get("summary", "").strip()
    title = entry.get("window_title", "").strip()
    if summary and not _is_template_summary(summary):
        what = summary
    elif title:
        what = f"{title.rstrip('.')}."
    else:
        what = ""

    decisions = [d for d in entry.get("decisions", []) if d.strip() and not _is_template_summary(d)]
    errors = [e for e in entry.get("errors", []) if e.strip()]
    # Mirror of summariser::sentences::first_sentence: a sentence ends at
    # . ! or ? only before whitespace or the end, so file names stay whole.
    first_sentence = re.split(r"(?<=[.!?])\s", summary, maxsplit=1)[0].rstrip(".!?").strip()
    if decisions:
        why = decisions[0][:MAX_WHY_CHARS]
    elif errors:
        why = f"Encountered error: {errors[0][: MAX_WHY_CHARS - 20]}"
    elif (
        first_sentence
        and not _is_template_summary(first_sentence)
        and len(first_sentence.split()) >= 5
        and not _says_the_same(first_sentence, what)
    ):
        why = first_sentence[:MAX_WHY_CHARS]
    else:
        why = ""

    # Outcomes only: decisions not already shown as why_mattered. The seeded
    # personas carry no results or files. A session id is not a thread.
    changed = "; ".join(d.strip() for d in entry.get("decisions", []) if d.strip() and not _says_the_same(d, why))
    return (
        _clip_chars(what, MAX_WHAT_CHARS),
        _clip_chars(why, MAX_WHY_CHARS),
        _clip_chars(changed, MAX_CHANGED_CHARS),
        "",
    )


def _says_the_same(a: str, b: str) -> bool:
    """Mirror of `says_the_same` in memory_insight/derive.rs."""

    def norm(value: str) -> str:
        kept = "".join(ch for ch in value.lower() if ch.isalnum() or ch.isspace())
        return " ".join(kept.split())

    a, b = norm(a), norm(b)
    return bool(a) and bool(b) and (a in b or b in a)


def app_primary_text(entry: dict) -> str:
    """Python mirror of the app's primary embedding text for a seeded memory.

    seed_demo.rs maps summary to memory_context, project, topic, decisions,
    errors, next_steps, url, and session onto the record; insert-time
    normalization runs derive_insight_for_record and then
    compose_insight_embedding_text, which joins labeled segments in this order
    (empty or "unknown" skipped): intent, project, topic, workflow, context,
    what_happened, why_mattered, what_changed, context_thread, entities,
    aliases, decisions, errors, blockers, todos (next_steps when todos is
    empty), results, files, urls, commands. The result is whitespace
    normalized and capped at 2,000 chars.

    Not mirrored (documented gaps): intent, workflow, and aliases, which Rust
    heuristics infer at insert time; strip_fluff and
    dedupe_repeating_phrases, which are no-ops on these personas (no repeated
    app, project, or domain names, separators, or preambles in the fields they
    touch).
    """
    what, why, changed, thread = _derived_insight(entry)
    segments = []

    def push(label: str, value: str) -> None:
        value = value.strip()
        if value and value.lower() != "unknown":
            segments.append(f"{label}: {value}")

    # Document version 2: the card summary leads, and what_happened is not
    # repeated when it is the same sentence.
    summary = entry.get("summary", "").strip()
    has_summary = bool(summary) and not _is_template_summary(summary)
    if has_summary:
        push("summary", summary)
    title = entry.get("window_title", "").strip()
    if title.lower() != entry.get("app_name", "").strip().lower():
        push("title", title)
    push("project", entry.get("project", ""))
    push("topic", entry.get("topic", ""))
    push("context", entry.get("summary", ""))
    if not (has_summary and what.strip() == summary):
        push("what_happened", what)
    push("why_mattered", why)
    push("what_changed", changed)
    push("context_thread", thread)
    push("decisions", "; ".join(entry.get("decisions", [])))
    push("errors", "; ".join(entry.get("errors", [])))
    push("todos", "; ".join(entry.get("next_steps", [])))
    push("urls", entry.get("url") or "")
    return normalize_document_text("\n".join(segments))[:PRIMARY_TEXT_MAX_CHARS]


def chunk_source_text(entry: dict) -> str:
    """Window title as the first line, then the OCR text (as the app's OCR chunker orders them)."""
    parts = (entry.get("window_title", "").strip(), entry.get("ocr_text", "").strip())
    return "\n".join(part for part in parts if part)


def estimate_tokens(text: str, chars_per_token: int = CHARS_PER_TOKEN) -> int:
    return math.ceil(len(text) / chars_per_token)


def chunk_text(
    text: str,
    max_tokens: int = CHUNK_MAX_TOKENS,
    overlap_tokens: int = CHUNK_OVERLAP_TOKENS,
    chars_per_token: int = CHARS_PER_TOKEN,
) -> list[str]:
    """Whitespace-word windows of at most max_tokens estimated tokens.

    Consecutive windows share a tail of at most overlap_tokens; a single word
    longer than the budget becomes its own window; every window advances.
    """
    words = text.split()
    if not words:
        return []
    budget = max_tokens * chars_per_token
    overlap_budget = overlap_tokens * chars_per_token
    chunks = []
    start = 0
    while True:
        end = start + 1
        length = len(words[start])
        while end < len(words) and length + 1 + len(words[end]) <= budget:
            length += 1 + len(words[end])
            end += 1
        chunks.append(" ".join(words[start:end]))
        if end >= len(words):
            return chunks
        next_start = end
        tail = 0
        while next_start - 1 > start:
            added = len(words[next_start - 1]) + (1 if tail else 0)
            if tail + added > overlap_budget:
                break
            tail += added
            next_start -= 1
        start = next_start


def rollup_chunk_scores(hits) -> dict[str, float]:
    """Memory score = best chunk + bonus per other chunk close to the best (capped)."""
    by_memory: dict[str, list[float]] = {}
    for memory_id, score in hits:
        by_memory.setdefault(memory_id, []).append(float(score))
    rolled = {}
    for memory_id, scores in by_memory.items():
        best = max(scores)
        close = sum(1 for score in scores if score >= best - MULTI_CHUNK_WINDOW) - 1
        rolled[memory_id] = best + MULTI_CHUNK_BONUS * min(close, MULTI_CHUNK_BONUS_CAP)
    return rolled


# --------------------------------------------------------------------------
# Ranking and metrics (retrieval_qa.rs semantics)


def rank_ids(scores: dict[str, float], k: int) -> list[tuple[str, float]]:
    """Top k by score, ties broken by id so runs are deterministic."""
    return sorted(scores.items(), key=lambda item: (-item[1], item[0]))[:k]


def first_relevant_rank(ranked_ids, relevant, k: int = TOP_K) -> int | None:
    for index, memory_id in enumerate(list(ranked_ids)[:k]):
        if memory_id in relevant:
            return index + 1
    return None


def median(values) -> float | None:
    ordered = sorted(values)
    if not ordered:
        return None
    mid = len(ordered) // 2
    if len(ordered) % 2 == 0:
        return (ordered[mid - 1] + ordered[mid]) / 2.0
    return ordered[mid]


def percentile(values, p: float):
    """Nearest rank with Rust's round-half-away-from-zero index, as in retrieval_qa.rs."""
    if not values:
        return 0
    ordered = sorted(values)
    index = math.floor((p / 100.0) * (len(ordered) - 1) + 0.5)
    return ordered[min(index, len(ordered) - 1)]


def score_cases(results) -> dict:
    """results: (kind, rank within top 10 or None, top score or None) per case."""
    core_hits = core_count = 0
    reciprocal_sum = 0.0
    by_kind: dict[str, list[int]] = {}
    positive_top, negative_top = [], []
    for kind, rank, top_score in results:
        if kind == NEGATIVE_KIND:
            negative_top.append(top_score)
            continue
        hit = rank is not None and rank <= 5
        counted = by_kind.setdefault(kind, [0, 0])
        counted[0] += 1
        counted[1] += int(hit)
        if kind in CORE_KINDS:
            core_count += 1
            core_hits += int(hit)
            if rank is not None and rank <= TOP_K:
                reciprocal_sum += 1.0 / rank
        if top_score is not None:
            positive_top.append(top_score)
    no_match = None
    if negative_top:
        returned = [score for score in negative_top if score is not None]
        negative_median = median(returned)
        positive_median = median(positive_top)
        no_match = {
            "cases": len(negative_top),
            "returned_nothing": len(negative_top) - len(returned),
            "top_score_median": negative_median,
            "positive_top_score_median": positive_median,
            "separation": (
                positive_median - negative_median
                if positive_median is not None and negative_median is not None
                else None
            ),
            "separation_auc": separation_auc(positive_top, returned),
        }
    return {
        "recall_at_5": core_hits / max(core_count, 1),
        "mrr_at_10": reciprocal_sum / max(core_count, 1),
        "core_cases": core_count,
        "core_hits_at_5": core_hits,
        "recall_at_5_by_kind": {kind: hits / count for kind, (count, hits) in sorted(by_kind.items())},
        "cases_by_kind": {kind: count for kind, (count, _) in sorted(by_kind.items())},
        "no_match": no_match,
    }


def separation_auc(positive, negative) -> float | None:
    """P(a positive query's top score beats a negative query's), ties count half.

    Scale free, so models with different cosine ranges compare fairly.
    """
    if not positive or not negative:
        return None
    wins = sum(1.0 if pos > neg else 0.5 if pos == neg else 0.0 for pos in positive for neg in negative)
    return wins / (len(positive) * len(negative))


def mcnemar_exact(gains: int, losses: int) -> float:
    """Two-sided exact McNemar (sign test) p-value on discordant cases."""
    total = gains + losses
    if total == 0:
        return 1.0
    tail = sum(math.comb(total, i) for i in range(min(gains, losses) + 1))
    return min(1.0, 2.0 * tail / 2**total)


def compare_to_baseline(mine: list[dict], base: list[dict]) -> dict:
    """Paired per-query comparison on the headline (keyword, paraphrase) cases.

    gains/losses: hit at 5 here and not in the baseline, or the reverse.
    rr_better/rr_worse: reciprocal rank within the top 10 higher or lower.
    Both get a two-sided exact sign test.
    """

    def reciprocal(rank):
        return 1.0 / rank if rank is not None and rank <= TOP_K else 0.0

    gains = losses = better = worse = 0
    for left, right in zip(mine, base):
        if left["kind"] not in CORE_KINDS:
            continue
        left_hit = left["rank_at_10"] is not None and left["rank_at_10"] <= 5
        right_hit = right["rank_at_10"] is not None and right["rank_at_10"] <= 5
        gains += int(left_hit and not right_hit)
        losses += int(right_hit and not left_hit)
        left_rr, right_rr = reciprocal(left["rank_at_10"]), reciprocal(right["rank_at_10"])
        better += int(left_rr > right_rr)
        worse += int(left_rr < right_rr)
    return {
        "gains": gains,
        "losses": losses,
        "mcnemar_p": mcnemar_exact(gains, losses),
        "rr_better": better,
        "rr_worse": worse,
        "rr_sign_p": mcnemar_exact(better, worse),
    }


def rerank_candidates(case: dict, depth: int = CANDIDATE_DEPTH) -> list[list[str]]:
    """[memory id, best chunk text] for the vector top `depth` of one case."""
    return [[memory_id, case["best_chunk"][memory_id]] for memory_id, _ in case["ranked"][:depth]]


def apply_rerank(ranked: list, rerank_scores: dict[str, float]) -> list[list]:
    """Reranked candidates first (by cross-encoder score, ties by id), then the
    rest of the vector ranking unchanged."""
    head = sorted(
        ([memory_id, float(rerank_scores[memory_id])] for memory_id, _ in ranked if memory_id in rerank_scores),
        key=lambda item: (-item[1], item[0]),
    )
    tail = [[memory_id, score] for memory_id, score in ranked if memory_id not in rerank_scores]
    return head + tail


def rerank_verdict(mrr_before: float, mrr_after: float, p95_ms: float | None) -> tuple[str, str]:
    """VS-19 rule applied to a cloud run: rejected, or needs-M1 when quality passes.

    The latency half of the rule is decided on the M1, so this never returns kept.
    """
    gain = mrr_after - mrr_before
    if gain < RERANK_MIN_MRR_GAIN - 1e-9:
        return "rejected", f"MRR@10 gain {gain:+.3f} is under +{RERANK_MIN_MRR_GAIN:.2f}"
    where = "under" if p95_ms is not None and p95_ms < RERANK_MAX_P95_MS else "over"
    p95_label = "n/a" if p95_ms is None else f"{p95_ms:.1f} ms"
    return (
        "needs-M1",
        f"MRR@10 gain {gain:+.3f} meets +{RERANK_MIN_MRR_GAIN:.2f}; p95 {p95_label} is {where} "
        f"{RERANK_MAX_P95_MS:.0f} ms on this host; confirm latency on the M1",
    )


# --------------------------------------------------------------------------
# Worker: one model per process


def peak_rss_mb() -> float:
    import resource

    peak = resource.getrusage(resource.RUSAGE_SELF).ru_maxrss
    # Bytes on macOS, KiB on Linux.
    return peak / (1024 * 1024) if sys.platform == "darwin" else peak / 1024


def snapshot_bytes(repo: str, revision: str) -> int | None:
    """Bytes the Hugging Face cache holds for this repo at this revision."""
    from huggingface_hub import constants

    snapshot = Path(constants.HF_HUB_CACHE) / f"models--{repo.replace('/', '--')}" / "snapshots" / revision
    if not snapshot.is_dir():
        return None
    return sum(path.stat().st_size for path in snapshot.rglob("*") if path.is_file())


def _seed_everything() -> None:
    import random

    import numpy as np
    import torch

    random.seed(SEED)
    np.random.seed(SEED)
    torch.manual_seed(SEED)


def load_sentence_transformer(spec: ModelSpec, device: str):
    from sentence_transformers import SentenceTransformer

    failures = []
    for repo, revision in spec.sources:
        try:
            started = time.perf_counter()
            model = SentenceTransformer(
                repo,
                revision=revision,
                device=device,
                tokenizer_kwargs=spec.tokenizer_kwargs or None,
                truncate_dim=spec.truncate_dim,
            )
            return model, repo, revision, time.perf_counter() - started, failures
        except Exception as error:  # noqa: BLE001 - record and try the next source
            failures.append(f"{repo}@{revision[:12]}: {type(error).__name__}: {str(error).splitlines()[0][:200]}")
    raise RuntimeError("; ".join(failures))


def embed_worker(
    key: str,
    personas: list[str],
    device: str,
    chunk_tokens: int,
    chunk_overlap: int,
    timing_repeats: int = TIMING_REPEATS,
) -> dict:
    import numpy as np
    import sentence_transformers
    import torch
    import transformers

    _seed_everything()
    spec = MODELS[key]
    rss_after_imports = peak_rss_mb()
    model, repo, revision, load_seconds, load_failures = load_sentence_transformer(spec, device)

    def encode(texts, prompt, batch_size):
        return model.encode(
            list(texts),
            prompt=prompt or None,
            batch_size=batch_size,
            normalize_embeddings=True,
            convert_to_numpy=True,
            show_progress_bar=False,
        ).astype(np.float32)

    def token_count(text, prompt):
        return len(model.tokenizer(f"{prompt}{text}")["input_ids"])

    encode(["warm up"], spec.query_prompt, 1)
    encode(["warm up"], spec.document_prompt, 1)

    query_ms_all = []
    max_tokens = 0
    truncated = 0
    persona_out = {}
    for persona in personas:
        memories, cases = load_persona(DEMO_DIR, persona)
        query_vectors = []
        for repeat in range(timing_repeats):
            for case in cases:
                started = time.perf_counter()
                vector = encode([case["query"]], spec.query_prompt, 1)[0]
                query_ms_all.append((time.perf_counter() - started) * 1000.0)
                if repeat == 0:
                    query_vectors.append(vector)
        queries = np.stack(query_vectors)

        modes_out = {}
        for mode in MODES:
            units = []
            for memory in memories:
                if mode == "record":
                    units.append((memory["id"], app_primary_text(memory)))
                else:
                    for chunk in chunk_text(chunk_source_text(memory), chunk_tokens, chunk_overlap):
                        units.append((memory["id"], chunk))
            texts = [text for _, text in units]
            pass_ms = []
            for _ in range(timing_repeats):
                started = time.perf_counter()
                documents = encode(texts, spec.document_prompt, DOC_BATCH_SIZE)
                pass_ms.append((time.perf_counter() - started) * 1000.0)
            embed_ms = min(pass_ms)
            lengths = [token_count(text, spec.document_prompt) for text in texts]
            max_tokens = max(max_tokens, *lengths)
            truncated += sum(1 for length in lengths if length > model.max_seq_length)
            estimate_ratio = max(
                len(model.tokenizer(text)["input_ids"]) / max(estimate_tokens(text), 1) for text in texts
            )

            similarity = queries @ documents.T
            case_out = []
            for row, case in enumerate(cases):
                scores = similarity[row]
                memory_scores = rollup_chunk_scores(
                    (memory_id, float(scores[col])) for col, (memory_id, _) in enumerate(units)
                )
                ranked = rank_ids(memory_scores, CANDIDATE_DEPTH)
                best_chunk = {}
                for col, (memory_id, text) in enumerate(units):
                    if memory_id not in best_chunk or scores[col] > scores[best_chunk[memory_id]]:
                        best_chunk[memory_id] = col
                case_out.append(
                    {
                        "query": case["query"],
                        "kind": case["kind"],
                        "relevant_ids": case["relevant_ids"],
                        "ranked": [[memory_id, round(score, 6)] for memory_id, score in ranked],
                        "best_chunk": {memory_id: units[best_chunk[memory_id]][1] for memory_id, _ in ranked},
                    }
                )
            modes_out[mode] = {
                "memories": len(memories),
                "units": len(units),
                "max_units_per_memory": max(
                    sum(1 for memory_id, _ in units if memory_id == memory["id"]) for memory in memories
                ),
                "ms_per_unit": embed_ms / max(len(units), 1),
                "pass_ms": pass_ms,
                "max_input_tokens": max(lengths),
                # Largest real-tokens over chars/4-estimate ratio among the inputs.
                "max_tokens_per_estimated_token": estimate_ratio,
                "cases": case_out,
            }
        persona_out[persona] = modes_out

    return {
        "model": key,
        "model_id": spec.model_id,
        "loaded_from": repo,
        "revision": revision,
        "load_failures": load_failures,
        "dimension": model.get_sentence_embedding_dimension(),
        "max_seq_length": model.max_seq_length,
        "max_input_tokens": max_tokens,
        "truncated_inputs": truncated,
        "download_bytes": snapshot_bytes(repo, revision),
        "load_seconds": load_seconds,
        "rss_after_imports_mb": rss_after_imports,
        "peak_rss_mb": peak_rss_mb(),
        "query_ms_p50": percentile(query_ms_all, 50.0),
        "query_ms_p95": percentile(query_ms_all, 95.0),
        "torch_threads": torch.get_num_threads(),
        "timing_repeats": timing_repeats,
        "versions": {
            "torch": torch.__version__,
            "transformers": transformers.__version__,
            "sentence_transformers": sentence_transformers.__version__,
        },
        "personas": persona_out,
    }


def rerank_worker(input_path: Path, device: str, timing_repeats: int = TIMING_REPEATS) -> dict:
    """Score every group's (query, candidate text) pairs with the cross-encoder.

    Each query is one predict call over all of its candidates (up to 30 pairs),
    timed `timing_repeats` times.
    """
    import sentence_transformers
    from sentence_transformers import CrossEncoder

    _seed_everything()
    repo, revision = RERANK_MODEL
    rss_after_imports = peak_rss_mb()
    started = time.perf_counter()
    model = CrossEncoder(repo, revision=revision, device=device)
    load_seconds = time.perf_counter() - started
    model.predict([("warm up", "warm up")], show_progress_bar=False)

    groups = json.loads(Path(input_path).read_text())
    out = {}
    for name, queries in groups.items():
        scored = []
        for query in queries:
            pairs = [(query["query"], text) for _, text in query["candidates"]]
            samples = []
            for _ in range(timing_repeats):
                started = time.perf_counter()
                scores = model.predict(pairs, batch_size=max(len(pairs), 1), show_progress_bar=False)
                samples.append((time.perf_counter() - started) * 1000.0)
            scored.append(
                {
                    "scores": {
                        memory_id: round(float(score), 6)
                        for (memory_id, _), score in zip(query["candidates"], scores)
                    },
                    "pairs": len(pairs),
                    "ms": samples,
                }
            )
        out[name] = scored
    activation = getattr(model, "activation_fn", None) or getattr(model, "default_activation_function", None)
    return {
        "model": repo,
        "revision": revision,
        "activation": type(activation).__name__ if activation is not None else "unknown",
        "max_length": getattr(model, "max_length", None),
        "load_seconds": load_seconds,
        "rss_after_imports_mb": rss_after_imports,
        "peak_rss_mb": peak_rss_mb(),
        "download_bytes": snapshot_bytes(repo, revision),
        "timing_repeats": timing_repeats,
        "sentence_transformers": sentence_transformers.__version__,
        "groups": out,
    }


def run_worker(argv: list[str], label: str) -> dict:
    with tempfile.TemporaryDirectory() as tmp:
        out = Path(tmp) / "worker.json"
        env = dict(os.environ, TOKENIZERS_PARALLELISM="false")
        print(f"[bakeoff] {label} ...", file=sys.stderr, flush=True)
        started = time.perf_counter()
        proc = subprocess.run(
            [sys.executable, str(Path(__file__).resolve()), *argv, "--worker-out", str(out)],
            capture_output=True,
            text=True,
            env=env,
        )
        elapsed = time.perf_counter() - started
        if proc.returncode != 0 or not out.exists():
            tail = " | ".join(line for line in proc.stderr.strip().splitlines()[-3:])
            print(f"[bakeoff] {label} failed after {elapsed:.0f}s: {tail}", file=sys.stderr)
            return {"error": f"exit {proc.returncode}: {tail[-600:]}"}
        print(f"[bakeoff] {label} done in {elapsed:.0f}s", file=sys.stderr, flush=True)
        return json.loads(out.read_text())


# --------------------------------------------------------------------------
# Report


def evaluate(cases: list[dict]) -> tuple[dict, list[dict]]:
    results, per_query = [], []
    for case in cases:
        ranked_ids = [memory_id for memory_id, _ in case["ranked"]]
        rank = first_relevant_rank(ranked_ids, set(case["relevant_ids"]), TOP_K)
        top_score = case["ranked"][0][1] if case["ranked"] else None
        results.append((case["kind"], rank, top_score))
        per_query.append({"query": case["query"], "kind": case["kind"], "rank_at_10": rank, "top_score": top_score})
    return score_cases(results), per_query


def build_report(args, workers: dict[str, dict], personas: list[str]) -> dict:
    corpus = {}
    for persona in personas:
        memories, cases = load_persona(DEMO_DIR, persona)
        counts = {kind: sum(1 for case in cases if case["kind"] == kind) for kind in CASE_KINDS}
        corpus[persona] = {
            "memories": len(memories),
            "cases": len(cases),
            "case_count_by_kind": {kind: count for kind, count in counts.items() if count},
        }

    scopes = [*personas, POOLED] if len(personas) > 1 else list(personas)
    results = []
    for key, worker in workers.items():
        if "error" in worker:
            continue
        for mode in MODES:
            for scope in scopes:
                members = personas if scope == POOLED else [scope]
                cases = [case for persona in members for case in worker["personas"][persona][mode]["cases"]]
                metrics, per_query = evaluate(cases)
                for query, persona in zip(
                    per_query,
                    [persona for persona in members for _ in worker["personas"][persona][mode]["cases"]],
                ):
                    query["persona"] = persona
                results.append({"model": key, "persona": scope, "mode": mode, "metrics": metrics, "queries": per_query})

    comparisons = []
    baseline = {(r["persona"], r["mode"]): r for r in results if r["model"] == BASELINE_MODEL}
    for result in results:
        base = baseline.get((result["persona"], result["mode"]))
        if base is None or result["model"] == BASELINE_MODEL:
            continue
        comparisons.append(
            {
                "model": result["model"],
                "persona": result["persona"],
                "mode": result["mode"],
                "baseline": BASELINE_MODEL,
                **compare_to_baseline(result["queries"], base["queries"]),
            }
        )

    models = {}
    for key, worker in workers.items():
        if "error" in worker:
            models[key] = {"model_id": MODELS[key].model_id, "error": worker["error"]}
            continue
        models[key] = {name: value for name, value in worker.items() if name != "personas"}
        models[key]["units"] = {
            persona: {
                mode: {
                    name: worker["personas"][persona][mode][name]
                    for name in (
                        "units",
                        "max_units_per_memory",
                        "ms_per_unit",
                        "pass_ms",
                        "max_input_tokens",
                        "max_tokens_per_estimated_token",
                    )
                }
                for mode in MODES
            }
            for persona in personas
        }

    return {
        "schema": "fndr-embedding-bakeoff/1",
        "generated_at": dt.datetime.now(dt.timezone.utc).replace(microsecond=0).isoformat(),
        "host": {
            "platform": platform.platform(),
            "machine": platform.machine(),
            "python": platform.python_version(),
            "cpu_count": os.cpu_count(),
            "device": args.device,
        },
        "settings": {
            "top_k": TOP_K,
            "chunk_max_tokens": args.chunk_tokens,
            "chunk_overlap_tokens": args.chunk_overlap,
            "chars_per_token": CHARS_PER_TOKEN,
            "multi_chunk_bonus": MULTI_CHUNK_BONUS,
            "multi_chunk_bonus_cap": MULTI_CHUNK_BONUS_CAP,
            "multi_chunk_window": MULTI_CHUNK_WINDOW,
            "doc_batch_size": DOC_BATCH_SIZE,
            "timing_repeats": args.timing_repeats,
            "seed": SEED,
        },
        "corpus": corpus,
        "models": models,
        "specs": {
            key: {
                "model_id": spec.model_id,
                "query_prompt": spec.query_prompt,
                "document_prompt": spec.document_prompt,
                "pooling": spec.pooling,
                "license": spec.license,
                "onnx": spec.onnx,
            }
            for key, spec in MODELS.items()
            if key in workers
        },
        "results": results,
        "comparisons": comparisons,
    }


def rerank_groups(workers: dict[str, dict], personas: list[str]) -> dict:
    """Rerank input: per embedder and persona, each query's chunk-mode top 30."""
    return {
        f"{key}|{persona}": [
            {"query": case["query"], "candidates": rerank_candidates(case)}
            for case in worker["personas"][persona][RERANK_MODE]["cases"]
        ]
        for key, worker in workers.items()
        if "error" not in worker
        for persona in personas
    }


def build_rerank_section(workers: dict[str, dict], personas: list[str], reranked: dict) -> dict:
    if "error" in reranked:
        return {"error": reranked["error"]}
    scopes = [*personas, POOLED] if len(personas) > 1 else list(personas)
    rows, latency, verdicts = [], [], []
    for key, worker in workers.items():
        if "error" in worker:
            continue
        before_cases, after_cases, pairs = {}, {}, {}
        samples_full, samples_all = [], []
        for persona in personas:
            cases = worker["personas"][persona][RERANK_MODE]["cases"]
            scored = reranked["groups"][f"{key}|{persona}"]
            before_cases[persona] = cases
            after_cases[persona] = [
                dict(case, ranked=apply_rerank(case["ranked"], result["scores"]))
                for case, result in zip(cases, scored)
            ]
            pairs[persona] = [result["pairs"] for result in scored]
            # A query's latency is its fastest timing (least disturbed by other load).
            for result in scored:
                samples_all.append(min(result["ms"]))
                if result["pairs"] == CANDIDATE_DEPTH:
                    samples_full.append(min(result["ms"]))
        scope_rows = {}
        for scope in scopes:
            members = personas if scope == POOLED else [scope]
            before, before_queries = evaluate([case for p in members for case in before_cases[p]])
            after, after_queries = evaluate([case for p in members for case in after_cases[p]])
            scope_pairs = [count for p in members for count in pairs[p]]
            scope_rows[scope] = {
                "model": key,
                "persona": scope,
                "pairs_min": min(scope_pairs),
                "pairs_max": max(scope_pairs),
                "before": before,
                "after": after,
                "mrr_gain": after["mrr_at_10"] - before["mrr_at_10"],
                **compare_to_baseline(after_queries, before_queries),
            }
            rows.append(scope_rows[scope])
        p95_full = percentile(samples_full, 95.0) if samples_full else None
        latency.append(
            {
                "model": key,
                "queries_at_full_depth": len(samples_full),
                "p50_ms_full_depth": percentile(samples_full, 50.0) if samples_full else None,
                "p95_ms_full_depth": p95_full,
                "p95_ms_all": percentile(samples_all, 95.0),
            }
        )
        headline = scope_rows[scopes[-1]]
        verdict, reason = rerank_verdict(
            headline["before"]["mrr_at_10"],
            headline["after"]["mrr_at_10"],
            p95_full if p95_full is not None else percentile(samples_all, 95.0),
        )
        verdicts.append({"model": key, "scope": scopes[-1], "verdict": verdict, "reason": reason})
    meta = {name: value for name, value in reranked.items() if name != "groups"}
    return {"reranker": meta, "mode": RERANK_MODE, "depth": CANDIDATE_DEPTH, "rows": rows, "latency": latency, "verdicts": verdicts}


def render_rerank_markdown(section: dict, personas: list[str]) -> list[str]:
    lines = ["", "## Cross-encoder rerank of the top 30 (VS-19)", ""]
    if "error" in section:
        return lines + [f"Rerank failed: {_cell(section['error'][:400])}"]
    meta = section["reranker"]
    lines += [
        f"Reranker {meta['model']}@{meta['revision'][:7]} (activation {meta['activation']}, max length "
        f"{meta['max_length']}) rescores each query's top {section['depth']} {section['mode']}-mode vector candidates "
        "on (query, the memory's best chunk text); fewer than 30 when the persona has fewer memories. "
        "Reranked order by cross-encoder score, ties by id; the vector order is kept below the reranked head. "
        f"Rule: keep only if MRR@10 gains at least {RERANK_MIN_MRR_GAIN} and the rerank adds under "
        f"{RERANK_MAX_P95_MS:.0f} ms at p95 (latency to be confirmed on the M1).",
        "",
        "| Persona | Embedder | Pairs per query | MRR@10 vector | MRR@10 reranked | Gain | Recall@5 vector "
        "| Recall@5 reranked | Gains | Losses | RR better | RR worse | p (RR) | AUC reranked |",
        "|---|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|",
    ]
    order = {key: index for index, key in enumerate(MODELS)}
    scope_order = {scope: index for index, scope in enumerate([*personas, POOLED])}
    for row in sorted(section["rows"], key=lambda r: (scope_order[r["persona"]], order[r["model"]])):
        before, after = row["before"], row["after"]
        pairs = str(row["pairs_min"]) if row["pairs_min"] == row["pairs_max"] else f"{row['pairs_min']}-{row['pairs_max']}"
        lines.append(
            f"| {row['persona']} | {row['model']} | {pairs} | {_fmt(before['mrr_at_10'])} | {_fmt(after['mrr_at_10'])} "
            f"| {row['mrr_gain']:+.3f} | {_fmt(before['recall_at_5'])} ({before['core_hits_at_5']}/{before['core_cases']}) "
            f"| {_fmt(after['recall_at_5'])} ({after['core_hits_at_5']}/{after['core_cases']}) "
            f"| {row['gains']} | {row['losses']} | {row['rr_better']} | {row['rr_worse']} | {_fmt(row['rr_sign_p'])} "
            f"| {_fmt((after['no_match'] or {}).get('separation_auc'))} |"
        )
    download = meta.get("download_bytes")
    lines += [
        "",
        "Gains / losses and RR better / worse compare the reranked list with the same embedder's vector list.",
        "",
        f"Reranker cost on this host (relative only): load {meta['load_seconds']:.1f} s, peak RSS "
        f"{meta['peak_rss_mb']:.0f} MB (after imports {meta['rss_after_imports_mb']:.0f} MB), download "
        f"{_fmt(download / 1e6 if download else None, 0)} MB; each query is one predict call over all its pairs, "
        f"timed {meta['timing_repeats']} times; a query's latency is its fastest timing.",
        "",
        "| Embedder | Queries at 30 pairs | ms p50 (30 pairs) | ms p95 (30 pairs) | ms p95 (all queries) |",
        "|---|---:|---:|---:|---:|",
    ]
    for item in sorted(section["latency"], key=lambda r: order[r["model"]]):
        lines.append(
            f"| {item['model']} | {item['queries_at_full_depth']} | {_fmt(item['p50_ms_full_depth'], 1)} "
            f"| {_fmt(item['p95_ms_full_depth'], 1)} | {_fmt(item['p95_ms_all'], 1)} |"
        )
    lines += ["", "| Embedder | Scope | Verdict | Why |", "|---|---|---|---|"]
    for item in sorted(section["verdicts"], key=lambda r: order[r["model"]]):
        lines.append(f"| {item['model']} | {item['scope']} | {item['verdict']} | {item['reason']} |")
    return lines


def _fmt(value, digits: int = 3) -> str:
    if value is None:
        return "n/a"
    return f"{value:.{digits}f}"


def _cell(text: str) -> str:
    return text.replace("|", "\\|").replace("\n", "\\n")


def render_markdown(report: dict) -> str:
    personas = list(report["corpus"])
    lines = [
        "# Embedding bake-off (VS-17)",
        "",
        f"Generated {report['generated_at']} on {report['host']['platform']} "
        f"({report['host']['machine']}, {report['host']['cpu_count']} CPUs, device {report['host']['device']}).",
        "Pure vector retrieval: cosine over L2-normalized vectors, top 10, ties by memory id. "
        "Recall@5 is case-level over keyword and paraphrase cases (the headline); MRR@10 over the same cases.",
        "record = one vector per memory from the app-like primary text; chunks = window title plus OCR in windows of "
        f"about {report['settings']['chunk_max_tokens']} tokens ({report['settings']['chunk_overlap_tokens']} overlap, "
        f"{report['settings']['chars_per_token']} chars per token), memory score = best chunk + "
        f"{report['settings']['multi_chunk_bonus']} per other chunk within {report['settings']['multi_chunk_window']} "
        f"of the best (at most {report['settings']['multi_chunk_bonus_cap']}).",
        "Timings and memory are from this host through PyTorch on CPU: compare models with each other only. "
        f"Document timings are the fastest of {report['settings']['timing_repeats']} passes (batches of "
        f"{report['settings']['doc_batch_size']}); query timings pool {report['settings']['timing_repeats']} "
        "passes of single-query encodes. Peak RSS is per model process and includes the PyTorch runtime.",
        "",
        "Corpus: "
        + "; ".join(
            f"{persona} {info['memories']} searchable memories, {info['cases']} queries ("
            + ", ".join(f"{count} {kind}" for kind, count in info["case_count_by_kind"].items())
            + ")"
            for persona, info in report["corpus"].items()
        )
        + ".",
        "",
        "## Retrieval quality",
        "",
        "Neg/Pos median top = median top-1 cosine on negative / positive queries; Gap = their difference "
        "(raw cosine, so not comparable across models); AUC = chance a positive query's top score beats a "
        "negative query's (scale free).",
        "",
        "| Persona | Mode | Model | Recall@5 | MRR@10 | Keyword R@5 | Paraphrase R@5 | Time R@5 | App R@5 "
        "| Neg median top | Pos median top | Gap | AUC |",
        "|---|---|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|",
    ]
    order = {key: index for index, key in enumerate(MODELS)}
    scope_order = {scope: index for index, scope in enumerate([*personas, POOLED])}
    results = sorted(
        report["results"],
        key=lambda r: (scope_order[r["persona"]], MODES.index(r["mode"]), order[r["model"]]),
    )
    for result in results:
        metrics = result["metrics"]
        by_kind = metrics["recall_at_5_by_kind"]
        no_match = metrics["no_match"] or {}
        lines.append(
            f"| {result['persona']} | {result['mode']} | {result['model']} "
            f"| {_fmt(metrics['recall_at_5'])} ({metrics['core_hits_at_5']}/{metrics['core_cases']}) "
            f"| {_fmt(metrics['mrr_at_10'])} "
            + " ".join(f"| {_fmt(by_kind.get(kind))}" for kind in POSITIVE_KINDS)
            + f" | {_fmt(no_match.get('top_score_median'))} | {_fmt(no_match.get('positive_top_score_median'))} "
            f"| {_fmt(no_match.get('separation'))} | {_fmt(no_match.get('separation_auc'))} |"
        )

    if report["comparisons"]:
        lines += [
            "",
            "## Against MiniLM on the headline cases (paired, per query)",
            "",
            "Gains / losses = keyword or paraphrase cases this model recalls at 5 and MiniLM misses, or the reverse. "
            "RR better / worse = cases where the reciprocal rank within the top 10 is higher or lower than MiniLM's. "
            "p = two-sided exact sign test (McNemar) on those discordant cases.",
            "",
            "| Persona | Mode | Model | Gains | Losses | p (Recall@5) | RR better | RR worse | p (RR) |",
            "|---|---|---|---:|---:|---:|---:|---:|---:|",
        ]
        comparisons = sorted(
            report["comparisons"],
            key=lambda c: (scope_order[c["persona"]], MODES.index(c["mode"]), order[c["model"]]),
        )
        for comparison in comparisons:
            lines.append(
                f"| {comparison['persona']} | {comparison['mode']} | {comparison['model']} "
                f"| {comparison['gains']} | {comparison['losses']} | {_fmt(comparison['mcnemar_p'])} "
                f"| {comparison['rr_better']} | {comparison['rr_worse']} | {_fmt(comparison['rr_sign_p'])} |"
            )

    lines += [
        "",
        "## Cost on this host (relative only)",
        "",
        "| Model | Loaded from | Dim | Download MB | Peak RSS MB | RSS after imports MB | Load s "
        "| ms per record text | ms per chunk | Query ms p50 | Query ms p95 | Max input tokens | Truncated |",
        "|---|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|",
    ]
    for key in MODELS:
        info = report["models"].get(key)
        if info is None:
            continue
        if "error" in info:
            lines.append(f"| {key} | failed: {_cell(info['error'][:200])} |" + " |" * 11)
            continue
        units = info["units"]
        total = {mode: sum(units[p][mode]["units"] for p in personas) for mode in MODES}
        ms = {
            mode: sum(units[p][mode]["ms_per_unit"] * units[p][mode]["units"] for p in personas) / max(total[mode], 1)
            for mode in MODES
        }
        download = info["download_bytes"]
        lines.append(
            f"| {key} | {info['loaded_from']}@{info['revision'][:7]} | {info['dimension']} "
            f"| {_fmt(download / 1e6 if download else None, 0)} | {_fmt(info['peak_rss_mb'], 0)} "
            f"| {_fmt(info['rss_after_imports_mb'], 0)} | {_fmt(info['load_seconds'], 1)} "
            f"| {_fmt(ms['record'], 1)} | {_fmt(ms['chunks'], 1)} | {_fmt(info['query_ms_p50'], 1)} "
            f"| {_fmt(info['query_ms_p95'], 1)} | {info['max_input_tokens']} of {info['max_seq_length']} "
            f"| {info['truncated_inputs']} |"
        )
    first = next((info for info in report["models"].values() if "error" not in info), None)
    if first:
        lines += [
            "",
            "Chunking: "
            + "; ".join(
                f"{persona} {first['units'][persona]['chunks']['units']} chunks for "
                f"{report['corpus'][persona]['memories']} memories "
                f"(at most {first['units'][persona]['chunks']['max_units_per_memory']} per memory)"
                for persona in personas
            )
            + ".",
            "Real tokens per estimated token (chars / "
            f"{report['settings']['chars_per_token']}), worst input in chunks mode: "
            + ", ".join(
                f"{key} {max(info['units'][p]['chunks']['max_tokens_per_estimated_token'] for p in personas):.2f}"
                for key, info in report["models"].items()
                if "error" not in info
            )
            + ".",
            f"Versions: torch {first['versions']['torch']}, transformers {first['versions']['transformers']}, "
            f"sentence-transformers {first['versions']['sentence_transformers']}; {first['torch_threads']} torch threads.",
        ]
    failures = [
        f"{key}: {'; '.join(info['load_failures'])}"
        for key, info in report["models"].items()
        if "error" not in info and info.get("load_failures")
    ]
    if failures:
        lines += ["", "Sources that failed before the one used: " + " / ".join(failures)]

    lines += [
        "",
        "## Models",
        "",
        "| Model | Query prompt | Document prompt | Pooling | License | ONNX for the app's ort runtime |",
        "|---|---|---|---|---|---|",
    ]
    for key, spec in report["specs"].items():
        lines.append(
            f"| {key} ({spec['model_id']}) | {_cell(repr(spec['query_prompt']))} "
            f"| {_cell(repr(spec['document_prompt']))} | {spec['pooling']} | {spec['license']} | {spec['onnx']} |"
        )

    ranks = {}
    for result in report["results"]:
        if result["persona"] == POOLED and len(personas) > 1:
            continue
        for query in result["queries"]:
            ranks.setdefault((query["persona"], query["query"], query["kind"]), {})[
                (result["model"], result["mode"])
            ] = query["rank_at_10"]
    model_keys = [key for key in MODELS if key in report["models"] and "error" not in report["models"][key]]
    lines += [
        "",
        "## Rank of the first relevant memory per query (record / chunks; miss = not in top 10)",
        "",
        "| Persona | Query | Kind | " + " | ".join(model_keys) + " |",
        "|---|---|---|" + "---|" * len(model_keys),
    ]

    def rank_label(rank):
        return "miss" if rank is None else str(rank)

    for (persona, query, kind), cells in ranks.items():
        if kind == NEGATIVE_KIND:
            continue
        lines.append(
            f"| {persona} | {_cell(query)} | {kind} | "
            + " | ".join(
                f"{rank_label(cells.get((key, 'record')))} / {rank_label(cells.get((key, 'chunks')))}"
                for key in model_keys
            )
            + " |"
        )
    if "rerank" in report:
        lines += render_rerank_markdown(report["rerank"], personas)
    return "\n".join(lines) + "\n"


# --------------------------------------------------------------------------
# CLI


def parse_selection(raw: str, allowed, label: str) -> list[str]:
    if raw == "all":
        return list(allowed)
    chosen = [item.strip() for item in raw.split(",") if item.strip()]
    unknown = [item for item in chosen if item not in allowed]
    if unknown or not chosen:
        raise SystemExit(f"unknown {label}: {unknown or raw!r}; choose from {', '.join(allowed)} or all")
    return chosen


def main(argv=None) -> int:
    parser = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    parser.add_argument("--models", default="all", help=f"all or a comma list of {', '.join(MODELS)}")
    parser.add_argument("--personas", default="all", help=f"all or a comma list of {', '.join(PERSONAS)}")
    parser.add_argument("--out-json", type=Path)
    parser.add_argument("--out-md", type=Path)
    parser.add_argument("--device", default="cpu", help="torch device (default cpu)")
    parser.add_argument("--chunk-tokens", type=int, default=CHUNK_MAX_TOKENS)
    parser.add_argument("--chunk-overlap", type=int, default=CHUNK_OVERLAP_TOKENS)
    parser.add_argument("--timing-repeats", type=int, default=TIMING_REPEATS)
    parser.add_argument(
        "--rerank",
        action="store_true",
        help=f"VS-19: rerank each embedder's top {CANDIDATE_DEPTH} chunk-mode candidates with {RERANK_MODEL[0]}",
    )
    parser.add_argument("--worker-embed", help=argparse.SUPPRESS)
    parser.add_argument("--worker-rerank", type=Path, help=argparse.SUPPRESS)
    parser.add_argument("--worker-out", type=Path, help=argparse.SUPPRESS)
    args = parser.parse_args(argv)
    personas = parse_selection(args.personas, PERSONAS, "persona")

    if args.worker_rerank:
        result = rerank_worker(args.worker_rerank, args.device, args.timing_repeats)
        args.worker_out.write_text(json.dumps(result))
        return 0

    if args.worker_embed:
        result = embed_worker(
            args.worker_embed,
            personas,
            args.device,
            args.chunk_tokens,
            args.chunk_overlap,
            args.timing_repeats,
        )
        args.worker_out.write_text(json.dumps(result))
        return 0

    models = parse_selection(args.models, MODELS, "model")
    common = [
        "--personas", ",".join(personas),
        "--device", args.device,
        "--chunk-tokens", str(args.chunk_tokens),
        "--chunk-overlap", str(args.chunk_overlap),
        "--timing-repeats", str(args.timing_repeats),
    ]
    workers = {key: run_worker(["--worker-embed", key, *common], f"embed {key}") for key in models}
    report = build_report(args, workers, personas)
    reranked = None
    if args.rerank:
        with tempfile.TemporaryDirectory() as tmp:
            groups_path = Path(tmp) / "rerank-input.json"
            groups_path.write_text(json.dumps(rerank_groups(workers, personas)))
            reranked = run_worker(
                [
                    "--worker-rerank", str(groups_path),
                    "--device", args.device,
                    "--timing-repeats", str(args.timing_repeats),
                ],
                "rerank",
            )
        report["rerank"] = build_rerank_section(workers, personas, reranked)
    markdown = render_markdown(report)
    if args.out_json:
        args.out_json.parent.mkdir(parents=True, exist_ok=True)
        args.out_json.write_text(json.dumps(report, indent=2) + "\n")
    if args.out_md:
        args.out_md.parent.mkdir(parents=True, exist_ok=True)
        args.out_md.write_text(markdown)
    print(markdown, end="")
    failed = any("error" in worker for worker in workers.values()) or (reranked is not None and "error" in reranked)
    return 1 if failed else 0


if __name__ == "__main__":
    sys.exit(main())
