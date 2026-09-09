---
title: Asana Extension
description: Add Asana MCP Server as a goose Extension
---

import Tabs from '@theme/Tabs';
import TabItem from '@theme/TabItem';
import YouTubeShortEmbed from '@site/src/components/YouTubeShortEmbed';
import GooseDesktopInstaller from '@site/src/components/GooseDesktopInstaller';
import CLIExtensionInstructions from '@site/src/components/CLIExtensionInstructions';

<!--<YouTubeShortEmbed videoUrl="https://www.youtube.com/embed/VIDEO_ID" />-->


This tutorial covers how to add the [Asana MCP Server](https://github.com/roychri/mcp-server-asana) as a goose extension to enable task automation, project tracking, and team collaboration.

:::tip Quick Install
<Tabs groupId="interface">
  <TabItem value="ui" label="goose Desktop" default>
  [Launch the installer](goose://extension?cmd=npx&arg=-y&arg=%40roychri%2Fmcp-server-asana&id=asana-mcp&name=Asana&description=enable%20task%20automation%2C%20project%20tracking%2C%20and%20team%20collaboration&env=ASANA_ACCESS_TOKEN%3DAsana%20Access%20Token)
  </TabItem>
  <TabItem value="cli" label="goose CLI">
  **Command**
  ```sh
  npx -y @roychri/mcp-server-asana
  ```
  </TabItem>
</Tabs>
  **Environment Variable**
  ```
  ASANA_ACCESS_TOKEN: <YOUR_TOKEN>
  ```
:::

## Configuration

:::info
Note that you'll need [Node.js](https://nodejs.org/) installed on your system to run this command, as it uses `npx`.
:::


<Tabs groupId="interface">
  <TabItem value="ui" label="goose Desktop" default>
  <GooseDesktopInstaller
    extensionId="asana-mcp"
    extensionName="Asana"
    description="Enable task automation, project tracking, and team collaboration"
    command="npx"
    args={["-y", "@roychri/mcp-server-asana"]}
    envVars={[
      { name: "ASANA_ACCESS_TOKEN", label: "Asana Access Token" }
    ]}
    apiKeyLink="https://app.asana.com/0/my-apps"
    apiKeyLinkText="Asana Access Token"
  />
  :::info
  See [Asana's developer docs](https://developers.asana.com/docs/personal-access-token) if you need detailed instructions on creating an access token.
  :::
  </TabItem>
  <TabItem value="cli" label="goose CLI">
    <CLIExtensionInstructions
      name="Asana"
      description="Enable task automation, project tracking, and team collaboration"
      command="npx -y @roychri/mcp-server-asana"
      envVars={[
        { key: "ASANA_ACCESS_TOKEN", value: "▪▪▪▪▪▪▪▪▪▪▪▪▪▪▪▪▪▪▪▪▪▪▪▪▪▪▪▪▪▪▪▪▪▪▪▪▪▪▪" }
      ]}
      infoNote={
        <>
          Obtain your <a href="https://app.asana.com/0/my-apps" target="_blank" rel="noopener noreferrer">Asana Access Token</a> and paste it in. See <a href="https://developers.asana.com/docs/personal-access-token" target="_blank" rel="noopener noreferrer">Asana's developer docs</a> if you need detailed instructions on creating an access token.
        </>
      }
    />
  </TabItem>
</Tabs>

## Example Usage

### goose Prompt

> _goose, I have one hour. Look through uncompleted tasks assigned to me in Asana and show me ones that you estimate will take an hour or less. Order them by deadline._


### goose Output

Note that IDs have been changed to generic ones

:::note CLI

<details>
    <summary>Tool Calls</summary>

    ─── asana_search_tasks | abcdefgh ──────────────────────────

    assignee.any: me

    completed: false

    workspace: 1234567890123



    ─── asana_get_task | abcdefgh ──────────────────────────

    opt_fields: due_on,estimated_hours

    task_id: 1100000000000001


    ─── asana_get_task | abcdefgh ──────────────────────────

    opt_fields: due_on,estimated_hours

    task_id: 1100000000000002


    ─── asana_get_task | abcdefgh ──────────────────────────

    opt_fields: due_on,estimated_hours

    task_id: 1100000000000003


    ─── asana_get_task | abcdefgh ──────────────────────────

    opt_fields: due_on,estimated_hours

    task_id: 1100000000000004


    ─── asana_get_task | abcdefgh ──────────────────────────

    opt_fields: due_on,estimated_hours

    task_id: 1100000000000005


    ─── asana_get_task | abcdefgh ──────────────────────────

    opt_fields: due_on,estimated_hours

    task_id: 1100000000000006


    ─── asana_get_task | abcdefgh ──────────────────────────

    opt_fields: due_on,estimated_hours

    task_id: 1100000000000007


    ─── asana_get_task | abcdefgh ──────────────────────────

    opt_fields: due_on,estimated_hours

    task_id: 1100000000000008


    ─── asana_get_task | abcdefgh ──────────────────────────

    opt_fields: due_on,estimated_hours

    task_id: 1100000000000009


    ─── asana_get_task | abcdefgh ──────────────────────────

    opt_fields: due_on,estimated_hours

    task_id: 1100000000000010
</details>



Here are your uncompleted tasks in Asana estimated to take an hour or less, ordered by their deadline:

1. **Task:** Review team meeting notes
   - **Due Date:** 2025-03-21

2. **Task:** Draft quick status update
   - **Due Date:** 2025-03-21

3. **Task:** Proofread blog post
   - **Due Date:** 2025-03-21

4. **Task:** Send reminder to partners
   - **Due Date:** 2025-03-27

Please let me know if you'd like more information or help with any specific task!
:::
