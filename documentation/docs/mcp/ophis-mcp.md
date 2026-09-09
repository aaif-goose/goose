---
title: Ophis Extension
description: Add Ophis as a goose Extension to swap tokens with natural-language intents
---

import Tabs from '@theme/Tabs';
import TabItem from '@theme/TabItem';
import CLIExtensionInstructions from '@site/src/components/CLIExtensionInstructions';
import GooseDesktopInstaller from '@site/src/components/GooseDesktopInstaller';

This tutorial covers how to add [Ophis](https://ophis.fi) as a goose extension so goose can turn natural-language swap requests into executable orders across supported chains.

:::tip Quick Install
<Tabs groupId="interface">
  <TabItem value="ui" label="goose Desktop" default>
    [Launch the installer](goose://extension?type=streamable_http&url=https%3A%2F%2Fmcp.ophis.fi%2Fmcp&id=ophis&name=Ophis&description=Natural-language%20intent%20DEX%20aggregator%20with%20a%20keyless%20MCP%20server%20for%20AI%20agents)
  </TabItem>
  <TabItem value="cli" label="goose CLI">
    Use `goose configure` to add a `Remote Extension (Streamable HTTP)` extension type with:

    **Endpoint URL**
    ```
    https://mcp.ophis.fi/mcp
    ```
  </TabItem>
</Tabs>
:::

## What is Ophis?

Ophis is an intent-based DEX aggregator with a natural-language layer and a keyless MCP server for AI agents. It is non-custodial, gasless, and MEV-protected, and returns swap surplus to the trader. Ophis is a fork of CoW Protocol. It deploys its own settlement contracts on Optimism and Unichain and routes through CoW Protocol on the other supported chains: Ethereum, Base, Arbitrum, Polygon, BNB Chain, Gnosis, Avalanche, Linea, Plasma, and Ink.

The MCP server is keyless and requires no API key or environment variables. It exposes fourteen tools: `parse_intent`, `resolve_token`, `list_chains`, `get_quote`, `expected_surplus`, `build_order`, `validate_order`, `submit_order`, `lookup_tier`, `get_integrator_earnings`, `get_balances`, `get_portfolio`, `get_gas`, and `get_token_chart`. Trades are signed by the user's own wallet, so the server never holds keys or funds.

## Configuration

<Tabs groupId="interface">
  <TabItem value="ui" label="goose Desktop" default>
    <GooseDesktopInstaller
      extensionId="ophis"
      extensionName="Ophis"
      description="Natural-language intent DEX aggregator with a keyless MCP server for AI agents"
      type="http"
      url="https://mcp.ophis.fi/mcp"
    />
  </TabItem>
  <TabItem value="cli" label="goose CLI">
    <CLIExtensionInstructions
      name="Ophis"
      description="Natural-language intent DEX aggregator with a keyless MCP server for AI agents"
      type="http"
      url="https://mcp.ophis.fi/mcp"
    />
  </TabItem>
</Tabs>

## Example Usage

Once Ophis is configured, you can describe a swap in plain language and ask goose to prepare an order. Here are some examples:

**Parse an intent**
```
Parse this swap intent: "swap 100 USDC for ETH on Base".
```

**Get a quote**
```
Get a quote to swap 0.5 WETH for USDC on Arbitrum.
```

**List supported chains**
```
Which chains does Ophis support?
```

The server returns a quote and an unsigned order. The actual trade is signed by your own wallet, so no keys or funds are ever shared with the extension.

## Resources

- Website: [ophis.fi](https://ophis.fi)
- App: [swap.ophis.fi](https://swap.ophis.fi)
- Docs: [docs.ophis.fi](https://docs.ophis.fi)
- Agent skills for shell-capable agents: [ophis.fi/.well-known/agent-skills](https://ophis.fi/.well-known/agent-skills/index.json)
- Source: [github.com/ophis-fi/ophis](https://github.com/ophis-fi/ophis)
