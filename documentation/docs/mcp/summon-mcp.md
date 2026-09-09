---
title: Summon Extension
description: Load sources and delegate tasks to subagents
---

import Tabs from '@theme/Tabs';
import TabItem from '@theme/TabItem';
import { PlatformExtensionNote } from '@site/src/components/PlatformExtensionNote';
import GooseBuiltinInstaller from '@site/src/components/GooseBuiltinInstaller';

The Summon extension lets you load reusable task sources into goose's context and delegate work to [subagents](/docs/guides/context-engineering/subagents).

You can load different types of sources:
- [**Recipes**](/docs/guides/recipes) - Automated task definitions with prompts and parameters
- **Agents** - Reusable agent definitions stored in agent directories
- **Subrecipes** - Recipe-local tasks available from the current recipe

This is useful when you want goose to reuse a task definition, hand work to another agent, or run read-only research in parallel. Skills are loaded by the separate [Skills platform extension](/docs/guides/context-engineering/using-skills).

:::info
This extension is available in v1.25.0+.
:::

## Configuration

<PlatformExtensionNote/>

<Tabs groupId="interface">
  <TabItem value="ui" label="goose Desktop" default>
  <GooseBuiltinInstaller
    extensionName="Summon"
    description="Load sources and delegate tasks to subagents"
  />
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
  │  ● summon
  // highlight-end  
  └  Extension settings updated successfully
  ```
  </TabItem>
</Tabs>

## Example Usage

In this example, we'll create a reusable recipe and use Summon to delegate it to a subagent.

### Create a Recipe

```yaml title=".agents/recipes/release-notes.yaml"
title: Release Notes
description: Draft release notes from recent git changes
instructions: |
  Review the recent git history and changed files.
  Write concise release notes with:
  - user-facing changes
  - fixes
  - migration notes, if any
prompt: Draft release notes for the current branch.
```

### goose Prompt

```
Use summon to delegate the release-notes recipe for this branch.
```

### goose Output

```
─── delegate | summon ───────────────────────────────────────
source: release-notes

The release-notes subagent reviewed the branch and drafted release notes:

## User-facing changes
- Added support for configuring project-specific extensions.
- Improved error messages when extension startup fails.

## Fixes
- Fixed stale extension state after disabling an extension.
```

## Common Summon Commands

Ask goose to use Summon in natural language, or call the tools directly:

```text
load()
load(source: "release-notes")
delegate(source: "release-notes")
delegate(instructions: "Review these docs and report stale links")
delegate(source: "release-notes", async: true)
load(source: "20260219_1", peek: true)
load(source: "20260219_1")
```

Calling `load()` with no arguments lists available sources. For background tasks, `delegate(..., async: true)` returns a task id, and `load(source: "<task_id>")` waits for the result.
