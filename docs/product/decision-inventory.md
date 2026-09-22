# Decision inventory

Date: 2026-09-22

This inventory applies the published DEC-01 rubric to FNDR's current code.
The score weights volume, current cost, and label availability twice; bounded
options and stakes once. Stakes control the required escalation and review
posture, not the priority score. Scores are reproduced from the WS6 plan so
they are deterministic and do not rely on captured user data.

| Decision | Current location and input | Options | Current mechanism and known failure | Label source | Vol | Cost | Labels | Bounded | Stakes | Total |
| --- | --- | --- | --- | --- | ---: | ---: | ---: | ---: | ---: | ---: |
| Enrichment gate | `capture_pixel_vlm_route`, frame pressure and quality signals | run, skip | Pressure and rules can spend VLM work on low-value frames and provide no calibrated confidence. | Frame-quality and enrichment-review outcomes, collected before activation | 5 | 5 | 3 | 5 | 3 | 34 |
| Merge vs append vs new | `merge_or_append_memory_record`, candidate similarities and app context | merge, append, new | Fixed similarity and lexical thresholds can create false merges or duplicate records. | MEM-04 replay sessions and later correction outcomes | 5 | 3 | 4 | 5 | 4 | 33 |
| Review verdict | `memory_review/pipeline`, proposed patch plus validators | pass, review, fail | The current LLM review happens before a bounded prediction can avoid costly likely failures. | `review_failed` records and the model gold set | 3 | 4 | 4 | 5 | 3 | 30 |
| Activity type | `StructuredMemoryExtraction.activity_type`, OCR evidence | 19 canonical activity types | A general LLM emits a JSON value that can be invalid or multi-valued before normalization. | WS2 extraction gold set | 5 | 3 | 4 | 3 | 2 | 29 |
| Admission | `capture/admission`, frame and text signals | keep, drop | Rules need a measured uncertainty path for marginal frames. | Capture-quality review outcomes | 5 | 2 | 3 | 5 | 4 | 29 |
| Sensitive context | privacy blocklist and capture privacy checks | allow, escalate, block | Heuristics have incomplete coverage, but no model may override a code-level block. | Privacy guard set | 5 | 2 | 3 | 4 | 5 | 29 |
| Query intent and route | `context_runtime/query_plan`, user query | bounded retrieval routes | Rule and LLM routing has unmeasured confidence. | Query evaluation set | 3 | 4 | 3 | 4 | 2 | 26 |
| Result relevance verify | `context_runtime/verifier`, retrieved evidence | grounded, review | Explicit checks may reject useful partial answers without a confidence measure. | Answer review outcomes | 3 | 3 | 3 | 5 | 3 | 26 |
| Entity resolution | `context_runtime/entity_route`, entity names and aliases | same, distinct | Alias and matching rules have sparse labeled corrections. | User merge and split corrections | 4 | 2 | 2 | 5 | 3 | 24 |
| Task extraction | `maybe_create_tasks_from_memory`, structured memory | task, no task | Rules and LLM extraction can create noisy tasks. | User task edits and dismissals | 3 | 3 | 2 | 5 | 2 | 23 |
| Deja vu | WS3 FEA-04 error signature input | same, distinct | No implementation or baseline exists yet. | Error-signature fixture set | 2 | 1 | 3 | 5 | 2 | 19 |
| Tool-call safety | `agent/policy`, action kind, risk, and mode | allow, review, deny | Hand-written policy is authoritative; a decider can only escalate. | Policy fixtures and audit outcomes | 1 | 2 | 2 | 4 | 5 | 19 |

## Selected pilots

1. **Enrichment gate.** Good means reducing VLM work at the stated automated
   share without reducing gold-set capture quality. It is selected first for
   the resource benefit on an 8 GB M1. A decider can only advise the existing
   rules and may not bypass privacy or admission blocks.
2. **Merge vs append vs new.** Good means no additional false merges on
   replayed MEM-04 sessions while reducing duplicate records. The three
   options fit the decision-core contract directly. Activation waits for the
   replay labels and a held-out calibration report.
3. **Review verdict.** Good means routing likely failures to review before the
   expensive review workflow, at a precision threshold chosen from held-out
   labels. It remains advisory until the report and review queue integration
   exist.

## Guardrails

- A confidence score never overrides a code-level deny rule.
- Each pilot requires a labeled evaluation set, calibration measurement, and
  coverage-versus-precision report before it is connected to a runtime path.
- Uncertain outcomes are unresolved and go to review or a person.
