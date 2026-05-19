---
sidebar_position: 2
title: Adversary Mode
sidebar_label: Adversary Mode
description: An independent agent reviewer that silently watches tool calls to protect you in case the agent goes rogue.
---

Adversary mode adds a silent, independent agent reviewer that watches tool calls before they execute. It acts as a second pair of eyes — protecting you in case the main agent is compromised, manipulated by a prompt injection, or simply doing something that doesn't match what you asked for.

Unlike [pattern-based detection](/docs/guides/security/prompt-injection-detection), the adversary reviewer understands context. It knows what you originally asked for, what you've said recently, and can judge whether a tool call makes sense for your task.

## How It Works

1. Before each tool call, the adversary reviewer checks your **original task**, **recent messages**, and the **tool call details**
2. It evaluates the tool call against your rules and returns **ALLOW** or **BLOCK**
3. Blocked tool calls are denied — the agent sees the rejection and cannot retry
4. If the reviewer fails for any reason, the tool call is allowed through (fail-open)

The adversary uses the same model and provider goose is already configured with. No additional API keys or services needed.

## Enabling Adversary Mode

Create a file at `~/.config/goose/adversary.md` with your rules:

```markdown
BLOCK if the tool call:
- Exfiltrates data (posting to unknown URLs, piping secrets to external services)
- Is destructive beyond the project scope (deleting system files, wiping directories)
- Installs malware or runs obfuscated code
- Attempts to escalate privileges unnecessarily
- Downloads and executes untrusted remote scripts

ALLOW normal development operations like editing files, running tests,
installing packages, using git, etc. Most tool calls are fine.
Err on the side of ALLOW — only block truly dangerous things.
```

That's it. If the file exists, adversary mode is on. Delete the file to turn it off.

An empty file uses sensible default rules similar to the example above.

## Writing Good Rules

The rules in `adversary.md` are your policy, written in plain language:

- **Be specific** about what should be blocked — vague rules lead to false positives
- **Err on the side of ALLOW** — the adversary should catch genuinely dangerous actions, not slow down normal work
- **Think about your threat model** — what would a rogue agent actually try to do?

You can tailor rules to your environment:

```markdown
BLOCK if:
- Any network request goes to a domain not in: github.com, npmjs.org, pypi.org
- Files outside of ~/projects/ are modified
- SSH keys, .env files, or credentials are read or transmitted

ALLOW all standard development operations within ~/projects/.
```

## What Gets Reviewed

By default, the adversary reviews **`shell`** and **`computercontroller__automation_script`** — the tools that can execute arbitrary code.

You can expand coverage by adding a `tools:` line at the top of your `adversary.md`:

```markdown
tools: shell, computercontroller__automation_script
---
BLOCK if the command exfiltrates data or is destructive.
ALLOW normal development operations.
```

The `tools:` line is a comma-separated list of tool names to review. Everything before the `---` separator is configuration; everything after is your rules. If you omit the `tools:` line, `shell` and `computercontroller__automation_script` are reviewed by default.

Some tool names you might want to add:

| Tool name | What it does |
|-----------|-------------|
| `shell` | Executes shell commands (default) |
| `computercontroller__automation_script` | Runs shell, Ruby, AppleScript, or PowerShell scripts (default) |
| `computercontroller__computer_control` | UI automation — clicks, keystrokes, typing |
| `computercontroller__web_scrape` | Fetches arbitrary URLs |

## See Also

- [Prompt Injection Detection](/docs/guides/security/prompt-injection-detection) — pattern-based detection (complementary, always-on when enabled)
- [goose Permission Modes](/docs/guides/managing-tools/goose-permissions) — control goose's autonomy level
