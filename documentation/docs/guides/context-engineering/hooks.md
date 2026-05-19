---
title: Hooks
sidebar_position: 5
sidebar_label: Hooks
---

# Hooks

Hooks let you run your own scripts when key events happen during a goose session. Use hooks to log activity, send notifications, format files after edits, run checks after shell commands, or integrate goose with local workflows without writing a custom extension.

goose follows the [Open Plugins hooks specification](https://open-plugins.com/agent-builders/components/hooks). Hooks are discovered from [plugins](/docs/guides/context-engineering/plugins) on disk and run as shell commands when matching lifecycle events fire.

:::warning Run trusted hooks only
Hooks execute local commands on your machine. Only install or create hooks from sources you trust, and review hook scripts before enabling them.
:::

## Where Hooks Live

A hook belongs to a [plugin](/docs/guides/context-engineering/plugins) directory. goose discovers plugins from these locations:

| Scope | Location |
|---|---|
| User | `~/.agents/plugins/<plugin-name>/` |
| Project | `<project>/.agents/plugins/<plugin-name>/` |
| Installed plugin | goose's plugin install directory |

Each plugin that defines hooks must include a `hooks/hooks.json` file:

```text
my-plugin/
├── plugin.json
├── hooks/
│   └── hooks.json
└── scripts/
    └── notify.sh
```

Project plugins are loaded when goose is started from that project. User plugins are available across projects.

## Create a Hook

To create any hook, choose the event you want to react to, create a plugin directory, add a `hooks/hooks.json` file that maps that event to a command, then write the script or command that should run. The command receives the event payload as JSON on stdin, so it can inspect details like the session ID, prompt text, tool name, file path, or shell command.

A hook plugin needs this basic structure:

```text
session-logger/
├── plugin.json
├── hooks/
│   └── hooks.json
└── scripts/
    └── log-session.sh
```

The plugin manifest identifies the plugin:

```json title="plugin.json"
{
  "name": "session-logger",
  "version": "0.1.0",
  "description": "Log goose session events"
}
```

The hook configuration maps an event to a command. This example runs a script when the `SessionEnd` event fires:

```json title="hooks/hooks.json"
{
  "hooks": {
    "SessionEnd": [
      {
        "hooks": [
          {
            "type": "command",
            "command": "${PLUGIN_ROOT}/scripts/log-session.sh"
          }
        ]
      }
    ]
  }
}
```

The script reads the event payload from stdin and performs the automation:

```bash title="scripts/log-session.sh"
#!/usr/bin/env bash
payload="$(cat)"
session_id="$(printf '%s' "$payload" | jq -r .session_id)"
date_str="$(date '+%Y-%m-%d %H:%M')"

echo "- $date_str — session $session_id ended" >> ~/goose-session-log.md
```

Place the plugin under a discovered plugin location, such as `~/.agents/plugins/session-logger/`, and make command scripts executable when your operating system requires it.

## Hook Configuration

`hooks.json` has a top-level `hooks` object. Each key is an event name, and each event contains one or more rules:

```json
{
  "hooks": {
    "PostToolUse": [
      {
        "matcher": "developer__shell|developer__text_editor",
        "hooks": [
          {
            "type": "command",
            "command": "${PLUGIN_ROOT}/scripts/log-tool.sh",
            "timeout": 10
          }
        ]
      }
    ]
  }
}
```

| Field | Required | Description |
|---|---:|---|
| `matcher` | No | Regular expression used to decide whether the rule runs for the event. If omitted, the rule runs for every event of that type. |
| `hooks` | Yes | Actions to run when the event and matcher apply. |
| `type` | No | Action type. goose currently supports `command`. If omitted, `command` is used. |
| `command` | Yes for command hooks | Shell command to run. goose runs it with `sh -c`. |
| `timeout` | No | Timeout in seconds for the command. Defaults to 30 seconds. |

Use `${PLUGIN_ROOT}` in a command to reference the plugin directory. goose also sets `PLUGIN_ROOT` in the hook command's environment.

## Supported Events

| Event | When it runs | Matcher target |
|---|---|---|
| `SessionStart` | A session starts | None |
| `SessionEnd` | A session ends | None |
| `Stop` | goose receives a stop event | None |
| `UserPromptSubmit` | The user submits a prompt | Prompt text |
| `PreToolUse` | Before goose runs a tool | Tool name |
| `PostToolUse` | After a tool succeeds | Tool name |
| `PostToolUseFailure` | After a tool fails | Tool name |
| `BeforeReadFile` | Before goose reads a file | File path |
| `AfterFileEdit` | After goose successfully edits a file | File path |
| `BeforeShellExecution` | Before goose runs a shell command | Shell command |
| `AfterShellExecution` | After goose successfully runs a shell command | Shell command |

The matcher is a regular expression matched against the most relevant string for the event. For example, use `"\\.rs$"` to match Rust files on `AfterFileEdit`, or `"^(cargo test|pnpm test)"` to match test commands on `AfterShellExecution`.

:::note
`AfterFileEdit` and `AfterShellExecution` only run after successful tool calls. To react to failed edits, failed shell commands, or other failed tool calls, use `PostToolUseFailure`.
:::

## Hook Payload

When a hook runs, goose writes a JSON payload to the command's stdin. The payload always includes the event name and session ID, and may include fields such as the tool name, tool input, user message, or working directory.

Example payload for a tool event:

```json
{
  "event": "PostToolUse",
  "session_id": "abc-123",
  "matcher_context": "developer__shell",
  "tool_name": "developer__shell",
  "tool_input": { "command": "rg TODO" },
  "working_dir": "/Users/you/project"
}
```

Example script that reads the payload:

```bash
#!/usr/bin/env bash
set -euo pipefail

payload="$(cat)"
event="$(printf '%s' "$payload" | jq -r .event)"
tool="$(printf '%s' "$payload" | jq -r '.tool_name // "none"')"

echo "goose hook: event=$event tool=$tool" >> "${PLUGIN_ROOT}/hook.log"
```

## Examples

### Notify When a Tool Fails

```json
{
  "hooks": {
    "PostToolUseFailure": [
      {
        "hooks": [
          {
            "type": "command",
            "command": "${PLUGIN_ROOT}/scripts/notify.sh"
          }
        ]
      }
    ]
  }
}
```

```bash title="scripts/notify.sh"
#!/usr/bin/env bash
payload="$(cat)"
tool="$(printf '%s' "$payload" | jq -r '.tool_name // "tool"')"

osascript -e "display notification \"$tool failed\" with title \"goose\""
```

### Format Files After goose Edits Them

```json
{
  "hooks": {
    "AfterFileEdit": [
      {
        "matcher": "\\.(ts|tsx|js|jsx|json|md)$",
        "hooks": [
          {
            "type": "command",
            "command": "${PLUGIN_ROOT}/scripts/prettier.sh"
          }
        ]
      },
      {
        "matcher": "\\.rs$",
        "hooks": [
          {
            "type": "command",
            "command": "cargo fmt"
          }
        ]
      }
    ]
  }
}
```

```bash title="scripts/prettier.sh"
#!/usr/bin/env bash
set -euo pipefail

payload="$(cat)"
file="$(printf '%s' "$payload" | jq -r '.matcher_context // empty')"

if [ -n "$file" ]; then
  npx prettier --write "$file"
fi
```

### React to Long-Running Commands

```json
{
  "hooks": {
    "AfterShellExecution": [
      {
        "matcher": "^(cargo (test|build|clippy)|pnpm (test|build)|just )",
        "hooks": [
          {
            "type": "command",
            "command": "say 'goose finished running your command'"
          }
        ]
      }
    ]
  }
}
```

## Try the Example Plugin

goose includes an example plugin at `examples/plugins/hello-hooks`.

```bash
mkdir -p ~/.agents/plugins
cp -R examples/plugins/hello-hooks ~/.agents/plugins/hello-hooks
chmod +x ~/.agents/plugins/hello-hooks/scripts/announce.sh

goose session
```

The example prints hook events to stderr and appends full payloads to:

```text
~/.agents/plugins/hello-hooks/last-event.log
```

## Disable a Hook Plugin

To disable a plugin, add its name to `disabledPlugins` in your goose settings file:

```json title="~/.config/goose/settings.json"
{
  "disabledPlugins": ["session-logger"]
}
```

For project-specific settings, use:

```text
<project>/.config/goose/settings.json
```

A plugin listed in `disabledPlugins` is skipped during plugin discovery, so its hooks will not run.

## Troubleshooting

### My Hook Did Not Run

Check the following:

- The plugin directory is under `~/.agents/plugins/<name>/` or `<project>/.agents/plugins/<name>/`.
- The hook config is at `hooks/hooks.json` inside the plugin directory.
- The event name matches one of the [supported events](#supported-events).
- The `matcher` regular expression matches the event's matcher target.
- The command path is correct. Use `${PLUGIN_ROOT}` for scripts inside the plugin.
- The script is executable if you call it directly.
- The plugin is not listed in `disabledPlugins`.

### My Hook Timed Out or Failed

Hook failures are logged but do not crash goose or the tool that triggered the hook. If a hook fails or exceeds its timeout, goose logs the failure and continues.

Set a larger timeout for long-running hooks:

```json
{
  "hooks": {
    "SessionEnd": [
      {
        "hooks": [
          {
            "type": "command",
            "command": "${PLUGIN_ROOT}/scripts/archive.sh",
            "timeout": 120
          }
        ]
      }
    ]
  }
}
```

### My Script Cannot Find `jq` or Another Command

Hooks run as local shell commands. Make sure any commands your script uses are installed and available on your shell `PATH`. For portability, prefer absolute paths for tools that may not be installed everywhere.

## Additional Resources

import ContentCardCarousel from '@site/src/components/ContentCardCarousel';
import hooksBanner from '@site/static/img/blog/goose-hooks.jpg';

<ContentCardCarousel
  items={[
    {
      type: 'blog',
      title: 'Hooks: run your own scripts on every goose event',
      description: 'Learn how lifecycle hooks let you react to session, prompt, tool, file, and shell events with your own scripts.',
      thumbnailUrl: hooksBanner,
      linkUrl: '/blog/2026/05/14/goose-hooks',
      date: '2026-05-14',
      duration: '5 min read'
    }
  ]}
/>
