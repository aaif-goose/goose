# charter-sfdipot

Phase 1's first real Skein recipe. Produces a structured exploratory test charter from a feature brief, using James Bach's SFDIPOT heuristic.

## What it does

- **Input:** a feature brief, PR description, or design-doc excerpt.
- **Output:** a markdown document with seven H2 sections — Structure, Function, Data, Interfaces, Platform, Operations, Time. Each section names FEW HICCUPPS oracles and lists actionable tactics.
- **Refusal:** if the brief is empty, single-word, or off-scope, the recipe asks clarifying questions instead of fabricating a charter.

## What it deliberately does *not* do

- Produce step-by-step test cases.
- Issue pass/fail verdicts.
- Auto-run anything against the system under test.

A charter frames investigation; the human runs the session.

## Eval-bench artifacts

| File | Purpose |
|---|---|
| `evals/tasks.jsonl` | 10 seed tasks: 7 positive (full charter expected), 3 negative (refusal expected). |
| `evals/failure-modes.yaml` | 7 active failure modes drawn from the recipe's known risks. |
| `evals/graders.yaml` | L1 (`output_shape`, `markdown_sections`) + L2 (sampled SME) + L3 (LLM judge, gated on calibration). |
| `evals/rubrics/sme_quality.md` | The L2 dimension-by-dimension rubric. |
| `evals/rubrics/judge_charter_quality.md` | The L3 single-dimension rubric. |
| `evals/calibration.md` | Calibration plan and history. The L3 judge auto-skips until a green record exists. |
| `evals/runbook.md` | Ownership, cadence, what-to-do-when-pass^k-drops. |

## Running

Until the harness's recipe-execution path is wired up (next chunk on `run_kpass.py`), the eval suite can be validated end-to-end via the existing `--dry-run` mode:

```bash
python eval-bench/run_kpass.py --recipe recipes/test/charter-sfdipot --dry-run
```

This loads tasks, failure-modes, and graders; reports the L3 judge as auto-skipped (no calibration log yet); and prints a sensible run plan.

## Known follow-ups

- L1 `g-charter-sections` grader fires on negative tasks (refusals, which legitimately have no SFDIPOT sections). The harness needs polarity-aware grader inversion; tracked in `SKEIN_STATUS.md`.
- L3 judge needs its first calibration run before it contributes to grading.
