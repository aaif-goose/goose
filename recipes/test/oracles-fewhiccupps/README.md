# oracles-fewhiccupps

Phase 1's second Skein recipe. Produces a structured FEW HICCUPPS oracle suite for a function, endpoint, or screen — the natural complement to `charter-sfdipot`.

## What it does

- **Input:** a target description (function signature, endpoint contract, or screen / flow), optionally with a `target_kind` hint.
- **Output:** a markdown document with eleven H2 sections — Familiar problems, Explainability, World, History, Image, Comparable products, Claims, User expectations, Product, Purpose, Statutes. Each section gives the tester a concrete consistency oracle to compare the target against.
- **Refusal:** if the target description is empty, single-word, or off-scope, the recipe asks clarifying questions instead of fabricating oracles.

## What it deliberately does *not* do

- Produce step-by-step test cases.
- Issue pass/fail verdicts.
- Auto-run anything against the system under test.

Oracles frame what to compare against; the human runs the comparison.

## Eval-bench artifacts

| File | Purpose |
|---|---|
| `evals/tasks.jsonl` | 10 seed tasks: 7 positive (full oracle suite expected, mix of function / endpoint / screen targets), 3 negative (refusal expected). |
| `evals/failure-modes.yaml` | 7 active failure modes, including `fm-statutes-hallucination` (highest-severity dangerous variant of fabrication). |
| `evals/graders.yaml` | L1 (`output_shape`, `markdown_sections` for all 11 oracles) + L2 (sampled SME) + L3 (LLM judge, gated on calibration). |
| `evals/rubrics/sme_quality.md` | The L2 dimension-by-dimension rubric. |
| `evals/rubrics/judge_oracle_quality.md` | The L3 single-dimension rubric, with a hard rule on Statutes citations. |
| `evals/calibration.md` | Calibration plan and history. The L3 judge auto-skips until a green record exists. |
| `evals/runbook.md` | Ownership, cadence, what-to-do-when-pass^k-drops, Statutes-specific watch. |

## Running

```bash
# Validate artifacts only (no recipe execution):
python eval-bench/run_kpass.py --recipe recipes/test/oracles-fewhiccupps --dry-run

# Real execution against the recipe (needs goose installed):
python eval-bench/run_kpass.py --recipe recipes/test/oracles-fewhiccupps --k 5
```

## Why FEW HICCUPPS

Bach and Bolton's heuristic gives a tester eleven distinct kinds of "consistency" to compare a product against. Asking an LLM to generate a script-style test list collapses these into one dimension; asking it to generate a *named oracle suite* preserves the distinctions a context-driven tester reasons in.

## Known follow-ups

- L3 judge needs its first calibration run (`eval-bench/calibrate.py recipes/test/oracles-fewhiccupps --grader-id g-oracle-judge`).
- Statutes citations get a hard rule in the L3 rubric; consider an external check (e.g., a small list of valid RFC numbers) before deploying the judge in CI.
