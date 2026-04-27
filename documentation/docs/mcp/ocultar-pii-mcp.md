---
title: Ocultar PII Refinery Extension
description: Add zero-egress PII detection and redaction to your goose workflows
unlisted: true
---

import Tabs from '@theme/Tabs';
import TabItem from '@theme/TabItem';
import GooseDesktopInstaller from '@site/src/components/GooseDesktopInstaller';

This tutorial will get you started with the [Ocultar PII Refinery](https://github.com/Edu963/ocultar) as a goose extension. Ocultar detects and tokenizes personally identifiable information — names, emails, IBANs, phone numbers, addresses and more — in-place before any text reaches an upstream API. No raw PII ever leaves your infrastructure.

:::info Prerequisites
You need a running Ocultar instance before using this extension. The fastest way:
```sh
git clone https://github.com/Edu963/ocultar
cd ocultar
docker compose -f docker-compose.community.yml up
```
See the [Ocultar repository](https://github.com/Edu963/ocultar) for full setup instructions.
:::

:::tip Quick Install
<Tabs groupId="interface">
  <TabItem value="ui" label="goose Desktop" default>
  [Launch the installer](goose://extension?cmd=uvx&arg=ocultar-goose-mcp&id=ocultar-pii&name=Ocultar%20PII%20Refinery&description=Zero-egress%20PII%20detection%20and%20redaction&env=OCULTAR_URL%3Dhttp%3A%2F%2Flocalhost%3A8080)
  </TabItem>
  <TabItem value="cli" label="goose CLI">
  **Command**
  ```sh
  uvx ocultar-goose-mcp
  ```
  </TabItem>
</Tabs>
  **Environment Variables** (both optional)
  ```
  OCULTAR_URL: http://localhost:8080   # URL of your Ocultar instance
  OCULTAR_API_KEY: <your-key>          # Only required for authenticated deployments
  ```
:::

## Configuration

:::info
You need [uv](https://docs.astral.sh/uv/getting-started/installation/) installed on your system to run this command, as it uses `uvx`.
:::

<Tabs groupId="interface">
  <TabItem value="ui" label="goose Desktop" default>
  <GooseDesktopInstaller
    extensionId="ocultar-pii"
    extensionName="Ocultar PII Refinery"
    description="Zero-egress PII detection and redaction"
    command="uvx"
    args={["ocultar-goose-mcp"]}
    envVars={[
      { name: "OCULTAR_URL", label: "Ocultar instance URL (default: http://localhost:8080)" },
      { name: "OCULTAR_API_KEY", label: "API key (only for authenticated deployments)" }
    ]}
  />
  </TabItem>
  <TabItem value="cli" label="goose CLI">
  1. Run the `configure` command:
  ```sh
  goose configure
  ```

  2. Choose to add a `Command-line Extension`
  ```sh
    ┌   goose-configure
    │
    ◇  What would you like to configure?
    │  Add Extension (Connect to a new extension)
    │
    ◆  What type of extension would you like to add?
    │  ○ Built-in Extension
    // highlight-start
    │  ● Command-line Extension (Run a local command or script)
    // highlight-end
    │  ○ Remote Extension (Streamable HTTP)
    └
  ```

  3. Give your extension a name
  ```sh
    ┌   goose-configure
    │
    // highlight-start
    ◆  What would you like to call this extension?
    │  ocultar-pii
    // highlight-end
    └
  ```

  4. Enter the command
  ```sh
    ┌   goose-configure
    │
    // highlight-start
    ◆  What command should be run?
    │  uvx ocultar-goose-mcp
    // highlight-end
    └
  ```

  5. Set the timeout (default 300s is fine)
  ```sh
    ┌   goose-configure
    │
    // highlight-start
    ◆  Please set the timeout for this tool (in secs):
    │  300
    // highlight-end
    └
  ```

  6. Add environment variables if your Ocultar instance is not on `localhost:8080`
  ```sh
    ┌   goose-configure
    │
    // highlight-start
    ◆  Would you like to add environment variables?
    │  Yes
    │
    ◇  Environment variable name:
    │  OCULTAR_URL
    │
    ◇  Environment variable value:
    │  http://localhost:8080
    │
    ◇  Add another environment variable?
    │  No
    // highlight-end
    └  Added ocultar-pii extension
  ```
  </TabItem>
</Tabs>

## Example Usage

The Ocultar PII Refinery extension exposes a single `refine_text` tool. goose calls it automatically when you ask it to sanitize or redact content before sending it anywhere sensitive.

#### goose Prompt

```
( O)> I need to send this support ticket to our AI summarizer, but it contains customer PII. 
Redact it first: "Hi, I'm Jean-Pierre Dumont, my email is jp.dumont@societe-generale.fr 
and my IBAN is FR76 3000 6000 0112 3456 7890 189. I have a billing issue."
```

#### goose Output

```
I'll redact the PII from that ticket before it goes anywhere.

refine_text
input: "Hi, I'm Jean-Pierre Dumont, my email is jp.dumont@societe-generale.fr 
and my IBAN is FR76 3000 6000 0112 3456 7890 189. I have a billing issue."

Redacted result:

"Hi, I'm [PERSON_181bc039], my email is [EMAIL_b227fb2b] and my IBAN is 
[IBAN_72eb0d24]. I have a billing issue."

3 PII tokens detected and replaced:
- PERSON_181bc039 → name
- EMAIL_b227fb2b  → email address  
- IBAN_72eb0d24   → French IBAN

The sanitized text is safe to forward. Tokens are stored encrypted in your 
local Ocultar vault and can be re-hydrated by authorized callers only.
```

## What Gets Detected

Ocultar's community tier runs five detection layers with no external API calls:

| Layer | What it catches |
|---|---|
| Dictionary shield | Named entities from your protected list |
| Pattern + entropy | High-entropy strings, secrets, tokens |
| Regex rules | Email, SSN, phone, credit card, IBAN, passport |
| Phone validator | International numbers via libphonenumber |
| Address heuristics | Street addresses across formats |

Enterprise tier adds a Tier 2 AI scanner (local SLM) for contextual name and entity detection.
