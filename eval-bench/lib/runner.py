"""RecipeRunner abstraction.

A RecipeRunner takes a recipe path and a task's input parameters, runs the
recipe somehow, and returns a RunResult with the recipe output (plus optional
trace id and error). The harness uses whichever runner the user picked at
the CLI; tests instantiate StubRunner directly.

Two implementations ship:

    StubRunner — synchronous, in-process; constructed with a callable that
                 maps (recipe_path, input_params) to a RunResult. Use in tests
                 and for harness smoke without goose installed.

    GooseSubprocessRunner — invokes `goose run --recipe <path> --params <json>`
                            as a subprocess. The exact CLI shape is captured
                            in one place so it can be adjusted without
                            touching the harness.
"""

from __future__ import annotations

import json
import shutil
import subprocess
import time
from dataclasses import dataclass, field
from pathlib import Path
from typing import Any, Callable, Protocol, runtime_checkable


@dataclass
class RunResult:
    """One recipe execution's outcome.

    `output` is the recipe's primary text output (the thing graders consume).
    `trace_id` is a Langfuse / observability handle when available; absent
    runs leave it None and downstream tooling treats that as "no trace."
    `error` is set when the runner could not produce a usable output (e.g.,
    recipe runner crashed, timeout, model rejected the request). When set,
    graders are not run for this trial; the trial is recorded as failed
    with the error preserved in evidence.
    """

    output: str
    trace_id: str | None = None
    error: str | None = None
    duration_ms: int = 0
    extra: dict[str, Any] = field(default_factory=dict)


@runtime_checkable
class RecipeRunner(Protocol):
    """Contract for runners. Synchronous; one trial at a time. Parallelism
    is the harness's concern, not the runner's."""

    def run(self, recipe_path: Path, input_params: dict[str, Any]) -> RunResult: ...


# ---------- Stub runner (tests + smoke) ----------


class StubRunner:
    """Test runner. Constructed with a callable that decides what to return.

    The callable receives the recipe path and the task's input dict and
    returns a RunResult. The caller is responsible for whatever simulation
    they want (fixed output, vary by input, raise to simulate timeout, etc).

    A convenience factory `from_outputs` lets tests use a dict mapping
    a key extracted from the input to a fixed string output.
    """

    def __init__(self, fn: Callable[[Path, dict[str, Any]], RunResult]) -> None:
        self._fn = fn

    def run(self, recipe_path: Path, input_params: dict[str, Any]) -> RunResult:
        return self._fn(recipe_path, input_params)

    @classmethod
    def from_outputs(
        cls,
        outputs_by_input_key: dict[str, str],
        *,
        input_key: str,
        fallback: str = "",
    ) -> "StubRunner":
        """Build a StubRunner that returns a fixed string per input value.

        `input_key` names the field in the task input to use as the lookup key
        (typically the recipe's main parameter name). Inputs not in the map
        return `fallback` if non-empty, otherwise raise KeyError so test
        misses are loud rather than silent.
        """

        def _fn(_recipe: Path, params: dict[str, Any]) -> RunResult:
            key = params.get(input_key, "")
            if key in outputs_by_input_key:
                return RunResult(output=outputs_by_input_key[key])
            if fallback:
                return RunResult(output=fallback)
            raise KeyError(
                f"StubRunner.from_outputs: no output for {input_key}={key!r} "
                f"and no fallback set"
            )

        return cls(_fn)


# ---------- Goose subprocess runner (real execution) ----------


@dataclass
class GooseSubprocessRunner:
    """Invoke `goose` as a subprocess to run the recipe.

    Defaults assume the `goose` binary is on PATH. Set `goose_bin` to an
    absolute path to override. The exact CLI we send is captured in
    `_build_command` so future goose CLI changes are a one-place edit.

    Phase 1 status: the GooseSubprocessRunner is wired up but is not what
    eval-bench's automated tests use — those use StubRunner. The first
    real-goose end-to-end run is its own bring-up exercise; until then the
    runner errors clearly when the binary is missing.
    """

    goose_bin: str = "goose"
    timeout_s: int = 300
    extra_env: dict[str, str] | None = None

    def run(self, recipe_path: Path, input_params: dict[str, Any]) -> RunResult:
        if shutil.which(self.goose_bin) is None:
            return RunResult(
                output="",
                error=(
                    f"goose binary {self.goose_bin!r} not found on PATH. "
                    "Install goose or pass --runner stub for development smoke runs."
                ),
            )

        cmd = self._build_command(recipe_path, input_params)
        started = time.monotonic()
        try:
            completed = subprocess.run(
                cmd,
                capture_output=True,
                text=True,
                check=False,
                timeout=self.timeout_s,
            )
        except subprocess.TimeoutExpired as e:
            elapsed = int((time.monotonic() - started) * 1000)
            return RunResult(
                output="",
                error=f"timeout after {self.timeout_s}s running goose",
                duration_ms=elapsed,
                extra={"timeout_command": " ".join(cmd), "timeout_partial": e.stdout or ""},
            )
        elapsed_ms = int((time.monotonic() - started) * 1000)

        if completed.returncode != 0:
            return RunResult(
                output=completed.stdout or "",
                error=(
                    f"goose exited {completed.returncode}: "
                    f"{(completed.stderr or '').strip()[:500]}"
                ),
                duration_ms=elapsed_ms,
            )

        return RunResult(output=completed.stdout, duration_ms=elapsed_ms)

    def _build_command(self, recipe_path: Path, input_params: dict[str, Any]) -> list[str]:
        return [
            self.goose_bin,
            "run",
            "--recipe",
            str(recipe_path),
            "--params",
            json.dumps(input_params),
            "--no-session",
        ]
