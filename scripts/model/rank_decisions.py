#!/usr/bin/env python3
"""Rank bounded-decision candidates using the FNDR decision rubric."""

WEIGHTS = {"volume": 2, "cost": 2, "labels": 2, "bounded": 1, "stakes": 1}


def rank(decisions):
    """Return `(total, name, scores)` rows, ranked highest score then name."""
    rows = []
    for name, scores in decisions.items():
        if set(scores) != set(WEIGHTS):
            raise ValueError(f"{name}: need exactly {sorted(WEIGHTS)}")
        if any(not 1 <= value <= 5 for value in scores.values()):
            raise ValueError(f"{name}: scores must be 1 to 5")
        total = sum(WEIGHTS[key] * value for key, value in scores.items())
        rows.append((total, name, scores))
    return sorted(rows, key=lambda row: (-row[0], row[1]))
