---
title: Coinbase Agentic Wallet Extension
description: Add the Coinbase Agentic Wallet MCP Server as a goose Extension to autonomously discover and pay for x402 APIs
---

import Tabs from '@theme/Tabs';
import TabItem from '@theme/TabItem';
import CLIExtensionInstructions from '@site/src/components/CLIExtensionInstructions';
import GooseDesktopInstaller from '@site/src/components/GooseDesktopInstaller';

This tutorial covers how to add the [Coinbase Agentic Wallet MCP Server](https://docs.cdp.coinbase.com/agentic-wallet/mcp/welcome) as a goose extension. Agents can discover and pay for HTTP APIs autonomously via the x402 protocol, using a Coinbase-managed embedded wallet. Funding happens through the built-in Coinbase Onramp when needed, with no API keys or seed phrases to manage.

## Install Agentic Wallet MCP

Run the Coinbase installer once to create the local MCP bundle:

```sh
npx @coinbase/payments-mcp install --client other
```

The installer creates the stdio server bundle at `~/.payments-mcp/bundle.js`.

:::tip Quick Install
<Tabs groupId="interface">
  <TabItem value="ui" label="goose Desktop" default>
  [Launch the installer](goose://extension?cmd=sh&arg=-c&arg=node%20%22%24HOME%2F.payments-mcp%2Fbundle.js%22&id=coinbase-agentic-wallet&name=Coinbase%20Agentic%20Wallet&description=Discover%20and%20pay%20for%20x402%20APIs)
  </TabItem>
  <TabItem value="cli" label="goose CLI">
  **Command**

  ```sh
  sh -c 'node "$HOME/.payments-mcp/bundle.js"'
  ```

  </TabItem>
</Tabs>
:::

## Configuration

:::info
You'll need [Node.js](https://nodejs.org/) installed on your system to run this command, as it uses `npx`.
:::

<Tabs groupId="interface">
  <TabItem value="ui" label="goose Desktop" default>
    <GooseDesktopInstaller
      extensionId="coinbase-agentic-wallet"
      extensionName="Coinbase Agentic Wallet"
      description="Discover and pay for x402 APIs with USDC via a Coinbase embedded wallet"
      type="stdio"
      command="sh"
      args={["-c", "node \"$HOME/.payments-mcp/bundle.js\""]}
      timeout={300}
    />
  </TabItem>
  <TabItem value="cli" label="goose CLI">
    <CLIExtensionInstructions
      name="Coinbase Agentic Wallet"
      description="Discover and pay for x402 APIs with USDC"
      type="stdio"
      command={`sh -c 'node "$HOME/.payments-mcp/bundle.js"'`}
      timeout={300}
    />
  </TabItem>
</Tabs>

## Sign in

On first use, the extension opens an in-app companion window for email and OTP sign-in. This creates a Coinbase-managed embedded wallet, so you don't need to manage a seed phrase. Add USDC through the built-in Coinbase Onramp.

## Example Usage

### goose Prompt

> _Find a paid web scraping API on the x402 bazaar and use it to summarize https://example.com_

### goose Output

:::note Desktop

goose will use the Coinbase Agentic Wallet tools to:

1. Search the x402 bazaar for a web scraping API.
2. Inspect a candidate service's price and payment requirements.
3. Pay for the request in USDC over x402.
4. Return the paid API response for summarization.

See the [Agentic Wallet docs](https://docs.cdp.coinbase.com/agentic-wallet/mcp/welcome) for the full tool catalog and supported chains.

:::
