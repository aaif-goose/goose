# bug-advocacy

Phase 1's fourth Skein recipe — completes the four-recipe context-driven daily loop alongside `charter-sfdipot`, `oracles-fewhiccupps`, and `triage-failure`. Given a tester's observation, drafts a Kaner-style bug report.

## What it does

- **Input:** a free-text `observation` (required); optional `context` (what the tester was doing), `attempts_to_reproduce`, and `environment` (browser / OS / build).
- **Output:** a markdown bug report with seven H2 sections — Summary, Steps to reproduce, Actual result, Expected result, Environment, Severity rationale, Variations and open questions. Each section's shape is enforced by the recipe instructions and validated by the L1 grader.
- **Refusal:** if the observation is empty, vague, or off-scope, the recipe asks specific clarifying questions instead of fabricating a report.

## What it deliberately does *not* do

- Propose code changes. No patches, no "fix this by …" lines, no design-of-fix descriptions. **Hard rule** enforced in the recipe instructions, the SME rubric, and the L3 judge rubric.
- Fabricate reproduction steps. Every step in the report comes from the inputs.
- Fabricate environment values. If `environment` wasn't supplied, the report says so and asks in Open questions.
- Overstate severity. High / critical severity is named only when the inputs include a specific high-impact element (data loss, security exposure, blocked workflow).

## Why this scope

Bug reports drive engineer triage prioritisation downstream. A fabricated repro step costs an engineer a "can't reproduce" rabbit hole; a fabricated severity gets the report mis-prioritised. The Bach-test for this recipe: would a triaging engineer following the report's steps end up at the bug, with a clear understanding of *why* it matters and *what would prove they're wrong about it*?

## Eval-bench artifacts

| File | Purpose |
|---|---|
| `evals/tasks.jsonl` | 10 seed tasks: 6 positive (rich observation, intermittent loading, data loss with evidence, tooltip typo, accessibility keyboard trap, sort-by-string regression), 4 negative (vague, empty, fix-bait prompt injection, feature-request-disguised-as-bug). |
| `evals/failure-modes.yaml` | 7 active failure modes including 4 high-severity ones: `fm-fabricates-repro-steps`, `fm-overstates-severity`, `fm-prescriptive`, `fm-overhelpful-on-vague-input`. |
| `evals/graders.yaml` | L1 (`output_shape`, `markdown_sections` for the 7 sections) + L2 (sampled SME, **15% sample rate** — same as `triage-failure`, higher than charter / oracles) + L3 (LLM judge, gated on calibration). |
| `evals/rubrics/sme_quality.md` | The L2 dimension-by-dimension rubric. **Reproducibility and No-fix-proposals are the two highest-stakes dimensions.** |
| `evals/rubrics/judge_bug_quality.md` | The L3 single-dimension rubric, with three hard rules: fabricated repro = automatic fail, fix proposals = automatic fail, fabricated environment values = automatic fail. |
| `evals/calibration.md` | Calibration plan and history. The L3 judge auto-skips until a green record exists. |
| `evals/runbook.md` | Ownership, cadence, what-to-do-when-pass^k-drops, reproducibility-failure watch. |

## Running

```bash
# Validate artifacts only (no recipe execution):
python eval-bench/run_kpass.py --recipe recipes/test/bug-advocacy --dry-run

# Draft a bug report from a single observation:
python eval-bench/run_once.py recipes/test/bug-advocacy \
    --input observation="I clicked Subscribe on the Pro plan and got 'Payment failed: card declined' even though the same card just worked on Amazon 10 minutes ago." \
    --input context="On /pricing, logged in as a returning customer with a saved Visa." \
    --input environment="Chrome 124 on macOS 14.4, build 2026.05.07.1" \
    --output-only > bugs/subscribe-failure.md
```

## Known follow-ups

- L3 judge needs its first calibration run.
- A deterministic "step provenance" L1 grader could verify that every numbered step in the output appears in the inputs verbatim — currently this is enforced only at L2/L3.
