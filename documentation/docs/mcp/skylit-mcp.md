---
title: Skylit Extension
description: Add Skylit MCP Server as a goose Extension for Options Flow, Dealer Positioning (GEX) and Volatility Data
---

import Tabs from '@theme/Tabs';
import TabItem from '@theme/TabItem';
import GooseDesktopInstaller from '@site/src/components/GooseDesktopInstaller';
import CLIExtensionInstructions from '@site/src/components/CLIExtensionInstructions';

This tutorial covers how to add the [Skylit MCP Server](https://github.com/SkylitAI/skylit-mcp) as a goose extension to give goose read-only US options market data: options flow and trade scores, sweeps, screeners, dark-pool prints, dealer positioning (GEX and VEX levels, with point-in-time replay) and implied-volatility analytics. No tool places orders.

## Configuration

:::tip Quick Install
<Tabs groupId="interface">
  <TabItem value="ui" label="goose Desktop" default>
  [Launch the installer](goose://extension?type=streamable_http&url=https%3A%2F%2Fmcp.skylit.ai%2Fmcp&id=skylit-mcp&name=Skylit&description=Options%20flow%2C%20dealer%20positioning%20(GEX)%20and%20volatility%20market%20data&header=Authorization%3DBearer%20YOUR_SKYLIT_API_KEY)
  </TabItem>
  <TabItem value="cli" label="goose CLI">
  Add a `Remote Extension (Streamable HTTP)` extension type with:

  **Endpoint URL**
  ```
  https://mcp.skylit.ai/mcp
  ```
  </TabItem>
</Tabs>

  **Custom Request Header**
  ```
  Authorization: Bearer <YOUR_SKYLIT_API_KEY>
  ```
:::

<Tabs groupId="interface">
  <TabItem value="ui" label="goose Desktop" default>
    <GooseDesktopInstaller
      extensionId="skylit-mcp"
      extensionName="Skylit"
      description="Options flow, dealer positioning (GEX) and volatility market data"
      type="http"
      url="https://mcp.skylit.ai/mcp"
      envVars={[
        { name: "Authorization", label: "Bearer YOUR_SKYLIT_API_KEY" }
      ]}
      apiKeyLink="https://app.skylit.ai/developer"
      apiKeyLinkText="Skylit API key"
    />
  </TabItem>

  <TabItem value="cli" label="goose CLI">
    <CLIExtensionInstructions
      name="Skylit"
      description="Options flow, dealer positioning (GEX) and volatility market data"
      type="http"
      url="https://mcp.skylit.ai/mcp"
      timeout={300}
      envVars={[
        { key: "Authorization", value: "Bearer YOUR_SKYLIT_API_KEY" }
      ]}
      infoNote={
        <>
          Create a <a href="https://app.skylit.ai/developer" target="_blank" rel="noopener noreferrer">Skylit API key</a> and paste it in as the <code>Bearer</code> token. Calls are billed in credits from your Skylit account (1 credit = $0.001); failed calls are free, and the <code>account_usage</code> tool shows your balance at no charge.
        </>
      }
    />
  </TabItem>
</Tabs>

## Example Usage

Let's ask goose for a pre-market read on SPY.

### goose Prompt

```
Check my Skylit balance first. Then for SPY:
1. Get the key gamma levels (King node, flip and the largest walls) and how far each is from spot.
2. Summarize today's options flow tone and the number of sweeps.
3. Show the implied volatility rank and the expected move for the nearest expiration.
Keep the whole answer under 10 lines and list the tools you used.
```

goose calls `account_usage`, `heat_levels`, `chain_bull_bear`, `sweeps` and the Tempest volatility tools (`tempest_iv`, `tempest_cones`), then answers with the levels, the flow read and the volatility context, each with its as-of time. Outside market hours the data is the last session's close.

For every tool, its arguments and its credit cost, see the [Skylit tool catalog](https://www.skylit.ai/docs/mcp/tools). Setup guides for other clients and runnable examples are in the [skylit-mcp repository](https://github.com/SkylitAI/skylit-mcp).
