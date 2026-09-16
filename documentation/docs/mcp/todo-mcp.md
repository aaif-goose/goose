---
title: Todo Extension
description: Breaks complex work into trackable steps to help goose stay organized
---

import Tabs from '@theme/Tabs';
import TabItem from '@theme/TabItem';
import { PlatformExtensionNote } from '@site/src/components/PlatformExtensionNote';
import GooseBuiltinInstaller from '@site/src/components/GooseBuiltinInstaller';

The Todo extension helps goose stay organized by breaking complex work into a series of steps and keeping you updated as it completes each step.

The Todo extension is not enabled by default. Once enabled, goose can jot down a short plan for multi-step tasks and update it if the plan changes. The notes are shown to goose every turn and survive context compaction, but goose is not asked to check items off or re-verify finished work.

:::tip
You can ask goose to "show me the current todo list" at any time to see what's being tracked.
:::

This tutorial will cover enabling and using the Todo extension.

## Configuration

<PlatformExtensionNote defaultEnabled={false}/>

<Tabs groupId="interface">
  <TabItem value="ui" label="goose Desktop" default>
  <GooseBuiltinInstaller
    extensionName="Todo"
    description="Manage complex tasks with structured checklists and progress tracking"
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
  │  ● todo
  // highlight-end  
  └  Extension settings updated successfully
  ```
  </TabItem>
</Tabs>

## Example Usage

In this example, I'm going to have goose help me fix formatting inconsistencies in documentation files. This demonstrates how the Todo extension helps goose stay organized when working through systematic improvements.

### goose Prompt

```
I need to update our documentation to fix formatting inconsistencies between two tutorial files. There are several sections that don't match the expected pattern. Can you help me systematically work through these issues and track our progress?
```

### goose Output

```md
I'll help you fix the documentation inconsistencies systematically. Let me break this down into organized tasks and track our progress.

## Fix Documentation Tutorial Inconsistencies

### Missing Components (vs Reference Pattern)
- [x] Add video embed at the top
- [x] Add LLM info box in Example Usage section  
- [x] Remove extra sections not in reference pattern

### Configuration Section Issues
- [x] Change terminology to match reference format
- [x] Fix CLI configuration section format
  - [x] Show exact command flow
  - [x] Include proper menu output with syntax highlighting
  - [x] Remove vague instructions

### Introduction/Description Issues
- [x] Simplify intro paragraph to match reference style
- [x] Fix tutorial description line to match pattern
- [x] Fix capitalization consistency throughout

### Example Structure Issues
- [x] Simplify example to match reference format
  - [x] Use clear "Prompt" and "Output" sections
  - [x] Add timing information
  - [x] Show actual response in proper code blocks
- [x] Use focused, achievable example
- [x] Show clear before/after states

### Formatting Consistency
- [x] Ensure code blocks use proper syntax highlighting
- [x] Match tone and structure of reference
- [x] Use consistent terminology throughout
- [x] Fix capitalization issues

## Documentation Update Complete! ✅

All tutorial inconsistencies have been resolved:

✅ **Structure aligned** - Matches reference pattern exactly
✅ **Components added** - Video embed, info boxes, proper tabs  
✅ **Terminology fixed** - Consistent language throughout
✅ **Example updated** - Simple, focused, with clear timing
✅ **CLI instructions** - Exact command flow with highlighting
✅ **Formatting consistent** - Proper code blocks and syntax

The documentation now follows a consistent pattern and provides a clear, organized experience for users.
```
