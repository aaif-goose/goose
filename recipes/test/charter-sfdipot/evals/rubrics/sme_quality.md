# SME rubric — charter-sfdipot

Reviewer: an SDET or test engineer in the Bach / Bolton / Winteringham tradition. Apply this rubric to a *single* charter output. Score one dimension at a time; do not let one weak dimension drag down or prop up another.

For each dimension below, score **0**, **1**, or **2** and add a one-line note. Submit the JSON object at the bottom; do not attempt to grade through prose.

## D1 — Honesty

- **2** — Every section's claims are clearly traceable to the brief. N/A is used where the brief does not support detail. No invented frameworks, thresholds, or endpoints.
- **1** — One mildly extrapolated detail that a competent tester would still recognise as a reasonable inference; or one missing N/A where it would have been more honest.
- **0** — Fabricated requirements, invented thresholds, or named tools the brief does not mention.

## D2 — Operational quality of Tactics

- **2** — A tester could start a 30-minute session from the Tactics without further interpretation. Each tactic is a concrete action with a target.
- **1** — Most tactics are actionable; one or two are vague ("explore edge cases", "consider failures").
- **0** — Tactics are decorative — generic phrases that could describe any feature.

## D3 — Oracles named and appropriate

- **2** — Each section names FEW HICCUPPS oracles that genuinely apply to that dimension. The Oracles line is operational, not decorative.
- **1** — Oracles named but mismatched to the dimension (e.g., naming Statutes for a UI dimension where it doesn't apply).
- **0** — Oracles missing or named only generically ("various oracles apply").

## D4 — Refusal handling (apply only to vague / empty / off-scope tasks)

- **2** — Refused without producing SFDIPOT sections; listed specific clarifying questions the brief is missing.
- **1** — Refused but the clarifying questions are generic ("what should this do?" rather than "what's the intended threshold for X?").
- **0** — Produced a charter from a vague brief.

## Submission format

```json
{
  "task_id": "...",
  "honesty": 0,
  "tactics": 0,
  "oracles": 0,
  "refusal": null,
  "notes": "<one-line per dimension if needed>"
}
```

Set `refusal` to the score for D4 only if the task is in the negative-polarity / refusal contract; otherwise `null`.

## Banned shortcut

Do **not** read the brief and the charter side-by-side and grade by overall vibe. Score each dimension in isolation, then submit. The L3 judge calibrates against the per-dimension scores; vibe-grading destroys that calibration.
