---
title: Alby Bitcoin Payments Extension

description: Connect goose to your Bitcoin Lightning Wallet
---

import Tabs from '@theme/Tabs';
import TabItem from '@theme/TabItem';
import CLIExtensionInstructions from '@site/src/components/CLIExtensionInstructions';
import GooseDesktopInstaller from '@site/src/components/GooseDesktopInstaller';
import { PanelLeft } from 'lucide-react';




This tutorial covers how to add the [Alby Bitcoin Payments MCP Server](https://github.com/getalby/mcp) as a goose extension to interact with your lightning wallet, make and receive payments, list transactions, convert fiat amounts to sats, request invoices from lightning addresses, and interact with paid MCP tools (such as ones built with [PaidMCP](https://github.com/getAlby/paidmcp)).

:::info
You'll need a lightning wallet that supports [NWC](https://nwc.dev). If you don't have one yet, consider trying [Alby Hub](https://albyhub.com).
:::

:::tip Quick Install
<Tabs groupId="interface">
  <TabItem value="ui" label="goose Desktop" default>
  [Launch the installer](goose://extension?cmd=npx&arg=-y&arg=%40getalby%2Fmcp&id=alby-mcp&name=Alby&description=Connect%20goose%20to%20your%20Bitcoin%20Lightning%20Wallet&env=NWC_CONNECTION_STRING%3DNWC%20Connection%20Secret)
  </TabItem>
  <TabItem value="cli" label="goose CLI">
  **Command**
  ```sh
  npx -y @getalby/mcp
  ```
  </TabItem>
</Tabs>
  **Environment Variable**
  ```
  NWC_CONNECTION_STRING: nostr+walletconnect://...
  ```
:::

## Configuration

:::info
You'll need [Node.js](https://nodejs.org/) installed on your system to run this command, as it uses `npx`

**or** you can use the Alby-hosted MCP (see remote options below).
:::

<Tabs groupId="interface">
  <TabItem value="ui" label="goose Desktop" default>
    <Tabs>
      <TabItem value="local" label="Local" default>
        <GooseDesktopInstaller
          extensionId="alby-mcp"
          extensionName="Alby"
          description="Connect goose to your Bitcoin Lightning Wallet"
          command="npx"
          args={["-y", "@getalby/mcp"]}
          envVars={[
            { name: "NWC_CONNECTION_STRING", label: "NWC Connection Secret" }
          ]}
          customStep3="Obtain a NWC Connection secret from your lightning wallet (nostr+walletconnect://...) and paste it in to the 'NWC Connection Secret' field"
        />
      </TabItem>
      <TabItem value="remote" label="Remote">
          <GooseDesktopInstaller
            extensionId="alby-remote"
            extensionName="Alby"
            description="Connect goose to your Bitcoin Lightning Wallet"
            type="http"
            url="https://mcp.getalby.com/mcp"
            envVars={[
              { name: "Authorization", label: "Bearer YOUR_NWC_CONNECTION_STRING" }
            ]}
            apiKeyLink="https://nwc.dev"
            apiKeyLinkText="NWC connection secret"
            customStep3="Obtain an NWC connection secret from your lightning wallet and paste it as the Bearer token (Bearer nostr+walletconnect://...)"
          />
      </TabItem>
    </Tabs>
  </TabItem>
  <TabItem value="cli" label="goose CLI">
    <Tabs>
      <TabItem value="local" label="Local" default>
        <CLIExtensionInstructions
          name="Alby"
          description="Connect goose to your Bitcoin Lightning Wallet"
          command="npx -y @getalby/mcp"
          envVars={[
            { key: "NWC_CONNECTION_STRING", value: "▪▪▪▪▪▪▪▪▪▪▪▪▪▪▪▪▪▪▪▪▪▪▪▪▪▪▪▪▪▪▪▪▪▪▪▪▪▪▪" }
          ]}
          infoNote={
            <>
              Obtain an NWC Connection secret from your lightning wallet (<code>nostr+walletconnect://...</code>) and paste it in.
            </>
          }
        />
      </TabItem>
      <TabItem value="remote" label="Remote">
        <CLIExtensionInstructions
          name="Alby"
          description="Connect goose to your Bitcoin Lightning Wallet"
          type="http"
          url="https://mcp.getalby.com/mcp"
          envVars={[
            { key: "Authorization", value: "Bearer nostr+walletconnect://..." }
          ]}
          infoNote={
            <>
              Obtain an NWC connection secret from your lightning wallet and paste it as the <code>Bearer</code> token.
            </>
          }
        />
      </TabItem>  
    </Tabs>
  </TabItem>
</Tabs>

## Example Usage

:::tip Memory Extension
Use the built-in memory extension to save your contacts. e.g. "My friend Rene's lightning address is reneaaron@getalby.com. Please save it to your memory."
:::

### goose Prompt

> _Pay Rene $1 USD, with message "Hi Rene, it's Roland. I'm making a goose tutorial!"_

### goose Output

:::note CLI

<details>
    <summary>Tool Calls</summary>

    ─── fiat_to_sats | alby ──────────────────────────

    fiat_amount: 1

    fiat_currency: USD



    ─── request_invoice | alby ──────────────────────────

    amount_in_sats: 935

    description: ...

    lightning_address: reneaaron@getalby.com



    ─── pay_invoice | alby ──────────────────────────

    invoice: ...



</details>



✅ **Payment Successful!**

I've successfully sent $1 USD (935 sats) to Rene at `reneaaron@getalby.com` with your message: "Hi Rene, it's Roland. I'm making a goose tutorial!"

**Transaction Details:**
- Amount: 935 sats ($1 USD)
- Fees: 3 sats
- Total cost: 938 sats
- Message: "Hi Rene, it's Roland. I'm making a goose tutorial!"
- Payment hash: `cd30642470d81c2e8eb04c9ac95e679c53778537aecdf5554b15288ac42fe129`

Rene will receive the message "Thanks, sats received!" as confirmation of the payment.
:::
