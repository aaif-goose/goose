# Calibration log — `<recipe slug>`

This document is the human-readable narrative for L3 (LLM-as-judge) calibrations.
The machine-readable log lives at `calibration.jsonl` next to it; the harness
reads the JSONL log to decide whether to deploy each L3 grader on a given run.

## How calibration works here

1. Draw a set of recipe outputs (50+ recommended) covering both positive and
   negative tasks and a spread of recorded axes (model, complexity, language…).
2. Have one or more SMEs grade each output against the same rubric the L3
   judge uses. Resolve disagreements before recording.
3. Run the L3 judge against the same outputs.
4. Compute agreement and (recommended) Cohen's kappa.
5. Append a record to `calibration.jsonl` with `deployed: true` if agreement
   meets the recipe's threshold; `deployed: false` otherwise. The harness
   will skip the L3 grader on subsequent runs until a green calibration
   record is appended.

## History

| Date | Judge | Sample | Agreement | Kappa | Deployed | Notes |
|------|-------|--------|-----------|-------|----------|-------|
| (placeholder — replace) | g-judge-policy / claude-opus-4-7 | 50 | — | — | no | Initial template; no real run yet |

## Drift triggers

Re-calibrate immediately if any of these happen:
- Judge model is upgraded or swapped.
- The rubric is materially edited.
- A new active failure mode lands in `failure-modes.yaml`.
- Slice Explorer surfaces an unexpected pass-rate gap by judge_model.

## Banned shortcut

Do not "calibrate" by re-running the judge on the same set of human-graded
outputs that the rubric was tuned on. That measures the rubric, not the
judge. Calibration must use held-out outputs the judge has not seen during
rubric development.
