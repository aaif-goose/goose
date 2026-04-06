---
title: PQS - Prompt Quality Score Extension
description: Add PQS MCP Server as a goose Extension
---

import CLIExtensionInstructions from '@site/src/components/CLIExtensionInstructions';
import GooseDesktopInstaller from '@site/src/components/GooseDesktopInstaller';

This tutorial covers how to add the [PQS MCP Server](https://github.com/OnChainAIIntel/pqs-mcp-server) as a goose extension to score, optimize, and compare LLM prompts before inference. PQS is the world's first named AI prompt quality score — grade any prompt A-F on a 40-point scale, get an optimized version, or compare Claude vs GPT-4o on the same prompt.

## Configuration

:::info Note that you'll need [Node.js](https://nodejs.org/) installed on your system to run this command, as it uses `npx`. :::

<GooseDesktopInstaller
  extensionId="pqs"
  extensionName="PQS - Prompt Quality Score"
  description="Score, optimize, and compare LLM prompts before inference"
  type="stdio"
  command="npx"
  args={["pqs-mcp-server"]}
  timeout={300}
  envVars={[
    { name: "PQS_API_KEY", label: "PQS API Key — optional, only needed for paid tools. Get one at pqs.onchainintel.net" }
  ]}
  apiKeyLink="https://pqs.onchainintel.net"
  apiKeyLinkText="PQS API Key"
/>

<CLIExtensionInstructions
  name="PQS - Prompt Quality Score"
  description="Score, optimize, and compare LLM prompts before inference"
  type="stdio"
  command="npx pqs-mcp-server"
  timeout={300}
  envVars={[
    { key: "PQS_API_KEY", value: "●●●●●●●●●●●●●●●●●●●●●●●●●●●●●●" }
  ]}
  infoNote={
    <>
      Get your API key from{" "}
      <a href="https://pqs.onchainintel.net" target="_blank" rel="noopener noreferrer">
        pqs.onchainintel.net
      </a>.
      The free score_prompt tool requires no API key.
    </>
  }
/>

## Example Usage

Use PQS to check prompt quality before sending to any model, or have goose optimize your prompts automatically.

### goose Prompt

> Score this prompt before I send it: "Summarize the key risks in this document"

### goose Output

:::note Desktop

PQS scored your prompt 14/40 — Grade D. The main issues are lack of specificity, missing output format instructions, and no context about what type of risks to focus on. Would you like me to optimize it?

:::

### goose Prompt

> Optimize my prompt for the crypto vertical: "What wallets should I watch?"

### goose Output

:::note Desktop

Here is your optimized prompt (scored 31/40 — Grade B):

"Identify the top 5 crypto wallets currently exhibiting unusual on-chain activity on Ethereum mainnet. For each wallet provide: address, 30-day PnL, primary trading pattern (e.g. swing, DeFi yield, NFT), and one actionable signal for the next 48 hours."

:::
