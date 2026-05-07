"""eval-bench: Skein's recipe evaluation library.

Public API:
    load_tasks(path)               -> list[Task]
    load_failure_modes(path)       -> FailureModes
    load_graders(path)             -> Graders
    compute_passk(per_trial, k)    -> (pass_at_k, pass_pow_k)
    slice_results(results, axis)   -> dict[str, list[Result]]
"""

from .calibration import CalibrationRecord, load_calibration_log
from .composition import GraderOutcome, compose_trial_pass, effective_passed
from .failure_modes import FailureMode, FailureModes, load_failure_modes
from .graders import (
    Grader,
    Graders,
    L1Grader,
    L2Grader,
    L3Grader,
    load_graders,
)
from .grading import grade_one
from .judge import AnthropicJudge, Judge, JudgeVerdict, StubJudge, parse_verdict
from .kpass import compute_passk, passk_by_slice, slice_results
from .runner import GooseSubprocessRunner, RecipeRunner, RunResult, StubRunner
from .store import ResultsStore
from .tasks import Task, load_tasks

__all__ = [
    "AnthropicJudge",
    "CalibrationRecord",
    "FailureMode",
    "FailureModes",
    "GooseSubprocessRunner",
    "Grader",
    "GraderOutcome",
    "Graders",
    "Judge",
    "JudgeVerdict",
    "L1Grader",
    "L2Grader",
    "L3Grader",
    "RecipeRunner",
    "ResultsStore",
    "RunResult",
    "StubJudge",
    "StubRunner",
    "Task",
    "compose_trial_pass",
    "compute_passk",
    "effective_passed",
    "grade_one",
    "load_calibration_log",
    "load_failure_modes",
    "load_graders",
    "load_tasks",
    "parse_verdict",
    "passk_by_slice",
    "slice_results",
]
