"""Tests for failure-modes.yaml loading and the active-mode-needs-approval invariant."""

from __future__ import annotations

from pathlib import Path

import pytest

from lib.failure_modes import FailureModes, load_failure_modes


def _write_yaml(path: Path, body: str) -> Path:
    path.write_text(body, encoding="utf-8")
    return path


# ---------- load_failure_modes ----------


def test_load_minimal_active_mode(tmp_path: Path) -> None:
    p = _write_yaml(
        tmp_path / "failure-modes.yaml",
        """
version: 1
modes:
  - id: fm-001
    name: "Truncation"
    description: "Recipe truncates output mid-sentence."
    first_observed: "2026-05-07"
    status: active
    approved_by: "lead goose"
""",
    )
    fm = load_failure_modes(p)
    assert isinstance(fm, FailureModes)
    assert len(fm.modes) == 1
    assert fm.modes[0].id == "fm-001"
    assert fm.active() == fm.modes


def test_active_mode_without_approver_is_rejected(tmp_path: Path) -> None:
    p = _write_yaml(
        tmp_path / "failure-modes.yaml",
        """
version: 1
modes:
  - id: fm-001
    name: "Truncation"
    description: "x"
    first_observed: "2026-05-07"
    status: active
""",
    )
    with pytest.raises(ValueError, match="approved_by"):
        load_failure_modes(p)


def test_proposed_mode_does_not_need_approver(tmp_path: Path) -> None:
    """`proposed` is the agent-suggested state; humans only sign off when promoting to `active`."""
    p = _write_yaml(
        tmp_path / "failure-modes.yaml",
        """
version: 1
modes:
  - id: fm-002
    name: "Hallucination"
    description: "x"
    first_observed: "2026-05-07"
    status: proposed
""",
    )
    fm = load_failure_modes(p)
    assert fm.modes[0].status == "proposed"
    assert fm.active() == []


def test_unknown_status_is_rejected(tmp_path: Path) -> None:
    p = _write_yaml(
        tmp_path / "failure-modes.yaml",
        """
version: 1
modes:
  - id: fm-001
    name: "x"
    description: "x"
    first_observed: "2026-05-07"
    status: maybe
""",
    )
    with pytest.raises(ValueError, match="invalid status"):
        load_failure_modes(p)


def test_unsupported_version_is_rejected(tmp_path: Path) -> None:
    p = _write_yaml(
        tmp_path / "failure-modes.yaml",
        """
version: 99
modes: []
""",
    )
    with pytest.raises(ValueError, match="version"):
        load_failure_modes(p)


def test_duplicate_ids_rejected(tmp_path: Path) -> None:
    p = _write_yaml(
        tmp_path / "failure-modes.yaml",
        """
version: 1
modes:
  - id: fm-dup
    name: "a"
    description: "a"
    first_observed: "2026-05-07"
    status: proposed
  - id: fm-dup
    name: "b"
    description: "b"
    first_observed: "2026-05-07"
    status: proposed
""",
    )
    with pytest.raises(ValueError, match="duplicate failure mode id"):
        load_failure_modes(p)


def test_by_id_lookup(tmp_path: Path) -> None:
    p = _write_yaml(
        tmp_path / "failure-modes.yaml",
        """
version: 1
modes:
  - id: fm-x
    name: "x"
    description: "x"
    first_observed: "2026-05-07"
    status: proposed
""",
    )
    fm = load_failure_modes(p)
    assert fm.by_id("fm-x").id == "fm-x"
    assert fm.by_id("fm-missing") is None
