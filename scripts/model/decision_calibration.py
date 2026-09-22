#!/usr/bin/env python3
"""Calibration and abstain-threshold tools for bounded decisions."""

import math


def softmax(logits, temperature=1.0):
    scaled = [value / temperature for value in logits]
    top = max(scaled)
    exps = [math.exp(value - top) for value in scaled]
    total = sum(exps)
    return [value / total for value in exps]


def nll(logit_rows, labels, temperature=1.0):
    """Mean negative log-likelihood of the true labels."""
    total = 0.0
    for logits, label in zip(logit_rows, labels):
        total += -math.log(max(softmax(logits, temperature)[label], 1e-12))
    return total / len(labels)


def fit_temperature(logit_rows, labels, low=0.25, high=8.0, step=0.05):
    """Grid-search the temperature that minimizes negative log-likelihood."""
    best_t, best = 1.0, float("inf")
    temperature = low
    while temperature <= high + 1e-9:
        value = nll(logit_rows, labels, temperature)
        if value < best:
            best, best_t = value, temperature
        temperature += step
    return round(best_t, 4)


def confidences_and_correctness(logit_rows, labels, temperature=1.0):
    confs, correct = [], []
    for logits, label in zip(logit_rows, labels):
        probs = softmax(logits, temperature)
        pick = max(range(len(probs)), key=lambda index: probs[index])
        confs.append(probs[pick])
        correct.append(pick == label)
    return confs, correct


def expected_calibration_error(confs, correct, bins=10):
    """Weighted confidence and accuracy gap across equal-width bins."""
    count = len(confs)
    error = 0.0
    for bucket in range(bins):
        low, high = bucket / bins, (bucket + 1) / bins
        indices = [
            index
            for index, confidence in enumerate(confs)
            if (confidence > low or bucket == 0) and confidence <= high
        ]
        if not indices:
            continue
        accuracy = sum(correct[index] for index in indices) / len(indices)
        confidence = sum(confs[index] for index in indices) / len(indices)
        error += len(indices) / count * abs(accuracy - confidence)
    return error


def threshold_for_precision(confs, correct, target_precision):
    """Return lowest `(threshold, coverage, precision)` reaching target, or None."""
    order = sorted(range(len(confs)), key=lambda index: -confs[index])
    best = None
    hits = 0
    for rank, index in enumerate(order, start=1):
        hits += correct[index]
        boundary = rank == len(order) or confs[order[rank]] < confs[index]
        if boundary and hits / rank >= target_precision:
            best = (confs[index], rank / len(order), hits / rank)
    return best


def route(confidence, automate_at, review_at):
    """Route a confidence to automation, review, or a person."""
    if review_at > automate_at:
        raise ValueError("review_at must not exceed automate_at")
    if confidence >= automate_at:
        return "automate"
    if confidence >= review_at:
        return "review"
    return "human"
