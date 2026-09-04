---
title: Compartment Extension
description: Add Compartment MCP Server as a goose Extension
---

import Tabs from '@theme/Tabs';
import TabItem from '@theme/TabItem';
import CLIExtensionInstructions from '@site/src/components/CLIExtensionInstructions';
import GooseDesktopInstaller from '@site/src/components/GooseDesktopInstaller';

This tutorial covers how to add the [Compartment MCP Server](https://github.com/MaxFreedomPollard/Compartment) as a goose extension. Compartment gives goose a long-term memory that is encrypted at rest and works fully offline. One vault file on your machine is shared by every MCP client you use, so what goose learns is available to Claude Code, Cursor or Hermes Agent and back. Records and vectors are encrypted with XChaCha20-Poly1305 under an Argon2id-derived key, embeddings run locally on a model that ships in the package, search is hybrid vector plus full-text, and memories carry importance tiers, expiry dates and supersession so stale facts retire themselves. No API key, no account, no network.

:::tip Quick Install
<Tabs groupId="interface">
  <TabItem value="ui" label="goose Desktop" default>
  [Launch the installer](goose://extension?cmd=uvx&arg=compartment&arg=--caller&arg=goose&arg=serve&id=compartment&name=Compartment&description=Encrypted%2C%20fully%20offline%20long-term%20memory%20for%20AI%20agents)
  </TabItem>
  <TabItem value="cli" label="goose CLI">
  **Command**
  ```sh
  uvx compartment --caller goose serve
  ```
  </TabItem>
</Tabs>
:::

## Configuration

:::info
Note that you'll need [uv](https://docs.astral.sh/uv/#installation) installed on your system to run this command, as it uses `uvx`. Compartment needs Python 3.11 or newer.
:::

:::note Create the vault once
Before the extension can start, create the encrypted vault and choose its passphrase:
```sh
uvx compartment init
```
The vault stays unlocked until the machine restarts or you run `uvx compartment lock`. To unlock it again later, run `uvx compartment unlock`. Compartment does not accept the passphrase through goose by default, so it never enters the model's context. The `--caller goose` argument only labels goose in the vault's audit log.
:::

<Tabs groupId="interface">
  <TabItem value="ui" label="goose Desktop" default>
    <GooseDesktopInstaller
      extensionId="compartment"
      extensionName="Compartment"
      description="Encrypted, fully offline long-term memory for AI agents"
      type="stdio"
      command="uvx"
      args={["compartment", "--caller", "goose", "serve"]}
    />
  </TabItem>
  <TabItem value="cli" label="goose CLI">
    <CLIExtensionInstructions
      name="compartment"
      description="Encrypted, fully offline long-term memory for AI agents"
      type="stdio"
      command="uvx compartment --caller goose serve"
      timeout={300}
    />
  </TabItem>
</Tabs>

## Example Usage

Compartment adds 14 tools: `memory_store`, `memory_store_many`, `memory_search`, `memory_get`, `memory_recent`, `memory_forget`, `memory_link`, `memory_unlink`, `memory_relations`, `memory_list_namespaces`, `memory_status`, `memory_selftest`, `memory_lock` and `memory_unlock`. Each carries a title and a read-only or destructive annotation, so goose knows which ones change the vault.

In this example, goose stores a fact about a project in one session and recalls it in a later one. The tool outputs below come from a real run against the server; the record ids and timestamps will differ on your machine.

### Session 1: remember something

#### goose Prompt

> _Remember that the expense tracker runs on Vite 6 with React, and that `npm run dev` serves it on port 5173._

#### goose Output

```
─── memory_store | compartment ──────────────────────────
text: The expense tracker runs on Vite 6 with React; `npm run dev` serves it on port 5173.
source: from chat
tags: [project, expense-tracker]
importance: 0.75

{"id": "cd8cfa463a714f00aa9bff122c6556ef", "duplicate": false, "namespace": "main", "kind": "fact", "created_local": "2026-09-02 23:34", "source": "from chat", "discovered": "2026-09-02"}

Saved. I'll recall this whenever you ask about running the expense tracker.
```

### Session 2: recall it

Start a new goose session, even days later.

#### goose Prompt

> _How do I start the expense tracker dev server?_

#### goose Output

```
─── memory_search | compartment ──────────────────────────
query: how do I start the expense tracker dev server
limit: 3

{"results": [{"id": "cd8cfa463a714f00aa9bff122c6556ef", "text": "The expense tracker runs on Vite 6 with React; `npm run dev` serves it on port 5173. [from chat, 2026-09-02]", "score": 2.26007, "cosine": 0.8015, "tags": ["project", "expense-tracker"], "importance": 0.75, "kind": "fact", "provenance": {"agent": "goose"}}, ...]}

Run `npm run dev` in the expense tracker project; it is a Vite 6 + React app and serves on http://localhost:5173.
```

Everything goose stored is encrypted in the vault on your disk. Delete a single memory with `memory_forget`, which destroys that record's key, or set an expiry when storing a fact that stops being true on a known day.
