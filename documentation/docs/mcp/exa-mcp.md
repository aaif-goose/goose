---
title: Exa Search Extension
description: Add Exa MCP Server as a goose Extension
---

import Tabs from '@theme/Tabs';
import TabItem from '@theme/TabItem';
import GooseDesktopInstaller from '@site/src/components/GooseDesktopInstaller';
import CLIExtensionInstructions from '@site/src/components/CLIExtensionInstructions';

This tutorial covers how to add the [Exa MCP Server](https://github.com/exa-labs/exa-mcp-server) as a goose extension to enable AI-powered web search functionality.

:::tip Quick Install
<Tabs groupId="interface">
  <TabItem value="ui" label="goose Desktop" default>
  [Launch the installer](goose://extension?cmd=npx&arg=-y&arg=exa-mcp-server&id=exa&name=Exa%20Search&description=AI-powered%20web%20search&env=EXA_API_KEY%3DExa%20API%20Key)
  </TabItem>
  <TabItem value="cli" label="goose CLI">
  **Command**
  ```sh
  npx -y exa-mcp-server
  ```
  </TabItem>
</Tabs>
  **Environment Variable**
  ```
  EXA_API_KEY: <YOUR_API_KEY>
  ```
:::

## Configuration

:::info
Note that you'll need [Node.js](https://nodejs.org/) installed on your system to run this command, as it uses `npx`.
:::

<Tabs groupId="interface">
  <TabItem value="ui" label="goose Desktop" default>
  <GooseDesktopInstaller
    extensionId="exa"
    extensionName="Exa Search"
    description="AI-powered web search"
    command="npx"
    args={["-y", "exa-mcp-server"]}
    envVars={[
      { name: "EXA_API_KEY", label: "Exa API Key" }
    ]}
    apiKeyLink="https://dashboard.exa.ai/api-keys"
    apiKeyLinkText="Exa API Key"
  />
</TabItem>
  <TabItem value="cli" label="goose CLI">
    <CLIExtensionInstructions
      name="Exa Search"
      description="AI-powered web search"
      command="npx -y exa-mcp-server"
      envVars={[
        { key: "EXA_API_KEY", value: "▪▪▪▪▪▪▪▪▪▪▪▪▪▪▪▪▪▪▪▪▪▪▪▪▪▪▪▪▪▪▪▪▪▪▪▪▪▪▪" }
      ]}
      infoNote={
        <>
          Obtain your <a href="https://dashboard.exa.ai/api-keys" target="_blank" rel="noopener noreferrer">Exa API Key</a> and paste it in. You can get your API key by signing up at <a href="https://exa.ai" target="_blank" rel="noopener noreferrer">exa.ai</a> and navigating to the API keys page in the dashboard.
        </>
      }
    />
  </TabItem>
</Tabs>

## Example Usage

The Exa MCP server enables AI-powered web search in your goose interactions. Exa offers advanced search capabilities including:

1. Neural, keyword, and auto search modes
2. Content retrieval with full text, highlights, or summaries
3. Category filtering (news, research papers, companies, and more)
4. Domain and date range filtering

### goose Prompt

```
Find recent research papers about large language model reasoning capabilities and summarize the key findings.
```

### goose Output

```
I'll use Exa to search for recent research papers on LLM reasoning.

─── web_search_exa | exa ──────────────────────────
query: large language model reasoning capabilities research paper
category: research paper


Here are the key findings from recent research on LLM reasoning:

1. Chain-of-Thought Reasoning:
- Recent papers show that explicit chain-of-thought prompting significantly improves performance on multi-step reasoning tasks
- Models trained with reasoning traces demonstrate better generalization to novel problem types

2. Mathematical and Logical Reasoning:
- New benchmarks reveal that while LLMs have improved at arithmetic and formal logic, they still struggle with compositional reasoning
- Hybrid approaches combining neural methods with symbolic solvers show promising results

3. Commonsense Reasoning:
- Studies demonstrate that scaling model size continues to improve commonsense reasoning benchmarks
- However, models still fail on tasks requiring physical intuition or temporal reasoning about everyday events

4. Limitations and Future Directions:
- Several papers identify "reasoning shortcuts" where models exploit dataset patterns rather than performing genuine inference
- Research into retrieval-augmented reasoning shows potential for grounding model outputs in verified facts

Would you like me to search for more details on any of these topics?
```

:::tip
Exa supports multiple search types including neural search for semantic queries and auto mode that intelligently combines search strategies. You can also filter results by category, domain, and date range for more targeted research.
:::
