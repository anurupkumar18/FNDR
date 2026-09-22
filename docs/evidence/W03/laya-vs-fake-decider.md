# Laya sidecar spike

Status: rejected for a per-decision FNDR cascade on this machine.

## Machine result

- Host: the project M1 8 GB development machine.
- Setup: an isolated Python 3.12 venv at `~/Documents/FNDR Laya/venv`.
- Installed disk footprint: approximately 902 MB.
- Command: `cargo test laya_predict_returns_a_choice_and_confidence_for_a_simple_routing_question --lib -- --ignored --nocapture`.
- Cold inference: the two-option routing request ran for more than three
  minutes in the Python sidecar without returning an answer. It was stopped
  rather than consuming the rest of the development session.
- Resident memory at stop: 306,048 KiB. This is a point-in-time reading, not a
  peak-RSS claim.

## Verdict

Laya is not suitable as a subprocess-per-decision tier on an 8 GB M1. The
sidecar was intentionally left unconnected to capture, privacy, policy, and
agent action paths. No warm-latency or coverage-versus-precision claim is made:
the cold run did not finish, and no real labeled enrichment-gate data exists in
this checkout.

The useful outcome is architectural: the DEC-02 `Decider` contract supports a
typed local tier, but any future Laya experiment needs a persistent process,
an explicit memory budget, completed local benchmarks, and a real held-out
pilot set before it can be reconsidered. The rules tier and human review remain
the safe cascade path.
