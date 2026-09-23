---
title: Atomic Mail Extension
description: Add Atomic Mail MCP Server as a goose Extension
---

import Tabs from '@theme/Tabs';
import TabItem from '@theme/TabItem';
import CLIExtensionInstructions from '@site/src/components/CLIExtensionInstructions';
import GooseDesktopInstaller from '@site/src/components/GooseDesktopInstaller';

This tutorial covers how to add the [Atomic Mail MCP Server](https://github.com/Atomic-Mail/atomic-mail-agentic) as a goose extension, so goose has an email inbox of its own — one it registers, reads and sends from, rather than a mailbox borrowed from you.

Most email extensions connect goose to an account you already own. Atomic Mail goes the other way: the `register` tool solves a proof-of-work challenge and provisions a fresh `@atomicmail.ai` address on the spot. No signup form, no domain, no card, no key to paste before the first run. From there goose reads, sends and searches over [JMAP](https://jmap.io/) (RFC 8620/8621).

:::tip Quick Install
<Tabs groupId="interface">
  <TabItem value="ui" label="goose Desktop" default>
  [Launch the installer](goose://extension?cmd=npx&arg=-y&arg=%40atomicmail%2Fmcp&id=atomicmail&name=Atomic%20Mail&description=Give%20goose%20its%20own%20email%20inbox)
  </TabItem>
  <TabItem value="cli" label="goose CLI">
  **Command**
  ```sh
  npx -y @atomicmail/mcp
  ```
  </TabItem>
</Tabs>
:::

## Configuration

:::info
You'll need [Node.js](https://nodejs.org/) installed on your system to run this command, as it uses `npx`.
:::

<Tabs groupId="interface">
  <TabItem value="ui" label="goose Desktop" default>
    <Tabs>
      <TabItem value="local" label="Local" default>
        <GooseDesktopInstaller
          extensionId="atomicmail"
          extensionName="Atomic Mail"
          description="Give goose its own email inbox"
          command="npx"
          args={["-y", "@atomicmail/mcp"]}
          customStep3="No environment variable is required. Ask goose to register an inbox on first use; to reuse an inbox you already have, set ATOMIC_MAIL_API_KEY instead."
        />
      </TabItem>
      <TabItem value="remote" label="Remote">
        <GooseDesktopInstaller
          extensionId="atomicmail-remote"
          extensionName="Atomic Mail"
          description="Give goose its own email inbox"
          type="http"
          url="https://mcp.atomicmail.ai/mcp"
          envVars={[
            { name: "Authorization", label: "Bearer YOUR_ATOMIC_MAIL_API_KEY" }
          ]}
          apiKeyLink="https://dashboard.atomicmail.ai"
          apiKeyLinkText="Atomic Mail dashboard"
          customStep3="Obtain an API key from the Connect dialog in the Atomic Mail dashboard and paste it as the Bearer token. OAuth is also supported."
        />
      </TabItem>
    </Tabs>
  </TabItem>
  <TabItem value="cli" label="goose CLI">
    <Tabs>
      <TabItem value="local" label="Local" default>
        <CLIExtensionInstructions
          name="Atomic Mail"
          description="Give goose its own email inbox"
          command="npx -y @atomicmail/mcp"
          infoNote={
            <>
              No environment variable is required. Ask goose to register an inbox on first use, or set <code>ATOMIC_MAIL_API_KEY</code> to reuse one you already have.
            </>
          }
        />
      </TabItem>
      <TabItem value="remote" label="Remote">
        <CLIExtensionInstructions
          name="Atomic Mail"
          description="Give goose its own email inbox"
          type="http"
          url="https://mcp.atomicmail.ai/mcp"
          envVars={[
            { key: "Authorization", value: "Bearer ▪▪▪▪▪▪▪▪▪▪▪▪▪▪▪▪▪▪▪▪▪▪▪▪" }
          ]}
          infoNote={
            <>
              Obtain an API key from the Connect dialog in the Atomic Mail dashboard and paste it as the <code>Bearer</code> token.
            </>
          }
        />
      </TabItem>
    </Tabs>
  </TabItem>
</Tabs>

## Available Tools

The extension exposes three tools:

| Tool | What it does |
| --- | --- |
| `register` | Provisions an inbox by proof of work and stores its credentials. Idempotent for the same username; pass a separate `credentials_dir` to hold more than one inbox. |
| `jmap_request` | Runs a JMAP method-call batch, authenticated automatically. Takes either inline `ops` or a named preset such as `list_inbox.json`, `send_mail.json` or `reply.json`, with placeholder substitution and local-file attachments. |
| `help` | Serves the bundled docs — JMAP cheatsheet, preset list, troubleshooting. Worth calling before the first `jmap_request` rather than guessing method shapes. |

## Multiple Inboxes

`credentials_dir` can be passed per call, so one goose session can hold several inboxes — for example one that files receipts and one that talks to customers — without them sharing credentials.

## Example Usage

Ask goose to set itself up with an inbox and then check it.

### goose Prompt

> _Register yourself an email inbox as `goose-demo`, then tell me the address and whether anything has arrived._

### goose Output

:::note CLI

<details>
    <summary>Tool Calls</summary>

    ─── help | atomicmail ──────────────────────────

    topic: overview



    ─── register | atomicmail ──────────────────────────

    username: goose-demo



    ─── jmap_request | atomicmail ──────────────────────────

    ops_file: list_inbox.json



</details>

I registered an inbox and it's live.

**Address:** `goose-demo@atomicmail.ai`

The mailbox is empty right now — nothing has arrived since it was created a moment ago. Send something to that address and ask me to check again, and I'll read it back to you.
:::

:::warning Scheduled inbox checks
`register` takes a `watch` value that decides who reads the inbox afterwards. If you choose scheduled checks, run them on goose's own scheduler rather than at the OS level, and give that session only the tools it needs — an unattended agent reading mail from strangers should not also hold broad shell access. Call `help` with the `cron` topic for the details.
:::
