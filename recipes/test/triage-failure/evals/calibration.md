# Calibration log — triage-failure

The L3 grader `g-triage-judge` is **not yet calibrated**. It will auto-skip on every run until a green calibration record is appended to `calibration.jsonl` next to this file.

## Calibration plan

1. Draw 50 outputs covering positive and negative tasks across all recorded axes (`complexity`, `failure_kind`).
2. Two SMEs grade each output independently against `rubrics/sme_quality.md`. Resolve disagreements before recording. Pay special attention to the Honesty and No-fix-proposals dimensions — these are the recipe's high-stakes failures.
3. Run the L3 judge against the same outputs using `rubrics/judge_triage_quality.md`.
4. Compute Honesty agreement (the headline metric for this recipe).
5. Append a record to `calibration.jsonl`. Set `deployed: true` only if Honesty agreement ≥ 0.85 and the recipe's declared `max_divergence_from_l2` is satisfied.

```bash
python eval-bench/calibrate.py recipes/test/triage-failure \
    --grader-id g-triage-judge \
    --judge anthropic \
    --note "initial calibration"
```

## History

| Date | Judge | Sample | Honesty agreement | Deployed | Notes |
|------|-------|--------|-------------------|----------|-------|
| (no record yet) | | | | | initial bring-up |

## Banned shortcut

Do not "calibrate" by re-running the judge on the same outputs the rubric was tuned on. Calibration must use held-out outputs neither the SME rubric nor the judge rubric was developed against.

## Honesty-failure watch

For triage, Honesty failures break down into two categories. Track both separately in calibration runs:

- **Fabricated frame** — judge said pass, SME said fail, with the SME citing a frame that does not appear in the inputs.
- **Fix proposal** — judge said pass, SME said fail, with the SME citing a code-change suggestion in the output.

If `divergence_breakdown` shows systematic L3-says-pass-when-SME-says-fail on either pattern, recalibrate immediately. The judge rubric's hard rules need to be sharper.
