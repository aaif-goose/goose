---
title: Voidly Pay Extension
description: Pay for HTTP 402 endpoints autonomously via the x402 protocol
---

import Tabs from '@theme/Tabs';
import TabItem from '@theme/TabItem';
import CLIExtensionInstructions from '@site/src/components/CLIExtensionInstructions';
import GooseDesktopInstaller from '@site/src/components/GooseDesktopInstaller';

This tutorial covers how to add the [Voidly Pay MCP server](https://www.npmjs.com/package/@voidly/pay-mcp) as a goose extension to browse a marketplace of paid HTTP services and pay for them via the [x402 protocol](https://www.x402.org). 42 tools spanning marketplace browse, paid fetch, signed scrape, PDF→text, HTML→markdown, hash, timestamp, random, QR codes, Wikipedia summaries, FX rates, plus wallet/escrow/x402 primitives.

Settlement happens off-chain in Voidly Pay credits (Stage 1) or on-chain USDC on Base mainnet (Stage 2). The vault is Sourcify-verified at [`0xb592...1c12`](https://basescan.org/address/0xb592512932a7b354969bb48039c2dc7ad6ad1c12) with public reserves at [voidly.ai/pay/proof](https://voidly.ai/pay/proof). Sub-200ms typical settlement.

:::info
Identity is an Ed25519 keypair on disk — no API keys, no Stripe customer object. Provision a wallet with 10 free credits via [voidly.ai/pay/claim](https://voidly.ai/pay/claim) (browser-only, no install).
:::

:::tip Quick Install
<Tabs groupId="interface">
  <TabItem value="ui" label="goose Desktop" default>
  [Launch the installer](goose://extension?cmd=npx&arg=-y&arg=%40voidly%2Fpay-mcp&id=voidly-pay&name=Voidly%20Pay&description=42%20paid%20agent%20tools)
  </TabItem>
  <TabItem value="cli" label="goose CLI">
  **Command**
  ```sh
  npx -y @voidly/pay-mcp
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
      extensionId="voidly-pay"
      extensionName="Voidly Pay"
      description="42 paid agent tools — marketplace browse, signed scrape, PDF→text, HTML→markdown, Wikipedia, FX, plus wallet/escrow/x402 primitives"
      command="npx"
      args={["-y", "@voidly/pay-mcp"]}
      customStep3={"On first run, the MCP server creates an Ed25519 keypair at ~/.voidly-pay-keypair.json. Visit https://voidly.ai/pay/claim to claim 10 free credits — paste the keypair's DID into the faucet form."}
    />
  </TabItem>
  <TabItem value="cli" label="goose CLI">
    <CLIExtensionInstructions
      name="Voidly Pay"
      description="42 paid agent tools — marketplace browse, signed scrape, PDF→text, HTML→markdown, Wikipedia, FX, plus wallet/escrow/x402 primitives"
      command="npx -y @voidly/pay-mcp"
    />
  </TabItem>
</Tabs>

## What You Can Do

### Browse the Marketplace

Discover every paid endpoint Voidly itself runs (17 today) plus self-served third-party listings. Goose can filter by category and price.

**Prompt:**
```
What's available on the Voidly Pay marketplace under 5¢?
```

<details>
<summary>goose Output</summary>

```
Here are paid endpoints under 5¢:

1. voidly_wiki — $0.01 — Wikipedia summary by title
2. voidly_country_fetch — $0.01 — Country profile by ISO code
3. voidly_random — $0.01 — Cryptographically random bytes
4. voidly_qr — $0.02 — Generate a QR code (PNG)
5. voidly_hash — $0.01 — SHA-256 / SHA-3 / BLAKE3 of a string
6. voidly_timestamp — $0.01 — RFC 3161 timestamp authority
7. voidly_fx — $0.02 — Live FX rate between two currencies
8. voidly_html_to_markdown — $0.03 — Clean Markdown from a URL

Want me to call any of these?
```

</details>

### Pay for a Wikipedia Summary

Once goose has a wallet with credits, paid endpoints work just like free ones — the SDK auto-handles the 402 → quote → settle → retry loop.

**Prompt:**
```
Pay 1¢ to fetch the Wikipedia summary for "Alan Turing", then summarize it for me.
```

<details>
<summary>goose Output</summary>

```
Paid 1¢ via voidly_wiki — settlement receipt:
  transfer_id: tx_3a9f2c
  recipient: did:voidly:PBcsB2avN4ZfDC5yvEwBBT
  ts: 2026-05-02T14:32:11Z

Alan Turing (1912–1954) was a British mathematician and computer
scientist. He formalized the concepts of algorithm and computation
with the Turing machine, broke German naval Enigma during WWII at
Bletchley Park, and proposed the Turing test for machine intelligence.
He's widely considered the father of theoretical computer science
and AI.
```

</details>

### Run a Health Check

Before a long-running paid task, verify your wallet, the facilitator, and the on-chain vault are all healthy.

**Prompt:**
```
Run the Voidly Pay health check and show me the results.
```

<details>
<summary>goose Output</summary>

```
Voidly Pay 6-check trust report:

✓ Facilitator reachable        api.voidly.ai/v1/pay/x402 (87ms)
✓ Vault verified on Sourcify   0xb592...1c12 on Base mainnet
✓ Wallet balance               10 credits (≈ $0.10)
✓ Keypair valid                did:voidly:6z32S...utX
✓ Recent settlements healthy   last 100 OK, p50 174ms
✓ All checks passed

Safe to run paid tasks.
```

</details>

### List Your Own Paid Endpoint

Anyone can list a paid HTTP endpoint on the open marketplace. Voidly takes zero platform cut on Stage 1.

**Prompt:**
```
List my endpoint https://api.example.com/extract on the Voidly marketplace
for 3¢ per call. Name it "Receipt OCR".
```

## Example Prompts

Voidly Pay is most powerful when goose pairs paid endpoints with the rest of your toolchain:

- **Research with citations:** *"Pay for Wikipedia summaries of the top 5 turing-award winners and write a comparison."*
- **Scrape behind paywalls:** *"Use voidly_signed_scrape to fetch this article and save the markdown to my notes."*
- **Verify documents:** *"Compute the SHA-256 of this PDF via voidly_hash and timestamp it via voidly_timestamp."*
- **Currency-aware planning:** *"Convert $5,000 to EUR using voidly_fx, then split it across these expense categories."*
- **Build your own:** *"What would it cost to run my OCR endpoint on Voidly Pay? Walk me through listing it."*

## Resources

- [Voidly Pay docs](https://voidly.ai/pay)
- [Live marketplace JSON](https://api.voidly.ai/v1/pay/marketplace)
- [Sourcify-verified vault](https://repo.sourcify.dev/contracts/full_match/8453/0xb592512932A7B354969BB48039C2dC7Ad6AD1c12/)
- [Public reserves dashboard](https://voidly.ai/pay/proof)
- [@voidly/pay-mcp on npm](https://www.npmjs.com/package/@voidly/pay-mcp)
