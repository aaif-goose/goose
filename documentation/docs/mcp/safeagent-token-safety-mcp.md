---
title: SafeAgent Token Safety Extension
description: Add SafeAgent Token Safety MCP Server as a goose Extension
---

import Tabs from '@theme/Tabs';
import TabItem from '@theme/TabItem';
import CLIExtensionInstructions from '@site/src/components/CLIExtensionInstructions';
import GooseDesktopInstaller from '@site/src/components/GooseDesktopInstaller';

This tutorial covers how to add the [SafeAgent Token Safety MCP Server](https://github.com/Aigen-Protocol/aigen-protocol) as a goose extension to analyze DeFi tokens before you trade, including honeypot simulation, safety scoring, and scam-pattern detection across Ethereum, Base, Optimism, Arbitrum, BSC, and Polygon.

:::tip Quick Install
<Tabs groupId="interface">
  <TabItem value="ui" label="goose Desktop" default>
  [Launch the installer](goose://extension?type=streamable_http&url=https%3A%2F%2Fcryptogenesis.duckdns.org%2Fmcp&id=safeagent-token-safety&name=SafeAgent%20Token%20Safety&description=Analyze%20DeFi%20token%20safety%2C%20detect%20honeypots%2C%20and%20simulate%20swaps%20across%20six%20EVM%20chains.)
  </TabItem>
  <TabItem value="cli" label="goose CLI">
  Use `goose configure` to add a `Remote Extension (Streamable HTTP)` extension type with:

  **Endpoint URL**
  ```
  https://cryptogenesis.duckdns.org/mcp
  ```
  </TabItem>
</Tabs>

No API key or custom headers are required for the public endpoint.
:::

## Configuration

The public SafeAgent endpoint exposes DeFi safety tools including `shield`, `test_honeypot`, and `check_token_safety`.

<Tabs groupId="interface">
  <TabItem value="ui" label="goose Desktop" default>
    <GooseDesktopInstaller
      extensionId="safeagent-token-safety"
      extensionName="SafeAgent Token Safety"
      description="Analyze DeFi token safety, detect honeypots, and simulate swaps across six EVM chains."
      type="http"
      url="https://cryptogenesis.duckdns.org/mcp"
    />
  </TabItem>
  <TabItem value="cli" label="goose CLI">
    <CLIExtensionInstructions
      name="SafeAgent Token Safety"
      description="Analyze DeFi token safety, detect honeypots, and simulate swaps across six EVM chains."
      type="http"
      url="https://cryptogenesis.duckdns.org/mcp"
      timeout={300}
    />
  </TabItem>
</Tabs>

## Example Usage

Use SafeAgent Token Safety when you want goose to screen a token before buying, compare risk across chains, or simulate whether a token can actually be sold.

### goose Prompt

```
Use SafeAgent Token Safety to check whether this token on Base looks safe before I buy it.
Run a honeypot simulation, explain any red flags you find, and tell me clearly whether I should block or proceed.
```

### goose Output

:::note Desktop

I checked the token with SafeAgent Token Safety and found multiple high-risk signals.

- `shield` returned a BLOCK decision because the contract can still change fee behavior after launch.
- `test_honeypot` showed that buys succeed but sells fail in the swap simulation.
- `check_token_safety` scored the token 22/100 and flagged ownership concentration plus transfer restrictions.

This token does not look safe to trade. I would avoid buying it unless those risks are removed and independently verified.

:::
