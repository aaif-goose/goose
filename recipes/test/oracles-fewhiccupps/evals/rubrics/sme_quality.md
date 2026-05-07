# SME rubric — oracles-fewhiccupps

Reviewer: an SDET or test engineer in the Bach / Bolton / Winteringham tradition. Apply this rubric to a *single* oracle-suite output. Score one dimension at a time; do not let one weak dimension drag down or prop up another.

For each dimension below, score **0**, **1**, or **2** and add a one-line note. Submit the JSON object at the bottom; do not attempt to grade through prose.

## D1 — Honesty

- **2** — Every claim about the target is clearly traceable to the target description. N/A is used where the oracle does not apply. No invented frameworks, claims, or statutes.
- **1** — One mildly extrapolated detail that a competent tester would still recognise as a reasonable inference; or one missing N/A where it would have been more honest.
- **0** — Fabricated target details, invented thresholds, or cited statutes / RFCs that do not apply or do not exist.

## D2 — Oracle labelling

- **2** — Each section's "Applied here" content matches that oracle's meaning. Statutes is about laws / standards. World is about real-world consistency. Familiar problems is about classical fault patterns. No mislabelling.
- **1** — One borderline label where the content could plausibly fit two oracles.
- **0** — Material mislabel — "Statutes" content that's really user expectations, "World" content that's really a Familiar problem.

## D3 — Operational quality of "Applied here"

- **2** — A tester could start a 30-minute session against the target from the listed checks without further interpretation. Each check references the target's named elements.
- **1** — Most checks are actionable; one or two are vague ("test consistency", "check edge cases").
- **0** — Checks are decorative — generic phrases that could describe any target.

## D4 — Refusal handling (apply only to vague / empty / off-scope targets)

- **2** — Refused without producing FEW HICCUPPS sections; listed specific clarifying questions the target description is missing.
- **1** — Refused but the clarifying questions are generic ("what should this do?" rather than "what's the intended response code on invalid input?").
- **0** — Produced an oracle suite for a vague target.

## Submission format

```json
{
  "task_id": "...",
  "honesty": 0,
  "labelling": 0,
  "actionable": 0,
  "refusal": null,
  "notes": "<one-line per dimension if needed>"
}
```

Set `refusal` to the score for D4 only if the task is in the negative-polarity / refusal contract; otherwise `null`.

## Banned shortcut

Do **not** read the target and the oracle suite side-by-side and grade by overall vibe. Score each dimension in isolation, then submit. The L3 judge calibrates against the per-dimension scores; vibe-grading destroys that calibration.
