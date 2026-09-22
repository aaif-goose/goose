---
title: Keploy Extension
description: Add Keploy MCP Server as a goose Extension
---

import Tabs from '@theme/Tabs';
import TabItem from '@theme/TabItem';
import CLIExtensionInstructions from '@site/src/components/CLIExtensionInstructions';
import GooseDesktopInstaller from '@site/src/components/GooseDesktopInstaller';

This tutorial covers how to add the [Keploy MCP Server](https://github.com/keploy/keploy) as a goose extension to generate API test suites from OpenAPI specs, curl commands, or Postman collections, run them against a deployed environment, and surface coverage gaps.

Keploy is an API testing platform that turns specs and real traffic into deterministic tests with auto-generated mocks. This remote MCP server lets goose generate test suites from OpenAPI specs, curl commands, or Postman collections, run them against a publicly reachable deployment (such as staging), and report coverage gaps. Recording live traffic with eBPF and replaying it against virtualized dependencies runs through the local Keploy CLI on your own machine — see the [Keploy docs](https://keploy.io/docs/running-keploy/agent-test-generation/).

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
Generate a Keploy test suite from my OpenAPI spec at ./openapi.yaml and run
it against my staging API at https://staging.example.com, then show coverage.
```

### goose Output
```
I'll generate a suite from your spec and run it against staging.

1. Read ./openapi.yaml and generated test suites covering its endpoints
2. Ran the suites against https://staging.example.com and collected results
3. Reported failures and prioritized the endpoints your spec defines but the
   suites don't yet cover

Your staging API now has a Keploy suite, plus a ranked list of coverage gaps
to close next.
```
