---
title: Skills Extension
description: Load reusable instruction sets that teach goose specific workflows
---

import Tabs from '@theme/Tabs';
import TabItem from '@theme/TabItem';
import { PlatformExtensionNote } from '@site/src/components/PlatformExtensionNote';
import GooseBuiltinInstaller from '@site/src/components/GooseBuiltinInstaller';

:::caution Deprecated
This extension has been deprecated and is only available in v1.16.0 - v1.24.0. For v1.25.0+, see the [Summon extension](/docs/mcp/summon-mcp).
:::

The Skills extension loads *skills* &mdash; reusable sets of instructions that teach goose how to perform specific tasks or follow particular workflows.

goose automatically discovers skills at startup and uses them when relevant to your request. goose loads skills from `.agents/skills/` in your project directory and `~/.agents/skills/` globally, making skills portable across different AI coding agents. To learn about creating skills and how goose uses them, see [Using Skills](/docs/guides/context-engineering/using-skills).

## Configuration

<PlatformExtensionNote/>

<Tabs groupId="interface">
  <TabItem value="ui" label="goose Desktop" default>
  <GooseBuiltinInstaller
    extensionName="Skills"
    description="Load and use skills from the .agents/skills directory"
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
  │  ● skills
  // highlight-end  
  └  Extension settings updated successfully
  ```
  </TabItem>
</Tabs>

## Example Usage

Let's say you have a skill that goose discovers on startup in `~/.agents/skills/deploy/SKILL.md`:

```markdown
---
name: production-deploy
description: Safe deployment procedure for production environment
---

# Production Deployment

## Pre-deployment Checklist
1. Ensure all tests pass
2. Check staging environment is stable
3. Notify #deployments channel

## Deployment Steps
1. Create release branch from main
2. Run `npm run build:prod`
3. Deploy to staging first, then production
4. Monitor error rates for 30 minutes
```

### goose Prompt

```
Deploy the current branch to production
```

### goose Output

:::note Desktop
I'll help you deploy the current branch to production. Let me first load the production deployment skill to follow the safe deployment procedure.

I can see you're on the **`my-feature-branch`** branch with some untracked files. Before proceeding with production deployment, I need to follow the safe deployment procedure.

#### ⚠️ Pre-deployment Confirmation Required

According to the production deployment skill, I need to complete the following checklist:

**Pre-deployment Checklist**
1. **Ensure all tests pass** - Have tests been run?
2. **Check staging environment is stable** - Has this been deployed to staging first?
3. **Notify #deployments channel** - Should I notify the team?

**Deployment Steps (once confirmed)**
1. Create release branch from main
2. Run `npm run build:prod`
3. Deploy to staging first, then production
4. Monitor error rates for 30 minutes

---

**Before I proceed, please confirm:**

1. ✅ Have all tests passed for the `my-feature-branch` branch?
2. ✅ Has this been tested on staging?
3. ✅ Should I notify the #deployments channel?
4. ✅ Are you sure you want to deploy the `my-feature-branch` branch (not `main`) to production?

Please confirm these items and I'll proceed with the deployment.
:::

