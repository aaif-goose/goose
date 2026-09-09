---
title: Linux MCP Server Extension
description: Add Linux MCP Server as a goose Extension
---

import Tabs from '@theme/Tabs';
import TabItem from '@theme/TabItem';
import YouTubeShortEmbed from '@site/src/components/YouTubeShortEmbed';
import CLIExtensionInstructions from '@site/src/components/CLIExtensionInstructions';
import GooseDesktopInstaller from '@site/src/components/GooseDesktopInstaller';
import GooseBuiltinInstaller from '@site/src/components/GooseBuiltinInstaller';

This tutorial covers how to add the [Linux MCP Server](https://github.com/rhel-lightspeed/linux-mcp-server) as a goose extension to enable AI assistants to run, discover, and troubleshoot complex issues on Linux systems.

:::tip TLDR
<Tabs groupId="interface">
  <TabItem value="ui" label="goose Desktop" default>
  [Install Linux MCP Server](https://rhel-lightspeed.github.io/linux-mcp-server/install/)
  </TabItem>
  <TabItem value="cli" label="goose CLI">
  **Command**
  ```sh
  # Using uv (recommended)
  uvx linux-mcp-server
  ```
  </TabItem>
</Tabs>
:::

## Configuration

:::info
Note that you'll need [uv](https://docs.astral.sh/uv/#installation) installed on your system to run this command, as it uses *uvx*. 
:::


<Tabs groupId="interface">
  <TabItem value="ui" label="goose Desktop" default>

    <GooseDesktopInstaller
      extensionId="linux-mcp-server"
      extensionName="Linux MCP server"
      description="Tools for Linux system discovery and troubleshooting"
      type="stdio"
      command="uvx"
      args={["linux-mcp-server"]}
    />

 
  </TabItem>
  <TabItem value="cli" label="goose CLI">

    <CLIExtensionInstructions
      name="Linux MCP Server"
      description="Tools for Linux system discovery and troubleshooting"
      type="stdio"
      command="uvx linux-mcp-server"
      timeout={300}
    />
    
  </TabItem>
</Tabs>

## Example Usage

Follow the instructions on how to use the Linux MCP Server for system diagnostics and troubleshooting.

### goose Prompt

> _My wifi connection is not working very well. Find the error messages in the system logs and diagnose the problem to help me fix it._


### goose Output

:::note Desktop

🤖 LLM output 🤖
I’ll help you diagnose your WiFi connectivity issues by examining the system logs for error messages related to your network interfaces and wireless connectivity. Let me start by gathering information about your system and checking the relevant logs.

:::
