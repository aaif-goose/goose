# Calibration log — charter-sfdipot

The L3 grader `g-charter-judge` is **not yet calibrated**. It will auto-skip on every run until a green calibration record is appended to `calibration.jsonl` next to this file.

## Calibration plan

1. Draw 50 outputs covering positive and negative tasks across all recorded axes (`complexity`, `domain`).
2. Two SMEs grade each output independently against `rubrics/sme_quality.md`. Resolve disagreements before recording.
3. Run the L3 judge against the same outputs using `rubrics/judge_charter_quality.md`.
4. Compute per-dimension agreement (Honesty is the headline; others as resourcing allows).
5. Append a record to `calibration.jsonl`. Set `deployed: true` only if Honesty agreement ≥ 0.85 and the recipe's declared `max_divergence_from_l2` is satisfied.

## History

| Date | Judge | Sample | Honesty agreement | Deployed | Notes |
|------|-------|--------|-------------------|----------|-------|
| (no record yet) | | | | | initial bring-up |

## Banned shortcut

Do not "calibrate" by re-running the judge on the same outputs the rubric was tuned on. Calibration must use held-out outputs neither the SME rubric nor the judge rubric was developed against.
