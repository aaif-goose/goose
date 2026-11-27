---
title: Weavetab Extension
description: Add Weavetab MCP Server as a goose Extension
---

import Tabs from '@theme/Tabs';
import TabItem from '@theme/TabItem';
import CLIExtensionInstructions from '@site/src/components/CLIExtensionInstructions';
import GooseDesktopInstaller from '@site/src/components/GooseDesktopInstaller';

This tutorial covers how to add the [Weavetab MCP Server](https://github.com/Weavetab/mcp) as a goose extension for local Chrome DevTools Protocol (CDP) automation across Chromium browsers and Electron desktop applications.

:::tip Quick Install
<Tabs groupId="interface">
  <TabItem value="ui" label="goose Desktop" default>
  [Launch the installer](goose://extension?cmd=npx&arg=-y&arg=@weavetab/mcp&id=weavetab&name=Weavetab&description=Local%20CDP%20browser%20and%20Electron%20desktop%20app%20automation)
  </TabItem>
  <TabItem value="cli" label="goose CLI">
  **Command**
  ```sh
  npx -y @weavetab/mcp
  ```
  </TabItem>
</Tabs>
:::

## Configuration

:::info
Note that you'll need [Node.js](https://nodejs.org/) (>= 20) installed on your system to run this command, as it uses `npx`.
:::

<Tabs groupId="interface">
  <TabItem value="ui" label="goose Desktop" default>
    <GooseDesktopInstaller
      extensionId="weavetab"
      extensionName="Weavetab"
      description="Local CDP browser and Electron desktop app automation"
      command="npx"
      args={["-y", "@weavetab/mcp"]}
    />
  </TabItem>
  <TabItem value="cli" label="goose CLI">
    <CLIExtensionInstructions
      name="Weavetab"
      description="Local CDP browser and Electron desktop app automation"
      command="npx -y @weavetab/mcp"
    />
  </TabItem>
</Tabs>

## Example Usage

Weavetab connects directly to local Chromium browsers and Electron apps running with a CDP remote debugging port (e.g., `--remote-debugging-port=9222`).

### goose Prompt

```
Use Weavetab to detect running local browsers or Electron apps, map the active window, and check browser_console for any runtime errors.
```

### goose Output

```
Let me check which local browser and Electron targets are available.

─── browser_detect | weavetab ──────────────────────────

Found a running Chromium/Electron target on port 9222. Mapping the active DOM tree:

─── browser_map | weavetab ──────────────────────────

Now checking the console buffer for errors:

─── browser_console | weavetab ──────────────────────────

No runtime errors found in the active window.
```
