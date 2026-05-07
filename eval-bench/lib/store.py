"""Local SQLite results store.

Single source of truth for eval-bench results across runs. Written to by
run_kpass.py; read by the Tauri Slice Explorer, Quality Observatory, and the
Saturation Alarm. No remote backend. Path defaults to ~/.skein/eval-bench.sqlite.
"""

from __future__ import annotations

import json
import sqlite3
from contextlib import contextmanager
from datetime import datetime, timezone
from pathlib import Path
from typing import Any, Iterator

DEFAULT_STORE_PATH = Path.home() / ".skein" / "eval-bench.sqlite"


SCHEMA_SQL = """
CREATE TABLE IF NOT EXISTS run (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    recipe TEXT NOT NULL,
    started_at TEXT NOT NULL,
    finished_at TEXT,
    k INTEGER NOT NULL,
    notes TEXT
);

CREATE TABLE IF NOT EXISTS trial (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    run_id INTEGER NOT NULL REFERENCES run(id) ON DELETE CASCADE,
    task_id TEXT NOT NULL,
    trial_index INTEGER NOT NULL,
    passed INTEGER NOT NULL CHECK (passed IN (0, 1)),
    polarity TEXT NOT NULL,
    tags TEXT NOT NULL,
    axes_json TEXT NOT NULL,
    grader_scores_json TEXT NOT NULL,
    duration_ms INTEGER,
    trace_id TEXT
);

CREATE INDEX IF NOT EXISTS idx_trial_run ON trial(run_id);
CREATE INDEX IF NOT EXISTS idx_trial_task ON trial(task_id);
CREATE INDEX IF NOT EXISTS idx_trial_recipe ON trial(run_id, task_id);
"""


class ResultsStore:
    def __init__(self, path: Path | None = None) -> None:
        self.path = path or DEFAULT_STORE_PATH
        self.path.parent.mkdir(parents=True, exist_ok=True)
        with self._connect() as conn:
            conn.executescript(SCHEMA_SQL)

    @contextmanager
    def _connect(self) -> Iterator[sqlite3.Connection]:
        conn = sqlite3.connect(self.path)
        conn.execute("PRAGMA foreign_keys = ON")
        try:
            yield conn
            conn.commit()
        finally:
            conn.close()

    def start_run(self, recipe: str, k: int, notes: str | None = None) -> int:
        with self._connect() as conn:
            cur = conn.execute(
                "INSERT INTO run (recipe, started_at, k, notes) VALUES (?, ?, ?, ?)",
                (recipe, _now_iso(), k, notes),
            )
            return int(cur.lastrowid)

    def finish_run(self, run_id: int) -> None:
        with self._connect() as conn:
            conn.execute(
                "UPDATE run SET finished_at = ? WHERE id = ?",
                (_now_iso(), run_id),
            )

    def record_trial(
        self,
        *,
        run_id: int,
        task_id: str,
        trial_index: int,
        passed: bool,
        polarity: str,
        tags: list[str],
        axes: dict[str, Any],
        grader_scores: dict[str, Any],
        duration_ms: int | None = None,
        trace_id: str | None = None,
    ) -> None:
        with self._connect() as conn:
            conn.execute(
                """
                INSERT INTO trial (
                    run_id, task_id, trial_index, passed, polarity, tags,
                    axes_json, grader_scores_json, duration_ms, trace_id
                ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
                """,
                (
                    run_id,
                    task_id,
                    trial_index,
                    1 if passed else 0,
                    polarity,
                    ",".join(tags),
                    json.dumps(axes, sort_keys=True),
                    json.dumps(grader_scores, sort_keys=True),
                    duration_ms,
                    trace_id,
                ),
            )


def _now_iso() -> str:
    return datetime.now(timezone.utc).isoformat()
