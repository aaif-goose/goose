"""Tests for the calibration log JSONL roundtrip."""

from __future__ import annotations

from datetime import datetime, timezone
from pathlib import Path

from lib.calibration import CalibrationRecord, append_calibration_record, load_calibration_log


def test_round_trip_single_record(tmp_path: Path) -> None:
    rec = CalibrationRecord(
        timestamp=datetime(2026, 5, 7, 12, 0, 0, tzinfo=timezone.utc),
        judge_id="g-judge-x",
        judge_model="anthropic:claude-opus-4-7",
        sample_size=50,
        agreement=0.92,
        deployed=True,
        cohen_kappa=0.84,
        divergence_breakdown={"fm-001": 0.05, "fm-002": 0.10},
        notes="initial pass",
    )
    log_path = tmp_path / "cal.jsonl"
    append_calibration_record(log_path, rec)

    [loaded] = load_calibration_log(log_path)
    assert loaded.judge_id == "g-judge-x"
    assert loaded.judge_model == "anthropic:claude-opus-4-7"
    assert loaded.sample_size == 50
    assert loaded.agreement == 0.92
    assert loaded.deployed is True
    assert loaded.cohen_kappa == 0.84
    assert loaded.divergence_breakdown == {"fm-001": 0.05, "fm-002": 0.10}
    assert loaded.notes == "initial pass"
    assert loaded.timestamp == rec.timestamp


def test_appending_preserves_history(tmp_path: Path) -> None:
    log_path = tmp_path / "cal.jsonl"
    for i, agreement in enumerate([0.7, 0.85, 0.95]):
        append_calibration_record(
            log_path,
            CalibrationRecord(
                timestamp=datetime(2026, 5, i + 1, tzinfo=timezone.utc),
                judge_id="g",
                judge_model="m",
                sample_size=50,
                agreement=agreement,
                deployed=agreement >= 0.85,
            ),
        )
    records = load_calibration_log(log_path)
    assert [r.agreement for r in records] == [0.7, 0.85, 0.95]
    assert [r.deployed for r in records] == [False, True, True]


def test_skips_blank_and_comment_lines(tmp_path: Path) -> None:
    p = tmp_path / "cal.jsonl"
    p.write_text(
        "\n"
        "# a human-readable header\n"
        '{"timestamp": "2026-05-07T00:00:00+00:00", "judge_id": "g", "judge_model": "m",'
        ' "sample_size": 30, "agreement": 0.9, "deployed": true}\n',
        encoding="utf-8",
    )
    records = load_calibration_log(p)
    assert len(records) == 1
    assert records[0].agreement == 0.9


def test_z_suffix_timestamp_is_accepted(tmp_path: Path) -> None:
    """Common in tooling output; lib.calibration normalises it to +00:00."""
    p = tmp_path / "cal.jsonl"
    p.write_text(
        '{"timestamp": "2026-05-07T00:00:00Z", "judge_id": "g", "judge_model": "m",'
        ' "sample_size": 30, "agreement": 0.9, "deployed": true}\n',
        encoding="utf-8",
    )
    [r] = load_calibration_log(p)
    assert r.timestamp.tzinfo is not None


def test_create_parent_dir_on_append(tmp_path: Path) -> None:
    nested = tmp_path / "deeply" / "nested" / "cal.jsonl"
    rec = CalibrationRecord(
        timestamp=datetime(2026, 5, 7, tzinfo=timezone.utc),
        judge_id="g",
        judge_model="m",
        sample_size=30,
        agreement=0.9,
        deployed=True,
    )
    append_calibration_record(nested, rec)
    assert nested.exists()
    [loaded] = load_calibration_log(nested)
    assert loaded.judge_id == "g"
