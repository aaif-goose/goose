"""Local SQLite results store.

Single source of truth for eval-bench results across runs. Written to by
run_kpass.py; read by the slices CLI (and eventually a Tauri Slice
Explorer / Quality Observatory). No remote backend. Path defaults to
~/.skein/eval-bench.sqlite.
"""

from __future__ import annotations

import json
import sqlite3
from contextlib import contextmanager
from dataclasses import dataclass, field
from datetime import datetime, timezone
from pathlib import Path
from typing import Any, Iterator

DEFAULT_STORE_PATH = Path.home() / ".skein" / "eval-bench.sqlite"


@dataclass
class RunRow:
    """One row from the `run` table, plus a derived trial count for display."""

    id: int
    recipe: str
    started_at: str
    finished_at: str | None
    k: int
    notes: str | None
    n_trials: int = 0


@dataclass
class TrialRow:
    """One row from the `trial` table, with axes and grader_scores deserialised."""

    id: int
    run_id: int
    task_id: str
    trial_index: int
    passed: bool
    polarity: str
    tags: list[str]
    axes: dict[str, Any]
    grader_scores: dict[str, Any]
    duration_ms: int | None
    trace_id: str | None


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

    def list_runs(self, *, limit: int = 50, recipe: str | None = None) -> list[RunRow]:
        """Return the most recent runs, newest first.

        If `recipe` is set, restricts to that recipe path. Each row includes
        the trial count, so a one-screen `runs` listing shows headline shape.
        """
        sql = """
            SELECT r.id, r.recipe, r.started_at, r.finished_at, r.k, r.notes,
                   COALESCE(c.n, 0) AS n_trials
            FROM run r
            LEFT JOIN (
                SELECT run_id, COUNT(*) AS n FROM trial GROUP BY run_id
            ) c ON c.run_id = r.id
        """
        params: list[Any] = []
        if recipe is not None:
            sql += " WHERE r.recipe = ?"
            params.append(recipe)
        sql += " ORDER BY r.id DESC LIMIT ?"
        params.append(limit)
        with self._connect() as conn:
            rows = conn.execute(sql, params).fetchall()
        return [
            RunRow(
                id=r[0], recipe=r[1], started_at=r[2], finished_at=r[3],
                k=r[4], notes=r[5], n_trials=r[6],
            )
            for r in rows
        ]

    def get_run(self, run_id: int) -> RunRow | None:
        with self._connect() as conn:
            row = conn.execute(
                """
                SELECT r.id, r.recipe, r.started_at, r.finished_at, r.k, r.notes,
                       COALESCE((SELECT COUNT(*) FROM trial WHERE run_id = r.id), 0)
                FROM run r WHERE r.id = ?
                """,
                (run_id,),
            ).fetchone()
        if row is None:
            return None
        return RunRow(
            id=row[0], recipe=row[1], started_at=row[2], finished_at=row[3],
            k=row[4], notes=row[5], n_trials=row[6],
        )

    def get_trials(self, run_id: int) -> list[TrialRow]:
        """Return all trials for a run, ordered by (task_id, trial_index)."""
        with self._connect() as conn:
            rows = conn.execute(
                """
                SELECT id, run_id, task_id, trial_index, passed, polarity, tags,
                       axes_json, grader_scores_json, duration_ms, trace_id
                FROM trial WHERE run_id = ? ORDER BY task_id, trial_index
                """,
                (run_id,),
            ).fetchall()
        out: list[TrialRow] = []
        for r in rows:
            out.append(
                TrialRow(
                    id=r[0], run_id=r[1], task_id=r[2], trial_index=r[3],
                    passed=bool(r[4]),
                    polarity=r[5],
                    tags=[t for t in (r[6] or "").split(",") if t],
                    axes=json.loads(r[7] or "{}"),
                    grader_scores=json.loads(r[8] or "{}"),
                    duration_ms=r[9],
                    trace_id=r[10],
                )
            )
        return out

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
