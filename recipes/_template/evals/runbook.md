# Runbook — `<recipe slug>`

## Ownership

| Role | Name |
|---|---|
| Recipe owner | _replace_ |
| Backup owner | _replace_ |
| L2 reviewers | _list of SMEs who do sampled human review_ |

A recipe without an owner is not approved for ship.

## Cadence

- **Daily** — work the Annotation Queue for traces produced by this recipe.
  Target: 5–10 sampled traces / active day. The first task each day for the
  on-call quality engineer.
- **Weekly** — review `failure-modes.yaml`. Promote `proposed` modes that
  the team has now seen in real traces. Retire modes not seen in N weeks
  (suggested N=8). Keep retired entries for archaeology.
- **Monthly** — re-run the L3 calibration suite. Append a record to
  `calibration.jsonl`. If divergence > `max_divergence_from_l2`, the
  L3 grader auto-skips; treat that as an incident.
- **Quarterly** — pairwise SME comparison batch. Two reviewers, blinded.
  Records go in `annotations/` keyed by trace id.

## When pass^k drops

1. Pull the failing tasks from the Trace Inspector (filter by run id and
   `passed = 0`).
2. Hand-review at least five. Tag each against `failure-modes.yaml`. Propose
   new modes if needed.
3. If you find a mode is materially different from any existing entry,
   that's the signal to add a new task to `tasks.jsonl` rather than chase
   the symptom.
4. Resist the urge to "fix the eval" by relaxing graders. If a grader is
   wrong, prove it on hand-reviewed cases first.

## When the L3 judge is auto-skipped

The harness reports the reason in the run header (stale calibration,
divergence too high, no record). Treat as an incident: the recipe is now
running with reduced grading coverage. Schedule a calibration run within
the week.

## Banned shortcuts

- "Quarantine the failing test" without first hand-reviewing it.
- Bumping `min_passk_target` down to make CI green.
- Adding tasks that are paraphrases of passing tasks. Tasks must come from
  observed failures or deliberately constructed two-sided coverage.
- Disabling the saturation alarm because it's "noisy." Saturation is signal.

## Banned metrics

Per the Skein doctrine, none of these belong on a dashboard for this recipe:
"recipe executions / day," "tasks added per week," "annotations completed,"
"tokens consumed," "lines of generated test code." If you find yourself
defending one of these, re-read [SKEIN.md](../../../SKEIN.md).
