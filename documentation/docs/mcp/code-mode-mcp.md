---
title: Code Mode Extension
description: Execute JavaScript code to interact with multiple MCP tools
---

import Tabs from '@theme/Tabs';
import TabItem from '@theme/TabItem';
import { PlatformExtensionNote } from '@site/src/components/PlatformExtensionNote';
import GooseBuiltinInstaller from '@site/src/components/GooseBuiltinInstaller';

The Code Mode extension enables [Code Mode](/docs/guides/managing-tools/code-mode), a programmatic approach for interacting with MCP tools.

The Code Mode extension is an MCP server that exposes three meta-tools. When Code Mode is enabled, the LLM writes JavaScript code that goose executes in a fresh goose-owned [Boa](https://boajs.dev/) JavaScript context to discover tools, learn their interfaces, and call them programmatically.

This helps manage context window usage more efficiently when multiple extensions are enabled and when performing workflows with multiple tool calls.

## Configuration

<PlatformExtensionNote defaultEnabled={false} />

<Tabs groupId="interface">
  <TabItem value="ui" label="goose Desktop" default>
  <GooseBuiltinInstaller
    extensionName="Code Mode"
    description="Execute JavaScript code to interact with MCP tools efficiently"
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
  │  ● code_execution
  // highlight-end  
  └  Extension settings updated successfully
  ```
  </TabItem>
</Tabs>

## Example Usage

In this example, we'll ask goose to compile a report that requires multiple tool calls.

### goose Prompt

```
Create a LOG.md file with the current git branch, last 3 commits, and the version from package.json
```

### goose Output

:::note Desktop
I'll help you create a LOG.md file with the git branch, last 3 commits, and version from package.json. Let me gather all this information in one operation.

`execute_javascript: async function run() { const result = await tools.call("shell", { command: "git branch --show-current" }); return result; }`

Let me check the package.json path first:

`execute_javascript: async function run() { const result = await tools.call("shell", { command: "git branch --show-current" }); return result; }`

Perfect! I've created the **LOG.md** file with:

- ✅ **Current git branch**
- ✅ **Last 3 commits** (with hash, message, author, and relative time)
- ✅ **Version** from ui/desktop/package.json

The file has been saved to the root directory as `LOG.md`.
:::
