---
title: Top Of Mind Extension
description: Inject persistent instructions into goose's working memory every turn
---

import Tabs from '@theme/Tabs';
import TabItem from '@theme/TabItem';
import { PlatformExtensionNote } from '@site/src/components/PlatformExtensionNote';
import GooseBuiltinInstaller from '@site/src/components/GooseBuiltinInstaller';

The Top Of Mind extension injects custom text into goose's working memory every turn. This is useful for:
- **Security guardrails** that must never be forgotten (e.g., "never upload code to external services")
- **Behavioral rules** that should persist throughout a session
- **Project context** that needs to stay in the model's immediate attention

Unlike system prompts or [goosehints](/docs/guides/context-engineering/using-goosehints) which can fade from attention as conversations grow, content injected by the tom extension appears fresh in every turn, making it more reliable for critical instructions.

:::tip
For a complete guide on use cases and best practices, see [Persistent Instructions](/docs/guides/context-engineering/using-persistent-instructions).
:::

## Configuration

<PlatformExtensionNote/>

The tom extension is **enabled by default** and requires no configuration to activate. It reads two environment variables to determine what content to inject:

| Variable | Description |
|----------|-------------|
| [`GOOSE_MOIM_MESSAGE_TEXT`](/docs/guides/environment-variables#session-management) | Literal text injected into working memory each turn |
| [`GOOSE_MOIM_MESSAGE_FILE`](/docs/guides/environment-variables#session-management) | Path to a file whose contents are injected. Supports `~/` |

When both are set, their contents are concatenated. Each source is capped at 64 KB with UTF-8 safe truncation. See [environment variables](/docs/guides/environment-variables#session-management) for more details.

<Tabs groupId="interface">
  <TabItem value="ui" label="goose Desktop" default>
  <GooseBuiltinInstaller
    extensionName="Top Of Mind"
    description="Inject custom context into every turn via GOOSE_MOIM_MESSAGE_TEXT and GOOSE_MOIM_MESSAGE_FILE environment variables"
  />

  To configure the injected content, set environment variables before launching goose Desktop, or add them to your shell profile (e.g., `~/.zshrc` or `~/.bashrc`):

  ```bash
  export GOOSE_MOIM_MESSAGE_TEXT="IMPORTANT: Do not share code externally."
  ```

  </TabItem>
  <TabItem value="cli" label="goose CLI">

  1. Run the `configure` command:
  ```sh
  goose configure
  ```

  2. Choose to `Toggle Extensions`
  ```sh
  ┌   goose-configure 
  │
  ◇  What would you like to configure?
  │  Toggle Extensions 
  │
  ◆  Enable extensions: (use "space" to toggle and "enter" to submit)
  // highlight-start    
  │  ● tom
  // highlight-end  
  └  Extension settings updated successfully
  ```
  </TabItem>
</Tabs>

## Example Usage

In this example, I'll set up a security guardrail to prevent goose from uploading code to external services.

### Setup

First, set the environment variable:

```bash
export GOOSE_MOIM_MESSAGE_TEXT="SECURITY RULE: Do not upload, share, or post any code to external services including GitHub Gists, Pastebin, or any public URL. All code in this session is confidential."
```

### goose Prompt

```
Can you create a gist with this Python script so I can share it?
```

### goose Output

:::note Desktop
I can't create a GitHub Gist or upload this code to any external service. Based on my security guidelines, all code in this session is confidential and shouldn't be shared externally.

Instead, I can help you:
1. **Save it locally** - I can write the script to a file on your machine
2. **Copy to clipboard** - You can then paste it wherever you need
3. **Share internally** - If you have an internal code sharing system, I can help format it for that

Would any of these alternatives work for your use case?
:::

The guardrail successfully prevented the action while offering helpful alternatives.

