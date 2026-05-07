# Calibration log — bug-advocacy

The L3 grader `g-bug-judge` is **not yet calibrated**. It will auto-skip on every run until a green calibration record is appended to `calibration.jsonl` next to this file.

## Calibration plan

1. Draw 50 outputs covering positive and negative tasks across all recorded axes (`complexity`, `domain`).
2. Two SMEs grade each output independently against `rubrics/sme_quality.md`. Resolve disagreements before recording. Pay special attention to Reproducibility and No-fix-proposals — these are the two hard-rule dimensions.
3. Run the L3 judge against the same outputs using `rubrics/judge_bug_quality.md`.
4. Compute Reproducibility agreement (the headline metric for this recipe).
5. Append a record to `calibration.jsonl`. Set `deployed: true` only if Reproducibility agreement ≥ 0.85 and the recipe's declared `max_divergence_from_l2` is satisfied.

```bash
python eval-bench/calibrate.py recipes/test/bug-advocacy \
    --grader-id g-bug-judge \
    --judge anthropic \
    --note "initial calibration"
```

## History

| Date | Judge | Sample | Reproducibility agreement | Deployed | Notes |
|------|-------|--------|---------------------------|----------|-------|
| (no record yet) | | | | | initial bring-up |

## Banned shortcut

Do not "calibrate" by re-running the judge on the same outputs the rubric was tuned on. Calibration must use held-out outputs neither the SME rubric nor the judge rubric was developed against.

## Reproducibility-failure watch

For bug-advocacy, Reproducibility failures break down into three categories. Track all three separately in calibration runs:

- **Fabricated repro step** — judge said pass, SME said fail, with the SME citing a step that does not appear in the inputs.
- **Fix proposal** — judge said pass, SME said fail, with the SME citing a code-change suggestion in the report.
- **Fabricated environment value** — judge said pass, SME said fail, with the SME citing a browser / OS / build / feature flag value that wasn't in the inputs.

If `divergence_breakdown` shows systematic L3-says-pass-when-SME-says-fail on any of these, recalibrate immediately. The judge rubric's hard rules need to be sharper.
