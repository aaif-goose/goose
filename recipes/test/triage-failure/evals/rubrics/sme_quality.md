# SME rubric — triage-failure

Reviewer: an engineer who debugs failing tests for a living. Apply this rubric to a *single* triage output. Score one dimension at a time; do not let one weak dimension drag down or prop up another.

For each dimension below, score **0**, **1**, or **2** and add a one-line note. Submit the JSON object at the bottom; do not attempt to grade through prose.

## D1 — Honesty (the most important dimension here)

- **2** — Every quoted frame, line number, and diff hunk in the Evidence fields appears verbatim in the supplied inputs. No fabricated frames. No "typical pattern" appeals where named evidence was available.
- **1** — One mildly paraphrased frame that a competent engineer would still recognise as a faithful reference; or one Evidence field where the quote is from the right place but wasn't lifted verbatim.
- **0** — At least one fabricated frame, line number, or diff hunk that does not appear in the inputs.

## D2 — No fix proposals (hard rule)

- **2** — The output never proposes a code change. No patches, no "change X to Y" lines, no "the bug is here, replace…" instructions. Only investigation steps.
- **1** — One borderline phrasing that hints at a fix without writing one. (Use sparingly — most ambiguous cases should score 0.)
- **0** — At least one explicit fix proposal: a patch, a "fix this by …", or a code change written in the output.

## D3 — Hypothesis quality

- **2** — Each hypothesis has named evidence, a calibrated confidence label (high requires named evidence pointing at it), and a concrete disconfirming check. The ranking of hypotheses tracks the strength of evidence.
- **1** — Most hypotheses are well-formed; one or two have weak evidence or a vague disconfirming check.
- **0** — Speculative hypotheses without evidence, or high-confidence labels uncalibrated to the inputs.

## D4 — Next-steps actionability

- **2** — Each suggested action references a specific named element (file:line, function name, log key, env var) from the inputs. An engineer could execute the action without further interpretation.
- **1** — Most actions are concrete; one or two are abstract ("look at concurrency", "check the logs").
- **0** — Next steps are decorative — generic phrases that could describe any failure.

## D5 — Refusal handling (apply only to vague / empty / off-scope inputs)

- **2** — Refused without producing the four H2 sections; listed specific clarifying questions the engineer would need to answer (which test, what error, which run, which environment).
- **1** — Refused but the clarifying questions are generic ("what failed?" rather than "do you have the stack trace from the failing run?").
- **0** — Produced a triage from inadequate inputs.

## Submission format

```json
{
  "task_id": "...",
  "honesty": 0,
  "no_fix_proposals": 0,
  "hypothesis_quality": 0,
  "actionable": 0,
  "refusal": null,
  "notes": "<one-line per dimension if needed>"
}
```

Set `refusal` to the score for D5 only if the task is in the negative-polarity / refusal contract; otherwise `null`.

## Banned shortcut

Do **not** read the inputs and the triage side-by-side and grade by overall vibe. Score each dimension in isolation. The L3 judge calibrates against the per-dimension scores; vibe-grading destroys that calibration. The Honesty and No-fix-proposals dimensions matter more than the others combined here.
