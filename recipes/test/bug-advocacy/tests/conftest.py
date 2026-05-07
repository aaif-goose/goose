"""Make eval-bench/lib importable from this recipe's tests."""

from __future__ import annotations

import sys
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parents[4]
EVAL_BENCH_DIR = REPO_ROOT / "eval-bench"
if str(EVAL_BENCH_DIR) not in sys.path:
    sys.path.insert(0, str(EVAL_BENCH_DIR))
