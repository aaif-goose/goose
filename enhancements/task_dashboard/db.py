"""Read-only, cached access layer for Goose sessions.db.

All queries open the DB in read-only URI mode so the dashboard cannot
interfere with a live Goose process writing to the same file.
"""
from __future__ import annotations

import sqlite3
from contextlib import contextmanager
from pathlib import Path
from typing import Iterator

import pandas as pd
import streamlit as st


@contextmanager
def _ro_connect(db_path: Path) -> Iterator[sqlite3.Connection]:
    uri = f"file:{db_path.as_posix()}?mode=ro"
    con = sqlite3.connect(uri, uri=True, detect_types=sqlite3.PARSE_DECLTYPES)
    try:
        yield con
    finally:
        con.close()


def _db_mtime_key(db_path: Path) -> float:
    """Cache key: when the file changes, caches invalidate automatically."""
    try:
        return db_path.stat().st_mtime
    except FileNotFoundError:
        return 0.0


@st.cache_data(ttl=60, show_spinner=False)
def list_tables(db_path_str: str, _mtime: float) -> list[str]:
    with _ro_connect(Path(db_path_str)) as con:
        return [
            r[0]
            for r in con.execute(
                "SELECT name FROM sqlite_master WHERE type='table' ORDER BY name"
            ).fetchall()
        ]


@st.cache_data(ttl=60, show_spinner=False)
def load_sessions(db_path_str: str, _mtime: float) -> pd.DataFrame:
    """Return the full sessions table as a DataFrame, with message count joined."""
    db_path = Path(db_path_str)
    with _ro_connect(db_path) as con:
        sql = """
        SELECT
            s.id,
            s.name,
            COALESCE(s.description, '') AS description,
            s.session_type,
            s.working_dir,
            s.created_at,
            s.updated_at,
            s.total_tokens,
            s.input_tokens,
            s.output_tokens,
            s.accumulated_total_tokens,
            s.provider_name,
            COALESCE(m.msg_count, 0) AS message_count
        FROM sessions s
        LEFT JOIN (
            SELECT session_id, COUNT(*) AS msg_count
            FROM messages
            GROUP BY session_id
        ) m ON m.session_id = s.id
        ORDER BY s.updated_at DESC
        """
        df = pd.read_sql_query(sql, con)

    for col in ("created_at", "updated_at"):
        df[col] = pd.to_datetime(df[col], errors="coerce", utc=True)

    df["duration_min"] = (
        (df["updated_at"] - df["created_at"]).dt.total_seconds() / 60.0
    ).round(2)
    return df


@st.cache_data(ttl=60, show_spinner=False)
def load_session_messages(
    db_path_str: str, session_id: str, _mtime: float
) -> pd.DataFrame:
    with _ro_connect(Path(db_path_str)) as con:
        sql = """
        SELECT id, message_id, role, content_json, created_timestamp,
               timestamp, tokens, metadata_json
        FROM messages
        WHERE session_id = ?
        ORDER BY id ASC
        """
        df = pd.read_sql_query(sql, con, params=(session_id,))
    df["timestamp"] = pd.to_datetime(df["timestamp"], errors="coerce", utc=True)
    return df


@st.cache_data(ttl=60, show_spinner=False)
def load_session_row(db_path_str: str, session_id: str, _mtime: float) -> dict | None:
    with _ro_connect(Path(db_path_str)) as con:
        cur = con.execute("SELECT * FROM sessions WHERE id = ?", (session_id,))
        row = cur.fetchone()
        if row is None:
            return None
        cols = [c[0] for c in cur.description]
        return dict(zip(cols, row))


@st.cache_data(ttl=60, show_spinner=False)
def distinct_working_dirs(db_path_str: str, _mtime: float) -> list[str]:
    with _ro_connect(Path(db_path_str)) as con:
        return [
            r[0]
            for r in con.execute(
                "SELECT DISTINCT working_dir FROM sessions "
                "WHERE working_dir IS NOT NULL AND working_dir != '' "
                "ORDER BY working_dir"
            ).fetchall()
        ]


def mtime_for(db_path: Path) -> float:
    """Public helper for pages to pass a cache-invalidation key."""
    return _db_mtime_key(db_path)


@st.cache_data(ttl=60, show_spinner=False)
def load_all_tags(db_path_str: str, _mtime: float) -> pd.DataFrame:
    """Return all session_tags rows as a DataFrame. Returns empty frame if table absent."""
    db_path = Path(db_path_str)
    with _ro_connect(db_path) as con:
        tables = [
            r[0]
            for r in con.execute(
                "SELECT name FROM sqlite_master WHERE type='table' AND name='session_tags'"
            ).fetchall()
        ]
        if not tables:
            return pd.DataFrame(columns=["session_id", "tag", "source", "created_at"])
        sql = """
        SELECT session_id, tag, source, created_at
        FROM session_tags
        ORDER BY created_at DESC
        """
        return pd.read_sql_query(sql, con)


@st.cache_data(ttl=60, show_spinner=False)
def load_tags_for_session(db_path_str: str, session_id: str, _mtime: float) -> list[str]:
    """Return tags for one session. Returns empty list if table absent."""
    db_path = Path(db_path_str)
    with _ro_connect(db_path) as con:
        tables = [
            r[0]
            for r in con.execute(
                "SELECT name FROM sqlite_master WHERE type='table' AND name='session_tags'"
            ).fetchall()
        ]
        if not tables:
            return []
        rows = con.execute(
            "SELECT tag FROM session_tags WHERE session_id = ? ORDER BY tag", (session_id,)
        ).fetchall()
        return [r[0] for r in rows]
