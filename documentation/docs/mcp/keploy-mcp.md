---
title: Keploy Extension
description: Add Keploy MCP Server as a goose Extension
---

import Tabs from '@theme/Tabs';
import TabItem from '@theme/TabItem';
import CLIExtensionInstructions from '@site/src/components/CLIExtensionInstructions';
import GooseDesktopInstaller from '@site/src/components/GooseDesktopInstaller';

This tutorial covers how to add the [Keploy MCP Server](https://github.com/keploy/keploy) as a goose extension to record real API traffic, generate deterministic integration tests with auto-generated mocks, and surface coverage gaps.

Keploy captures real API, database, and Kafka traffic with eBPF and replays it as deterministic tests. This remote MCP server lets goose record traffic, generate test suites from that traffic or from OpenAPI specs, curl commands, and Postman collections, run them against virtualized dependencies, and report coverage gaps.

:::tip Quick Install
<Tabs groupId="interface">
  <TabItem value="ui" label="goose Desktop" default>
  [Launch the installer](goose://extension?type=streamable_http&url=https%3A%2F%2Fapi.keploy.io%2Fclient%2Fv1%2Fmcp&id=keploy&name=Keploy&description=Generate%2C%20mock%2C%20and%20run%20API%20tests%20from%20real%20traffic%2C%20OpenAPI%20specs%2C%20curl%2C%20or%20Postman%20collections&header=X-API-Key%3DYOUR_KEPLOY_API_KEY)
  </TabItem>
  <TabItem value="cli" label="goose CLI">
  Add a `Remote Extension (Streamable HTTP)` extension type with:

  **Endpoint URL**
  ```
  https://api.keploy.io/client/v1/mcp
  ```
  </TabItem>
</Tabs>

  **Custom Request Header**
  ```
  X-API-Key: <YOUR_KEPLOY_API_KEY>
  ```
:::

## Configuration

<Tabs groupId="interface">
  <TabItem value="ui" label="goose Desktop" default>
    <GooseDesktopInstaller
      extensionId="keploy"
      extensionName="Keploy"
      description="Generate, mock, and run API tests from real traffic, OpenAPI specs, curl, or Postman collections"
      type="http"
      url="https://api.keploy.io/client/v1/mcp"
      envVars={[
        { name: "X-API-Key", label: "Your Keploy API key" }
      ]}
      apiKeyLink="https://app.keploy.io/settings/api-keys"
      apiKeyLinkText="Keploy API Key"
    />
  </TabItem>

  <TabItem value="cli" label="goose CLI">
    <CLIExtensionInstructions
      name="Keploy"
      description="Generate, mock, and run API tests from real traffic, OpenAPI specs, curl, or Postman collections"
      type="http"
      url="https://api.keploy.io/client/v1/mcp"
      timeout={300}
      envVars={[
        { key: "X-API-Key", value: "kep_xxxxxxxxxxxxxxxxxxxx" }
      ]}
      infoNote={
        <>
          Obtain your <a href="https://app.keploy.io/settings/api-keys" target="_blank" rel="noopener noreferrer">Keploy API Key</a> and paste it in.
        </>
      }
    />
  </TabItem>
</Tabs>

## Example Usage

### goose Prompt
```
Record the traffic from my checkout API and generate a test suite with mocks for the database and downstream calls.
```

### goose Output
```
I'll help you record traffic from your checkout API and turn it into a deterministic test suite.

1. Started Keploy's eBPF-based recorder against your checkout service
2. Captured incoming API requests along with the database and downstream calls they triggered
3. Generated a test suite from the captured traffic, with mocks for each database and downstream dependency
4. Ran the generated tests against the virtualized dependencies and reported coverage gaps

Your checkout API now has a deterministic test suite you can run in CI without hitting real dependencies.
```
