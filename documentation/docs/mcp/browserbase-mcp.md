---
title: Browserbase Extension
description: Add Browserbase MCP Server as a goose Extension for Web Automation
---

import Tabs from '@theme/Tabs';
import TabItem from '@theme/TabItem';
import GooseDesktopInstaller from '@site/src/components/GooseDesktopInstaller';
import CLIExtensionInstructions from '@site/src/components/CLIExtensionInstructions';

This tutorial covers how to add the Browserbase MCP Server as a goose extension for browser automation, enabling programmatic control over navigation, page interactions, and content capture.

:::tip Quick Install

<Tabs groupId="interface">
  <TabItem value="ui" label="goose Desktop" default>
  [Launch the installer](goose://extension?cmd=npx&arg=-y&arg=%40browserbasehq%2Fmcp&id=browserbase-mcp&name=Browserbase&description=Automate%20web%20browsing%20and%20data%20extraction&env=BROWSERBASE_PROJECT_ID%3DBrowserbase%20Project%20ID&env=BROWSERBASE_API_KEY%3DBrowserbase%20API%20Key&env=GEMINI_API_KEY%3DGemini%20API%20Key)
  </TabItem>
  <TabItem value="cli" label="goose CLI">
  **Command**
  ```sh
  npx -y @browserbasehq/mcp
  ```
  </TabItem>
</Tabs>
  **Environment Variables**
  ```
  BROWSERBASE_PROJECT_ID: <YOUR_PROJECT_ID>
  BROWSERBASE_API_KEY: <YOUR_API_KEY>
  GEMINI_API_KEY: <YOUR_GEMINI_API_KEY>
  ```
:::

## Configuration

:::info
Note that you'll need [Node.js](https://nodejs.org/) installed on your system to run this command, as it uses `npx`.
:::

<Tabs groupId="interface">
  <TabItem value="ui" label="goose Desktop" default>
  <GooseDesktopInstaller
    extensionId="browserbase-mcp"
    extensionName="Browserbase"
    description="Automate web browsing and data extraction"
    command="npx"
    args={["-y", "@browserbasehq/mcp"]}
    envVars={[
      { name: "BROWSERBASE_PROJECT_ID", label: "Browserbase Project ID" },
      { name: "BROWSERBASE_API_KEY", label: "Browserbase API Key" },
      { name: "GEMINI_API_KEY", label: "Gemini API Key" }
    ]}
    apiKeyLink="https://browserbase.io/dashboard"
    apiKeyLinkText="Browserbase credentials"
  />
  </TabItem>
  <TabItem value="cli" label="goose CLI">
    <CLIExtensionInstructions
      name="Browserbase"
      description="Automate web browsing and data extraction"
      command="npx -y @browserbasehq/mcp"
      envVars={[
        { key: "BROWSERBASE_PROJECT_ID", value: "▪▪▪▪▪▪▪▪▪▪▪▪▪▪▪▪" },
        { key: "BROWSERBASE_API_KEY", value: "▪▪▪▪▪▪▪▪▪▪▪▪▪▪▪▪▪▪▪▪▪▪▪▪▪▪▪▪▪▪▪▪▪▪▪▪▪▪▪" },
        { key: "GEMINI_API_KEY", value: "▪▪▪▪▪▪▪▪▪▪▪▪▪▪▪▪▪▪▪▪▪▪▪▪▪▪▪▪▪▪▪▪▪" }
      ]}
      infoNote={
        <>
          Obtain your <a href="https://browserbase.io/dashboard" target="_blank" rel="noopener noreferrer">Browserbase credentials</a> and <a href="https://aistudio.google.com/app/apikey" target="_blank" rel="noopener noreferrer">Gemini API key</a> and paste them in.
        </>
      }
    />
  </TabItem>
</Tabs>

## Example Usage

Let's use the Browserbase extension to gather information about trending MCP-related repositories on GitHub.

### goose Prompt

```
1. Go to https://github.com/trending

2. Search for "MCP server" or scroll through and find repos with "MCP" or "Model Context Protocol" in the title or description

3. For the top 3 relevant repositories:
    • Click into the repo
    • Take a screenshot of the repo page
    • Extract:
        - Repo name
        - Description
        - Stars today
        - First paragraph of README 
    • Navigate back

- Close all tabs

- Output a Markdown report showing:
    • Repo name (link)
    • Screenshot
    • Description + Stars
    • README excerpt
```

### goose Output

```
# MCP Repositories Report

## 1. [FastMCP](https://github.com/jlowin/fastmcp)

![FastMCP Repository Screenshot]

**Description**: The fast, Pythonic way to build MCP servers and clients.  
**Stars Today**: 10.2k stars

**README Excerpt**: The Model Context Protocol (MCP) is a new, standardized way to provide context and tools to your LLMs, and FastMCP makes building MCP servers and clients simple and intuitive. Create tools, expose resources, define prompts, and connect components with clean, Pythonic code.

## 2. [GitHub MCP Server](https://github.com/github/github-mcp-server)

![GitHub MCP Server Screenshot]

**Description**: GitHub's official MCP Server that provides seamless integration with GitHub APIs, enabling advanced automation and interaction capabilities for developers and tools.  
**Stars Today**: 13.9k stars

**README Excerpt**: The GitHub MCP Server is a Model Context Protocol (MCP) server that provides seamless integration with GitHub APIs, enabling advanced automation and interaction capabilities for developers and tools.

## 3. [Playwright MCP](https://github.com/microsoft/playwright-mcp)

![Playwright MCP Screenshot]

**Description**: A Model Context Protocol (MCP) server that provides browser automation capabilities using Playwright.  
**Stars Today**: 10.2k stars

**README Excerpt**: A Model Context Protocol (MCP) server that provides browser automation capabilities using Playwright. This server enables LLMs to interact with web pages through structured accessibility snapshots, bypassing the need for screenshots or visually-tuned models.
```
