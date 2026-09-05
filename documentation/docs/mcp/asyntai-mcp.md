---
title: Asyntai Extension
description: Add the Asyntai MCP Server as a goose Extension to run your website's AI support agent
---

import Tabs from '@theme/Tabs';
import TabItem from '@theme/TabItem';
import CLIExtensionInstructions from '@site/src/components/CLIExtensionInstructions';
import GooseDesktopInstaller from '@site/src/components/GooseDesktopInstaller';

This tutorial covers how to add the [Asyntai MCP Server](https://github.com/asyntai/mcp-bridge) as a goose extension so goose can manage the AI support agent that runs on your website: its knowledge base and instructions, the conversations and leads it collects, and live replies to visitors.

:::tip Quick Install
<Tabs groupId="interface">
  <TabItem value="ui" label="goose Desktop" default>
    [Launch the installer](goose://extension?type=streamable_http&url=https%3A%2F%2Fasyntai.com%2Fmcp&id=asyntai&name=Asyntai&description=Manage%20your%20website%27s%20Asyntai%20AI%20support%20agent%3A%20knowledge%20base%2C%20instructions%2C%20conversations%2C%20leads%2C%20and%20live%20replies)
  </TabItem>
  <TabItem value="cli" label="goose CLI">
    Use `goose configure` to add a `Remote Extension (Streamable HTTP)` extension type with:

    **Endpoint URL**
    ```
    https://asyntai.com/mcp
    ```
  </TabItem>
</Tabs>
:::

:::info OAUTH FLOW
An OAuth window opens in your browser on first use. Sign in to your [Asyntai](https://asyntai.com) account and approve the connection. No API key or environment variables are needed. The free plan works.
:::

## What is Asyntai?

[Asyntai](https://asyntai.com) is an AI chat agent for websites. It answers visitor questions from the site's own content, captures leads, and hands conversations to a human when needed. The hosted MCP server exposes the same account you manage in the Asyntai dashboard, so goose can read and change it for you.

The server exposes tools to:

- read and update the agent's AI instructions (`get_ai_instructions`, `update_ai_instructions`)
- add, list, and remove knowledge base content
- list recent conversations, read a conversation, and review captured leads
- find questions the agent could not answer (`list_knowledge_gaps`)
- take over a live conversation, reply to the visitor, and hand it back (`take_over_conversation`, `send_agent_reply`, `release_conversation`)
- check plan usage and message limits

## Configuration

<Tabs groupId="interface">
  <TabItem value="ui" label="goose Desktop" default>
    <GooseDesktopInstaller
      extensionId="asyntai"
      extensionName="Asyntai"
      description="Manage your website's Asyntai AI support agent: knowledge base, instructions, conversations, leads, and live replies"
      type="http"
      url="https://asyntai.com/mcp"
    />
  </TabItem>
  <TabItem value="cli" label="goose CLI">
    <CLIExtensionInstructions
      name="asyntai"
      description="Manage your website's Asyntai AI support agent: knowledge base, instructions, conversations, leads, and live replies"
      type="http"
      url="https://asyntai.com/mcp"
      timeout={300}
    />
  </TabItem>
</Tabs>

## Example Usage

Once Asyntai is connected, you can ask goose to review and improve your support agent.

**Find unanswered questions**
```
Which questions could my Asyntai agent not answer this week?
```

**Update the agent's instructions**
```
Tell my Asyntai agent to always mention that shipping to the EU takes 3 to 5 business days.
```

**Review leads**
```
List the leads my website chat collected in the last 7 days.
```

**Reply to a visitor live**
```
Take over the open conversation from the visitor asking about refunds and tell them a refund takes 5 business days.
```

## Resources

- Website: [asyntai.com](https://asyntai.com)
- Documentation: [asyntai.com/documentation/mcp](https://asyntai.com/documentation/mcp/)
- Source: [github.com/asyntai/mcp-bridge](https://github.com/asyntai/mcp-bridge)
