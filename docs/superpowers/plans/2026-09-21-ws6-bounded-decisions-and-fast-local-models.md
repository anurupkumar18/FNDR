# WS6 Bounded Decisions and Fast Local Models Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking. Commit after every step so Claude Code or Codex can resume from git alone.

**Goal:** Decide, with evidence, where FNDR should replace "ask a general LLM and parse the text" with a bounded decision that returns a choice plus a calibrated confidence, and build that layer locally so uncertain cases escalate to a stronger model or a person.

**Architecture:** A `Decider` contract (typed options in, choice and probabilities out), a cascade that tries the cheapest tier first and escalates on low confidence, calibration and threshold tools that turn raw scores into honest confidences, and a decision ledger that records every decision and its later correction. Tiers: rules, a small fast classifier, a local LLM constrained to an enumerated answer, then a person. The design borrows the idea behind TypeSafe's Jev, not the service.

**Tech Stack:** Rust (`src-tauri/src/decision/`), Python 3 (`scripts/model/`), llama.cpp with GBNF grammars, ONNX Runtime for encoder classifiers, existing FNDR embeddings.

**Spec:** `2026-09-21-beta-final-master-plan.md` sections 2, 3, 7. Tickets: DEC-01, DEC-02 (Beta, P1) and Final epics DEC-03 to DEC-06. Owner and accountable DRI: Anurup.

## Global Constraints

- Strictly local models. No captured content, and nothing derived from it, goes to a hosted model, including Jev (section 3).
- A decision model may inform escalation. It never overrides a code-level deny rule, and it is never the only barrier for a safety-critical action.
- Fixtures and labels are synthetic or public. No real captures in git.
- No commits to `main`. No em dashes in code comments, docs, or commit messages.
- Verification: `make test` plus the ticket's own verify command. Say what you ran.

---

## 1. Short answer

Your reading of Jev is right. It is a bounded-decision model, not an LLM replacement, and its confidence score is the interesting part. Three things to know before using it:

1. **Jev is a hosted, closed-weight API in early access behind a waitlist. There is nothing to download.** Captured screen content would leave your Mac, so it cannot be part of FNDR under the strictly-local rule.
2. **"RLCD" in the Jev context is TypeSafe's "Reinforcement Learning for Calibrated Decisions".** It is not the 2024 paper of the same acronym ("Reinforcement Learning from Contrastive Distillation"). Do not cite one for the other.
3. **The idea is reproducible locally**: a typed decision, calibrated probabilities, a threshold, and an escalation path. We can build that with open models and prove it with tests and a coverage-versus-precision curve on our own labeled data. That is also a better resume story than "we called an API".

## 2. What Jev is (verified against three sources, September 2026)

| Claim | Source | Status |
|---|---|---|
| A "System One" model: takes a state plus typed questions and returns typed decisions with probabilities, not text | https://typesafe.ai/blog/introducing-system-one-models-and-jev | Vendor, primary |
| Non-autoregressive: produces all outputs in one parallel pass | same | Vendor |
| Question types: a choice among up to 255 options, an ordered score, and a boolean assertion (the summary I fetched garbled that third name) | https://www.marktechpost.com/2026/09/19/typesafe-ai-releases-jev/ | Secondary summary of docs |
| Confidence is derived from the shape of the probability distribution: high when one answer dominates | same | Vendor |
| Trained with RLCD, "Reinforcement Learning for Calibrated Decisions" | primary blog | Vendor |
| Latency 70 to 500 ms end to end; price $0.042 per million input tokens, output free | primary blog | Vendor |
| Vendor benchmark: one task in 0.114 s, 193.6 times faster and 444.6 times cheaper than reference LLMs; the workflows were written by TypeSafe's own team and TypeSafe says it cannot prove the price is unsubsidized | MarkTechPost | Vendor-run, not independent |
| Gives up string generation; single-stage decisions cap at 255 options | primary blog | Vendor |
| Hosted API, early access, closed weights, no self-hosting; data leaves your network (the company says it will not train on customer inputs) | https://www.modemguides.com/blogs/ai-news/jev-typesafe-reality-check-run-locally | Third party, matches the other two |
| "Valid is not correct": schema validity is guaranteed, factual correctness is not | MarkTechPost | Matches your point |
| Suggested routing: high confidence to automation, middle to review, low to a person | MarkTechPost | Vendor guidance |

Treat every performance number as a claim to validate on our own data, not a fact.

## 3. How this fits FNDR's rules

| Use | Allowed? | Why |
|---|---|---|
| Send captured text, memories, or queries to Jev at runtime | **No** | Data leaves the device; violates strictly local |
| Send synthetic or public decision examples to Jev to get a rough upper bound on calibrated accuracy | **Only with owner approval (D-8)** | D-3 proposes no cloud-generated labels; a benchmark on public data is a narrower exception. Recommended: not before the local baseline exists |
| Read its documentation for API design (typed questions, several questions over one shared state, confidence semantics) | Yes | Ideas are free |
| Watch for open weights or an on-device release and re-evaluate | Yes | Add a line to the weekly research hour |

## 4. The pattern to build

```
evidence (OCR text, memory, action, query)
        |
        v
   Tier 0 rules                --confident?--> accept
        | not sure
        v
   Tier 1 fast classifier      --confident?--> accept      (encoder or embeddings + calibrated head)
        | not sure
        v
   Tier 2 local LLM, answer constrained to the option list  --confident?--> accept
        | not sure
        v
   Tier 3 review queue (person, or a stronger model when the owner allows it)
        |
        v
   decision ledger: input hash, options, choice, probabilities, confidence, tier, latency, later correction
        |
        +--> calibration and threshold refit on held-out labels (scripts/model/decision_calibration.py)
```

Three properties make it worth building:

1. **Calibrated confidence.** Among decisions reported at 0.9, about 90 percent should be right. `fit_temperature` rescales scores, and `expected_calibration_error` measures the gap.
2. **A threshold chosen for a target precision.** `threshold_for_precision` returns the lowest confidence that reaches, say, 95 percent precision and the share of decisions that clears it (coverage). The price of precision is coverage, and the report shows the trade.
3. **Several questions over one shared state, one forward pass.** Jev's API answers many typed questions about the same input at once. Locally the same shape is a multi-head classifier on a shared encoder pass: one embedding or encoder run answers "activity type", "sensitive?", "worth enriching?", and "same story as the last memory?" together. That is the efficiency win.

A valid answer is not a correct answer. Grammar-constrained decoding makes an LLM's answer valid by construction; calibration and held-out evaluation are what say whether it is correct.

## 5. Local building blocks (fast and cheap)

| Building block | Size | Good for | Caveat | Source |
|---|---|---|---|---|
| Existing rules and features (thresholds, regex, app class) | none | Tier 0, obvious cases | Brittle at the edges, which is why we want confidence | current code |
| FNDR embeddings plus a logistic-regression head | tiny head on vectors we already compute | Tier 1 with almost no marginal cost; easy to calibrate | Quality bounded by the embedding | classic method |
| Static embeddings (Model2Vec-style distillation of a sentence encoder) | very small, CPU only | Very fast features for classification | Weaker than a full encoder on subtle meaning | https://medium.com/kx-systems/model2vec-making-large-scale-embedding-generation-manageable-8cd55b7a288f |
| ModernBERT encoder (about 149M parameters) with classification heads | 149M | Domain, jailbreak, and PII classification in the vLLM Semantic Router; reported large routing speedups | Heavier than static embeddings; needs an ONNX or Candle path; check RAM on 8 GB | https://arxiv.org/pdf/2603.12646 |
| Gemma 3 270M fine-tuned for a task | 270M (170M of it embeddings) | Text classification, entity extraction, query routing, structured output, compliance checks; built to be fine-tuned | Gemma terms of use; needs a fine-tune (WS2 Tasks 10 and 11) | https://developers.googleblog.com/en/introducing-gemma-3-270m/ |
| FunctionGemma | small | Function-calling style tool selection | Verify size and license on the day | https://blog.google/innovation-and-ai/technology/developers-tools/functiongemma/ |
| Arch-Router (1.5B, Qwen2.5-1.5B base) | 1.5B | Routing by human-preference policies | Heavier than an encoder; license to check | https://arxiv.org/abs/2506.16655 |
| Local LLM with GBNF constrained to the option list, reading token probabilities | the model already loaded | The "local Jev pattern": one forward pass, valid by construction | Not calibrated until we calibrate it; sacrifices Jev's parallel questions | https://www.modemguides.com/blogs/ai-news/jev-typesafe-reality-check-run-locally |

Theory to lean on: cascades escalate when confidence is low (FrugalGPT lineage), a reject option is called selective classification, and raw token confidences are miscalibrated and need correction (UCCI). See https://arxiv.org/html/2603.04445v2 and https://arxiv.org/html/2605.18796. For guardrail-style triage, SafeRoute applies a larger safety model only to inputs a router marks hard: https://arxiv.org/abs/2502.12464.

## 6. Where in FNDR: the decision inventory

Scoring rubric, each 1 to 5, weights volume 2, current cost 2, labels available 2, bounded options 1, stakes 1 (maximum 40). Stakes do not raise priority by themselves; they set how strict the threshold and the escalation must be. The totals below were computed by script, not by hand. Re-score together in DEC-01.

| Decision | Where today | Mechanism today | Vol | Cost | Labels | Bounded | Stakes | Total |
|---|---|---|---|---|---|---|---|---|
| Enrichment gate: run the VLM on this frame? | `capture_pixel_vlm_route` (`capture/mod.rs:193`), pressure gates | Pressure check and rules | 5 | 5 | 3 | 5 | 3 | **34** |
| Merge, append, or new memory | `merge_or_append_memory_record` (`capture/mod.rs:3925`), scoring 4906 to 5108 | Cosine and lexical thresholds plus cross-app rules | 5 | 3 | 4 | 5 | 4 | **33** |
| Review verdict: is this patch grounded? | `memory_review/pipeline.rs` | LLM review then validators | 3 | 4 | 4 | 5 | 3 | **30** |
| Activity type (19 classes) | `StructuredMemoryExtraction.activity_type`, `normalize_activity_type` | Emitted by the LLM inside a JSON blob, then normalized | 5 | 3 | 4 | 3 | 2 | **29** |
| Admission: keep or drop the frame | `capture/admission.rs`, `text_heavy_override` | Rules | 5 | 2 | 3 | 5 | 4 | **29** |
| Sensitive-context detection | privacy blocklist, `capture/privacy` | Lists and heuristics | 5 | 2 | 3 | 4 | 5 | **29** |
| Query intent and route | `context_runtime/query_plan.rs` `plan`, `refine_plan_with_llm` (line 195) | Rules plus an LLM call per query | 3 | 4 | 3 | 4 | 2 | **26** |
| Result relevance verify | `context_runtime/verifier.rs` | Explicit rules | 3 | 3 | 3 | 5 | 3 | **26** |
| Entity resolution: same entity? | `context_runtime/entity_route.rs`, graph | Aliases and matching | 4 | 2 | 2 | 5 | 3 | **24** |
| Task extraction: is there a to-do? | `maybe_create_tasks_from_memory` (`capture/mod.rs:5360`) | Rules and LLM | 3 | 3 | 2 | 5 | 2 | **23** |
| Deja vu: same error? | new (WS3 FEA-04) | Signature match | 2 | 1 | 3 | 5 | 2 | **19** |
| Tool-call safety | `agent/policy.rs` `policy_for_action` | Hand-written match on kind, risk, mode | 1 | 2 | 2 | 4 | 5 | **19** |

**Proposed pilots (Final phase):** the enrichment gate (highest score, biggest resource win on 8 GB), merge decision (labels arrive from MEM-04), and review verdict (labels exist from `review_failed` cases and the gold set). Activity type is the natural quick win for Tier 1 because the gold set already labels it, and it removes a field the LLM currently has to emit. **Safety-critical rows (sensitive context, tool-call safety) use bounded decisions only to escalate**, never to allow; the code-level rules remain the authority.

## 7. Safety and quality rules for every decision

1. Confidence never overrides a deny rule. For actions, the policy in `agent/policy.rs` decides what is allowed; a decision model can only route an allowed-but-uncertain case to review.
2. Every decider ships with a labeled evaluation set, a calibration check (expected calibration error), and a coverage-versus-precision report before it goes live.
3. Thresholds are chosen on a held-out split, not the data used to fit the temperature. `decision_report.py` warns when they are the same.
4. Abstain is a first-class result. An `Unresolved` cascade result goes to the review queue with its best guess attached.
5. Drift is watched: the ledger records later corrections (a user edit, a `Wrong` rating from `agent/audit.rs`), and calibration is refit on a schedule.
6. Evaluate a decider on the guard set as well as the target set (WS2), so a gain on one decision does not hide a loss elsewhere.

## 8. What we can build around it

| Build | Reuses | Value |
|---|---|---|
| Decision ledger (one JSONL line per decision) | `telemetry/llm_trace.rs` pattern (WS2 Task 1) | Ground truth for calibration and for the Intelligence panel |
| Coverage-versus-precision view in the Intelligence panel | WS2 Task 5 panel, `decision_report.py` output | Shows "we automate X percent at Y percent precision", a measurable claim |
| Confidence-gated review queue | `agent/` action statuses (`Proposed`, `NeedsApproval`, `Approved`) and the Approval Queue screen (WS3) | One queue for uncertain merges, uncertain reviews, and uncertain agent actions |
| Multi-head decision service on one encoder pass | Tier 1 building blocks | One pass answers many questions per capture |
| Correction-to-calibration loop | `agent/audit.rs` feedback, WS2 promotion gate | The same evidence gate that promotes adapters promotes new thresholds |

## 9. Tasks

### Task 1: Decision inventory and three pilots (DEC-01, W2)

**Files:**
- Create: `docs/product/decision-inventory.md`
- Create: `scripts/model/rank_decisions.py`, `scripts/model/test_rank_decisions.py`

**Interfaces:**
- Produces: `rank(decisions) -> list[(total, name, scores)]` with weights `volume 2, cost 2, labels 2, bounded 1, stakes 1`.

- [ ] **Step 1: Write the failing test**

`scripts/model/test_rank_decisions.py`:

```python
import sys
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parent))
import rank_decisions as rd


class RankTests(unittest.TestCase):
    def test_weighted_total_and_order(self):
        decisions = {
            "Enrichment gate": dict(volume=5, cost=5, labels=3, bounded=5, stakes=3),
            "Merge vs append vs new": dict(volume=5, cost=3, labels=4, bounded=5, stakes=4),
            "Tool-call safety": dict(volume=1, cost=2, labels=2, bounded=4, stakes=5),
        }
        ranked = rd.rank(decisions)
        self.assertEqual([(t, n) for t, n, _ in ranked], [(34, "Enrichment gate"), (33, "Merge vs append vs new"), (19, "Tool-call safety")])

    def test_ties_are_ordered_by_name_and_scores_are_validated(self):
        ranked = rd.rank({"B": dict(volume=1, cost=1, labels=1, bounded=1, stakes=1), "A": dict(volume=1, cost=1, labels=1, bounded=1, stakes=1)})
        self.assertEqual([n for _, n, _ in ranked], ["A", "B"])
        with self.assertRaises(ValueError):
            rd.rank({"X": dict(volume=6, cost=1, labels=1, bounded=1, stakes=1)})
        with self.assertRaises(ValueError):
            rd.rank({"X": dict(volume=1, cost=1, labels=1, bounded=1)})


if __name__ == "__main__":
    unittest.main()
```

- [ ] **Step 2: Run to verify it fails**

Run: `python3 scripts/model/test_rank_decisions.py`
Expected: FAIL, `ModuleNotFoundError: No module named 'rank_decisions'`.

- [ ] **Step 3: Implement `scripts/model/rank_decisions.py`**

```python
#!/usr/bin/env python3
"""Rank bounded-decision candidates. Weights: volume 2, cost 2, labels 2, bounded 1, stakes 1 (maximum 40)."""
WEIGHTS = {"volume": 2, "cost": 2, "labels": 2, "bounded": 1, "stakes": 1}


def rank(decisions):
    rows = []
    for name, scores in decisions.items():
        if set(scores) != set(WEIGHTS):
            raise ValueError(f"{name}: need exactly {sorted(WEIGHTS)}")
        if any(not 1 <= v <= 5 for v in scores.values()):
            raise ValueError(f"{name}: scores must be 1 to 5")
        rows.append((sum(WEIGHTS[k] * v for k, v in scores.items()), name, scores))
    return sorted(rows, key=lambda r: (-r[0], r[1]))
```

- [ ] **Step 4: Run to verify it passes**

Run: `python3 scripts/model/test_rank_decisions.py`
Expected: `Ran 2 tests ... OK`.

- [ ] **Step 5: Fill the inventory with the team**

Create `docs/product/decision-inventory.md` from the table in section 6, one row per decision, re-scored together for 30 minutes. Add for each: the input the decision sees, the options, the cost of a wrong answer, where labels will come from, and the current failure you know of. Name three pilots and write one paragraph per pilot on what "good" means (for example, "automate 70 percent of merge decisions at 98 percent precision").

- [ ] **Step 6: Commit**

```bash
git checkout -b docs/dec-01-decision-inventory
git add scripts/model docs/product/decision-inventory.md
git commit -m "docs(decisions): inventory of bounded decisions ranked by a rubric, three pilots chosen"
```

### Task 2: Decision core, calibration, and the coverage-precision report (DEC-02, W3)

**Files:**
- Create: `scripts/model/decision_calibration.py`, `scripts/model/test_decision_calibration.py`, `scripts/model/decision_report.py`, `scripts/model/test_decision_report.py` (tested assets)
- Create: `src-tauri/src/decision/mod.rs`
- Modify: `src-tauri/src/lib.rs` (declare `pub mod decision;`)
- Create (by running): `docs/evidence/W03/decision-pilot-report.md`

**Interfaces:**
- Produces (Python): `softmax`, `nll`, `fit_temperature`, `confidences_and_correctness`, `expected_calibration_error`, `threshold_for_precision(confs, correct, target) -> (threshold, coverage, precision) or None`, `route(confidence, automate_at, review_at) -> "automate" or "review" or "human"`, and `decision_report.py <eval.jsonl> [--fit-jsonl fit.jsonl] --name N --out report.md` reading lines `{"logits": [...], "label": int}`.
- Produces (Rust): `DecisionOutcome { choice, probs, confidence, tier }`, `trait Decider`, `Tier { decider, accept_at }`, `Resolution::{Accepted, Unresolved}`, `cascade(tiers, evidence) -> Option<Resolution>`.

The Python tools and their 15 tests (11 in `test_decision_calibration.py`, 4 in `test_decision_report.py`) were written test-first and run before this plan was written. The Rust cascade and its 4 tests were compiled and run in an isolated crate.

- [ ] **Step 1: Copy the tested Python tools and run their tests**

```bash
cp docs/superpowers/plans/assets/2026-09-21/ws6/decision_calibration.py \
   docs/superpowers/plans/assets/2026-09-21/ws6/test_decision_calibration.py \
   docs/superpowers/plans/assets/2026-09-21/ws6/decision_report.py \
   docs/superpowers/plans/assets/2026-09-21/ws6/test_decision_report.py scripts/model/
python3 scripts/model/test_decision_calibration.py && python3 scripts/model/test_decision_report.py
```

Expected: `Ran 11 tests ... OK` then `Ran 4 tests ... OK`.

- [ ] **Step 2: Write the failing Rust tests in `src-tauri/src/decision/mod.rs` (tests only)**

```rust
#[cfg(test)]
mod tests {
    use super::*;

    struct Fixed {
        name: &'static str,
        pick: usize,
        conf: f32,
    }
    impl Decider for Fixed {
        fn name(&self) -> &'static str {
            self.name
        }
        fn options(&self) -> &'static [&'static str] {
            &["merge", "append", "new"]
        }
        fn decide(&self, _: &str) -> DecisionOutcome {
            let mut probs = vec![0.0; 3];
            probs[self.pick] = self.conf;
            DecisionOutcome { choice: self.pick, probs, confidence: self.conf, tier: self.name }
        }
    }
    fn tier(name: &'static str, pick: usize, conf: f32, accept_at: f32) -> Tier {
        Tier { decider: Box::new(Fixed { name, pick, conf }), accept_at }
    }

    #[test]
    fn the_first_confident_tier_wins_and_later_tiers_are_never_called() {
        let tiers = vec![tier("rules", 0, 0.97, 0.9), tier("llm", 2, 0.99, 0.5)];
        match cascade(&tiers, "x").unwrap() {
            Resolution::Accepted { outcome, escalated_from } => {
                assert_eq!(outcome.tier, "rules");
                assert!(escalated_from.is_empty());
            }
            other => panic!("unexpected {other:?}"),
        }
    }

    #[test]
    fn an_unsure_tier_escalates_and_records_who_was_skipped() {
        let tiers = vec![tier("rules", 0, 0.60, 0.9), tier("classifier", 1, 0.95, 0.9)];
        match cascade(&tiers, "x").unwrap() {
            Resolution::Accepted { outcome, escalated_from } => {
                assert_eq!(outcome.choice, 1);
                assert_eq!(escalated_from, vec!["rules"]);
            }
            other => panic!("unexpected {other:?}"),
        }
    }

    #[test]
    fn when_nobody_is_confident_the_best_guess_is_kept_for_review() {
        let tiers = vec![tier("rules", 0, 0.55, 0.9), tier("llm", 2, 0.70, 0.9)];
        match cascade(&tiers, "x").unwrap() {
            Resolution::Unresolved { best_guess, tried } => {
                assert_eq!(best_guess.choice, 2);
                assert_eq!(tried, vec!["rules", "llm"]);
            }
            other => panic!("unexpected {other:?}"),
        }
    }

    #[test]
    fn no_tiers_means_no_resolution() {
        assert!(cascade(&[], "x").is_none());
    }
}
```

- [ ] **Step 3: Run to verify it fails**

Add `pub mod decision;` to `lib.rs`, then run: `cd src-tauri && cargo test decision::`
Expected: compile FAIL, "cannot find type `Decider`" and "cannot find function `cascade`".

- [ ] **Step 4: Implement above the tests in the same file**

```rust
//! Bounded decisions with confidence, and an escalation cascade across tiers.
//!
//! A `Decider` picks one option from a fixed list and reports probabilities. Tiers are ordered from cheapest to
//! most expensive. The cascade accepts the first tier whose confidence reaches that tier's threshold, and
//! otherwise leaves the decision unresolved for review or a person. A valid decision is not a correct one:
//! thresholds must come from a labeled set (see scripts/model/decision_calibration.py).

#[derive(Debug, Clone, PartialEq)]
pub struct DecisionOutcome {
    pub choice: usize,
    pub probs: Vec<f32>,
    pub confidence: f32,
    pub tier: &'static str,
}

pub trait Decider: Send + Sync {
    fn name(&self) -> &'static str;
    fn options(&self) -> &'static [&'static str];
    fn decide(&self, evidence: &str) -> DecisionOutcome;
}

#[derive(Debug, Clone, PartialEq)]
pub enum Resolution {
    /// A tier was confident enough. `escalated_from` lists the tiers that were not.
    Accepted { outcome: DecisionOutcome, escalated_from: Vec<&'static str> },
    /// No tier was confident enough; the best guess is kept for the reviewer.
    Unresolved { best_guess: DecisionOutcome, tried: Vec<&'static str> },
}

pub struct Tier {
    pub decider: Box<dyn Decider>,
    pub accept_at: f32,
}

pub fn cascade(tiers: &[Tier], evidence: &str) -> Option<Resolution> {
    let mut escalated = Vec::new();
    let mut best: Option<DecisionOutcome> = None;
    for tier in tiers {
        let outcome = tier.decider.decide(evidence);
        if outcome.confidence >= tier.accept_at {
            return Some(Resolution::Accepted { outcome, escalated_from: escalated });
        }
        escalated.push(tier.decider.name());
        if best.as_ref().map_or(true, |b| outcome.confidence > b.confidence) {
            best = Some(outcome);
        }
    }
    best.map(|best_guess| Resolution::Unresolved { best_guess, tried: escalated })
}
```

- [ ] **Step 5: Run to verify it passes**

Run: `cd src-tauri && cargo test decision::`
Expected: PASS (4 tests).

- [ ] **Step 6: Produce the first coverage-versus-precision table on one pilot**

Take the merge pilot data from MEM-04 or the activity-type labels from the WS2 gold set, produce `{"logits": [...], "label": n}` lines from the current mechanism (for a rules tier, map the score to a two-logit pair; for the LLM tier, take the option-token logits under a GBNF grammar), split 50/50 into fit and eval files, and run:

```bash
python3 scripts/model/decision_report.py eval.jsonl --fit-jsonl fit.jsonl --name "activity type, current mechanism" --out docs/evidence/W03/decision-pilot-report.md
```

Expected: accuracy, calibration error before and after, and thresholds for 90, 95, and 99 percent precision with the automated share. If a target is not reachable the report says so; that is a result.

- [ ] **Step 7: Commit**

```bash
git checkout -b feat/dec-02-decision-core
git add scripts/model src-tauri/src/decision src-tauri/src/lib.rs docs/evidence/W03
git commit -m "feat(decision): Decider cascade, calibration tools, and coverage-precision report"
```

## 10. Final phase epics (W5 to W12), decomposed at the Beta retro

| Epic | Owner | Outcome and acceptance |
|---|---|---|
| DEC-03 Merge decision pilot | Anurup | A Tier 1 classifier over existing embeddings decides merge, append, or new with calibrated confidence; uncertain cases go to the review queue. Acceptance: on the MEM-04 replay sessions, zero more false merges than the baseline and fewer duplicates, at the stated automated share |
| DEC-04 Enrichment gate pilot | Anurup with Kunj | `should_enrich_now` (MEM-06) gains a calibrated "is this frame worth the VLM" score. Acceptance: fewer VLM calls at no loss on the gold set, with the coverage-versus-precision table |
| DEC-05 Review verdict pilot | Anurup | A decider predicts whether a review patch will pass validation before the expensive review runs. Acceptance: saves review calls at a precision target set in DEC-01 |
| DEC-06 Local Jev-pattern comparator | Anurup | One forward pass of the loaded local model under a GBNF grammar over an option list, reading token probabilities, compared with Tier 1 on the same labeled sets. Acceptance: a table of accuracy, calibration error, and latency for each tier; decision recorded in an ADR |
| DEC-07 Multi-head decision service | Anurup | One encoder pass answers activity type, sensitive-context flag, enrich, and merge candidacy. Acceptance: p95 latency and RAM per capture measured against four separate calls |
| DEC-08 Decision ledger and panel | Felipe with Anurup | Ledger lines drive a coverage-versus-precision view in the Intelligence panel. Acceptance: numbers on screen come from the ledger, not from a report file |

## Self-review

**Spec coverage (the new ask, part 3):** what Jev is (section 2), what "RLCD" means here and the name collision (section 1), whether and where it can be used (section 3), other trending open-source local fast and cheap models (section 5), where in existing systems (section 6), what to optimize (pilots), and what to build around it (section 8, tasks and epics).

**Placeholder scan:** section 3 leaves one decision to the owner (D-8), on purpose. Task 2 Step 6 depends on labels that MEM-04 and MOD-04 produce; the step names both sources.

**Type consistency:** `threshold_for_precision` returns `(threshold, coverage, precision)` and `decision_report.py` unpacks exactly that. `DecisionOutcome`, `Tier`, `Resolution`, and `cascade` match the tested Rust source. Rubric weights and totals match the run that produced the table.

## Sources

- TypeSafe, Introducing System One Models and Jev: https://typesafe.ai/blog/introducing-system-one-models-and-jev
- MarkTechPost, TypeSafe AI releases Jev (2026-09-19): https://www.marktechpost.com/2026/09/19/typesafe-ai-releases-jev/
- Jev reality check, can you run it locally: https://www.modemguides.com/blogs/ai-news/jev-typesafe-reality-check-run-locally
- RLCD (2024, different method, same acronym): https://arxiv.org/abs/2307.12950
- Arch-Router: https://arxiv.org/abs/2506.16655
- vLLM Semantic Router speedups with ModernBERT classifiers: https://arxiv.org/pdf/2603.12646
- Gemma 3 270M: https://developers.googleblog.com/en/introducing-gemma-3-270m/
- FunctionGemma: https://blog.google/innovation-and-ai/technology/developers-tools/functiongemma/
- Model2Vec: https://medium.com/kx-systems/model2vec-making-large-scale-embedding-generation-manageable-8cd55b7a288f
- Cascade and routing survey: https://arxiv.org/html/2603.04445v2
- UCCI, calibrated uncertainty for cascade routing: https://arxiv.org/html/2605.18796
- SafeRoute: https://arxiv.org/abs/2502.12464
