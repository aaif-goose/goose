# Getting started with Skein

A 10-minute path from a fresh checkout to your first AI-assisted exploratory test charter on a real feature brief. Read [SKEIN.md](SKEIN.md) first if you want the philosophy; this doc is the operational walkthrough.

## What you can do today

Skein currently ships:

- **Two recipes** for context-driven exploratory testing:
  - `recipes/test/charter-sfdipot/` — feature brief → SFDIPOT charter (Bach).
  - `recipes/test/oracles-fewhiccupps/` — function / endpoint / screen → FEW HICCUPPS oracle suite (Bach & Bolton).
- **Three CLIs** under `eval-bench/`:
  - `run_once.py` — single-shot ad-hoc execution (the daily-driver).
  - `run_kpass.py` — full eval suite over a recipe's tasks (regression / capability runs).
  - `annotate.py` — work the L2 human-review queue.
  - `calibrate.py` — calibrate an L3 LLM judge against your reviewed annotations.
- **eval-bench library** for building more recipes — schemas, kpass math, judge / runner / annotation infrastructure, the L1/L2/L3 grader ladder.

What's *not* in yet: a desktop UI (the Skein Tauri app boots and is branded but has no in-app views beyond goose's defaults), MCP bridges to Promptfoo / Langfuse / AIO Tests, and the deeper Phase 2+ capabilities (LLM SLA probing, investigation mode, quality intelligence, multi-agent council). See [SKEIN_STATUS.md](SKEIN_STATUS.md) for what's done, in-progress, and queued.

## Prerequisites

```bash
# Python 3.9+ with PyYAML
python3 -m pip install --user pyyaml

# (Optional, for running the test suite)
python3 -m pip install --user pytest

# Build the goose binary from this repo's Rust crates so --runner goose works.
# This is upstream goose's normal build:
just build       # or `cargo build --release`

# Set the Anthropic API key for the L3 judge (optional for ad-hoc runs;
# required for `--judge anthropic` and for calibration).
export ANTHROPIC_API_KEY=sk-ant-...
```

You don't need everything for every command. Each CLI's `--help` lists what it actually requires.

## Validate your setup (no API or goose needed)

`--runner stub --judge stub` runs the entire pipeline in-process with deterministic placeholders. Useful to confirm Python imports, schemas, and grader runners are wired up.

```bash
python3 eval-bench/run_once.py recipes/test/charter-sfdipot \
    --input feature_brief="Add a /healthz endpoint" \
    --runner stub --judge stub
```

Expected output (the markdown charter on stdout, headers on stderr):

```
# recipe: recipes/test/charter-sfdipot
# runner: stub   judge: stub
# inputs: ['feature_brief']

# grading
#   [pass] g-output-shape — output length 206 within [1, 20000]
#   [pass] g-charter-sections — all 7 required H2 sections present
#   [skip] g-charter-sme-review — L2 sampling not applicable to ad-hoc runs
#   [skip] g-charter-judge — no calibration log...
# overall: PASS
## Structure
...
```

If you see something else, run `pytest` from the repo root — 280+ tests should pass green.

## Your first real charter

Replace `--runner stub` with `--runner goose` to invoke the actual recipe through goose. The L3 judge stays off by default for ad-hoc runs (no Anthropic charges until you opt in).

```bash
python3 eval-bench/run_once.py recipes/test/charter-sfdipot \
    --input feature_brief="Add rate limiting to POST /api/v1/posts: \
60 req/min/IP, sliding window, return HTTP 429 with Retry-After header."
```

Save the output for an exploratory session:

```bash
python3 eval-bench/run_once.py recipes/test/charter-sfdipot \
    --input feature_brief=@docs/specs/rate-limiting.md \
    --output-only \
    > charters/rate-limiting-2026-05-08.md
```

For long briefs, use `--input feature_brief=@<path>` to read the brief from a file. For a function / endpoint / screen target, swap to the oracle composer:

```bash
python3 eval-bench/run_once.py recipes/test/oracles-fewhiccupps \
    --input target_description="GET /api/v1/orders?cursor=...&limit=N — opaque cursor pagination, 100 max" \
    --input target_kind=endpoint \
    --output-only \
    > oracles/orders-pagination.md
```

## Daily workflow

Three loops, separately scheduled:

### Per-feature (when a brief lands)

```bash
# Charter the brief.
python3 eval-bench/run_once.py recipes/test/charter-sfdipot \
    --input feature_brief=@<spec> --output-only > charter.md

# (Optional) compose oracles for specific endpoints / screens in the spec.
python3 eval-bench/run_once.py recipes/test/oracles-fewhiccupps \
    --input target_description=@<endpoint_doc> --input target_kind=endpoint \
    --output-only > oracles.md
```

The output is for *your* exploratory session. Skein doesn't auto-test anything.

### Weekly (or per release) — regression eval

`run_kpass.py` runs the recipe against its full `tasks.jsonl` k times, reports pass^k / pass@k by axis, and persists trial-level outcomes to a SQLite store you can query later.

```bash
# Full regression on charter-sfdipot, 5 trials per task.
python3 eval-bench/run_kpass.py \
    --recipe recipes/test/charter-sfdipot \
    --k 5 \
    --runner goose \
    --judge anthropic       # only after the judge is calibrated
```

If `pass^5` for the regression suite drops below the recipe's `min_passk_target`, the harness exits non-zero — drop this in CI.

### Quarterly — calibrate the L3 judge

The L3 LLM judge auto-skips until calibrated against your team's L2 verdicts. To produce L2 verdicts, run a `run_kpass.py` with sampling enabled (the recipe's `sample_rate` already sets this), then work the queue.

```bash
# 1. Run the eval suite — sampling fires, annotation files appear.
python3 eval-bench/run_kpass.py --recipe recipes/test/charter-sfdipot \
    --k 5 --runner goose --judge off

# 2. Work the queue (you, the SME).
python3 eval-bench/annotate.py list recipes/test/charter-sfdipot
python3 eval-bench/annotate.py show recipes/test/charter-sfdipot --id <id>
python3 eval-bench/annotate.py review recipes/test/charter-sfdipot \
    --id <id> --verdict pass --reviewer "Your name" \
    --note "honest, all sections actionable" \
    --score honesty=2 --score tactics=2

# 3. After ~25 reviewed annotations, calibrate.
python3 eval-bench/calibrate.py recipes/test/charter-sfdipot \
    --grader-id g-charter-judge --judge anthropic --note "Q2 calibration"
```

If the calibration record's `deployed: true`, the judge contributes to grading on the next `run_kpass.py`. If `deployed: false`, the calibration tool tells you why (sample too small, divergence too high, judge-side `Unknown` rate too high) — re-grade more samples or refine the rubric.

The `recipes/<name>/evals/runbook.md` for each recipe documents the recommended cadence and the playbook when `pass^k` drops.

## Adding a new recipe

Copy the template and follow the bring-up checklist:

```bash
cp -r recipes/_template recipes/test/my-new-recipe
# Read recipes/_template/README.md and follow steps 1-7.
```

The bring-up checklist is *non-negotiable* on one point: 50 hand-reviewed real-or-realistic traces and a `failure-modes.yaml` come *before* you build the graders. "Look at your data first" (Hamel Husain). Recipes that skip this step ship as theatre, not measurement.

For the doctrine in full, see [SKEIN.md](SKEIN.md) → "What it is" and "Doctrine" sections.

## Troubleshooting

| Symptom | Likely cause | Fix |
|---|---|---|
| `ModuleNotFoundError: No module named 'yaml'` | PyYAML not installed | `pip3 install --user pyyaml` |
| `error: goose binary 'goose' not found on PATH` | Goose isn't built or not on PATH | `just build`, then `export PATH="$PWD/target/release:$PATH"` |
| L3 grader always reports `skipped` | Calibration log missing or stale | Run `eval-bench/calibrate.py` after grading at least 20 annotations |
| `pass^k = 0` on negative-polarity tasks | The composition layer's polarity inversion is correctly catching a recipe that didn't refuse a vague brief | Read the recipe output for those task ids — the recipe is being over-helpful |
| Two recipes with same-named test files collide | pytest import-mode | Already fixed via `--import-mode=importlib` in `pytest.ini` |
| `ANTHROPIC_API_KEY` is set but L3 still skips | Calibration is stale or divergence too high | Re-run `calibrate.py`; if it stays red, the rubric or the judge model needs revision |

## What to read next

- [SKEIN.md](SKEIN.md) — distro identity, doctrine, roadmap.
- [SKEIN_STATUS.md](SKEIN_STATUS.md) — what's done / in-progress / queued.
- [`eval-bench/README.md`](eval-bench/README.md) — the eval methodology and library architecture.
- [`recipes/_template/README.md`](recipes/_template/README.md) — bring-up checklist for new recipes.
- Each recipe's `evals/runbook.md` — recipe-specific cadence and incident playbook.
