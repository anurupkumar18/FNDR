#!/usr/bin/env python3
"""Deterministically score local post-capture review outputs from JSONL."""

import argparse
import json
from collections import Counter
from pathlib import Path


LABEL_STATUSES = {"draft", "human_reviewed"}
OUTCOMES = {
    "reviewed_local",
    "reviewed_daily",
    "review_failed",
    "skipped",
    "provider_error",
}
HUMAN_DECISIONS = {"improves", "preserves", "rejects"}
SPLITS = {"dev", "test", "guard"}


def fail(case_id, message):
    raise ValueError(f"{case_id}: {message}")


def require_string(case_id, row, field):
    value = row.get(field)
    if not isinstance(value, str) or not value.strip():
        fail(case_id, f"{field} must be a non-empty string")
    return value


def require_terms(case_id, expected, field):
    terms = expected.get(field)
    if not isinstance(terms, list) or not all(isinstance(term, str) and term.strip() for term in terms):
        fail(case_id, f"expected.{field} must be a list of non-empty strings")
    return terms


def validate_case(row):
    if not isinstance(row, dict):
        raise ValueError("case must be a JSON object")
    case_id = require_string("<unknown>", row, "id")
    split = require_string(case_id, row, "split")
    if split not in SPLITS:
        fail(case_id, f"split must be one of {sorted(SPLITS)}")

    require_string(case_id, row, "labeler")
    label_status = require_string(case_id, row, "label_status")
    if label_status not in LABEL_STATUSES:
        fail(case_id, f"label_status must be one of {sorted(LABEL_STATUSES)}")
    if label_status == "human_reviewed":
        require_string(case_id, row, "reviewed_by")
    require_string(case_id, row, "evidence")

    if not isinstance(row.get("before"), dict):
        fail(case_id, "before must be an object")
    if not isinstance(row.get("reviewed"), dict):
        fail(case_id, "reviewed must be an object")

    outcome = require_string(case_id, row, "outcome")
    if outcome not in OUTCOMES:
        fail(case_id, f"outcome must be one of {sorted(OUTCOMES)}")

    expected = row.get("expected")
    if not isinstance(expected, dict):
        fail(case_id, "expected must be an object")
    require_string(case_id, expected, "activity_type")
    require_terms(case_id, expected, "must_mention")
    require_terms(case_id, expected, "must_not")
    decision = require_string(case_id, expected, "human_decision")
    if decision not in HUMAN_DECISIONS:
        fail(case_id, f"expected.human_decision must be one of {sorted(HUMAN_DECISIONS)}")

    return row


def load_cases(path):
    rows = []
    for number, line in enumerate(Path(path).read_text().splitlines(), start=1):
        if not line.strip():
            continue
        try:
            row = json.loads(line)
        except json.JSONDecodeError as error:
            raise ValueError(f"line {number}: invalid JSON: {error.msg}") from error
        rows.append(validate_case(row))
    if not rows:
        raise ValueError("review fixture has no cases")
    return rows


def reviewed_text(reviewed):
    return "\n".join(
        value for value in reviewed.values() if isinstance(value, str)
    ).casefold()


def contains(text, term):
    return term.casefold() in text


def rate(numerator, denominator):
    return numerator / denominator if denominator else 0.0


def score_cases(cases):
    if not cases:
        raise ValueError("cannot score an empty case set")

    outcomes = Counter()
    expected_terms = 0
    matched_terms = 0
    forbidden_violations = 0
    activity_matches = 0
    unsafe_cases = 0
    improves = 0
    improved = 0

    for row in cases:
        expected = row["expected"]
        text = reviewed_text(row["reviewed"])
        required_hits = [contains(text, term) for term in expected["must_mention"]]
        forbidden_hits = [contains(text, term) for term in expected["must_not"]]
        activity_matches += row["reviewed"].get("activity_type") == expected["activity_type"]
        expected_terms += len(required_hits)
        matched_terms += sum(required_hits)
        forbidden_violations += sum(forbidden_hits)

        safe = (
            all(required_hits)
            and not any(forbidden_hits)
            and row["outcome"] not in {"review_failed", "provider_error"}
        )
        unsafe_cases += not safe
        if expected["human_decision"] == "improves":
            improves += 1
            improved += safe
        outcomes[row["outcome"]] += 1

    result = {
        "case_count": len(cases),
        "outcome_counts": dict(sorted(outcomes.items())),
        "structural": {
            "required_terms": expected_terms,
            "matched_required_terms": matched_terms,
            "forbidden_term_violations": forbidden_violations,
            "unsafe_cases": unsafe_cases,
        },
    }
    if all(row["label_status"] == "human_reviewed" for row in cases):
        result["quality_score_available"] = True
        result["quality"] = {
            "required_term_recall": rate(matched_terms, expected_terms),
            "forbidden_term_violations": forbidden_violations,
            "activity_accuracy": rate(activity_matches, len(cases)),
            "unsafe_review_rate": rate(unsafe_cases, len(cases)),
            "improvement_rate": rate(improved, improves),
        }
    else:
        result["quality_score_available"] = False
    return result


def render_report(name, split, result):
    lines = [
        f"# Post-capture review score: {name}",
        "",
        f"- Split: {split}",
        f"- Cases: {result['case_count']}",
        f"- Quality score available: {'yes' if result['quality_score_available'] else 'no'}",
        f"- Outcomes: {json.dumps(result['outcome_counts'], sort_keys=True)}",
        f"- Structural safety: {json.dumps(result['structural'], sort_keys=True)}",
    ]
    if result["quality_score_available"]:
        lines.append(f"- Quality: {json.dumps(result['quality'], sort_keys=True)}")
    else:
        lines.append("- Quality withheld: selected cases include draft labels.")
    return "\n".join(lines) + "\n"


def main(argv=None):
    parser = argparse.ArgumentParser()
    parser.add_argument("review_jsonl")
    parser.add_argument("--split", choices=["all", *sorted(SPLITS)], default="dev")
    parser.add_argument("--report-guard", action="store_true")
    parser.add_argument("--name", default="local-review")
    parser.add_argument("--out", required=True)
    args = parser.parse_args(argv)

    cases = load_cases(args.review_jsonl)
    selected = cases if args.split == "all" else [case for case in cases if case["split"] == args.split]
    includes_guard = args.split == "guard" or (args.split == "all" and any(case["split"] == "guard" for case in selected))
    if includes_guard and not args.report_guard:
        raise ValueError("guard cases require --report-guard and must not be used for tuning")
    result = score_cases(selected)
    output = Path(args.out)
    output.parent.mkdir(parents=True, exist_ok=True)
    output.write_text(render_report(args.name, args.split, result))
    print(f"wrote {output}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
