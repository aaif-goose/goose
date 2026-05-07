# Runbook — charter-sfdipot

## Ownership

| Role | Name |
|---|---|
| Recipe owner | Head of Software Quality |
| Backup owner | _replace_ |
| L2 reviewers | SDETs / test engineers familiar with SFDIPOT |

## Cadence

- **Daily** — work the Annotation Queue for sampled outputs of this recipe. Target 5–10 traces / active day.
- **Weekly** — review `failure-modes.yaml`. Promote any `proposed` modes to `active` after team sign-off; retire modes not seen in 8 weeks.
- **Monthly** — re-calibrate `g-charter-judge` against the SME-graded reference set. Append to `calibration.jsonl`.
- **Quarterly** — pairwise SME comparison batch (two reviewers, blinded) on 20+ recent outputs.

## When pass^k drops

1. Filter the failing trials in the Trace Inspector and hand-review at least five.
2. Tag each against `failure-modes.yaml`. If you find a recurring mode that's not in the file, propose it (status: `proposed`) — do not jump to grader changes.
3. Look for axis correlation in the Slice Explorer (which `complexity`, which `domain`, which judge_model). A drop concentrated in one slice is usually a brief-side or judge-side issue, not a recipe-side one.
4. If the L1 `g-charter-sections` grader is failing on positive tasks, check whether the model is regressing the section header format — adjust the recipe instructions before relaxing the grader.
5. If the L3 judge is auto-skipping, recalibration is overdue; treat as an incident.

## Negative-polarity handling

This recipe has both positive (full charter expected) and negative (refusal expected) tasks in the same `tasks.jsonl`. The L1 `g-charter-sections` grader is marked `negate_on_polarity_negative: true`, so the composition layer correctly treats "no SFDIPOT sections present" as a *pass* on refusal tasks and a *fail* on charter tasks. No special handling required when reading metrics; slice by `polarity` in the Slice Explorer if you want to see refusal vs. charter outcomes separately.

## Banned shortcuts

- Quarantining a failing positive task without first hand-reviewing it.
- Lowering `min_passk_target` to make CI green.
- Adding tasks that are paraphrases of passing tasks. New tasks come from observed failures or deliberately constructed coverage gaps.
- Disabling the saturation alarm because it's "noisy."

## Banned metrics for this recipe

Per the Skein doctrine, none of these belong on a dashboard for charter-sfdipot:

- "Charters generated per day"
- "Average charter length"
- "Token cost per charter"

Useful metrics (link to a decision):

- pass^k on the regression suite by `domain` axis — drives whether to add domain-specific guidance to the recipe instructions.
- Per-failure-mode incidence — drives which mode to address next.
- L3 vs L2 divergence on Honesty over time — drives recalibration cadence.
