"""Tests for the RecipeRunner abstraction, StubRunner, and GooseSubprocessRunner."""

from __future__ import annotations

import os
import sys
from pathlib import Path

import pytest

from lib.runner import GooseSubprocessRunner, RecipeRunner, RunResult, StubRunner


# ---------- Protocol & shape ----------


def test_protocol_recognises_concrete_runners() -> None:
    assert isinstance(StubRunner(lambda *_: RunResult(output="x")), RecipeRunner)
    assert isinstance(GooseSubprocessRunner(), RecipeRunner)


def test_run_result_defaults() -> None:
    r = RunResult(output="hi")
    assert r.trace_id is None
    assert r.error is None
    assert r.duration_ms == 0
    assert r.extra == {}


# ---------- StubRunner ----------


def test_stub_runner_calls_provided_callable() -> None:
    captured: dict[str, object] = {}

    def fn(recipe: Path, params: dict) -> RunResult:
        captured["recipe"] = recipe
        captured["params"] = params
        return RunResult(output="produced", trace_id="tr-1")

    runner = StubRunner(fn)
    result = runner.run(Path("/tmp/recipe"), {"feature_brief": "do thing"})
    assert result.output == "produced"
    assert result.trace_id == "tr-1"
    assert captured["recipe"] == Path("/tmp/recipe")
    assert captured["params"] == {"feature_brief": "do thing"}


def test_stub_runner_from_outputs_returns_per_input_string() -> None:
    runner = StubRunner.from_outputs(
        outputs_by_input_key={
            "do thing": "## Done thing",
            "do other": "## Other",
        },
        input_key="feature_brief",
    )
    assert runner.run(Path("/tmp"), {"feature_brief": "do thing"}).output == "## Done thing"
    assert runner.run(Path("/tmp"), {"feature_brief": "do other"}).output == "## Other"


def test_stub_runner_from_outputs_raises_on_unknown_input_without_fallback() -> None:
    runner = StubRunner.from_outputs(
        outputs_by_input_key={"a": "out-a"},
        input_key="feature_brief",
    )
    with pytest.raises(KeyError, match="no output for"):
        runner.run(Path("/tmp"), {"feature_brief": "missing"})


def test_stub_runner_from_outputs_falls_back_when_set() -> None:
    runner = StubRunner.from_outputs(
        outputs_by_input_key={"a": "out-a"},
        input_key="feature_brief",
        fallback="fallback-output",
    )
    assert runner.run(Path("/tmp"), {"feature_brief": "missing"}).output == "fallback-output"


# ---------- GooseSubprocessRunner ----------


def test_goose_runner_returns_clear_error_when_binary_missing() -> None:
    runner = GooseSubprocessRunner(goose_bin="this-binary-does-not-exist-zxcv")
    result = runner.run(Path("/tmp/recipe"), {"x": 1})
    assert result.error is not None
    assert "not found on PATH" in result.error
    assert result.output == ""


def test_goose_runner_command_shape() -> None:
    """The CLI we send is captured in one place. Pin it so future goose CLI
    changes are a deliberate, visible diff."""
    runner = GooseSubprocessRunner(goose_bin="goose")
    cmd = runner._build_command(Path("/repo/recipes/test/charter-sfdipot"), {"feature_brief": "hi"})
    assert cmd[0] == "goose"
    assert cmd[1] == "run"
    assert "--recipe" in cmd
    assert "/repo/recipes/test/charter-sfdipot" in cmd
    assert "--params" in cmd
    # Params are JSON-encoded so spaces and quotes in values pass through cleanly.
    params_idx = cmd.index("--params") + 1
    assert '"feature_brief"' in cmd[params_idx]
    assert "hi" in cmd[params_idx]


def test_goose_runner_real_subprocess_against_a_fake_binary(tmp_path: Path) -> None:
    """Sanity check: the runner's subprocess plumbing works when pointed at an
    arbitrary script that mimics goose. Verifies stdout capture, exit handling,
    and timing."""
    fake_goose = tmp_path / "fake_goose"
    fake_goose.write_text(
        '#!/usr/bin/env python3\n'
        'import sys\n'
        'print("# Fake charter\\n## Section")\n'
        'sys.exit(0)\n'
    )
    os.chmod(fake_goose, 0o755)

    runner = GooseSubprocessRunner(goose_bin=str(fake_goose))
    result = runner.run(Path("/tmp/recipe"), {"x": 1})
    assert result.error is None
    assert "# Fake charter" in result.output
    assert "## Section" in result.output
    assert result.duration_ms >= 0


def test_goose_runner_captures_failure_exit_code(tmp_path: Path) -> None:
    fake_goose = tmp_path / "fake_goose"
    fake_goose.write_text(
        '#!/usr/bin/env python3\n'
        'import sys\n'
        'sys.stderr.write("model rejected request\\n")\n'
        'sys.exit(7)\n'
    )
    os.chmod(fake_goose, 0o755)

    runner = GooseSubprocessRunner(goose_bin=str(fake_goose))
    result = runner.run(Path("/tmp/recipe"), {"x": 1})
    assert result.error is not None
    assert "exited 7" in result.error
    assert "model rejected request" in result.error


def test_goose_runner_timeout(tmp_path: Path) -> None:
    fake_goose = tmp_path / "fake_goose"
    fake_goose.write_text(
        '#!/usr/bin/env python3\n'
        'import time\n'
        'time.sleep(5)\n'
    )
    os.chmod(fake_goose, 0o755)

    runner = GooseSubprocessRunner(goose_bin=str(fake_goose), timeout_s=1)
    result = runner.run(Path("/tmp/recipe"), {"x": 1})
    assert result.error is not None
    assert "timeout" in result.error.lower()
