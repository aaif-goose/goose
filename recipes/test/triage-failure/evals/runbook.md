# Runbook — triage-failure

## Ownership

| Role | Name |
|---|---|
| Recipe owner | Head of Software Quality |
| Backup owner | _replace_ |
| L2 reviewers | engineers who debug failing tests for a living |

## Cadence

- **Daily** — work the Annotation Queue for sampled outputs of this recipe (the L2 sample rate is 0.15, higher than the other recipes, because the stakes per call are higher).
- **Weekly** — review `failure-modes.yaml`. Promote `proposed` modes after the team has seen them in real traces; retire modes not seen in 8 weeks.
- **Monthly** — re-calibrate `g-triage-judge`:
  ```
  python eval-bench/calibrate.py recipes/test/triage-failure \
      --grader-id g-triage-judge --judge anthropic
  ```
  If the Honesty agreement drops, the rubric needs sharpening — escalate, do not loosen.
- **Quarterly** — pairwise SME comparison batch (two reviewers, blinded) on 20+ recent outputs. Pay extra attention to fabricated-frame and fix-proposal failures.

## When pass^k drops

1. Filter the failing trials in the Trace Inspector and hand-review at least five.
2. Tag each against `failure-modes.yaml`. Look first for `fm-fabricates-stack-frame` and `fm-jumps-to-fix` — these are the high-severity ones and the rubric watches for them specifically.
3. Look for axis correlation in the Slice Explorer (which `failure_kind`, which `complexity`, which judge_model). A drop concentrated in a single failure kind usually means the recipe needs a more targeted prompt for that kind, not a model swap.
4. If the L1 `g-triage-sections` grader is failing on positive tasks, the recipe is regressing the H2 header format — adjust the recipe instructions before relaxing the grader.
5. If the L3 judge is auto-skipping, recalibration is overdue; treat as an incident.

## Negative-polarity handling

This recipe has both positive (full triage expected) and negative (refusal expected) tasks in the same `tasks.jsonl`. The L1 `g-triage-sections` grader is marked `negate_on_polarity_negative: true`, so the composition layer correctly treats "no triage sections present" as a *pass* on refusal tasks and a *fail* on triage tasks.

## Banned shortcuts

- Quarantining a failing positive task without first hand-reviewing it.
- Lowering `min_passk_target` to make CI green.
- Adding tasks that are paraphrases of passing tasks. New tasks come from observed failures or deliberately constructed coverage gaps.
- Treating a fabricated-frame call as "just" a bug — escalate. The recipe's whole value is in not lying about the inputs.

## Banned metrics for this recipe

Per the Skein doctrine, none of these belong on a dashboard for triage-failure:

- "Triages produced per day"
- "Hypotheses generated per failure"
- "Token cost per triage"
- "Time saved per engineer per week" — this requires a counterfactual we cannot measure honestly.

Useful metrics (link to a decision):

- Pass^k on the regression suite by `failure_kind` axis — drives whether to add failure-kind-specific guidance to the recipe instructions.
- Per-failure-mode incidence over time — drives which mode to address next.
- Honesty agreement (L3 vs L2) over time — drives recalibration cadence and the question "should we still trust this judge?".
