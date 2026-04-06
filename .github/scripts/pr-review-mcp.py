#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.11"
# dependencies = ["mcp"]
# ///
"""MCP server for collecting PR review comments and conclusion."""

import json
import os
from pathlib import Path

from mcp.server.fastmcp import FastMCP

server = FastMCP("pr-review")

output_dir = Path(os.environ.get("REVIEW_OUTPUT_DIR", "/tmp"))


def _append_comment(comment: dict) -> int:
    """Append a comment to the comments file and return the new total."""
    comments_file = output_dir / "comments.json"
    comments = json.loads(comments_file.read_text()) if comments_file.exists() else []
    comments.append(comment)
    comments_file.write_text(json.dumps(comments, indent=2))
    return len(comments)


@server.tool()
def add_comment(
    path: str,
    line: int,
    body: str,
    suggestion: str | None = None,
    side: str = "RIGHT",
    start_line: int | None = None,
) -> str:
    """Add a review comment on a specific line in the PR diff.

    Args:
        path: The relative file path in the repository (e.g. "src/main.rs").
        line: The line number in the file that the comment applies to.
              For added or modified lines, use the line number in the new version of the file (side=RIGHT).
              For deleted lines, use the line number in the old version of the file (side=LEFT).
        body: The review comment text (Markdown supported).
        suggestion: Optional replacement code for the line(s). When provided, GitHub renders an
                    "Apply suggestion" button the author can click. The suggestion replaces the
                    entire line (or range if start_line is set).
        side: Which version of the file the line number refers to.
              "RIGHT" for the new/modified version (default), "LEFT" for the old/deleted version.
        start_line: For multi-line comments, the first line of the range. When set, `line` is the last line.
    """
    if suggestion is not None:
        body = (
            f"{body}\n\n```suggestion\n{suggestion}\n```"
            if body
            else f"```suggestion\n{suggestion}\n```"
        )

    comment = {"path": path, "line": line, "side": side, "body": body}
    if start_line is not None:
        comment["start_line"] = start_line
        comment["start_side"] = side

    total = _append_comment(comment)
    return f"Comment added on {path}:{line} ({total} total)."


@server.tool()
def finish_review(body: str = "") -> str:
    """Finish the review.

    Args:
        body: Optional top-level review body (Markdown supported). Only include if it
              contains information not already covered by inline comments. Most reviews
              should leave this empty.
    """
    conclusion = {"body": body, "event": "COMMENT"}
    conclusion_file = output_dir / "conclusion.json"
    conclusion_file.write_text(json.dumps(conclusion, indent=2))
    return "Review finished."


if __name__ == "__main__":
    server.run(transport="stdio")
