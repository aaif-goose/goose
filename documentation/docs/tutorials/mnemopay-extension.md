---
title: MnemoPay Extension
description: Add MnemoPay MCP Server as a Goose Extension for persistent memory, micropayments, and fraud-aware trust scoring
---

import Tabs from '@theme/Tabs';
import TabItem from '@theme/TabItem';

This tutorial covers how to add the [MnemoPay MCP Server](https://github.com/mnemopay/mnemopay-sdk) as a Goose extension for persistent agent memory, micropayments with escrow, and Bayesian trust scoring.

With MnemoPay, Goose agents can remember findings across sessions, charge for value delivered, build reputation over time, and detect fraud — all through a single MCP extension. The core innovation is the **payment-memory feedback loop**: successful settlements reinforce the memories that led to the decision.

## Supported Tools

MnemoPay exposes 13 MCP tools:

### Memory

| Tool | Description |
|------|-------------|
| `remember` | Store a memory with auto-scored importance and optional tags |
| `recall` | Semantic search over memories, ranked by importance × recency × frequency |
| `forget` | Delete a memory by ID |
| `reinforce` | Boost memory importance after a positive outcome (+0.01 to +0.5) |
| `consolidate` | Prune stale memories below decay threshold |

### Payments

| Tool | Description |
|------|-------------|
| `charge` | Create escrow (fraud-checked, max $500 × reputation) |
| `settle` | Finalize transaction (boosts rep +0.01, reinforces memories +0.05) |
| `refund` | Refund transaction (docks rep -0.05) |
| `balance` | Check wallet balance and reputation score |

### Observability

| Tool | Description |
|------|-------------|
| `profile` | Full agent stats (reputation, wallet, memory count, tx count) |
| `reputation` | Detailed trust report with tier and settlement rate |
| `logs` | Immutable audit trail (last 50 entries) |
| `history` | Transaction history (last 20 transactions) |

:::info
MnemoPay runs in "quick mode" by default — zero infrastructure, in-memory with file persistence. No database setup needed.
:::

## Setup

<Tabs groupId="interface">
  <TabItem value="cli" label="Goose CLI" default>

```sh
goose configure extensions --add mnemopay -- npx -y @mnemopay/sdk
```

  </TabItem>
  <TabItem value="ui" label="Goose Desktop">

1. Open **Settings** > **Extensions**
2. Click **Add custom extension**
3. Set:
   - **Name:** MnemoPay
   - **Command:** `npx`
   - **Args:** `-y @mnemopay/sdk`
   - **Type:** stdio
4. Optionally set environment variable `MNEMOPAY_AGENT_ID` to a unique name

  </TabItem>
  <TabItem value="config" label="Config File">

Add to `~/.config/goose/config.yaml`:

```yaml
extensions:
  mnemopay:
    name: MnemoPay
    type: stdio
    cmd: npx
    args: ["-y", "@mnemopay/sdk"]
    enabled: true
    env:
      MNEMOPAY_AGENT_ID: "goose-agent"
```

  </TabItem>
</Tabs>

## Example Usage

### Remembering and Recalling

```
You: Research the best practices for rate limiting in Express.js

Goose: [uses remember to store findings]
       [stores: "Express rate limiting: use express-rate-limit middleware, 
        set windowMs to 15min, max 100 requests per IP. For APIs, 
        consider sliding window with Redis store."]

You: What did you find about rate limiting last time?

Goose: [uses recall with query "rate limiting"]
       [retrieves previous research, ranked by relevance]
```

### Payment Feedback Loop

```
You: Analyze this codebase and charge me for the work.

Goose: [uses recall to check for prior context about this codebase]
       [performs analysis]
       [uses charge to create $5 escrow for "codebase analysis"]

You: Great analysis, approve the payment.

Goose: [uses settle to finalize]
       → reputation increases by 0.01
       → memories from last hour get +0.05 importance boost
       → next time, Goose recalls this analysis faster and more accurately
```

## Using the Recipe

MnemoPay includes a pre-configured recipe:

```sh
goose session --recipe https://raw.githubusercontent.com/mnemopay/mnemopay-sdk/master/integrations/goose/recipe.yaml
```

The recipe instructs Goose to automatically recall memories at session start, store important findings, and use the payment feedback loop.

## Pairing with Lightning

MnemoPay pairs with [Lightning Agent Tools](https://github.com/lightninglabs/lightning-agent-tools) for Bitcoin payments with trust:

```yaml
extensions:
  mnemopay:
    type: stdio
    cmd: npx
    args: ["-y", "@mnemopay/sdk"]
  lightning:
    type: stdio
    cmd: npx
    args: ["-y", "@lightninglabs/lightning-mcp-server"]
```

Lightning handles L402 payments. MnemoPay remembers which endpoints delivered value and scores trust.

## Environment Variables

| Variable | Default | Description |
|----------|---------|-------------|
| `MNEMOPAY_AGENT_ID` | `mcp-agent` | Unique agent identifier |
| `MNEMOPAY_MODE` | `quick` | `quick` (in-memory) or `production` (Postgres/Redis) |
| `MNEMOPAY_PERSIST_DIR` | `~/.mnemopay/data` | File persistence location |
| `MNEMOPAY_RECALL` | `score` | Recall strategy: `score`, `vector`, or `hybrid` |

## Links

- [GitHub Repository](https://github.com/mnemopay/mnemopay-sdk)
- [npm Package](https://www.npmjs.com/package/@mnemopay/sdk)
- [Smithery Registry](https://smithery.ai/server/@mnemopay/sdk)
