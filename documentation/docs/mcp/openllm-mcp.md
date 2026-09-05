---
title: OpenLLM Extension
description: Add OpenLLM MCP tools as a goose Extension
---

import Tabs from '@theme/Tabs';
import TabItem from '@theme/TabItem';
import CLIExtensionInstructions from '@site/src/components/CLIExtensionInstructions';
import GooseDesktopInstaller from '@site/src/components/GooseDesktopInstaller';

The [OpenLLM](https://openllm.sh/) extension gives goose semantic search across code and documentation, plus persistent memory that can be recalled across sessions.

## Prerequisite

The launcher uses `npx` and Bun. If the OpenLLM CLI is not installed, the first run starts the official installer and guides you through setup.

## Configuration

:::tip Quick Install
<Tabs groupId="interface">
  <TabItem value="ui" label="goose Desktop" default>
  [Launch the installer](goose://extension?cmd=npx&arg=-y&arg=%40openllmsh%2Fnpm&arg=mcp&id=openllm&name=OpenLLM&description=Semantic%20code%20search%20and%20persistent%20memory)
  </TabItem>
  <TabItem value="cli" label="goose CLI">
  **Command**
  ```sh
  npx -y @openllmsh/npm mcp
  ```
  </TabItem>
</Tabs>
:::

<Tabs groupId="interface">
  <TabItem value="ui" label="goose Desktop" default>
  <GooseDesktopInstaller
    extensionId="openllm"
    extensionName="OpenLLM"
    description="Semantic code search and persistent memory"
    type="stdio"
    command="npx"
    args={["-y", "@openllmsh/npm", "mcp"]}
  />
  </TabItem>
  <TabItem value="cli" label="goose CLI">
  <CLIExtensionInstructions
    name="OpenLLM"
    description="Semantic code search and persistent memory"
    type="stdio"
    command="npx -y @openllmsh/npm mcp"
    timeout={300}
  />
  </TabItem>
</Tabs>

## Included Tools

| Tool group | What it provides |
|------------|------------------|
| Code and documentation search | Index and semantically search local codebases and documentation sites |
| Memory | Save useful context and recall it in later sessions |
