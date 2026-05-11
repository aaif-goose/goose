---
title: Skill Audit Extension
description: Add skill-audit-mcp as a goose Extension to scan MCP servers, AI agent skills, and plugins for security vulnerabilities
---

import Tabs from '@theme/Tabs';
import TabItem from '@theme/TabItem';
import CLIExtensionInstructions from '@site/src/components/CLIExtensionInstructions';
import GooseDesktopInstaller from '@site/src/components/GooseDesktopInstaller';

This tutorial covers how to add the [skill-audit-mcp](https://github.com/eltociear/skill-audit-mcp) MCP server as a goose extension, enabling goose to statically scan MCP server code, AI agent skill files, and plugins for **68 attack patterns** across 4 severity levels (CRITICAL/HIGH/MEDIUM/LOW) — credential exfiltration, prompt injection, arbitrary code execution, seed-phrase harvesting, auth bypass, and path traversal.

Output is SARIF 2.1.0, compatible with GitHub Code Scanning. Zero dependencies, Python 3.6+.

:::tip Quick Install
<Tabs groupId="interface">
  <TabItem value="ui" label="goose Desktop" default>
  [Launch the installer](goose://extension?cmd=npx&arg=-y&arg=%40eltociear%2Fskill-audit-mcp&id=skill-audit&name=Skill%20Audit&description=Scan%20MCP%20servers%20and%20AI%20agent%20skills%20for%2068%20attack%20patterns)
  </TabItem>
  <TabItem value="cli" label="goose CLI">
  **Command**
  ```sh
  npx -y @eltociear/skill-audit-mcp
  ```
  </TabItem>
</Tabs>
:::

## Configuration

<Tabs groupId="interface">
  <TabItem value="ui" label="goose Desktop" default>
    <GooseDesktopInstaller
      extensionId="skill-audit"
      extensionName="Skill Audit"
      description="Scan MCP servers, AI agent skills, and plugins for 68 security attack patterns"
      type="stdio"
      command="npx"
      args={["-y", "@eltociear/skill-audit-mcp"]}
      timeout={120}
    />
  </TabItem>

  <TabItem value="cli" label="goose CLI">
    <CLIExtensionInstructions
      name="Skill Audit"
      description="Scan MCP servers, AI agent skills, and plugins for 68 security attack patterns"
      type="stdio"
      command="npx -y @eltociear/skill-audit-mcp"
      timeout={120}
    />
  </TabItem>
</Tabs>

## Example Usage

Ask goose to audit an MCP server or skill directory:

```
audit the security of ./mcp_servers/example/
```

```
scan this skill file for prompt injection and credential exfiltration
```

```
check the package at https://github.com/owner/repo for malicious patterns
```

## Tools

The extension exposes three tools:

- **`audit`** — Scan a single code snippet or text content.
- **`audit_file`** — Scan a local file by path.
- **`audit_directory`** — Recursively scan all files under a directory.

Each tool returns a structured report with attack pattern matches, severity, line numbers, and CWE references where applicable.

## Also Available As

- **GitHub Action**: `uses: eltociear/skill-audit-mcp@v1` — drop-in CI/CD scanner with SARIF upload to the Security tab.
- **Hosted x402 API**: `https://x402.bankr.bot/0x130c617c8f636cad965ed57ca2164ee4e39ac6dd/security-audit` — pay-per-scan via USDC micropayments.
- **Glama listing**: [glama.ai/mcp/servers/@eltociear/skill-audit-mcp](https://glama.ai/mcp/servers/@eltociear/skill-audit-mcp)
