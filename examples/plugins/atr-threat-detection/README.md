# atr-threat-detection

Blocks tool calls that match [Agent Threat Rules (ATR)](https://github.com/Agent-Threat-Rule/agent-threat-rules) — 425 detection rules covering prompt injection, tool poisoning, credential exfiltration, and 9 other AI agent attack categories.

Uses the `PreToolUse` hook introduced in goose PR #9304. A tool call is blocked (exit code 2) when ATR finds a critical or high severity match in the tool input. Any scanner error allows the call through — a broken guard must never block legitimate work.

## Requirements

```
pip install pyatr
```

## Installation

```
goose plugin add /path/to/examples/plugins/atr-threat-detection
```

Or install directly from the ATR repo:

```
goose plugin add https://github.com/Agent-Threat-Rule/agent-threat-rules/tree/main/integrations/goose
```

## What gets blocked

| Category | Example |
|---|---|
| Prompt injection | Instructions embedded in tool responses to override agent behaviour |
| Tool poisoning | MCP tool descriptions that hijack agent goals |
| Credential exfiltration | Tool calls attempting to read and send `.env` / SSH keys |
| Privilege escalation | Commands that request elevated system access |

Full rule list: [Agent-Threat-Rule/agent-threat-rules/rules](https://github.com/Agent-Threat-Rule/agent-threat-rules/tree/main/rules)

## False positives

Security scanning tooling and educational content describing attack patterns may trigger rules. Add an ATR rule exclusion list or adjust `_BLOCK_SEVERITIES` in `scripts/atr_scan.py` to tune sensitivity.
