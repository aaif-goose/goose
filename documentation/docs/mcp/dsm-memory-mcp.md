---
title: DSM Memory Extension
description: Add DSM Memory MCP Server as a goose Extension
---

import Tabs from '@theme/Tabs';
import TabItem from '@theme/TabItem';
import CLIExtensionInstructions from '@site/src/components/CLIExtensionInstructions';
import GooseDesktopInstaller from '@site/src/components/GooseDesktopInstaller';

This tutorial covers how to add the [DSM Memory MCP Server](https://github.com/daryl-labs-ai/daryl/tree/main/src/dsm/integrations/goose) as a goose extension to give your agent an append-only, SHA-256 hash-chained memory layer. Every action goose logs is cryptographically chained — nothing can be silently altered, and the full session history is replayable and verifiable.

:::tip Quick Install
<Tabs groupId="interface">
  <TabItem value="ui" label="goose Desktop" default>
  [Launch the installer](goose://extension?cmd=uvx&arg=dsm-mcp&id=dsm-memory&name=DSM%20Memory&description=Append-only%2C%20hash-chained%20memory%20for%20goose)
  </TabItem>
  <TabItem value="cli" label="goose CLI">
  **Command**
  ```sh
  uvx dsm-mcp
  ```
  </TabItem>
</Tabs>
:::

## Configuration

:::info
Note that you'll need [uv](https://docs.astral.sh/uv/#installation) installed on your system to run this command, as it uses `uvx`.
:::

<Tabs groupId="interface">
  <TabItem value="ui" label="goose Desktop" default>
    <GooseDesktopInstaller
      extensionId="dsm-memory"
      extensionName="DSM Memory"
      description="Append-only, hash-chained memory layer for goose"
      type="stdio"
      command="uvx"
      args={["dsm-mcp"]}
    />
  </TabItem>
  <TabItem value="cli" label="goose CLI">
    <CLIExtensionInstructions
      name="DSM Memory"
      description="Append-only, hash-chained memory layer for goose"
      type="stdio"
      command="uvx dsm-mcp"
      timeout={300}
    />
  </TabItem>
</Tabs>

## Tool Reference

| Tool | What it does |
|---|---|
| `dsm_start_session` | Start a new provable session |
| `dsm_end_session` | End session, trigger digest rolling |
| `dsm_log_action` | Log an action intent (creates a hash chain entry) |
| `dsm_confirm_action` | Confirm an action with its result |
| `dsm_snapshot` | Record a state snapshot |
| `dsm_recall` | Budget-aware context recall with temporal digests |
| `dsm_recent` | Read the most recent entries |
| `dsm_summary` | Lightweight activity summary |
| `dsm_search` | Query actions across sessions |
| `dsm_verify` | Verify hash chain integrity (tamper detection) |
| `dsm_status` | Current system status |

## Example Usage

Use DSM Memory to give goose a persistent, verifiable record of what it has done across sessions.

### goose Prompt

> Start a DSM session called "research", log that you searched for "goose extensions", then verify the chain.

### goose Output

:::note Desktop

```
dsm_start_session("research")
→ {status: "started", session_id: "research-20260409"}

dsm_log_action("search", {"query": "goose extensions"})
→ {intent_id: "a3f9...", hash: "sha256:e8b1..."}

dsm_verify()
→ {status: "OK", total_entries: 1, tampered: 0, chain_breaks: 0}
```

Every entry is hash-chained to the previous one. `dsm_verify` confirms nothing has been altered.

:::
