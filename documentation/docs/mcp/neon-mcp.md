---
title: Neon Extension
description: Add Neon MCP Server as a goose Extension
---

import Tabs from '@theme/Tabs';
import TabItem from '@theme/TabItem';
import CLIExtensionInstructions from '@site/src/components/CLIExtensionInstructions';
import GooseDesktopInstaller from '@site/src/components/GooseDesktopInstaller';
import { PanelLeft } from 'lucide-react';

This tutorial covers how to add the [Neon MCP Server](https://github.com/neondatabase/mcp-server-neon) as a goose extension to interact with your Neon Postgres databases and manage your projects, branches, and more.

Neon offers two versions of the MCP server:

1. **Remote MCP server** hosted by Neon, which redirects you to neon.com to sign in to your Neon account.
2. **Local MCP server** that you can run on your own machine using an API key to connect with a specific organization or your personal account.

:::warning Security Note
The Neon MCP Server grants powerful database management capabilities and is intended for local development only - always review LLM-requested actions before execution and avoid using in production environments.
:::

## Configuration

<Tabs groupId="remote-or-local">
  <TabItem value="remote" label="Neon Remote MCP" default>
  :::tip Quick Install
  <Tabs groupId="interface">
    <TabItem value="ui" label="goose Desktop" default>
    [Launch the installer](goose://extension?type=streamable_http&url=https%3A%2F%2Fmcp.neon.tech%2Fmcp&id=neon&name=Neon&description=Manage%20Neon%20Postgres%20databases%2C%20projects%2C%20and%20branches)
    </TabItem>
    <TabItem value="cli" label="goose CLI">
    Use `goose configure` to add a `Remote Extension (Streamable HTTP)` extension type with:

    **Endpoint URL**
    ```
    https://mcp.neon.tech/mcp
    ```
    </TabItem>
  </Tabs>
  :::

  :::info OAUTH FLOW
  An OAuth window will open in your browser. Follow the prompts to authorize access to your Neon account.
  :::

  <Tabs groupId="interface">
    <TabItem value="ui" label="goose Desktop" default>
      <GooseDesktopInstaller
        extensionId="neon"
        extensionName="Neon"
        description="Manage Neon Postgres databases, projects, and branches"
        type="http"
        url="https://mcp.neon.tech/mcp"
      />
    </TabItem>
    <TabItem value="cli" label="goose CLI">
      <CLIExtensionInstructions
        name="neon-mcp-remote"
        description="Manage Neon Postgres databases, projects, and branches"
        type="http"
        url="https://mcp.neon.tech/mcp"
        timeout={300}
      />
      </TabItem>
  </Tabs>

  </TabItem>

  <TabItem value="local" label="Neon Local MCP">
  :::tip Quick Install
  <Tabs groupId="interface">
    <TabItem value="ui" label="goose Desktop" default>
      [Launch the installer](goose://extension?cmd=npx&arg=-y&arg=%40neondatabase%2Fmcp-server-neon&arg=start&arg=%3CYOUR_NEON_API_KEY%3E&id=neon&name=Neon&description=Manage%20your%20Neon%20Postgres%20databases%2C%20projects%2C%20and%20branches)
    </TabItem>
    <TabItem value="cli" label="goose CLI">
      **Command**
      ```sh
      npx -y @neondatabase/mcp-server-neon start <YOUR_NEON_API_KEY>
      ```
      
      No environment variables are needed.
    </TabItem>
  </Tabs>
  :::

  :::info Prerequisites

  - [Node.js](https://nodejs.org/) (v18.0.0 or higher) installed on your system to run this command, as it uses `npx`
  - Neon API key to authenticate the Neon MCP Server with your Neon account. You can create one from the [Neon Console](https://console.neon.tech) under your Profile settings. Refer to the [Neon documentation on API Keys](https://neon.com/docs/manage/api-keys#creating-api-keys) for detailed instructions.
  :::

  <Tabs groupId="interface">
    <TabItem value="ui" label="goose Desktop" default>
    <GooseDesktopInstaller
      extensionId="neon"
      extensionName="Neon"
      description="Manage your Neon Postgres databases, projects, and branches"
      command="npx"
      args={["-y", "@neondatabase/mcp-server-neon", "start", "<YOUR_NEON_API_KEY>"]}
    />
    </TabItem>

    <TabItem value="cli" label="goose CLI">
      <CLIExtensionInstructions
        name="Neon MCP"
        description="Manage Neon Postgres databases, projects, and branches"
        command="npx -y @neondatabase/mcp-server-neon start <YOUR_NEON_API_KEY>"
        timeout={300}
      />
    </TabItem>
  </Tabs>

  </TabItem>
</Tabs>

For all setup and configuration options, see the [official Neon MCP Server documentation](https://neon.com/docs/ai/neon-mcp-server).

## Example Usage 1

Get an overview of all your Neon database projects and their configurations.

### goose Prompt

```
List my Neon projects
```

### goose Output

```
I'll help you list your Neon projects using the neon__list_projects tool. By default, it will show the first 10 projects.

Here are your Neon projects:

1. laravel-test (ID: mute-mountain-08595850)
- Region: AWS US East 2
- Created: July 23, 2025
- PostgreSQL: v17

2. rls-test (ID: noisy-hill-99207787)
- Region: AWS US East 2
- Created: July 20, 2025
- PostgreSQL: v17

3. neon-auth-test (ID: rapid-pond-81053507)
- Region: Azure East US 2
- Created: July 7, 2025
- PostgreSQL: v17

4. saas-starter-kit-nextjs (ID: misty-poetry-78149700)
- Region: AWS US West 2
- Created: June 23, 2025
- PostgreSQL: v17
- Note: Has logical replication enabled

5. neon-auth-mcp-oauth (ID: gentle-base-21700263)
- Region: AWS US West 2
- Created: June 4, 2025
- PostgreSQL: v17

All projects are running PostgreSQL 17 and have autoscaling configured between 1-2 compute units. Would you like more detailed information about any specific project?
```

## Example Usage 2

Discover what tables and schemas exist within a specific project.

### goose Prompt

```
What tables do I have in my neon-auth-mcp-oauth project?
```

### goose Output

```
In your neon-auth-mcp-oauth project, there is one table:

users_sync in the neon_auth schema
```

## Example Usage 3

Run queries against your database tables to retrieve and analyze your data.

### goose Prompt

```
Count the rows in the users_sync table.
```

### goose Output

```
I'll count the rows in the neon_auth.users_sync table using a COUNT query.

Total Records: 1 row
Table Location: Schema neon_auth, Table users_sync
Project: neon-auth-mcp-oauth (Project ID: gentle-base-21700263)
Content: Contains one user record
```
