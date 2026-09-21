#!/usr/bin/env python3
"""Calibration and abstain-threshold tools for bounded decisions (pure Python, no model needed).

A bounded decision picks one of a small fixed set of options and reports a confidence. Two facts make it usable:
  1. The confidence must be calibrated: among decisions reported at 0.9, about 90 percent should be correct.
  2. A threshold on that confidence decides which decisions are automated and which escalate to a stronger
     model or a person. It is chosen on a labeled set to hit a target precision, and the price is coverage.

`fit_temperature` rescales raw scores (logits) so confidences are honest. `threshold_for_precision` picks the
threshold. `route` applies two thresholds: automate, review, or hand to a person.
A valid decision is not a correct decision: these tools measure correctness, not format.
"""
import math


def softmax(logits, temperature=1.0):
    scaled = [x / temperature for x in logits]
    top = max(scaled)
    exps = [math.exp(x - top) for x in scaled]
    total = sum(exps)
    return [e / total for e in exps]


def nll(logit_rows, labels, temperature=1.0):
    """Mean negative log-likelihood of the true labels."""
    total = 0.0
    for logits, label in zip(logit_rows, labels):
        total += -math.log(max(softmax(logits, temperature)[label], 1e-12))
    return total / len(labels)


def fit_temperature(logit_rows, labels, low=0.25, high=8.0, step=0.05):
    """Grid-search the temperature that minimizes NLL. T above 1 softens overconfident scores."""
    best_t, best = 1.0, float("inf")
    t = low
    while t <= high + 1e-9:
        value = nll(logit_rows, labels, t)
        if value < best:
            best, best_t = value, t
        t += step
    return round(best_t, 4)


def confidences_and_correctness(logit_rows, labels, temperature=1.0):
    confs, correct = [], []
    for logits, label in zip(logit_rows, labels):
        probs = softmax(logits, temperature)
        pick = max(range(len(probs)), key=lambda i: probs[i])
        confs.append(probs[pick])
        correct.append(pick == label)
    return confs, correct


def expected_calibration_error(confs, correct, bins=10):
    """Weighted average gap between confidence and accuracy across equal-width confidence bins."""
    n = len(confs)
    error = 0.0
    for b in range(bins):
        low, high = b / bins, (b + 1) / bins
        idx = [i for i, c in enumerate(confs) if (c > low or b == 0) and c <= high]
        if not idx:
            continue
        accuracy = sum(correct[i] for i in idx) / len(idx)
        confidence = sum(confs[i] for i in idx) / len(idx)
        error += len(idx) / n * abs(accuracy - confidence)
    return error


def threshold_for_precision(confs, correct, target_precision):
    """Lowest threshold whose accepted decisions (confidence at or above it) reach `target_precision`.

    Returns (threshold, coverage, precision), where coverage is the share of decisions accepted, or None if no
    threshold reaches the target. Cuts are only made between distinct confidence values.
    """
    order = sorted(range(len(confs)), key=lambda i: -confs[i])
    best = None
    hits = 0
    for rank, i in enumerate(order, start=1):
        hits += correct[i]
        boundary = rank == len(order) or confs[order[rank]] < confs[i]
        if boundary and hits / rank >= target_precision:
            best = (confs[i], rank / len(order), hits / rank)
    return best


def route(confidence, automate_at, review_at):
    """Three-way routing: automate when sure, send to review when unsure, hand to a person when very unsure."""
    if review_at > automate_at:
        raise ValueError("review_at must not exceed automate_at")
    if confidence >= automate_at:
        return "automate"
    if confidence >= review_at:
        return "review"
    return "human"
