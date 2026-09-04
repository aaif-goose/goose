---
title: QVeris Extension
description: Add QVeris MCP Server as a goose Extension
---

import Tabs from '@theme/Tabs';
import TabItem from '@theme/TabItem';
import GooseDesktopInstaller from '@site/src/components/GooseDesktopInstaller';
import CLIExtensionInstructions from '@site/src/components/CLIExtensionInstructions';

This tutorial covers how to add the [QVeris MCP Server](https://github.com/QVerisAI/qveris-agent-toolkit) as a goose extension. QVeris lets goose discover, inspect, and call real-world API capabilities through one MCP connection.

:::tip Quick Install
<Tabs groupId="interface">
  <TabItem value="ui" label="goose Desktop" default>
  [Launch the installer](goose://extension?type=streamable_http&url=https%3A%2F%2Fmcp.qveris.ai%2Fmcp&id=qveris&name=QVeris&description=Discover%2C%20inspect%2C%20and%20call%2010%2C000%2B%20real-world%20API%20capabilities&header=Authorization%3DBearer%20YOUR_QVERIS_API_KEY)
  </TabItem>
  <TabItem value="cli" label="goose CLI">
  Add a `Remote Extension (Streamable HTTP)` extension with:

  **Endpoint URL**
  ```text
  https://mcp.qveris.ai/mcp
  ```
  </TabItem>
</Tabs>

  **Custom Request Header**
  ```text
  Authorization: Bearer <YOUR_QVERIS_API_KEY>
  ```
:::

## Configuration

Create a QVeris API key on the [API Keys page](https://qveris.ai/account?page=api-keys). Keep the key private and enter it only in goose's extension configuration.

<Tabs groupId="interface">
  <TabItem value="ui" label="goose Desktop" default>
    <GooseDesktopInstaller
      extensionId="qveris"
      extensionName="QVeris"
      description="Discover, inspect, and call real-world API capabilities through one MCP server"
      type="http"
      url="https://mcp.qveris.ai/mcp"
      envVars={[
        { name: "Authorization", label: "Bearer YOUR_QVERIS_API_KEY" }
      ]}
      apiKeyLink="https://qveris.ai/account?page=api-keys"
      apiKeyLinkText="QVeris API key"
    />
  </TabItem>
  <TabItem value="cli" label="goose CLI">
    <CLIExtensionInstructions
      name="QVeris"
      description="Discover, inspect, and call real-world API capabilities through one MCP server"
      type="http"
      url="https://mcp.qveris.ai/mcp"
      timeout={300}
      envVars={[
        { key: "Authorization", value: "Bearer qveris_your_api_key" }
      ]}
      infoNote={
        <>
          Create a <a href="https://qveris.ai/account?page=api-keys" target="_blank" rel="noopener noreferrer">QVeris API key</a> and paste it after <code>Bearer</code>.
        </>
      }
    />
  </TabItem>
</Tabs>

## Example Usage

Ask goose for the outcome you need. QVeris first discovers matching capabilities, then lets goose inspect a candidate before making a call.

### goose Prompt

> Find a weather capability and get the current weather in Shanghai. Inspect the selected capability before calling it.

### goose Output

:::note Example

goose searches QVeris for a suitable weather capability, inspects its parameters and service metadata, calls it with Shanghai as the location, and summarizes the returned conditions.

:::

Capability discovery is free. Calling a capability can consume QVeris credits; use the extension's usage and credits tools when you need to audit a call.
