# SME rubric — bug-advocacy

Reviewer: a tester or engineer who triages bug reports for a living. Apply this rubric to a *single* bug-advocacy output. Score one dimension at a time; do not let one weak dimension drag down or prop up another.

For each dimension below, score **0**, **1**, or **2** and add a one-line note. Submit the JSON object at the bottom; do not attempt to grade through prose.

## D1 — Reproducibility (the most important dimension here)

- **2** — Every step in Steps to reproduce can be traced to the observation, context, or attempts_to_reproduce inputs. No invented steps. The observation's specifics (page, role, action) are preserved.
- **1** — One mildly inferred step that a competent triager would still recognise as a faithful interpretation; the rest are grounded.
- **0** — At least one fabricated step that does not appear in the inputs and does not follow obviously from them.

## D2 — No fix proposals (hard rule)

- **2** — The output never proposes a code change. No patches, no "fix this by …" lines, no design-of-fix descriptions, no "the bug is in X, change to Y."
- **1** — One borderline phrasing that hints at a fix without writing one.
- **0** — At least one explicit fix proposal anywhere in the report.

## D3 — Severity calibration

- **2** — Severity is calibrated to the evidence in the inputs. High / critical severity is named only when a specific high-impact element is in the observation (data loss, security exposure, blocked workflow). Low severity is named for cosmetic or trivial issues and the rationale says so.
- **1** — Severity is roughly right but the rationale doesn't quote the specific evidence; the reader has to read between the lines.
- **0** — Severity is overstated (cosmetic claimed as high) or understated (data loss claimed as low) given the inputs.

## D4 — Falsifier present and useful

- **2** — Variations and open questions includes a clear "What would prove this is NOT a bug?" line that names a concrete check (a specific user role, a specific feature flag, a specific environment).
- **1** — Falsifier is present but vague ("if it doesn't reproduce, it's not a bug").
- **0** — No explicit falsifier in the report.

## D5 — Refusal handling (apply only to vague / empty / off-scope inputs)

- **2** — Refused without producing the seven H2 sections; listed specific clarifying questions the tester would need to answer (which page / what was expected / when / can it be reproduced).
- **1** — Refused but the clarifying questions are generic.
- **0** — Produced a bug report from inadequate inputs.

## Submission format

```json
{
  "task_id": "...",
  "reproducibility": 0,
  "no_fix_proposals": 0,
  "severity_calibration": 0,
  "falsifier": 0,
  "refusal": null,
  "notes": "<one-line per dimension if needed>"
}
```

Set `refusal` to the score for D5 only if the task is in the negative-polarity / refusal contract; otherwise `null`.

## Banned shortcut

Do **not** read the inputs and the report side-by-side and grade by overall vibe. Score each dimension in isolation. The L3 judge calibrates against the per-dimension scores; vibe-grading destroys that calibration. Reproducibility and No-fix-proposals are the two highest-stakes dimensions on this recipe.
