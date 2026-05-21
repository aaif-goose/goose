"""Minimal goose SDK demo: ask the agent to ping aaif.io."""

from __future__ import annotations

import os
import sys
from pathlib import Path

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE.parent.parent))

from generated.goose_sdk import Agent, EventSink  # noqa: E402
from generated.goose_sdk_types import AgentEvent, ExtensionSpec, ProviderSpec  # noqa: E402

DIM = "\033[2m"
CYAN = "\033[36m"
GREEN = "\033[32m"
RED = "\033[31m"
RESET = "\033[0m"


def paint(color: str, text: str) -> str:
    return f"{color}{text}{RESET}"


def preview(output: str, max_lines: int = 3, max_width: int = 100) -> str:
    lines = (line[:max_width] for line in output.splitlines() if line.strip())
    return "\n  ".join(list(lines)[:max_lines])


class Printer(EventSink):
    def __init__(self) -> None:
        self._mid_text = False

    def on_event(self, event: AgentEvent) -> None:
        if isinstance(event, AgentEvent.ASSISTANT_TEXT):
            print(event.text, end="", flush=True)
            self._mid_text = True
            return

        self._end_text_line()

        if isinstance(event, AgentEvent.TOOL_REQUEST):
            args = event.arguments.replace("\n", " ")[:120]
            print(f"{paint(CYAN, '→ ' + event.name)} {paint(DIM, args)}", flush=True)

        elif isinstance(event, AgentEvent.TOOL_RESPONSE):
            color = RED if event.is_error else GREEN
            marker = "✗" if event.is_error else "✓"
            print(f"{paint(color, marker)} {paint(DIM, preview(event.output))}\n", flush=True)

    def on_error(self, error: str) -> None:
        print(f"\n{paint(RED, 'error:')} {error}", file=sys.stderr)

    def on_done(self) -> None:
        self._end_text_line()

    def _end_text_line(self) -> None:
        if self._mid_text:
            print()
            self._mid_text = False


def main() -> None:
    print(paint(DIM, "configuring agent…"), file=sys.stderr)

    agent = Agent()
    agent.configure(
        ProviderSpec(
            name=os.environ.get("GOOSE_PROVIDER"),
            model=os.environ.get("GOOSE_MODEL"),
        ),
        [ExtensionSpec.BUILTIN(name="developer")],
    )

    print(paint(DIM, "> ping aaif.io") + "\n", file=sys.stderr)
    agent.reply("ping aaif.io", Printer())


if __name__ == "__main__":
    main()
