---
title: fedit Extension
description: Add fedit MCP Server as a goose Extension for Surgical File Editing
---

import Tabs from '@theme/Tabs';
import TabItem from '@theme/TabItem';
import GooseDesktopInstaller from '@site/src/components/GooseDesktopInstaller';
import CLIExtensionInstructions from '@site/src/components/CLIExtensionInstructions';

This tutorial covers how to add fedit as a goose extension for surgical file
editing, enabling precise line-, content-, and block-anchored edits (insert,
delete, replace, move, copy) without full-file rewrites.

:::tip Quick Install

<Tabs groupId="interface">
  <TabItem value="ui" label="goose Desktop" default>
  [Launch the installer](goose://extension?cmd=fedit&arg=mcp&id=fedit&name=fedit&description=Surgical%20file%20editing%20via%2014%20MCP%20tools)
  </TabItem>
  <TabItem value="cli" label="goose CLI">
  **Command**
  ```sh
  fedit mcp
  ```
  </TabItem>
</Tabs>
:::

## Configuration

:::info
fedit is a standalone Go binary, not an npx package — install it first and
make sure it's on your `PATH` before adding the extension:
```sh
go install github.com/amalexico/fedit@latest
```
Requires [Go](https://go.dev/) 1.21+.
:::

<Tabs groupId="interface">
  <TabItem value="ui" label="goose Desktop" default>
  <GooseDesktopInstaller
    extensionId="fedit"
    extensionName="fedit"
    description="Surgical file editing via 14 MCP tools"
    command="fedit"
    args={["mcp"]}
  />
  </TabItem>
  <TabItem value="cli" label="goose CLI">
    <CLIExtensionInstructions
      name="fedit"
      description="Surgical file editing via 14 MCP tools"
      command="fedit mcp"
    />
  </TabItem>
</Tabs>

## What fedit adds

fedit exposes 14 tools for targeted file edits instead of full-file rewrites:
`fedit_show`, `fedit_find`, `fedit_map`, `fedit_insert`, `fedit_insertafter`,
`fedit_insertbefore`, `fedit_replace`, `fedit_replaceall`, `fedit_delete`,
`fedit_write`, `fedit_writeraw`, `fedit_move`, `fedit_copy`, `fedit_fields`.

Highlights:
- **Block-aware editing** — target a named function, class, or Terraform
  resource by name instead of computing line numbers (Go, Python, JS/TS,
  Rust, Java, C#, Ruby, PHP, HCL/Terraform, Nix).
- **Content-anchored ranges** — `-match`/`-endmatch` bound an edit to text
  markers instead of fixed line numbers, so edits survive earlier changes
  shifting line counts.
- **Streaming mode** — line-by-line I/O for multi-GB files on find/replaceall.
- **Structural overview** — `fedit_map` gives a symbol-level outline of a
  file across 17 languages before you edit it.

## Example Usage

### goose Prompt

```
Use fedit to show me main.go with line numbers, then replace the
handleLogin function body with an updated version.
```

### goose Output

```
I'll use fedit to inspect the file structure first, then make the
targeted replacement.

[fedit_map file=main.go lang=go]
[fedit_replace file=main.go block=handleLogin lang=go text="..."]

Replaced the handleLogin function body (lines 142-168) in main.go.
```

Repo: [github.com/amalexico/fedit](https://github.com/amalexico/fedit) (MIT)
