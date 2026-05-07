# eval-bench

The eval framework every Skein recipe ships with. Anchors three principles:

1. **Look at your data first** (Hamel Husain). A recipe is not approved for ship until 50 production-or-realistic traces have been hand-reviewed and a `failure-modes.yaml` exists. Graders come *after* the data.
2. **Compose graders across three levels** (Anthropic *Demystifying Evals for AI Agents*). Each grader sits at L1 (code assertions), L2 (sampled human review), or L3 (LLM-as-judge). L3 is only valid with current calibration evidence against L2.
3. **Measure what changes a decision** (Adam Mahdi). Aggregate pass rates without slicing are not allowed. Every recipe records pass@k *and* pass^k and slices by every registered axis.

This directory contains the schemas, the harness (`run_kpass.py`), and the small library code that all recipes call into.

## Layout

```
eval-bench/
├── README.md                          # this file
├── schemas/
│   ├── tasks.schema.json              # validates tasks.jsonl entries
│   ├── failure-modes.schema.json      # validates failure-modes.yaml
│   ├── graders.schema.json            # validates graders.yaml
│   └── calibration.schema.json        # validates calibration log records
├── lib/
│   ├── __init__.py
│   ├── kpass.py                       # pass@k / pass^k math + slicing
│   ├── tasks.py                       # tasks.jsonl loader + validator
│   ├── failure_modes.py               # failure-modes.yaml loader
│   ├── graders.py                     # grader composition (L1 + L2 + L3)
│   ├── calibration.py                 # calibration record reader / writer
│   └── store.py                       # local SQLite results store
└── run_kpass.py                       # CLI entry point
```

## How a recipe consumes eval-bench

Each recipe directory carries a self-contained `evals/` subdirectory. The contract is:

```
recipes/<domain>/<name>/
├── recipe.yaml
└── evals/
    ├── tasks.jsonl               # 20–50 tasks; required
    ├── reference/                # expected outputs / properties
    ├── failure-modes.yaml        # living taxonomy; required for new recipes
    ├── graders.yaml              # L1 + L2 + L3 composition; required
    ├── annotations/              # human labels on sampled traces
    ├── calibration.md            # human-readable calibration history
    └── runbook.md                # what to do when metrics drift; ownership
```

Run the eval suite for a recipe:

```bash
python eval-bench/run_kpass.py --recipe recipes/test/charter-sfdipot --k 5
```

The harness:
- Loads and schema-validates tasks, graders, failure-modes.
- Refuses to run an L3 judge whose `calibration.md` is older than its declared `requires_calibration_within_days`.
- Executes each task `k` times (parallelism configurable; default 1 per recipe).
- Composes grader scores into a per-task pass / fail.
- Computes pass@k *and* pass^k.
- Slices results by every axis registered on the tasks.
- Writes a result row to the local SQLite store at `~/.skein/eval-bench.sqlite`.
- Returns non-zero if any regression task drops below its declared `min_passk_target`.

## Determinism metrics

- **pass@k**: probability that at least one of `k` trials passes. Useful when one working solution suffices.
- **pass^k**: probability that *all* `k` trials pass. The CI / customer-facing metric. With per-trial pass rate `p`, `pass^k = p^k`. So a per-trial pass rate of 75 % gives `pass^10 ≈ 6 %`.

Both are reported on every run. A recipe declares its **minimum acceptable pass^k for the regression suite**; failing this target is a hard CI failure.

## Slice axes

A task may declare any number of `axes`. Common axes:

- `model` — which model produced the output
- `complexity` — `low | medium | high` per the recipe author's judgement
- `language` — the human language of the input (matters for charter / oracle recipes)
- `framework` — Playwright-Java / Playwright-Python / Tauri driver / etc.
- `time_of_day` — local time bucket (relevant for LLM SLA prober recipes)

The Slice Explorer Tauri view displays pass rates broken down by every recorded axis. Aggregate-only displays are configurably disabled and not the default.

## Two-sided tasks (Anthropic discipline)

Every "behaviour should fire" task in `tasks.jsonl` must be accompanied by at least one "behaviour should NOT fire" task. The schema's `polarity` field enforces this: a recipe whose tasks are all `positive` is rejected by the harness.

## Capability vs. regression suites

A task's `tags` declare its suite membership:

- `regression` — must pass at the recipe's `min_passk_target`. Backstop. New tasks here only by graduation from capability.
- `capability` — hard cases where pass rate is meant to be low at first. Improvement signal.

The Saturation Alarm watches: when a capability suite passes >95 % for `n_consecutive_runs` (configurable per recipe), the alarm proposes graduating tasks to regression and drawing harder tasks from recent failure-mode entries.

## L1 / L2 / L3 grader ladder

| Level | Type | Cost | Speed | Use |
|---|---|---|---|---|
| **L1** | Code assertions | cheap | fast | schema match, regex, exit code, deterministic outcome verification — the floor of correctness |
| **L2** | Human (sampled) | expensive | slow | source of truth; calibrates L3 |
| **L3** | LLM-as-judge | medium | medium | scales open-ended grading, but only with current L2 calibration |

`graders.yaml` declares the composition. L3 graders that fail their calibration check are skipped (not failed) and the result row is annotated. The recipe's overall pass / fail uses only L1 + (calibrated L3) + (sampled L2 if drawn for this task).

## The closed loop (production ↔ evals)

1. A recipe runs in dev, CI, or production.
2. Its trace lands in Langfuse via the Langfuse Bridge.
3. The Annotation Queue samples nightly; humans tag against `failure-modes.yaml`.
4. Confirmed novel failures land in the originating recipe's `tasks.jsonl` via the *Field Failure → Eval Task* recipe (proposes; humans approve).
5. AIO Tests / Jira defects feed step 4 too — every customer-reported defect with a Skein-touched component becomes a candidate eval task.

## Banned metrics

These are not displayed in any Skein dashboard, regardless of how easy they are to compute:

- "tests generated per minute"
- "lines of code emitted"
- "recipe executions count"
- "agents launched today"
- "tokens consumed"
- "annotations per week"

These are *activity*, not *value*. A metric earns its place by changing a decision (Mahdi).

## Status

This is the Phase 0 scaffold. The schemas and the CLI surface are real; the per-grader runners are stubs that will land alongside the first Phase 1 recipe (`charter-sfdipot`).
