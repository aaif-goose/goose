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
- [ ] Slice Explorer v0 (Tauri view).
- [ ] Langfuse Bridge MCP extension.
- [ ] `recipe-scanner/` made a required gate for non-local recipes.

## Phase 1 — Tester's Co-Pilot v1

- [x] Grader-runner framework (`eval-bench/grader_runners/`): `output_shape.py`, `markdown_sections.py`, contract documented in the README. 24 tests.
- [x] First recipe `recipes/test/charter-sfdipot/` — SFDIPOT charter composer with full eval artifacts (10 seed tasks, 7 active failure modes, L1+L2+L3 grader composition, SME and judge rubrics, calibration plan, runbook). 15 tests covering artifact validity, two-sided coverage, failure-mode/task linkage, and L1 grader behaviour against full charters and refusals.
- [ ] Polarity-aware grader inversion in the harness — `g-charter-sections` should pass on negative tasks when the recipe correctly refuses. Tracked in the recipe's runbook; a known follow-up.
- [ ] Recipe execution path in `run_kpass.py` — currently a stub; needs real wiring into Goose's recipe runner.
- [ ] L3 judge calibration for `charter-sfdipot` (judge auto-skips until calibrated).

## Phase 1 follow-ups (open)

- `oracles-fewhiccupps/` recipe.
- `triage-failure/` recipe.
- `bug-advocacy/` recipe.
- Multi-driver Playwright codegen (Java first, Python next).
- Tester's Notebook view in `ui/goose2`.
- Promptfoo Bridge MCP extension.
- AIO Tests Bridge v1 (read-only).

## How to contribute a Phase 0 chunk

1. Branch from `skein-phase-0-foundation` (or work directly on it for now).
2. Make a small, reviewable commit per deliverable. One commit per checkbox above is the target granularity.
3. Update this status file as part of the commit so the checkbox flips when the work lands.
4. Keep customizations additive; do not modify core upstream paths.
