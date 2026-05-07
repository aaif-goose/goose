"""Tests for graders.yaml loading, the L1 floor invariant, and the L3 calibration gate."""

from __future__ import annotations

import json
from datetime import datetime, timedelta, timezone
from pathlib import Path

import pytest

from lib.graders import (
    L1Grader,
    L3Grader,
    is_l3_calibrated,
    load_graders,
)


def _write(path: Path, body: str) -> Path:
    path.write_text(body, encoding="utf-8")
    return path


# ---------- load_graders: L1 floor invariant ----------


def test_loads_full_l1_l2_l3_composition(tmp_path: Path) -> None:
    p = _write(
        tmp_path / "graders.yaml",
        """
version: 1
min_passk_target: 0.8
graders:
  - id: g-l1
    level: L1
    type: code
    weight: 1.0
    runner: "echo ok"
  - id: g-l2
    level: L2
    type: human
    weight: 1.0
    sample_rate: 0.05
    queue: annotations/
  - id: g-l3
    level: L3
    type: llm_judge
    weight: 1.0
    judge_model: "anthropic:claude-opus-4-7"
    rubric: rubrics/judge.md
    requires_calibration_within_days: 30
""",
    )
    g = load_graders(p)
    assert g.min_passk_target == 0.8
    assert len(g.by_level("L1")) == 1
    assert len(g.by_level("L2")) == 1
    assert len(g.by_level("L3")) == 1
    assert isinstance(g.by_level("L1")[0], L1Grader)


def test_rejects_no_l1_floor(tmp_path: Path) -> None:
    p = _write(
        tmp_path / "graders.yaml",
        """
version: 1
graders:
  - id: g-l3
    level: L3
    type: llm_judge
    weight: 1.0
    judge_model: "anthropic:claude-opus-4-7"
    rubric: rubrics/judge.md
    requires_calibration_within_days: 30
""",
    )
    with pytest.raises(ValueError, match="at least one L1"):
        load_graders(p)


def test_rejects_empty_graders_list(tmp_path: Path) -> None:
    p = _write(tmp_path / "graders.yaml", "version: 1\ngraders: []\n")
    with pytest.raises(ValueError, match="non-empty"):
        load_graders(p)


def test_rejects_l1_without_runner(tmp_path: Path) -> None:
    p = _write(
        tmp_path / "graders.yaml",
        """
version: 1
graders:
  - id: g-l1
    level: L1
    type: code
    weight: 1.0
""",
    )
    with pytest.raises(ValueError, match="requires `runner`"):
        load_graders(p)


def test_rejects_l3_missing_calibration_window(tmp_path: Path) -> None:
    p = _write(
        tmp_path / "graders.yaml",
        """
version: 1
graders:
  - id: g-l1
    level: L1
    type: code
    weight: 1.0
    runner: "echo ok"
  - id: g-l3
    level: L3
    type: llm_judge
    weight: 1.0
    judge_model: "anthropic:claude-opus-4-7"
    rubric: rubrics/judge.md
""",
    )
    with pytest.raises(ValueError, match="requires_calibration_within_days"):
        load_graders(p)


def test_unsupported_version_rejected(tmp_path: Path) -> None:
    p = _write(tmp_path / "graders.yaml", "version: 99\ngraders: []\n")
    with pytest.raises(ValueError, match="version"):
        load_graders(p)


# ---------- is_l3_calibrated: the deployment gate ----------


def _write_calibration_log(path: Path, records: list[dict]) -> Path:
    path.parent.mkdir(parents=True, exist_ok=True)
    with path.open("w", encoding="utf-8") as f:
        for r in records:
            f.write(json.dumps(r) + "\n")
    return path


def _make_l3_grader(**overrides) -> L3Grader:
    base = dict(
        id="g-l3",
        level="L3",
        type="llm_judge",
        weight=1.0,
        dimension=None,
        judge_model="anthropic:claude-opus-4-7",
        rubric="rubrics/judge.md",
        requires_calibration_within_days=30,
        max_divergence_from_l2=0.15,
    )
    base.update(overrides)
    return L3Grader(**base)


def test_no_calibration_log_means_not_deployable(tmp_path: Path) -> None:
    grader = _make_l3_grader()
    ok, reason = is_l3_calibrated(grader, tmp_path / "missing.jsonl")
    assert not ok
    assert "no calibration log" in reason


def test_no_record_for_judge_means_not_deployable(tmp_path: Path) -> None:
    log = _write_calibration_log(
        tmp_path / "cal.jsonl",
        [
            {
                "timestamp": "2026-05-01T00:00:00+00:00",
                "judge_id": "g-other",
                "judge_model": "x:y",
                "sample_size": 50,
                "agreement": 0.9,
                "deployed": True,
            }
        ],
    )
    ok, reason = is_l3_calibrated(_make_l3_grader(), log)
    assert not ok
    assert "no calibration record" in reason


def test_judge_model_mismatch_means_not_deployable(tmp_path: Path) -> None:
    """Calibration is bound to the exact judge_model — swap the model, re-calibrate."""
    log = _write_calibration_log(
        tmp_path / "cal.jsonl",
        [
            {
                "timestamp": "2026-05-01T00:00:00+00:00",
                "judge_id": "g-l3",
                "judge_model": "anthropic:claude-opus-4-6",
                "sample_size": 50,
                "agreement": 0.9,
                "deployed": True,
            }
        ],
    )
    ok, reason = is_l3_calibrated(_make_l3_grader(), log)
    assert not ok
    assert "no calibration record" in reason


def test_stale_calibration_blocks_deployment(tmp_path: Path) -> None:
    now = datetime(2026, 5, 7, tzinfo=timezone.utc)
    too_old = (now - timedelta(days=45)).isoformat()
    log = _write_calibration_log(
        tmp_path / "cal.jsonl",
        [
            {
                "timestamp": too_old,
                "judge_id": "g-l3",
                "judge_model": "anthropic:claude-opus-4-7",
                "sample_size": 50,
                "agreement": 0.95,
                "deployed": True,
            }
        ],
    )
    ok, reason = is_l3_calibrated(_make_l3_grader(requires_calibration_within_days=30), log, now=now)
    assert not ok
    assert "exceeds requires_calibration_within_days" in reason


def test_high_divergence_blocks_deployment(tmp_path: Path) -> None:
    now = datetime(2026, 5, 7, tzinfo=timezone.utc)
    log = _write_calibration_log(
        tmp_path / "cal.jsonl",
        [
            {
                "timestamp": (now - timedelta(days=2)).isoformat(),
                "judge_id": "g-l3",
                "judge_model": "anthropic:claude-opus-4-7",
                "sample_size": 50,
                "agreement": 0.7,  # divergence = 0.30 > max 0.15
                "deployed": True,
            }
        ],
    )
    ok, reason = is_l3_calibrated(_make_l3_grader(max_divergence_from_l2=0.15), log, now=now)
    assert not ok
    assert "exceeds" in reason


def test_undeployed_calibration_record_blocks_deployment(tmp_path: Path) -> None:
    now = datetime(2026, 5, 7, tzinfo=timezone.utc)
    log = _write_calibration_log(
        tmp_path / "cal.jsonl",
        [
            {
                "timestamp": (now - timedelta(days=1)).isoformat(),
                "judge_id": "g-l3",
                "judge_model": "anthropic:claude-opus-4-7",
                "sample_size": 50,
                "agreement": 0.95,
                "deployed": False,
            }
        ],
    )
    ok, reason = is_l3_calibrated(_make_l3_grader(), log, now=now)
    assert not ok
    assert "not deployable" in reason


def test_fresh_green_calibration_allows_deployment(tmp_path: Path) -> None:
    now = datetime(2026, 5, 7, tzinfo=timezone.utc)
    log = _write_calibration_log(
        tmp_path / "cal.jsonl",
        [
            {
                "timestamp": (now - timedelta(days=2)).isoformat(),
                "judge_id": "g-l3",
                "judge_model": "anthropic:claude-opus-4-7",
                "sample_size": 50,
                "agreement": 0.95,
                "deployed": True,
            }
        ],
    )
    ok, reason = is_l3_calibrated(_make_l3_grader(), log, now=now)
    assert ok
    assert reason == "ok"


def test_latest_record_wins_when_multiple(tmp_path: Path) -> None:
    """If a fresh green calibration exists, an older red one does not block deployment."""
    now = datetime(2026, 5, 7, tzinfo=timezone.utc)
    log = _write_calibration_log(
        tmp_path / "cal.jsonl",
        [
            {
                "timestamp": (now - timedelta(days=20)).isoformat(),
                "judge_id": "g-l3",
                "judge_model": "anthropic:claude-opus-4-7",
                "sample_size": 50,
                "agreement": 0.7,
                "deployed": False,
            },
            {
                "timestamp": (now - timedelta(days=2)).isoformat(),
                "judge_id": "g-l3",
                "judge_model": "anthropic:claude-opus-4-7",
                "sample_size": 50,
                "agreement": 0.95,
                "deployed": True,
            },
        ],
    )
    ok, _ = is_l3_calibrated(_make_l3_grader(), log, now=now)
    assert ok
