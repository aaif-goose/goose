# Skein implementation status

This file tracks Phase 0 deliverables as they land. Maintained by hand; checked off when the corresponding chunk is committed.

## Phase 0 — Foundation, Identity, Eval Backbone

- [x] Upstream remote configured fetch-only (push URL disabled) to prevent accidental pushes to `block/goose`.
- [x] Branch `skein-phase-0-foundation` created.
- [x] Distro identity documented (`SKEIN.md`).
- [ ] Branded strings in `ui/goose2/` (productName, splash, about).
- [ ] `eval-bench/` library scaffolding: tasks schema, failure-modes schema, graders schema, `run_kpass.py`, calibration log format, slice axes registry.
- [ ] Recipe template directory `recipes/_template/` showing the required eval artifacts.
- [ ] Skein config profile via `CUSTOM_DISTROS.md` patterns.
- [ ] Trace Inspector v0 (Tauri view).
- [ ] Annotation Queue v0 (Tauri view).
- [ ] Failure-Mode Taxonomy view v0 (Tauri view).
- [ ] Slice Explorer v0 (Tauri view).
- [ ] Langfuse Bridge MCP extension.
- [ ] `recipe-scanner/` made a required gate for non-local recipes.

## Phase 1+ — to be tracked once Phase 0 lands

(Phase 1 deliverables are listed in [SKEIN.md](SKEIN.md). This status file will be extended as Phase 0 closes.)

## How to contribute a Phase 0 chunk

1. Branch from `skein-phase-0-foundation` (or work directly on it for now).
2. Make a small, reviewable commit per deliverable. One commit per checkbox above is the target granularity.
3. Update this status file as part of the commit so the checkbox flips when the work lands.
4. Keep customizations additive; do not modify core upstream paths.
