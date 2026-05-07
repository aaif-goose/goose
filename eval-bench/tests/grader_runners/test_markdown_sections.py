"""Unit + integration tests for the markdown_sections grader runner."""

from __future__ import annotations

import json
import subprocess
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[2] / "grader_runners"))
import markdown_sections  # noqa: E402


# ---------- unit: extract_headings ----------


def test_extracts_h2_headings() -> None:
    md = """
## Structure
content
## Function
more
"""
    assert markdown_sections.extract_headings(md, level=2) == ["Structure", "Function"]


def test_strips_trailing_closing_hashes() -> None:
    """Markdown allows `## Foo ##` — the closing hashes are decoration, not the title."""
    md = "## Function ##\n"
    assert markdown_sections.extract_headings(md, level=2) == ["Function"]


def test_ignores_wrong_heading_level() -> None:
    md = "# Title\n## Function\n### Detail\n"
    assert markdown_sections.extract_headings(md, level=2) == ["Function"]
    assert markdown_sections.extract_headings(md, level=1) == ["Title"]
    assert markdown_sections.extract_headings(md, level=3) == ["Detail"]


def test_skips_headings_inside_fenced_code_blocks() -> None:
    md = """
## Real

```
## Fake
```

## Also Real

~~~
## Also Fake
~~~

## Last
"""
    assert markdown_sections.extract_headings(md, level=2) == ["Real", "Also Real", "Last"]


def test_empty_heading_text_is_dropped() -> None:
    md = "## \n## Real\n"
    assert markdown_sections.extract_headings(md, level=2) == ["Real"]


# ---------- unit: grade ----------


def test_grade_passes_when_all_sections_present() -> None:
    md = "## Structure\n## Function\n## Data\n"
    r = markdown_sections.grade(md, required=["Structure", "Function", "Data"], level=2)
    assert r.passed is True
    assert r.score == 1.0


def test_grade_is_case_insensitive() -> None:
    md = "## structure\n## FUNCTION\n"
    r = markdown_sections.grade(md, required=["Structure", "Function"], level=2)
    assert r.passed is True


def test_grade_partial_score_when_some_missing() -> None:
    md = "## Structure\n## Function\n"
    r = markdown_sections.grade(md, required=["Structure", "Function", "Data", "Time"], level=2)
    assert r.passed is False
    assert r.score == 0.5  # 2 of 4
    assert "Data" in r.details and "Time" in r.details


def test_grade_zero_when_none_present() -> None:
    r = markdown_sections.grade("no headings here", required=["A", "B"], level=2)
    assert r.passed is False
    assert r.score == 0.0


def test_grade_section_order_does_not_matter() -> None:
    md = "## Time\n## Function\n## Structure\n"
    r = markdown_sections.grade(md, required=["Structure", "Function", "Time"], level=2)
    assert r.passed is True


# ---------- integration: subprocess ----------


RUNNER = Path(__file__).resolve().parents[2] / "grader_runners" / "markdown_sections.py"


def _invoke(stdin_payload: str, *args: str) -> subprocess.CompletedProcess:
    return subprocess.run(
        [sys.executable, str(RUNNER), *args],
        input=stdin_payload,
        capture_output=True,
        text=True,
        check=False,
    )


def test_subprocess_pass_for_full_sfdipot() -> None:
    md = "\n".join(f"## {s}" for s in ["Structure", "Function", "Data", "Interfaces", "Platform", "Operations", "Time"])
    res = _invoke(
        json.dumps({"output": md, "task": {}}),
        "--required", "Structure,Function,Data,Interfaces,Platform,Operations,Time",
    )
    assert res.returncode == 0
    body = json.loads(res.stdout.strip())
    assert body["passed"] is True


def test_subprocess_fail_when_section_missing() -> None:
    md = "## Structure\n## Function\n"
    res = _invoke(
        json.dumps({"output": md, "task": {}}),
        "--required", "Structure,Function,Data",
    )
    assert res.returncode == 1
    body = json.loads(res.stdout.strip())
    assert body["passed"] is False
    assert "Data" in body["details"]


def test_subprocess_input_error_on_missing_required_flag() -> None:
    res = _invoke(json.dumps({"output": "x", "task": {}}))
    # argparse rejects missing --required with exit code 2.
    assert res.returncode == 2


def test_subprocess_input_error_on_empty_required_after_split() -> None:
    res = _invoke(json.dumps({"output": "x", "task": {}}), "--required", ",,,")
    assert res.returncode == 2


def test_subprocess_input_error_on_bad_level() -> None:
    res = _invoke(json.dumps({"output": "x", "task": {}}), "--required", "A", "--level", "0")
    assert res.returncode == 2
