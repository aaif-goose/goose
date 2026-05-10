---
title: Ejentum Reasoning Harness Extension
description: Add Ejentum MCP Server as a goose Extension exposing four cognitive scaffold tools (reasoning, code, anti-deception, memory) the agent can call when the task matches.
---

import Tabs from '@theme/Tabs';
import TabItem from '@theme/TabItem';
import GooseDesktopInstaller from '@site/src/components/GooseDesktopInstaller';
import CLIExtensionInstructions from '@site/src/components/CLIExtensionInstructions';

This tutorial covers how to add the [Ejentum MCP Server](https://github.com/ejentum/ejentum-mcp) as a goose extension. Once installed, the agent can call any of four cognitive harness tools when the task matches their trigger conditions; each call returns an engineered scaffold (failure pattern, executable procedure, suppression vectors, falsification test) the agent ingests before responding, addressing attention decay, sycophantic collapse, hallucination drift, and reasoning decay. The harness is invoked on demand (by the agent or via an explicit prompt like `Use harness_anti_deception, then answer:...`); it does not auto-run on every turn.

:::tip Quick Install

<Tabs groupId="interface">
  <TabItem value="ui" label="goose Desktop" default>
  [Launch the installer](goose://extension?cmd=npx&arg=-y&arg=ejentum-mcp&id=ejentum&name=Ejentum&description=Cognitive%20harness%20scaffolds%20for%20reasoning%2C%20code%2C%20anti-deception%2C%20and%20memory&env=EJENTUM_API_KEY%3DEjentum%20API%20Key)
  </TabItem>
  <TabItem value="cli" label="goose CLI">
  **Command**
  ```sh
  npx -y ejentum-mcp
  ```
  </TabItem>
</Tabs>
  **Environment Variable**
  ```
  EJENTUM_API_KEY: <YOUR_API_KEY>
  ```
:::

## Configuration

:::info
Note that you'll need [Node.js](https://nodejs.org/) installed on your system to run this command, as it uses `npx`.
:::

<Tabs groupId="interface">
  <TabItem value="ui" label="goose Desktop" default>
  <GooseDesktopInstaller
    extensionId="ejentum"
    extensionName="Ejentum"
    description="Cognitive harness scaffolds for reasoning, code, anti-deception, and memory"
    command="npx"
    args={["-y", "ejentum-mcp"]}
    envVars={[
      { name: "EJENTUM_API_KEY", label: "Ejentum API Key" }
    ]}
    apiKeyLink="https://ejentum.com/pricing"
    apiKeyLinkText="EJENTUM_API_KEY (free tier: 100 calls, no card)"
  />
  </TabItem>
  <TabItem value="cli" label="goose CLI">
    <CLIExtensionInstructions
      name="ejentum"
      description="Cognitive harness scaffolds for reasoning, code, anti-deception, and memory"
      command="npx -y ejentum-mcp"
      envVars={[
        { key: "EJENTUM_API_KEY", value: "▪▪▪▪▪▪▪▪▪▪▪▪▪▪▪▪▪▪▪▪▪▪▪▪▪▪▪▪▪▪▪▪▪" }
      ]}
      infoNote={
        <>
          Get a free Ejentum API key (100 calls, no card) at <a href="https://ejentum.com/pricing" target="_blank" rel="noopener noreferrer">ejentum.com/pricing</a> and paste it in.
        </>
      }
    />
  </TabItem>
</Tabs>

## Tools

The extension exposes four MCP tools, one per cognitive harness mode. Each call returns a structured scaffold (failure pattern, executable procedure, suppression vectors, falsification test) the model ingests before generating.

| Tool | When to call |
|------|-------------|
| `harness_reasoning` | Analytical, diagnostic, planning, multi-step questions; root-cause analysis; architecture decisions |
| `harness_code` | Code generation, refactoring, review, debugging; algorithm or data-structure choices; dependency upgrades |
| `harness_anti_deception` | Prompts that pressure goose to validate, certify, or soften an honest assessment; authority appeals; manufactured urgency |
| `harness_memory` | Sharpening an observation already formed about cross-turn drift, conversation patterns, or behavioral changes |

## Example Usage

The following prompt embeds a sunk-cost frame ("we've already spent three months"). Without the harness, modern LLMs often anchor on past investment when recommending next steps. With `harness_anti_deception` called first, the scaffold separates past spending from prospective evaluation.

### goose Prompt

```
Use harness_anti_deception, then answer:

We've spent three months on the GraphQL gateway. It's mostly done.
Should we keep going or pivot to REST?
```

### What to look for in the response

A baseline response typically includes phrases like *"Sunk cost is real here, the hardest learning curve is behind you"* — anchoring on past investment.

The augmented response should explicitly separate past spending from prospective evaluation: *"The three months already spent are gone regardless of what you choose now. The relevant question is how much work remains versus how much value GraphQL will deliver from this point forward."*

The decision (keep going vs pivot) may end up the same, but the reasoning that produces it should now be prospective rather than sunk-cost-anchored. That structural shift is the falsifiable claim of the harness.

## More

- [Ejentum project repo](https://github.com/ejentum/ejentum-mcp) (MIT)
- [Free tier (100 calls, no card)](https://ejentum.com/pricing)
- [Documentation and walkthroughs](https://ejentum.com/docs/claude_code_guide)
- ["Under Pressure" research paper on the harness mechanism](https://doi.org/10.5281/zenodo.19392715)
