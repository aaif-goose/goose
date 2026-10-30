---
title: Build an app with goose
sidebar_label: Build an app with goose
description: Launch goose from your application, send prompts, and stream responses over ACP.
displayed_sidebar: gdkSidebar
---

# Build an app with goose

If your application needs an agent, you do not have to assemble one from GDK
components. You can launch the pre-built goose binary and talk to it over the
[Agent Client Protocol (ACP)](https://agentclientprotocol.com/).

Your application owns the interface and workflow. goose supplies the agent,
model connections, tools, and saved conversations. ACP connects them: your app
sends prompts and receives streamed responses, tool activity, and permission
requests. This is also how goose Desktop talks to its backend.

The example below uses Node.js 22 or later and the
[ACP TypeScript SDK](https://github.com/agentclientprotocol/typescript-sdk),
with JavaScript so it can run without a build step. Install the
[goose CLI](/docs/getting-started/installation) and configure a
[model provider](/docs/getting-started/providers) first. Use a working directory
and configured tools appropriate for the access you want the agent to have.

```bash
npm install @agentclientprotocol/sdk@1.5.0
```

Put the following three JavaScript blocks, in order, in a file named `app.mjs`.

## Connect over stdio

Launch `goose acp` as a subprocess. Its stdin and stdout carry ACP messages;
stderr is for logs. The SDK turns those pipes into a bidirectional connection.

```javascript
import { spawn } from "node:child_process";
import { Readable, Writable } from "node:stream";
import * as acp from "@agentclientprotocol/sdk";

const goose = spawn("goose", ["acp"], {
  stdio: ["pipe", "pipe", "inherit"],
});

const stream = acp.ndJsonStream(
  Writable.toWeb(goose.stdin),
  Readable.toWeb(goose.stdout),
);
```

Your application manages the process lifetime. Keep it running while the user
works with the agent, and close the connection and process when finished.

## Stream updates into your interface

Register handlers before sending a prompt. While a turn is running, goose sends
`session/update` notifications with the session ID and an update payload.
Append text chunks as they arrive, and use tool call IDs to update the matching
tool activity in your interface.

```javascript
const app = acp
  .client({ name: "my-app" })
  .onNotification(acp.methods.client.session.update, ({ params }) => {
    const update = params.update;
    if (
      update.sessionUpdate === "agent_message_chunk" &&
      update.content.type === "text"
    ) {
      process.stdout.write(update.content.text);
    } else if (update.sessionUpdate === "tool_call") {
      console.error(`Tool: ${update.title} (${update.toolCallId})`);
    } else if (update.sessionUpdate === "tool_call_update") {
      console.error(`Tool ${update.toolCallId}: ${update.status}`);
    }
  })
  .onRequest(acp.methods.client.session.requestPermission, () => ({
    outcome: { outcome: "cancelled" },
  }));
```

This example cancels permission requests rather than automatically approving
them. In your app, show the supplied choices to the user and return their
selection. Permission callbacks are not a sandbox: goose's configured mode
determines when approval is requested, and its tools run with the backend's
access to the machine.

## Create a session and send a prompt

Initialize the connection to negotiate the protocol version and capabilities.
Then create a session with an absolute working directory on the machine running
goose. The returned session ID identifies the conversation for subsequent
prompts and notifications.

```javascript
try {
  const result = await app.connectWith(stream, async (agent) => {
    const initialized = await agent.request(acp.methods.agent.initialize, {
      protocolVersion: acp.PROTOCOL_VERSION,
      clientCapabilities: {},
    });
    if (initialized.protocolVersion !== acp.PROTOCOL_VERSION) {
      throw new Error("Unsupported ACP protocol version");
    }

    const session = await agent.request(acp.methods.agent.session.new, {
      cwd: process.cwd(),
      mcpServers: [],
    });

    return agent.request(acp.methods.agent.session.prompt, {
      sessionId: session.sessionId,
      prompt: [{ type: "text", text: "Say hello in one sentence." }],
    });
  });

  console.log(`\nTurn finished: ${result.stopReason}`);
} finally {
  goose.kill();
}
```

Run it from the directory you want the session to work in:

```bash
node app.mjs
```

The prompt response reports why the turn stopped; the assistant's text arrives
through notifications while that request is pending. Do not wait for the prompt
response before displaying output. One turn may include several model calls
and tool calls before it completes.

`connectWith` closes the connection when its callback finishes. This example
ends after one prompt; an interactive app keeps that callback running, or uses
the SDK's `connect` API to manage a longer-lived connection.

## Keep the conversation going

Send follow-up prompts with the same session ID to retain context. Create a new
session for a separate conversation. Route updates by session ID if your
application supports more than one conversation.

To resume after reconnecting, save the session ID and working directory. Check
the `loadSession` capability returned by initialization, then use `session/load`.
History is replayed through the same notification handler. A stop button sends
`session/cancel` for the active turn rather than killing the backend.

## Use a server instead

If a subprocess is not the right fit, `goose serve` exposes ACP over HTTP and
WebSocket. Your app uses the same session and prompt concepts through a
different transport.

```bash
GOOSE_SERVER__SECRET_KEY='replace-with-a-long-random-secret' goose serve
```

By default, the endpoint is `http://127.0.0.1:3284/acp` (or `ws://` for
WebSocket). Authenticate with `X-Secret-Key`, or the `token` query parameter
when a browser WebSocket cannot set that header. Keep the backend secret out of
public frontend code. Browser origins may also need to be allowed explicitly.

See the [server command options](/docs/guides/goose-cli-commands#serve-options)
for authentication and origin flags, and the
[remote server guide](/docs/guides/remote-goose-server) for deployment and TLS.

## Examples and reference

- [Top Goose](/docs/gdk/demos/top-goose) maps GitHub issues and pull requests to
  persistent conversations with a `goose acp` subprocess.
- [Berd](/docs/gdk/demos/berd) builds a custom desktop interface around a bundled
  `goose serve` backend over WebSocket.
- The [ACP protocol reference](https://agentclientprotocol.com/protocol/v1/overview)
  defines initialization, sessions, prompts, updates, permissions, and cancellation.
- The [goose ACP reference](/docs/gdk/acp/reference) documents goose-specific
  methods and types.
