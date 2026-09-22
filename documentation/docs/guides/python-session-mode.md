---
title: Python Session Mode
sidebar_position: 32
sidebar_label: Python Session Mode
description: Run the Developer extension as one persistent Python session where data stays in variables instead of the conversation
---

import Tabs from '@theme/Tabs';
import TabItem from '@theme/TabItem';

Python session mode is an alternative way to run the [Developer extension](/docs/mcp/developer-mcp).
Instead of separate shell, file, tree, and image tools, Developer exposes a single `python` tool backed by one Python process that lives for the whole conversation.
goose reads files, runs commands, and transforms data inside that process, keeps the results in variables, and prints only the slice it needs to reason about the next step.
The variables survive context compaction and goose restarts, so goose reuses what it already computed instead of re-reading and recomputing it.
The idea is inspired by the Recursive Language Models paper, which treats the context as data the model operates on programmatically rather than text it has to hold in its window.

:::info
The `python` tool runs arbitrary code on your machine, the same class of action as the Developer shell.
In Smart Approve and Manual modes goose asks before each cell.
:::

## When to use it

Python session mode pays off when a task touches more data than fits comfortably in the conversation: large logs, wide CSVs, big codebases, long test outputs, or anything goose would otherwise re-read after a compaction.
It is also a good fit for multi-step analysis where intermediate results are worth keeping around.
For short interactive edits and one-off shell commands, the default tools mode works just as well and gives you the familiar per-tool permission controls.

## Enabling Python session mode

The mode is controlled by the `GOOSE_DEVELOPER_MODE` setting, which accepts `tools` (the default) or `python_session`.
The setting is read when the Developer extension starts, so a change applies the next time Developer loads: start a new session, or toggle the Developer extension off and on in the current one.

<Tabs groupId="interface">
  <TabItem value="ui" label="goose Desktop" default>

  1. Click the gear icon in the sidebar to open Settings
  2. Select the `Chat` tab
  3. In the `Developer Tools` card, choose `Python session`

  </TabItem>
  <TabItem value="cli" label="goose CLI">

  1. Run the `configure` command:
  ```sh
  goose configure
  ```

  2. Choose `goose settings`, then `Developer Mode`
  ```sh
  ┌   goose-configure
  │
  ◇  What would you like to configure?
  │  goose settings
  │
  ◇  What setting would you like to configure?
  │  Developer Mode
  │
  ◆  How should the Developer extension run?
  │  ○ Tools
  // highlight-start
  │  ● Python Session
  // highlight-end
  └  Set Developer to Python Session - applies when the Developer extension next starts
  ```

  </TabItem>
  <TabItem value="env" label="Environment variable">

  The environment variable overrides the value in `config.yaml`, which makes it handy for a single run:

  ```sh
  GOOSE_DEVELOPER_MODE=python_session goose run -t "Find the ten most common errors in server.log"
  ```

  You can also set it in `config.yaml` directly:

  ```yaml
  # ~/.config/goose/config.yaml
  GOOSE_DEVELOPER_MODE: python_session
  ```

  </TabItem>
</Tabs>

## Requirements

Python 3.9 or newer must be available.
goose looks for `python3` and then `python` on the login-shell `PATH`, the same path resolution the Developer shell uses.
The session uses only the standard library, so no packages need to be installed.
To use a specific interpreter, set `GOOSE_PYTHON_SESSION_PYTHON` to its path.

## Settings

| Setting | Default | Purpose |
| --- | --- | --- |
| `GOOSE_DEVELOPER_MODE` | `tools` | `tools` or `python_session`; how the Developer extension runs |
| `GOOSE_PYTHON_SESSION_PYTHON` | `python3`, then `python`, on `PATH` | Interpreter to run the session with |
| `GOOSE_PYTHON_SESSION_CELL_TIMEOUT_SECS` | `120` | Seconds a cell may run before it is interrupted |
| `GOOSE_PYTHON_SESSION_MAX_OUTPUT_CHARS` | `16384` | Cap on each of stdout, stderr, and the echoed value per cell |

Settings can be set as environment variables or in `config.yaml`.

## The `python` tool

In Python session mode the Developer extension exposes one tool, `python`, which takes a single `code` argument and runs it as a cell in the session.

- Every cell runs in the same process, so variables, imports, functions, and the working directory carry over between calls.
- The last expression of a cell is echoed like a REPL and is also available as `_`.
- stdout, stderr, and the echoed value are each capped per cell, so goose assigns large data to variables and prints slices.
- Helpers are preloaded in the session: `sh(command)` runs a shell command and returns an object with `.code`, `.out`, and `.err`, `edit(path, old, new)` does a unique-match text replacement, and `view_image(path, crop=None)` attaches an image to the result so goose can see it.
- Cells that run past the timeout are interrupted with `KeyboardInterrupt` and the session keeps its variables.
  On Windows, interrupts are not available, so a timed-out cell restarts the session instead.

### How state persists

- **Across calls**: the session is one process per conversation, so nothing is recomputed between cells.
- **Across compaction**: after the conversation is compacted, goose sees a short `<python-session>` listing of the variables that still exist and reuses them instead of re-reading their inputs.
- **Across restarts**: variables that can be pickled are snapshotted after every cell into the `python-session/` directory under the goose data directory.
  Resuming the session in a new goose process restores them, and the first cell reports what came back and what could not be restored.
  Snapshots are removed when the session is deleted.
- Sessions idle for 30 minutes are shut down and restored from the snapshot on the next call.

## Limitations

- The `shell`, `write`, `edit`, `tree`, and `read_image` tools are not available in this mode.
  Use `sh()`, `edit()`, and `view_image()` inside the session instead.
  Per-tool permissions apply to the single `python` tool.
- Forking a session starts the fork with a fresh Python namespace.
  Variables from the original session are not carried into the fork, so a forked conversation re-runs the Python it needs.
- Under Flatpak sandboxing, the session runs the interpreter inside the sandbox rather than on the host.
  Use tools mode there until host execution is supported.
- ACP clients that normally take over Developer's file and terminal tools have nothing to take over in this mode, so the session runs on the goose side.

## Example Usage

In this example, goose analyzes a large log file without ever pasting it into the conversation.

### goose Prompt

```
Find the ten most common error messages in server.log and tell me how many requests each one affected.
```

### goose Output

```md
I'll load the log into a variable first and inspect a few real lines before counting anything.

─── python ───
lines = open('server.log').read().splitlines()
len(lines), lines[:3]
=> (1842213, ['2026-09-21T08:00:01Z INFO  req=a1f3 GET /health 200', ...])

─── python ───
import re, collections
errors = [l for l in lines if ' ERROR ' in l]
pattern = re.compile(r'req=(\w+) .*? ERROR (.*)$')
by_message = collections.defaultdict(set)
for line in errors:
    m = pattern.search(line)
    if m:
        by_message[m.group(2)].add(m.group(1))
top = sorted(((len(v), k) for k, v in by_message.items()), reverse=True)[:10]
top
=> [(4210, 'upstream timeout after 30s'), (1873, 'invalid session token'), ...]

The ten most common errors and the number of distinct requests each affected:

1. upstream timeout after 30s - 4,210 requests
2. invalid session token - 1,873 requests
...
```

The 1.8 million log lines stayed in the `lines` variable the whole time.
If the conversation compacts later, goose still has `lines`, `errors`, and `top` available without re-reading the file.
