# Skein implementation status

This file tracks Phase 0 deliverables as they land. Maintained by hand; checked off when the corresponding chunk is committed.

## Phase 0 — Foundation, Identity, Eval Backbone

- [x] Upstream remote configured fetch-only (push URL disabled) to prevent accidental pushes to `block/goose`.
- [x] Branch `skein-phase-0-foundation` created.
- [x] Distro identity documented (`SKEIN.md`).
- [x] Branded top-level Tauri identity in `ui/goose2/` (productName, window title, HTML title, bundle identifier `com.skein.app`, Cargo description). Deeper React-component string sweep deferred to a follow-up chunk so the diff stays reviewable.
- [x] `eval-bench/` library scaffolding: JSON schemas (tasks, failure-modes, graders, calibration), Python lib (kpass, tasks, failure_modes, graders, calibration, SQLite store), and the `run_kpass.py` CLI with `--dry-run`. Recipe-execution path is intentionally a stub; lands with the first Phase 1 recipe.
- [x] `eval-bench/tests/` pytest suite — 59 tests covering kpass math, loaders' invariants, L3 calibration gate decision matrix, SQLite store, and an end-to-end harness smoke. Run with `pytest eval-bench/tests`.
- [x] Recipe template directory `recipes/_template/` with all required eval artifacts (tasks.jsonl, failure-modes.yaml, graders.yaml, calibration.md, runbook.md, README). Verified end-to-end: `python3 eval-bench/run_kpass.py --recipe recipes/_template --dry-run` loads cleanly and correctly auto-skips the uncalibrated L3 grader.
- [x] Skein config profile via `CUSTOM_DISTROS.md` patterns: `distro.json` carries the Skein appVersion tag, `config.yaml` is intentionally empty with a documented "no defaults without evidence" rationale, and `distro/SKEIN.md` records what the bundle is and is not used for.
- [ ] Trace Inspector v0 (Tauri view).
- [ ] Annotation Queue v0 (Tauri view).
- [ ] Failure-Mode Taxonomy view v0 (Tauri view).
- [x] Slice Explorer v0 (Tauri view) — `ui/goose2` feature `slice-explorer` with read-only Tauri command (`eval_bench_list_runs`, `eval_bench_get_run`) backed by rusqlite against `~/.skein/eval-bench.sqlite`. View renders the runs list, per-run axis breakdowns, and two-run comparison with the same `compute_passk` / `passk_by_slice` semantics as the Python CLI. 7 vitest tests for the TS math layer mirror the kpass.py invariants; 11 Rust unit tests cover store-missing, recipe filter, headline pass^k math, and parsed axes.
- [ ] Langfuse Bridge MCP extension.
- [ ] `recipe-scanner/` made a required gate for non-local recipes.

## Phase 1 — Tester's Co-Pilot v1

- [x] Grader-runner framework (`eval-bench/grader_runners/`): `output_shape.py`, `markdown_sections.py`, contract documented in the README. 24 tests.
- [x] First recipe `recipes/test/charter-sfdipot/` — SFDIPOT charter composer with full eval artifacts (10 seed tasks, 7 active failure modes, L1+L2+L3 grader composition, SME and judge rubrics, calibration plan, runbook). 15 tests covering artifact validity, two-sided coverage, failure-mode/task linkage, and L1 grader behaviour against full charters and refusals.
- [x] Polarity-aware grader inversion via `negate_on_polarity_negative` field on graders + `lib/composition.py` to compose per-grader outcomes into per-trial pass/fail. Refusals on negative-polarity tasks now compose to pass; fabricated charters on vague briefs compose to fail. 14 new composition tests + 2 loader tests + 4 recipe-artifact tests verify the contract end-to-end.
- [x] Recipe execution path wired in `run_kpass.py`: pluggable `RecipeRunner` (`StubRunner` for smoke, `GooseSubprocessRunner` for real runs), `lib/grading.py` for L1 grader subprocess dispatch, and the per-trial pipeline using polarity-aware composition. Per-grader outcomes (including skipped status) persist to SQLite. End-to-end smoke verified: `--runner stub` against `recipes/test/charter-sfdipot` produces meaningful pass/fail per task and per slice.
- [x] L3 judge invocation wired: `lib/judge.py` (`Judge` protocol, `StubJudge`, `AnthropicJudge` HTTP-based via stdlib `urllib`), `--judge anthropic|stub|off` CLI flag, calibrated L3 graders invoke the judge for each trial. Verdicts of `pass`/`fail` contribute to composition; `Unknown` is reported as skipped (Anthropic guidance: never silently treat Unknown as fail). Unparseable judge responses, HTTP errors, and missing `ANTHROPIC_API_KEY` all degrade gracefully to skipped with a clear reason.
- [x] L2 sampled human review automation — `lib/annotations.py` (deterministic sampling, JSON annotation file format, AnnotationStore for queue read/write) + `eval-bench/annotate.py` CLI (list/show/review/discard subcommands). The harness writes annotation files for sampled trials; L2 graders surface either "queued for review at &lt;path&gt;" or "not sampled" in the per-trial evidence. Annotation files are git-ignored except for `.gitkeep`.
- [x] L3 calibration tool: `lib/calibration.run_calibration` runs the judge against every completed L2 annotation, compares verdicts, computes agreement + Cohen's kappa, and decides deployability against the grader's `max_divergence_from_l2` and a configurable `min_sample_size`. `eval-bench/calibrate.py` CLI exposes this with `--grader-id`, `--judge anthropic|stub`, `--min-sample-size`, `--max-divergence`, `--note`, `--dry-run`. SME `unknown` verdicts and judge errors / `Unknown` verdicts are skipped from the agreement number with a clear reason rather than silently treated as fails. 31 tests: 20 unit (math, edge cases, kappa) + 11 CLI end-to-end against a charter-sfdipot copy.
- [ ] First real calibration run for `charter-sfdipot` (requires actual SME-graded annotations + an `ANTHROPIC_API_KEY`).

## Phase 1 follow-ups (open)

- [x] `oracles-fewhiccupps/` recipe — FEW HICCUPPS oracle composer for function / endpoint / screen targets, with full eval artifacts (10 seed tasks across all three target kinds, 7 active failure modes including the high-severity `fm-statutes-hallucination`, L1+L2+L3 grader composition, dedicated SME and judge rubrics with a Statutes-specific hard rule, calibration plan, runbook). 22 recipe-artifact tests; full suite 258 green. Pytest switched to `--import-mode=importlib` so each recipe can ship its own `test_recipe_artifacts.py` without basename collisions.
- [x] `triage-failure/` recipe — failing-test triage with ranked, evidence-backed root-cause hypotheses, calibrated confidence labels, and concrete next-investigation steps. **Hard rule: never propose code changes** (enforced in recipe instructions, SME rubric, and L3 judge rubric — fabricated frame OR fix proposal = automatic L3 fail). 10 seed tasks across diverse failure kinds (assertion / timeout / race / regression / mocking / environment / input-validation / refusal / prompt-injection). L2 sample rate set higher than other recipes (15%) to reflect higher per-call stakes. 23 recipe-artifact tests.
- [x] `bug-advocacy/` recipe — Kaner-style bug report drafting from a tester's observation. Seven H2 sections (Summary, Steps to reproduce, Actual / Expected result, Environment, Severity rationale, Variations and open questions). **Three hard rules: never fabricate repro steps, never propose fixes, never fabricate environment values** (enforced in instructions + SME rubric + L3 judge rubric). 10 seed tasks across diverse domains (billing / ui / data-integrity / copy / accessibility / off-scope), with explicit severity-calibration coverage (cosmetic vs data-loss). L2 sample rate 0.15 matching triage. 25 recipe-artifact tests. **Phase 1's four-recipe quartet is complete: charter / oracles / triage / bug-advocacy = the full context-driven daily loop.**
- Multi-driver Playwright codegen (Java first, Python next).
- Tester's Notebook view in `ui/goose2`.
- Promptfoo Bridge MCP extension.
- AIO Tests Bridge v1 (read-only).

## How to contribute a Phase 0 chunk

1. Branch from `skein-phase-0-foundation` (or work directly on it for now).
2. Make a small, reviewable commit per deliverable. One commit per checkbox above is the target granularity.
3. Update this status file as part of the commit so the checkbox flips when the work lands.
4. Keep customizations additive; do not modify core upstream paths.
