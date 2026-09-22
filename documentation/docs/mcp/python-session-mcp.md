---
title: Python Session Extension
description: Context as variables - a persistent Python runtime where data stays in variables instead of the conversation
---

import Tabs from '@theme/Tabs';
import TabItem from '@theme/TabItem';
import { PlatformExtensionNote } from '@site/src/components/PlatformExtensionNote';
import GooseBuiltinInstaller from '@site/src/components/GooseBuiltinInstaller';

The Python Session extension gives goose one persistent Python process per conversation and a single `python` tool to drive it.
Instead of pulling file contents, command output, and intermediate results into the conversation, goose keeps them in Python variables and only prints the slice it needs to reason about the next step.
The variables survive context compaction and goose restarts, so goose reuses what it already computed instead of re-reading and recomputing it.
The idea is inspired by the Recursive Language Models paper, which treats the context as data the model operates on programmatically rather than text it has to hold in its window.

The Python Session extension is not enabled by default.
While it is enabled it replaces the Developer extension: shell commands run through `sh()` inside the session, files are edited with `edit()`, and images are viewed with `view_image()`.
Enabling Python Session turns Developer off in your extensions list, and disabling it turns Developer back on.

:::info
The `python` tool runs arbitrary code on your machine, the same class of action as the Developer shell.
In Smart Approve and Manual modes goose asks before each cell.
:::

This tutorial will cover enabling and using the Python Session extension.

## Requirements

Python 3.9 or newer must be on your `PATH` as `python3` or `python`.
The session uses only the standard library, so no packages need to be installed.
To use a specific interpreter, set `GOOSE_PYTHON_SESSION_PYTHON` to its path.

| Setting | Default | Purpose |
| --- | --- | --- |
| `GOOSE_PYTHON_SESSION_PYTHON` | `python3` on `PATH` | Interpreter to run the session with |
| `GOOSE_PYTHON_SESSION_CELL_TIMEOUT_SECS` | `120` | Seconds a cell may run before it is interrupted |
| `GOOSE_PYTHON_SESSION_MAX_OUTPUT_CHARS` | `16384` | Cap on each of stdout, stderr, and the echoed value per cell |

Settings can be set as environment variables or in `config.yaml`.

## Configuration

<PlatformExtensionNote defaultEnabled={false}/>

<Tabs groupId="interface">
  <TabItem value="ui" label="goose Desktop" default>
  <GooseBuiltinInstaller
    extensionName="Python Session"
    description="Context as variables: a persistent Python runtime where data stays in variables instead of the conversation"
  />
  </TabItem>
  <TabItem value="cli" label="goose CLI">

  1. Run the `configure` command:
  ```sh
  goose configure
  ```

  2. Choose to `Toggle Extensions`
  ```sh
  ┌   goose-configure 
  │
  ◇  What would you like to configure?
  │  Toggle Extensions 
  │
  ◆  Enable extensions: (use "space" to toggle and "enter" to submit)
  // highlight-start    
  │  ● python_session
  // highlight-end  
  └  Extension settings updated successfully
  ```
  </TabItem>
</Tabs>

## How goose works with it

- Every `python` call runs in the same process, so variables, imports, functions, and the working directory carry over between calls.
- The last expression of a cell is echoed like a REPL, and each output stream is capped, so goose assigns large data to variables and prints slices.
- After a compaction, goose sees a short listing of the variables that still exist in the session and reuses them.
- Variables that can be pickled are snapshotted after every cell, so a resumed session in a new goose process restores them and tells goose what came back.
- Cells that run past the timeout are interrupted with `KeyboardInterrupt` and the session keeps its variables.
  On Windows, interrupts are not available, so a timed-out cell restarts the session instead.
- Sessions idle for 30 minutes are shut down and restored from the snapshot on the next call.

## Limitations

- Forking a session starts the fork with a fresh Python namespace.
  Variables from the original session are not carried into the fork, so a forked conversation re-runs the Python it needs.
- Under Flatpak sandboxing, the session runs the interpreter inside the sandbox rather than on the host.
  Use the Developer extension there until host execution is supported.

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
