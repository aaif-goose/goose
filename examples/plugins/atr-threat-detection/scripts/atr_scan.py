#!/usr/bin/env python3
"""ATR PreToolUse hook — blocks tool calls that match ATR detection rules.

Reads the goose HookContext JSON from stdin. If pyatr detects a critical or
high severity match in the tool_input, exits with code 2 (denial) and prints
the rule ID + title to stderr. Any error in the scanner (import failure,
parse error, timeout) allows the call through — a broken guard must never
block legitimate work.
"""
import json
import sys

_BLOCK_SEVERITIES = {"critical", "high"}


def main() -> None:
    try:
        payload = json.load(sys.stdin)
    except Exception:
        sys.exit(0)

    tool_name = payload.get("tool_name", "")
    tool_input = payload.get("tool_input")
    if tool_input is None:
        sys.exit(0)

    # Flatten tool_input to a string for ATR scanning.
    text = json.dumps(tool_input) if not isinstance(tool_input, str) else tool_input

    try:
        import pyatr

        matches = pyatr.scan(text)
    except ImportError:
        # pyatr not installed — allow through, print advisory once.
        print(
            "[atr-threat-detection] pyatr not installed; install with: pip install pyatr",
            file=sys.stderr,
        )
        sys.exit(0)
    except Exception:
        sys.exit(0)

    blocked = [m for m in matches if m.severity.lower() in _BLOCK_SEVERITIES]
    if not blocked:
        sys.exit(0)

    top = blocked[0]
    print(
        f"[ATR] Blocked tool call to '{tool_name}': [{top.rule_id}] {top.title} "
        f"(severity={top.severity}, confidence={top.confidence})",
        file=sys.stderr,
    )
    sys.exit(2)


if __name__ == "__main__":
    main()
