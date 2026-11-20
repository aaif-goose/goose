---
title: Top Goose
description: GitHub issues and pull requests with persistent private goose conversations over ACP.
displayed_sidebar: gdkSidebar
---

# Top Goose

Top Goose brings GitHub issues, pull requests, and private agent conversations
into one desktop application. Select an issue or PR to read the public
discussion alongside an agent working against your local clone.

Built with **goose over ACP**: an Electron application that runs `goose acp`
as a subprocess.

[Source](https://github.com/DOsinga/top-goose)

## The clever bit

The conversation belongs to the piece of work, not to whichever chat window
happens to be open. Each issue or pull request maps to a persistent goose
session. Switching views does not cancel another session's running turn, and
returning after an application restart loads the existing conversation.

The application owns the GitHub context and interface; goose owns the agent's
conversation history and tool execution. Separate git worktrees let sessions
work concurrently without sharing a checkout.

An application-provided MCP tool can place a draft reply in the composer for
review, giving the agent a way to participate in the interface rather than
only returning chat text.

## How it works

1. Read issues and PRs from GitHub and show their discussions in the app.
2. On the first prompt, create a goose session and, when enabled, its worktree.
3. Supply the public discussion as untrusted context for the agent.
4. Route streamed updates to the matching conversation. Load the stored
   session when the user returns.

The [session manager](https://github.com/DOsinga/top-goose/blob/main/src/main/goose/sessions.ts)
resumes a mapped session and applies the replay to the UI:

```typescript
const cwd = await sessionCwd(mapping);
const replay = await acp.loadSession(mapping.sessionId, cwd, []);
applyReplay(live, replay);
live.currentAgent = undefined;
await attachDraftServer(mapping.sessionId, issueNodeId, cwd);
await applyInstructions(mapping.sessionId, mapping.kind ?? "issue");
```

See the [ACP transport](https://github.com/DOsinga/top-goose/blob/main/src/main/goose/acp.ts),
[context synchronization](https://github.com/DOsinga/top-goose/blob/main/src/main/goose/contextSync.ts),
and [draft-reply MCP server](https://github.com/DOsinga/top-goose/blob/main/src/main/mcp/draftServer.ts)
for the other sides of that integration.

## Run it yourself

You need Node.js 22.12 or later, a local repository clone, a GitHub classic
personal access token, and goose 1.41 or later with a configured provider.
The project is developed and tested on macOS.

```bash
git clone https://github.com/DOsinga/top-goose.git
cd top-goose
npm install
npm run dev
```

First-run settings guide you through GitHub authentication, locating goose, and
choosing a local clone. Follow the
[setup instructions](https://github.com/DOsinga/top-goose#setup) for token scopes
and optional Projects integration.

:::warning Agent access
Top Goose runs sessions in automatic mode and approves tool requests. Worktrees
separate git state; they are not a security sandbox. Use a repository and
credentials appropriate for that level of access.
:::
