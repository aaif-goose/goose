---
title: "Hooks: run your own scripts on every goose event"
description: "goose now supports lifecycle hooks via the Open Plugins spec. Wire shell scripts into PreToolUse, PostToolUse, UserPromptSubmit, SessionStart, and more."
image: /img/blog/goose-hooks.jpg
authors:
  - alexhancock
---

![Hooks: run your own scripts on every goose event](/img/blog/goose-hooks.jpg)

goose now supports **lifecycle hooks**. Drop a plugin into a directory on disk and goose will run your shell scripts when things happen during a session: a tool is about to fire, a tool just finished, the user submitted a prompt, the session started, the session ended.

If you've used Claude Code's hooks or git hooks, it's the same idea. If you haven't: the agent loop is now scriptable from the outside, without writing any Rust or any MCP server.

<!-- truncate -->

## How it works

goose follows the [Open Plugins hooks specification](https://open-plugins.com/agent-builders/components/hooks). Any plugin directory under `~/.agents/plugins/<name>/` (user scope) or `<project>/.agents/plugins/<name>/` (project scope) that contains a `hooks/hooks.json` file is auto-discovered at startup.

A minimal hook config looks like this:

```json
{
  "hooks": {
    "PostToolUse": [
      {
        "matcher": "developer__shell|developer__text_editor",
        "hooks": [
          { "type": "command", "command": "${PLUGIN_ROOT}/scripts/log.sh" }
        ]
      }
    ]
  }
}
```

When the event fires, goose runs the command, sets `PLUGIN_ROOT` in the environment, and pipes a JSON payload to the script on stdin:

```json
{
  "event": "PostToolUse",
  "session_id": "abc-123",
  "tool_name": "developer__shell",
  "tool_input": { "command": "rg TODO" },
  "working_dir": "/Users/you/project"
}
```

The supported events are:

- `SessionStart`, `SessionEnd`, `Stop`
- `UserPromptSubmit`
- `PreToolUse`, `PostToolUse`, `PostToolUseFailure`
- `BeforeReadFile`, `AfterFileEdit`
- `BeforeShellExecution`, `AfterShellExecution`

The `matcher` field is a regex tested against the most relevant string for the event (tool name, file path, or shell command). Leave it off and the hook fires for every event of that type. Hooks that fail or time out are logged but won't crash the host tool, so your scripts can be as scrappy as you want.

## A few things to try

### 1. Have goose talk to you when it actually needs you

Pick a handful of events that mean "the human's attention would be useful right now" — a tool failed, the session wrapped, a long-running command finished — and have goose speak a line when one of them fires:

```json
{
  "hooks": {
    "PostToolUseFailure": [{ "hooks": [{ "type": "command", "command": "${PLUGIN_ROOT}/scripts/notify.sh" }] }],
    "SessionEnd":         [{ "hooks": [{ "type": "command", "command": "${PLUGIN_ROOT}/scripts/notify.sh" }] }],
    "AfterShellExecution": [
      {
        "matcher": "^(cargo (test|build|clippy)|pnpm (test|build)|just )",
        "hooks": [{ "type": "command", "command": "${PLUGIN_ROOT}/scripts/notify.sh" }]
      }
    ]
  }
}
```

Then `notify.sh` branches on the payload and picks a line:

```bash
#!/usr/bin/env bash
payload="$(cat)"
event="$(printf '%s' "$payload" | jq -r .event)"

case "$event" in
  PostToolUseFailure)  echo "That didn't work. Need a hand?" | say -v Daniel ;;
  SessionEnd)          echo "Done. Come check this out."     | say -v Daniel ;;
  AfterShellExecution) echo "Long command finished."         | say -v Daniel ;;
esac
```

Tune the `matcher` regex to whatever counts as "long-running" in your world — test suites, builds, deploys, `terraform apply`.

### 2. The "goose is doing something" desk light 🪿💡

If you have a smart bulb with an HTTP API (Hue, LIFX, Home Assistant, etc.), turn it on when goose starts a tool call and off when it finishes:

```json
{
  "hooks": {
    "PreToolUse":  [{ "hooks": [{ "type": "command", "command": "curl -s -X POST http://hue.local/light/on"  }] }],
    "PostToolUse": [{ "hooks": [{ "type": "command", "command": "curl -s -X POST http://hue.local/light/off" }] }]
  }
}
```

Now your desk lamp is a status indicator for the agent. Walk away, glance back, and if it's on, goose is still working.

### 3. Auto-format every file goose edits

Hook `AfterFileEdit` and run the formatter yourself so the agent doesn't have to remember:

```json
{
  "hooks": {
    "AfterFileEdit": [
      {
        "matcher": "\\.(ts|tsx|js|jsx|json|md)$",
        "hooks": [{ "type": "command", "command": "${PLUGIN_ROOT}/scripts/format.sh" }]
      },
      {
        "matcher": "\\.rs$",
        "hooks": [{ "type": "command", "command": "cargo fmt" }]
      }
    ]
  }
}
```

`scripts/format.sh` reads the file path from stdin and runs `prettier --write` against it.

### 4. Daily session journal

Hook `SessionEnd` and append a one-line summary to a markdown file:

```bash
#!/usr/bin/env bash
payload="$(cat)"
session_id="$(printf '%s' "$payload" | jq -r .session_id)"
date_str="$(date '+%Y-%m-%d %H:%M')"
echo "- $date_str — session $session_id ended" >> ~/notes/goose-journal.md
```

Capture `UserPromptSubmit` payloads too and you've got a log of every question you asked your agent today.

### 5. Make goose sound like a submarine

Because you can:

```json
{
  "hooks": {
    "PreToolUse": [
      {
        "hooks": [
          {
            "type": "command",
            "command": "afplay /System/Library/Sounds/Submarine.aiff"
          }
        ]
      }
    ],
    "SessionEnd": [
      {
        "hooks": [
          {
            "type": "command",
            "command": "say 'Captain, the session has ended.'"
          }
        ]
      }
    ]
  }
}
```

Useful when goose is working on a long task in another window. The audio cue tells you it's actually doing things instead of sitting there waiting on you.

## Try the example

There's a working example in the repo at [`examples/plugins/hello-hooks`](https://github.com/block/goose/tree/main/examples/plugins/hello-hooks) — a plugin that wires up `SessionStart`, `UserPromptSubmit`, `PreToolUse`, and `PostToolUse` and prints a friendly emoji to stderr for each one. Copy it to `~/.agents/plugins/`, start a session, and watch the events fly by:

```bash
mkdir -p ~/.agents/plugins
cp -R examples/plugins/hello-hooks ~/.agents/plugins/hello-hooks
chmod +x ~/.agents/plugins/hello-hooks/scripts/announce.sh

goose session
# 🚀 [hello-hooks] SessionStart
# 💬 [hello-hooks] UserPromptSubmit
# ⚡ [hello-hooks] PreToolUse tool=developer__shell
# ✅ [hello-hooks] PostToolUse tool=developer__shell
```

Every event also gets appended to `~/.agents/plugins/hello-hooks/last-event.log` so you can see the exact JSON your scripts receive. Fire some events, `tail` the log, build from there.

## Why this matters

MCP servers give goose new tools. Hooks go the other direction: they give you a way to react to what goose is doing, in real time, with whatever language you already know. Bash, Python, a Go binary, a one-line `curl`. It's all just a command on stdin.

The plugin model is small on purpose: a folder, a JSON file, a script. No registration step, no daemon, no rebuild. Drop it in, start goose, it works. Take it out and goose doesn't notice it's gone.

If you build something fun, share it. The `examples/plugins/` directory is a good home for community plugins, and the [Open Plugins spec](https://open-plugins.com) means anything you build here works with other agents that adopt it.

Happy hooking. 🪝
