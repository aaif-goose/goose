"""SQLite migration and tag CRUD for the session_tags table."""
from __future__ import annotations

import sqlite3
from pathlib import Path


def ensure_tags_table(db_path: str | Path) -> None:
    conn = sqlite3.connect(str(db_path))
    conn.execute(
        """
        CREATE TABLE IF NOT EXISTS session_tags (
            session_id TEXT NOT NULL,
            tag        TEXT NOT NULL,
            source     TEXT NOT NULL DEFAULT 'manual',
            created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
            PRIMARY KEY (session_id, tag),
            FOREIGN KEY (session_id) REFERENCES sessions(id) ON DELETE CASCADE
        )
        """
    )
    conn.execute(
        "CREATE INDEX IF NOT EXISTS idx_session_tags_tag ON session_tags(tag)"
    )
    conn.execute(
        "CREATE INDEX IF NOT EXISTS idx_session_tags_session_id ON session_tags(session_id)"
    )
    conn.commit()
    conn.close()


def get_tags_for_session(db_path: str | Path, session_id: str) -> list[dict]:
    conn = sqlite3.connect(str(db_path))
    conn.row_factory = sqlite3.Row
    cur = conn.execute(
        "SELECT tag, source, created_at FROM session_tags WHERE session_id = ? ORDER BY created_at",
        (session_id,),
    )
    rows = [dict(r) for r in cur.fetchall()]
    conn.close()
    return rows


def get_all_unique_tags(db_path: str | Path) -> list[str]:
    conn = sqlite3.connect(str(db_path))
    cur = conn.execute(
        "SELECT DISTINCT tag FROM session_tags ORDER BY tag"
    )
    tags = [row[0] for row in cur.fetchall()]
    conn.close()
    return tags


def add_tags(
    db_path: str | Path,
    session_id: str,
    tags: list[str],
    source: str = "auto",
) -> None:
    conn = sqlite3.connect(str(db_path))
    conn.executemany(
        "INSERT OR IGNORE INTO session_tags (session_id, tag, source) VALUES (?, ?, ?)",
        [(session_id, tag.lower().strip(), source) for tag in tags],
    )
    conn.commit()
    conn.close()


def remove_tag(db_path: str | Path, session_id: str, tag: str) -> None:
    conn = sqlite3.connect(str(db_path))
    conn.execute(
        "DELETE FROM session_tags WHERE session_id = ? AND tag = ?",
        (session_id, tag),
    )
    conn.commit()
    conn.close()


def get_untagged_sessions(db_path: str | Path) -> list[dict]:
    conn = sqlite3.connect(str(db_path))
    conn.row_factory = sqlite3.Row
    cur = conn.execute(
        """
        SELECT id, name, created_at
        FROM sessions
        WHERE session_type = 'user'
          AND id NOT IN (SELECT DISTINCT session_id FROM session_tags WHERE source = 'auto')
        ORDER BY created_at DESC
        """
    )
    rows = [dict(r) for r in cur.fetchall()]
    conn.close()
    return rows


def get_all_sessions(db_path: str | Path) -> list[dict]:
    conn = sqlite3.connect(str(db_path))
    conn.row_factory = sqlite3.Row
    cur = conn.execute(
        """
        SELECT id, name, created_at
        FROM sessions
        WHERE session_type = 'user'
        ORDER BY created_at DESC
        """
    )
    rows = [dict(r) for r in cur.fetchall()]
    conn.close()
    return rows


def get_sessions_by_tag(db_path: str | Path, tag: str) -> list[dict]:
    conn = sqlite3.connect(str(db_path))
    conn.row_factory = sqlite3.Row
    cur = conn.execute(
        """
        SELECT s.id, s.name, s.created_at
        FROM sessions s
        JOIN session_tags t ON t.session_id = s.id
        WHERE t.tag = ?
        ORDER BY s.created_at DESC
        """,
        (tag,),
    )
    rows = [dict(r) for r in cur.fetchall()]
    conn.close()
    return rows


def get_messages_for_session(
    db_path: str | Path,
    session_id: str,
    limit: int = 10,
) -> list[dict]:
    conn = sqlite3.connect(str(db_path))
    conn.row_factory = sqlite3.Row
    cur = conn.execute(
        """
        SELECT role, content_json
        FROM messages
        WHERE session_id = ?
        ORDER BY created_timestamp ASC
        LIMIT ?
        """,
        (session_id, limit),
    )
    rows = [dict(r) for r in cur.fetchall()]
    conn.close()
    return rows
