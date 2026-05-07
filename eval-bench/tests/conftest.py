"""pytest setup: make `lib` importable when running from repo root.

Usage:
    cd <repo root>
    pytest eval-bench/tests
"""

from __future__ import annotations

import sys
from pathlib import Path

EVAL_BENCH_DIR = Path(__file__).resolve().parent.parent
if str(EVAL_BENCH_DIR) not in sys.path:
    sys.path.insert(0, str(EVAL_BENCH_DIR))
