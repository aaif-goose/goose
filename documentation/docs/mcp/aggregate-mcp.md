---
title: Aggregate Extension
description: Add Aggregate MCP Server as a goose Extension
---

import Tabs from '@theme/Tabs';
import TabItem from '@theme/TabItem';
import CLIExtensionInstructions from '@site/src/components/CLIExtensionInstructions';
import GooseDesktopInstaller from '@site/src/components/GooseDesktopInstaller';

This tutorial covers how to add [Aggregate](https://github.com/Izzy-Gottz/aggregate-finance) as a goose extension, so goose can answer questions about your company's money: cash and runway, transactions, who owes you, bills, payroll and reports, from one ledger that combines your banks, cards, payment apps and accounting.

Aggregate is hosted at `app.aggregate.finance` and is open source (AGPL-3.0). It reads from your providers and can never move money, pay bills or send anything.

## Configuration

:::tip Quick Install
<Tabs groupId="interface">
  <TabItem value="ui" label="goose Desktop" default>
  [Launch the installer](goose://extension?type=streamable_http&url=https%3A%2F%2Fapp.aggregate.finance%2Fmcp&id=aggregate&name=Aggregate&description=Ask%20about%20your%20company%27s%20cash%2C%20runway%2C%20invoices%20and%20payroll)
  </TabItem>
  <TabItem value="cli" label="goose CLI">
  Use `goose configure` to add a `Remote Extension (Streamable HTTP)` extension type with:

  **Endpoint URL**
  ```
  https://app.aggregate.finance/mcp
  ```
  </TabItem>
</Tabs>
:::

:::info OAUTH FLOW
A browser window opens for you to sign in to Aggregate and choose what goose can see (which companies and kinds of data, read-only or not, and an expiry).
:::

<Tabs groupId="interface">
  <TabItem value="ui" label="goose Desktop" default>
    <GooseDesktopInstaller
      extensionId="aggregate"
      extensionName="Aggregate"
      description="Ask about your company's cash, runway, invoices and payroll"
      type="http"
      url="https://app.aggregate.finance/mcp"
    />
  </TabItem>
  <TabItem value="cli" label="goose CLI">
    <CLIExtensionInstructions
      name="aggregate"
      description="Ask about your company's cash, runway, invoices and payroll"
      type="http"
      url="https://app.aggregate.finance/mcp"
      timeout={300}
    />
  </TabItem>
</Tabs>

## Example Usage

To try it without your own accounts, use the demo server `https://app.aggregate.finance/demo/mcp`, which needs no sign-in and holds made-up companies.

### goose Prompt

```
What's our runway, and which customers are late paying us?
```

goose calls Aggregate's `get_overview` and `search_invoices` tools and answers with the cash, monthly net burn, runway in months, and the overdue invoices by customer, each traceable to the records behind it.
