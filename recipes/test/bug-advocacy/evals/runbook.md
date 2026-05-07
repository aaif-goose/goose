# Runbook — bug-advocacy

## Ownership

| Role | Name |
|---|---|
| Recipe owner | Head of Software Quality |
| Backup owner | _replace_ |
| L2 reviewers | testers and engineers who triage bug reports for a living |

## Cadence

- **Daily** — work the Annotation Queue for sampled outputs. The L2 sample rate is 0.15 (matching `triage-failure`); both recipes drive engineer prioritisation downstream and warrant the higher rate.
- **Weekly** — review `failure-modes.yaml`. Promote `proposed` modes after the team has seen them in real reports; retire modes not seen in 8 weeks.
- **Monthly** — re-calibrate `g-bug-judge`:
  ```
  python eval-bench/calibrate.py recipes/test/bug-advocacy \
      --grader-id g-bug-judge --judge anthropic
  ```
  If Reproducibility agreement drops, the rubric needs sharpening — escalate, do not loosen.
- **Quarterly** — pairwise SME comparison batch (two reviewers, blinded) on 20+ recent outputs. Pay extra attention to fabricated-repro and fix-proposal failures.

## When pass^k drops

1. Filter the failing trials in the Trace Inspector and hand-review at least five.
2. Tag each against `failure-modes.yaml`. Look first for `fm-fabricates-repro-steps` and `fm-prescriptive` — these are the high-severity ones the rubric watches for specifically.
3. Look for axis correlation in the Slice Explorer (which `domain`, which `complexity`, which judge_model). A drop concentrated in `domain=billing` or `domain=accessibility` usually means the recipe needs domain-aware guidance, not a model swap.
4. If the L1 `g-bug-sections` grader is failing on positive tasks, the recipe is regressing the H2 header format — adjust the recipe instructions before relaxing the grader.
5. If the L3 judge is auto-skipping, recalibration is overdue; treat as an incident.

## Negative-polarity handling

This recipe has both positive (full bug report expected) and negative (refusal expected) tasks in the same `tasks.jsonl`. The L1 `g-bug-sections` grader is marked `negate_on_polarity_negative: true`, so the composition layer correctly treats "no bug-report sections present" as a *pass* on refusal tasks and a *fail* on bug-report tasks.

## Banned shortcuts

- Quarantining a failing positive task without first hand-reviewing it.
- Lowering `min_passk_target` to make CI green.
- Adding tasks that are paraphrases of passing tasks. New tasks come from observed failures or deliberately constructed coverage gaps.
- Treating a fabricated-repro-step call as "just" a bug — escalate. Engineers acting on fabricated reproductions waste hours and lose trust in every future report.

## Banned metrics for this recipe

Per the Skein doctrine, none of these belong on a dashboard for bug-advocacy:

- "Reports drafted per day"
- "Average report length"
- "Time saved per engineer per week" — counterfactual we cannot measure honestly.

Useful metrics (link to a decision):

- Pass^k on the regression suite by `domain` axis — drives whether to add domain-specific guidance to the recipe.
- Per-failure-mode incidence over time — drives which mode to address next.
- Reproducibility agreement (L3 vs L2) over time — drives recalibration cadence.
- Severity-overstatement rate — proxy for whether engineers will trust the recipe's prioritisation.
