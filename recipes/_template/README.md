# Recipe template

Copy this directory to start a new Skein recipe. Rename `_template` to your
recipe slug, fill in `recipe.yaml`, and replace the seed eval artifacts with
real ones drawn from your hand-reviewed traces.

## What lives here

```
recipes/_template/
├── README.md             # this file
├── recipe.yaml           # the goose recipe
└── evals/
    ├── tasks.jsonl       # 20–50 hand-reviewed tasks; rejection-tested by the harness
    ├── reference/        # reference solutions / expected properties
    ├── failure-modes.yaml  # living taxonomy; promotes proposals to active
    ├── graders.yaml      # L1 + L2 + L3 composition; L1 floor required
    ├── annotations/      # human labels on sampled traces
    ├── calibration.md    # human-readable calibration narrative
    ├── calibration.jsonl # machine-readable calibration log (appended to)
    └── runbook.md        # what to do when metrics drift; ownership
```

## Bring-up checklist for a new recipe

1. Run the recipe's intended workflow against ~50 realistic inputs by hand. Save
   traces to Langfuse.
2. Tag each trace against an *initial* `failure-modes.yaml`. Modes can be
   `proposed` at this stage; promote to `active` when a human reviewer
   approves them.
3. Author `tasks.jsonl` from the traces. Every positive task gets at least one
   negative counterpart (the harness rejects one-sided eval sets).
4. Compose `graders.yaml`: at least one L1 (deterministic). Add L3 only with
   a calibration plan; the harness will refuse to deploy an uncalibrated L3
   grader.
5. Run `python eval-bench/run_kpass.py --recipe recipes/<your-slug> --dry-run`
   and confirm the harness loads everything cleanly.
6. Run a small `--k 3` capability pass; measure pass^3.
7. Iterate.

## Status workflow for a task

- `tags: [capability]` — hard cases; pass rate starts low; signals improvement.
- `tags: [regression]` — graduated cases that should pass at the recipe's
  declared `min_passk_target`.
- The Saturation Alarm watches capability suites; when one passes >95% for N
  consecutive runs it proposes graduating tasks to regression and drawing
  harder cases from recent failure-mode entries.

## Rituals

The recipe's `runbook.md` declares ownership and the cadence at which the
team agrees to:
- review sampled traces in the Annotation Queue (daily target),
- prune and promote `failure-modes.yaml` (weekly),
- re-calibrate L3 graders (monthly per recipe by default; sooner for
  high-stakes recipes),
- run a pairwise SME comparison batch (quarterly for high-stakes recipes).
