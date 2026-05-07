# triage-failure

Phase 1's third Skein recipe. Given a failing test (summary + stack trace, optionally with the test source and a recent diff), produces a structured failure triage: ranked root-cause hypotheses with named evidence, calibrated confidence labels, disconfirming checks, and concrete debug actions.

## What it does

- **Input:** a failing test's `failure_summary` and `stack_trace` (both required), plus optional `test_source` and `recent_diff` for richer triage.
- **Output:** a markdown document with four H2 sections — Failure summary, Hypotheses, Suggested next steps, Out of scope. Each hypothesis carries Evidence (verbatim quotes from the inputs), a Confidence label, and a Disconfirming check.
- **Refusal:** if the inputs are too thin (empty / vague / off-scope), the recipe asks specific clarifying questions instead of fabricating a triage.

## What it deliberately does *not* do

- Propose code changes. No patches, no "change X to Y" lines, no fix instructions. **This is a hard rule** enforced by both the SME rubric and the L3 judge rubric. The recipe directs investigation; the engineer authors fixes.
- Attach high confidence to a hypothesis without named evidence pointing at it.
- Auto-run anything against the system under test.

## Why this scope

Triage is the highest-leverage daily-use moment for a tester or developer: every failing run is a chance for the recipe to either save 20 minutes of debugging or waste 60 minutes on a fabricated lead. The honesty constraint matters more here than on any other recipe in Skein.

## Eval-bench artifacts

| File | Purpose |
|---|---|
| `evals/tasks.jsonl` | 10 seed tasks: 7 positive (assertion / timeout / race / regression / mocking / environment / leap-year), 3 negative (vague summary, empty summary, prompt-injection asking for a patch). |
| `evals/failure-modes.yaml` | 7 active failure modes including the high-severity `fm-fabricates-stack-frame`, `fm-jumps-to-fix`, `fm-confident-without-evidence`, and `fm-overhelpful-on-vague-input`. |
| `evals/graders.yaml` | L1 (`output_shape`, `markdown_sections` for the 4 triage sections) + L2 (sampled SME, **15% sample rate** vs 5–10% on other recipes — higher stakes) + L3 (LLM judge, gated on calibration). |
| `evals/rubrics/sme_quality.md` | The L2 dimension-by-dimension rubric. **Honesty and No-fix-proposals are the two highest-stakes dimensions.** |
| `evals/rubrics/judge_triage_quality.md` | The L3 single-dimension rubric, with two hard rules: fabricated frames = automatic fail, fix proposals = automatic fail. |
| `evals/calibration.md` | Calibration plan and history. The L3 judge auto-skips until a green record exists. |
| `evals/runbook.md` | Ownership, cadence, what-to-do-when-pass^k-drops, honesty-failure watch. |

## Running

```bash
# Validate artifacts only (no recipe execution):
python eval-bench/run_kpass.py --recipe recipes/test/triage-failure --dry-run

# Triage a real failing test (single shot):
python eval-bench/run_once.py recipes/test/triage-failure \
    --input failure_summary="test_calculate_total FAILED: AssertionError 95.0 != 90.0" \
    --input stack_trace=@/tmp/last-failure.log \
    --input test_source=@tests/test_pricing.py
```

## Known follow-ups

- L3 judge needs its first calibration run.
- Consider adding a deterministic frame-presence checker as an L1 grader (compare every quoted line in the output's Evidence fields against the input verbatim). Currently this is enforced only at L2/L3.
