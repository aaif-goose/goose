---
title: Rube Extension
description: Add Rube as a goose Extension to connect with 500+ apps
---

import Tabs from '@theme/Tabs';
import TabItem from '@theme/TabItem';
import CLIExtensionInstructions from '@site/src/components/CLIExtensionInstructions';
import GooseDesktopInstaller from '@site/src/components/GooseDesktopInstaller';

This tutorial covers how to add [Rube](https://rube.app) as a goose extension to enable seamless integration with 500+ apps including Slack, Gmail, Notion, Google Workspace, Microsoft Office, GitHub, and many more.

:::tip Quick Install
<Tabs groupId="interface">
  <TabItem value="ui" label="goose Desktop" default>
    [Launch the installer](goose://extension?type=streamable_http&url=https%3A%2F%2Frube.app%2Fmcp&id=rube-mcp&name=Rube&description=Seamlessly%20connect%20across%20500%2B%20applications%20including%20Slack%2C%20Gmail%2C%20Notion%2C%20Google%20Workspace%2C%20Microsoft%20Office%2C%20GitHub%2C%20and%20many%20more)
  </TabItem>
  <TabItem value="cli" label="goose CLI">
    Use `goose configure` to add a `Remote Extension (Streamable HTTP)` extension type with:
    
    **Endpoint URL**
    ```
    https://rube.app/mcp
    ```
  </TabItem>
</Tabs>
:::

:::info OAUTH FLOW
An OAuth window will open in your browser. Follow the prompts to authorize access to your Rube account.
:::

## What is Rube?

Rube is a platform powered by Composio that provides unified access to 500+ apps and services through a single integration. It enables seamless connectivity across different applications without additional setup, making it perfect for both individual users and teams.

## Configuration

<Tabs groupId="interface">
  <TabItem value="ui" label="goose Desktop" default>
    <GooseDesktopInstaller
      extensionId="rube-mcp"
      extensionName="Rube"
      description="Seamlessly connect across 500+ applications including Slack, Gmail, Notion, Google Workspace, Microsoft Office, GitHub, and many more"
      type="http"
      url="https://rube.app/mcp"
    />
  </TabItem>
  <TabItem value="cli" label="goose CLI">
    <CLIExtensionInstructions
      name="rube"
      description="Seamlessly connect across 500+ applications including Slack, Gmail, Notion, Google Workspace, Microsoft Office, GitHub, and many more"
      type="http"
      url="https://rube.app/mcp"
      timeout={300}
      infoNote="OAuth authentication will happen automatically in your browser when you first use Rube tools"
    />

  </TabItem>
</Tabs>

## Authentication

Rube uses OAuth browser authentication, which means:
- No manual API keys to manage
- Secure authentication handled automatically
- When you first use a Rube tool, your browser will open to authenticate with the relevant service
- Authentication tokens are securely stored and managed by Rube

## Example Usage

Once Rube is configured, you can interact with 500+ apps directly through goose. Here are some examples:

**Email Management**
```
Send an email to john@example.com with subject "Meeting Update" and body "The meeting has been moved to 3 PM"
```

**Slack Communication**
```
Post a message to the #general channel saying "Deploy completed successfully"
```

**GitHub Integration**
```
Create a new issue in my repository with title "Bug Report" and description "Found an issue with the login flow"
```

**Google Sheets**
```
Add a new row to my "Sales Data" spreadsheet with values: Date: today, Amount: 1500, Client: Acme Corp
```

**Calendar Management**
```
Schedule a meeting for tomorrow at 2 PM with title "Project Review" and invite sarah@company.com
```

The first time you use tools for a specific service, you'll be prompted to authenticate through OAuth in your browser. After that, all subsequent interactions with that service will work seamlessly.

## Troubleshooting

- **Extension not connecting**: Ensure you have a stable internet connection and that `https://rube.app` is accessible from your network.
- **Authentication issues**: If OAuth flows aren't working, try clearing your browser cache or using a different browser.
- **Tool timeouts**: Some operations with large datasets may take longer. You can increase the timeout value in the extension settings if needed.
- **Rate limits**: Rube respects the rate limits of individual services. If you hit limits, wait a few minutes before retrying.

## Getting Help

- Visit [rube.app](https://rube.app) for documentation and support for integration-specific questions
- Check the Rube status page for any service interruptions
