# Runbook — oracles-fewhiccupps

## Ownership

| Role | Name |
|---|---|
| Recipe owner | Head of Software Quality |
| Backup owner | _replace_ |
| L2 reviewers | SDETs / test engineers familiar with FEW HICCUPPS |

## Cadence

- **Daily** — work the Annotation Queue for sampled outputs of this recipe. Target 5–10 traces / active day.
- **Weekly** — review `failure-modes.yaml`. Promote any `proposed` modes to `active` after team sign-off; retire modes not seen in 8 weeks.
- **Monthly** — re-calibrate `g-oracle-judge` against the SME-graded reference set:
  ```
  python eval-bench/calibrate.py recipes/test/oracles-fewhiccupps \
      --grader-id g-oracle-judge --judge anthropic
  ```
- **Quarterly** — pairwise SME comparison batch (two reviewers, blinded) on 20+ recent outputs. Pay extra attention to Statutes-labelled content; that's the highest-stakes oracle.

## When pass^k drops

1. Filter the failing trials in the Trace Inspector and hand-review at least five.
2. Tag each against `failure-modes.yaml`. Look especially for `fm-statutes-hallucination` — false statutes citations are dangerous and warrant escalation, not just a bug.
3. Look for axis correlation in the Slice Explorer (which `target_kind`, which `complexity`, which judge_model). A drop concentrated in `target_kind=screen` or `target_kind=endpoint` is usually a brief-side issue (the recipe doesn't have enough of the target's contract); a drop in `complexity=high` is usually a model-side issue.
4. If the L1 `g-oracle-sections` grader is failing on positive tasks, the recipe is regressing the H2 header format — adjust the recipe instructions before relaxing the grader.
5. If the L3 judge is auto-skipping, recalibration is overdue; treat as an incident.

## Negative-polarity handling

This recipe has both positive (full oracle suite expected) and negative (refusal expected) tasks in the same `tasks.jsonl`. The L1 `g-oracle-sections` grader is marked `negate_on_polarity_negative: true`, so the composition layer correctly treats "no FEW HICCUPPS sections present" as a *pass* on refusal tasks and a *fail* on oracle tasks. No special handling required when reading metrics; slice by `polarity` in the Slice Explorer if you want to see refusal vs. oracle outcomes separately.

## Banned shortcuts

- Quarantining a failing positive task without first hand-reviewing it.
- Lowering `min_passk_target` to make CI green.
- Adding tasks that are paraphrases of passing tasks. New tasks come from observed failures or deliberately constructed coverage gaps.
- Treating a Statutes false-positive as "just" a bug — escalate.

## Banned metrics for this recipe

Per the Skein doctrine, none of these belong on a dashboard for oracles-fewhiccupps:

- "Oracle suites generated per day"
- "Average suite length"
- "Token cost per suite"

Useful metrics (link to a decision):

- pass^k on the regression suite by `target_kind` axis — drives whether to add target-kind-specific guidance to the recipe instructions.
- Per-failure-mode incidence — drives which mode to address next (Statutes hallucination is always priority).
- L3 vs L2 divergence on Honesty over time — drives recalibration cadence.
