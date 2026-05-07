"""eval-bench: Skein's recipe evaluation library.

Public API:
    load_tasks(path)               -> list[Task]
    load_failure_modes(path)       -> FailureModes
    load_graders(path)             -> Graders
    compute_passk(per_trial, k)    -> (pass_at_k, pass_pow_k)
    slice_results(results, axis)   -> dict[str, list[Result]]
"""

from .calibration import CalibrationRecord, load_calibration_log
from .failure_modes import FailureMode, FailureModes, load_failure_modes
from .graders import (
    Grader,
    Graders,
    L1Grader,
    L2Grader,
    L3Grader,
    load_graders,
)
from .kpass import compute_passk, passk_by_slice, slice_results
from .store import ResultsStore
from .tasks import Task, load_tasks

__all__ = [
    "CalibrationRecord",
    "FailureMode",
    "FailureModes",
    "Grader",
    "Graders",
    "L1Grader",
    "L2Grader",
    "L3Grader",
    "ResultsStore",
    "Task",
    "compute_passk",
    "load_calibration_log",
    "load_failure_modes",
    "load_graders",
    "load_tasks",
    "passk_by_slice",
    "slice_results",
]
