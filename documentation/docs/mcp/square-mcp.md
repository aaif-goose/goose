---
title: Square MCP Extension
description: Add the Square API as a goose Extension
---

import Tabs from '@theme/Tabs';
import TabItem from '@theme/TabItem';
import YouTubeShortEmbed from '@site/src/components/YouTubeShortEmbed';
import GooseDesktopInstaller from '@site/src/components/GooseDesktopInstaller';
import CLIExtensionInstructions from '@site/src/components/CLIExtensionInstructions';

<details>
  <summary> 🎥 Square MCP Server Video Walkthrough</summary>
  <iframe
  class="aspect-ratio"
  src="https://www.youtube.com/embed/y6pklrzhzNg"
  title="Run your Business with AI | Square MCP Server"
  frameBorder="0"
  allow="accelerometer; autoplay; clipboard-write; encrypted-media; gyroscope; picture-in-picture"
  allowFullScreen
  ></iframe>
</details>


This tutorial will get you started with the [Square MCP server](https://developer.squareup.com/docs/mcp) to enable interactive and automated work for your Square seller account. The Square MCP server is an open source project that allows you to interact with the Square API using goose.

Square offers two versions of the MCP server:

1. **Remote MCP server** hosted by Square, which uses OAuth for authentication and allows fine-grained permissions on API usage.
2. **Local MCP server** that you can run on your own machine, which uses an access token for authentication and allows full API access.

:::info
Note that you'll need [Node.js](https://nodejs.org/) installed on your system to run installation commands, which use `npx`.
:::

## Configuration

<Tabs groupId="remote-or-local">
  <TabItem value="remote" label="Square Remote MCP" default>
  :::tip Quick Install
  <Tabs groupId="interface">
    <TabItem value="ui" label="goose Desktop" default>
    [Launch the installer](https://mcp.squareup.com/goose)
    </TabItem>
    <TabItem value="cli" label="goose CLI">
    **Command**
    ```sh
    npx mcp-remote https://mcp.squareup.com/sse
    ```
    </TabItem>
  </Tabs>
  :::

  <Tabs groupId="interface">
    <TabItem value="ui" label="goose Desktop" default>
     1. [Launch the installer](https://mcp.squareup.com/goose)
     2. Click `OK` to confirm the installation
     3. goose should open a browser tab to an OAuth permissions page. Double-check which permissions you want to allow, and click `Grant Access`.
     4. It will ask you to login or reauthenticate to Square, and may ask you to confirm the permissions you want to allow.
     5. In goose, navigate to the chat

    </TabItem>
    <TabItem value="cli" label="goose CLI">
      <CLIExtensionInstructions
        name="square-mcp-remote"
        type="stdio"
        command="npx mcp-remote https://mcp.squareup.com/sse"
        timeout={300}
      />

      When you start the next session, goose will open a browser where you can grant permissions and sign in to your Square account.
  
    </TabItem>
  </Tabs>
</TabItem>

  <TabItem value="local" label="Square Local MCP">
  :::tip Quick Install
  <Tabs groupId="interface">
    <TabItem value="ui" label="goose Desktop" default>
    [Launch the installer](goose://extension?cmd=npx&arg=square-mcp-server&arg=start&id=square-mcp&name=Square%20MCP%20Server&description=Square%20API%20MCP%20Server&env=ACCESS_TOKEN%3DYour%20Access%20Token&env=SANDBOX%3Dtrue)

    </TabItem>
    <TabItem value="cli" label="goose CLI">
    **Command**
    ```sh
    npx square-mcp-server start
    ```
    </TabItem>
  </Tabs>
    **Environment Variables**
    ```
    ACCESS_TOKEN: <YOUR_API_KEY>
    SANDBOX: <true/false>
    PRODUCTION: <true/false>
    ```

    Note that you'll use `SANDBOX` -or- `PRODUCTION`, not both, and your `ACCESS_TOKEN` will either be a sandbox or production token, depending on which environment you choose.
  :::

  <Tabs groupId="interface">
    <TabItem value="ui" label="goose Desktop" default>
    <GooseDesktopInstaller
      extensionId="square-mcp"
      extensionName="Square MCP Server"
      description="Square API MCP Server"
      command="npx"
      args={["square-mcp-server", "start"]}
      envVars={[
        { name: "ACCESS_TOKEN", label: "Your Access Token" },
        { name: "SANDBOX", label: "true" }
      ]}
      appendToStep3="Set either SANDBOX or PRODUCTION to true (the access token must match the environment)"
      apiKeyLink="https://developer.squareup.com/apps"
      apiKeyLinkText="Square Access Token"
    />
    </TabItem>
  <TabItem value="cli" label="goose CLI">
    <CLIExtensionInstructions
      name="square-mcp"
      type="stdio"
      command="npx square-mcp-server start"
      timeout={300}
      envVars={[
        { key: "ACCESS_TOKEN", value: "▪▪▪▪▪▪▪▪▪▪▪▪▪▪▪▪▪▪▪▪▪▪▪▪▪▪▪▪▪▪▪" },
        { key: "SANDBOX", value: "true" }
      ]}
      infoNote={
        <>
          Obtain your <a href="https://developer.squareup.com/apps" target="_blank" rel="noopener noreferrer">Square Access Token</a> and paste it in. Set either <code>SANDBOX</code> or <code>PRODUCTION</code> to true (the access token must match the environment)
        </>
      }
    />
    </TabItem>
  </Tabs>

  </TabItem>
</Tabs>

## Example Usage

The Square MCP server allows you to interact with Square's connect API with an extensive [service catalog](https://github.com/square/square-mcp-server?tab=readme-ov-file#service-catalog) to access the Square API ecosystem.

#### goose Prompt

```
( O)> Find my food menu from the following image and import these into my Square seller account. "my-burger-menu.png"
```

[Here's the menu image used in the demo.](../assets/guides/square-mcp-goosin-menu.png)

#### goose Output

```
I'll help you import these menu items into your Square catalog. I'll use the Square API to create catalog items for each burger. Let me break this down into steps:

1. First, I'll check the catalog API service info and types
2. Then I'll create catalog items for each burger

Now let me check the type info for creating catalog objects

Now I'll start creating catalog items for each burger in your menu. I'll create them one by one using the catalog API. Let me process each burger:

(output truncated)

I've successfully created all the burger items from your menu in your Square catalog. Each burger has been added with:

- The exact name from your menu
- The full description
- The correct price
- A standard variation for ordering

Here's a summary of what was created:
- The Classic - $10.00
- Cheeseburger - $11.00
- Primetime Burger - $12.00
- Double Bacon - $14.00
- Texas Meatlover - $12.50
- Buffalo Burger - $12.00
- The Atomic - $12.50
- Little Bird - $11.00
- Mushroom Burger - $10.00

All items are now available in your Square catalog and ready for use in your point of sale system. Each burger includes its complete description and comes with a "Regular" variation at the specified price. The items are set as both sellable and stockable by default, and they're available at all locations.
```
