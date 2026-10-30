---
title: goose Perception
description: A background application that turns desktop activity into a personal wiki using goose over ACP.
displayed_sidebar: gdkSidebar
---

# goose Perception

Perception periodically captures the desktop and turns what it observes into a
linked personal wiki about projects, people, and daily activity. A local web
interface lets you browse, search, and edit the result.

Built with **goose over ACP**, using the TypeScript `@aaif/goose-sdk` client.
The application does not embed the Rust GDK runtime.

[Source](https://github.com/DOsinga/goose_perception)

## The clever bit

An agent does not need to wait for a chat message. Here, desktop observations
drive the work, and the wiki provides continuity between runs.

Perception uses fresh agent sessions rather than accumulating screenshots in an
ever-growing conversation. A fast model can describe a screenshot, and a
separate model can reconcile batches of observations into the wiki. The lasting
memory is in files the user can inspect and edit, not in a single chat history.

This also separates the application's scheduling and capture logic from the
agent's tools, provider configuration, and execution.

## How it works

1. Capture screenshots and extract descriptions of relevant changes.
2. Batch those observations for the wiki-maintenance agent.
3. Start a session in the wiki directory, supplying the current wiki context.
4. Update pages and cross-references. During idle periods, review existing
   pages using a separate lint prompt.

The [agent client](https://github.com/DOsinga/goose_perception/blob/main/src/agent.ts)
creates sessions with the wiki as their working directory:

```typescript
const session = await client.newSession({
  cwd: config.wikiDir,
  mcpServers: [],
  ...(provider ? { _meta: { provider } } : {}),
});
```

The same client sends image content to the extraction session and text
observations to the wiki session. The
[main loop](https://github.com/DOsinga/goose_perception/blob/main/src/main.ts)
handles batching, change detection, and scheduling; the
[prompts](https://github.com/DOsinga/goose_perception/blob/main/src/prompt.ts)
define what the agent should preserve.

## Run it yourself

You need Node.js 20 or later, macOS or Linux with a supported screenshot tool,
and goose configured with a vision-capable model. The package can install goose
if it is not already available.

```bash
npx goose_perception
```

This starts desktop capture and opens the wiki browser at
`http://localhost:8112`. Stop the process to stop capture. See the
[README](https://github.com/DOsinga/goose_perception#options) for capture intervals,
storage location, and connection options.

:::warning Desktop capture
Screenshots can contain private information and are sent to your configured
model provider. Start with a non-sensitive desktop, check your provider settings,
and review the files stored in the Perception directory.
:::
