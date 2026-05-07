# Calibration log — oracles-fewhiccupps

The L3 grader `g-oracle-judge` is **not yet calibrated**. It will auto-skip on every run until a green calibration record is appended to `calibration.jsonl` next to this file.

## Calibration plan

1. Draw 50 outputs covering positive and negative tasks across all recorded axes (`complexity`, `target_kind`).
2. Two SMEs grade each output independently against `rubrics/sme_quality.md`. Resolve disagreements before recording.
3. Run the L3 judge against the same outputs using `rubrics/judge_oracle_quality.md`.
4. Compute per-dimension agreement (Honesty is the headline; Statutes mismatch is a high-severity sub-signal).
5. Append a record to `calibration.jsonl`. Set `deployed: true` only if Honesty agreement ≥ 0.85 and the recipe's declared `max_divergence_from_l2` is satisfied.

Use the `eval-bench/calibrate.py` CLI:

```bash
python eval-bench/calibrate.py recipes/test/oracles-fewhiccupps \
    --grader-id g-oracle-judge \
    --judge anthropic \
    --note "initial calibration"
```

## History

| Date | Judge | Sample | Honesty agreement | Deployed | Notes |
|------|-------|--------|-------------------|----------|-------|
| (no record yet) | | | | | initial bring-up |

## Banned shortcut

Do not "calibrate" by re-running the judge on the same outputs the rubric was tuned on. Calibration must use held-out outputs neither the SME rubric nor the judge rubric was developed against.

## Statutes-specific watch

The Statutes oracle is the most consequential failure surface (acting on a fabricated law is worse than misjudging a Familiar problems heuristic). Track the divergence_breakdown for `fail_vs_pass` pairs separately on Statutes-heavy tasks (`endpoint-oauth-token`, `screen-account-deletion`, `screen-checkout-confirmation`). If those slices show systematic L3-says-pass-when-SME-says-fail, recalibrate immediately.
