#!/usr/bin/env python3
"""Pure scoring functions for the FNDR model eval. No model, no network.

Predictions and gold cases are JSONL. See docs/superpowers/plans/2026-09-21-ws2-local-model-harness.md.
"""
import argparse
import json
import math
import re
import sys
from pathlib import Path


# Fields the extraction prompt asks the model to produce (StructuredMemoryExtraction in inference/mod.rs).
# Adjust after the MOD-02 inventory if the prompt asks for more or fewer keys.
DEFAULT_REQUIRED = ["activity_type", "topic", "memory_context", "user_intent", "entities"]
DEFAULT_TEXT_FIELDS = ["topic", "memory_context", "user_intent", "entities", "decisions", "errors", "next_steps"]


def recall_at_k(ranked_ids, relevant_ids, k):
    relevant = set(relevant_ids)
    if not relevant:
        return 0.0
    return len(set(ranked_ids[:k]) & relevant) / len(relevant)


def mrr_at_k(ranked_ids, relevant_ids, k):
    relevant = set(relevant_ids)
    for rank, rid in enumerate(ranked_ids[:k], start=1):
        if rid in relevant:
            return 1.0 / rank
    return 0.0


def wilson_interval(successes, n, z=1.96):
    """95 percent Wilson score interval for a proportion."""
    if n == 0:
        return (0.0, 0.0)
    p = successes / n
    denom = 1 + z * z / n
    centre = (p + z * z / (2 * n)) / denom
    half = z * math.sqrt(p * (1 - p) / n + z * z / (4 * n * n)) / denom
    return (max(0.0, centre - half), min(1.0, centre + half))


def _norm(text):
    return re.sub(r"\s+", " ", str(text).lower()).strip()


def is_supported(value, evidence, min_overlap=0.6):
    """A value is supported if it is a substring of the evidence or most of its words appear in it."""
    v, e = _norm(value), _norm(evidence)
    if not v:
        return True
    if v in e:
        return True
    words = re.findall(r"\w+", v)
    if not words:
        return True
    evidence_words = set(re.findall(r"\w+", e))
    return sum(w in evidence_words for w in words) / len(words) >= min_overlap


def parse_prediction(raw, required_keys):
    """Return the parsed object or None if it is not valid JSON with the required keys."""
    try:
        obj = json.loads(raw)
    except (TypeError, ValueError):
        return None
    if not isinstance(obj, dict) or any(k not in obj for k in required_keys):
        return None
    return obj


def score_extraction(cases, predictions, required_keys, text_fields):
    """cases: {id: {"evidence": str, "expected": {...}}}; predictions: {id: raw model output string}."""
    valid = 0
    grounded_fields = 0
    total_fields = 0
    activity_ok = 0
    activity_n = 0
    for case_id, case in cases.items():
        obj = parse_prediction(predictions.get(case_id), required_keys)
        expected_activity = case["expected"].get("activity_type")
        if expected_activity is not None:
            # An invalid output counts as wrong, so it stays in the denominator.
            activity_n += 1
        if obj is None:
            continue
        valid += 1
        for field in text_fields:
            value = obj.get(field)
            values = value if isinstance(value, list) else [value]
            for item in values:
                if item in (None, ""):
                    continue
                total_fields += 1
                grounded_fields += is_supported(item, case["evidence"])
        if expected_activity is not None:
            activity_ok += obj.get("activity_type") == expected_activity
    n = len(cases)
    return {
        "cases": n,
        "format_validity": (valid, n, wilson_interval(valid, n)),
        "grounding_rate": (grounded_fields, total_fields, wilson_interval(grounded_fields, total_fields)),
        "activity_accuracy": (activity_ok, activity_n, wilson_interval(activity_ok, activity_n)),
    }


def score_retrieval(questions, rankings, k_recall=5, k_mrr=10):
    """questions: {id: [relevant ids]}; rankings: {id: [ranked ids]}."""
    if not questions:
        return {"questions": 0, "recall_at_5": 0.0, "mrr_at_10": 0.0}
    recalls = [recall_at_k(rankings.get(q, []), rel, k_recall) for q, rel in questions.items()]
    mrrs = [mrr_at_k(rankings.get(q, []), rel, k_mrr) for q, rel in questions.items()]
    return {
        "questions": len(questions),
        "recall_at_5": sum(recalls) / len(recalls),
        "mrr_at_10": sum(mrrs) / len(mrrs),
    }


def flat_metrics(model_label, extraction, retrieval):
    """Flat JSON-friendly metrics in [0, 1]. Consumed by promotion_gate.py and the Intelligence panel."""
    def rate(triple):
        hits, n, _ = triple
        return hits / n if n else 0.0

    return {
        "model_label": model_label,
        "cases": extraction["cases"],
        "format_validity": rate(extraction["format_validity"]),
        "grounding_rate": rate(extraction["grounding_rate"]),
        "activity_accuracy": rate(extraction["activity_accuracy"]),
        "recall_at_5": retrieval["recall_at_5"],
        "mrr_at_10": retrieval["mrr_at_10"],
        "questions": retrieval["questions"],
    }


def _fmt(triple):
    hits, n, (lo, hi) = triple
    rate = hits / n if n else 0.0
    return f"{100 * rate:.1f} percent ({hits}/{n}, 95 percent CI {100 * lo:.1f} to {100 * hi:.1f})"


def render_report(title, model_label, extraction, retrieval):
    lines = [
        f"# {title}",
        "",
        f"- Model: {model_label}",
        f"- Extraction cases: {extraction['cases']}",
        f"- Format validity: {_fmt(extraction['format_validity'])}",
        f"- Grounding rate: {_fmt(extraction['grounding_rate'])}",
        f"- Activity accuracy: {_fmt(extraction['activity_accuracy'])}",
        f"- Retrieval questions: {retrieval['questions']}",
        f"- Recall@5: {retrieval['recall_at_5']:.3f}",
        f"- MRR@10: {retrieval['mrr_at_10']:.3f}",
        "",
        "With n near 50, differences smaller than the confidence interval are noise.",
    ]
    return "\n".join(lines) + "\n"


def _read_jsonl(path):
    return [json.loads(line) for line in Path(path).read_text().splitlines() if line.strip()]


def main(argv):
    ap = argparse.ArgumentParser()
    ap.add_argument("--gold", required=True, help="gold extraction cases JSONL: id, evidence, expected")
    ap.add_argument("--predictions", required=True, help="JSONL: id, output")
    ap.add_argument("--questions", required=True, help="retrieval gold JSONL: id, relevant")
    ap.add_argument("--rankings", required=True, help="JSONL: id, ranked")
    ap.add_argument("--required-keys", nargs="*", default=DEFAULT_REQUIRED)
    ap.add_argument("--text-fields", nargs="*", default=DEFAULT_TEXT_FIELDS)
    ap.add_argument("--model-label", default="unknown")
    ap.add_argument("--out", required=True)
    ap.add_argument("--json-out", help="also write flat metrics JSON (for promotion_gate.py and the app)")
    args = ap.parse_args(argv)
    cases = {c["id"]: c for c in _read_jsonl(args.gold)}
    preds = {p["id"]: p["output"] for p in _read_jsonl(args.predictions)}
    questions = {q["id"]: q["relevant"] for q in _read_jsonl(args.questions)}
    rankings = {r["id"]: r["ranked"] for r in _read_jsonl(args.rankings)}
    extraction = score_extraction(
        cases, preds, required_keys=args.required_keys, text_fields=args.text_fields,
    )
    retrieval = score_retrieval(questions, rankings)
    Path(args.out).parent.mkdir(parents=True, exist_ok=True)
    Path(args.out).write_text(render_report("Model eval", args.model_label, extraction, retrieval))
    if args.json_out:
        Path(args.json_out).parent.mkdir(parents=True, exist_ok=True)
        Path(args.json_out).write_text(
            json.dumps(flat_metrics(args.model_label, extraction, retrieval), indent=2) + "\n"
        )
    print(f"wrote {args.out}")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
