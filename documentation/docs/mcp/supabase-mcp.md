---
title: Supabase Extension
description: Add Supabase MCP Server as a goose Extension
---

import Tabs from '@theme/Tabs';
import TabItem from '@theme/TabItem';
import CLIExtensionInstructions from '@site/src/components/CLIExtensionInstructions';
import GooseDesktopInstaller from '@site/src/components/GooseDesktopInstaller';

This tutorial covers how to add the [Supabase MCP Server](https://github.com/supabase-community/supabase-mcp) as a goose extension to interact with your Supabase projects, manage tables, query data, deploy Edge Functions, and interact with your Supabase backend directly.

:::tip Quick Install
<Tabs groupId="interface">
  <TabItem value="ui" label="goose Desktop" default>
  [Launch the installer](goose://extension?type=streamable_http&url=https%3A%2F%2Fmcp.supabase.com%2Fmcp&id=supabase&name=Supabase&description=Connect%20your%20Supabase%20projects%20to%20AI%20assistants.%20Manage%20tables%2C%20query%20data%2C%20deploy%20Edge%20Functions%2C%20and%20interact%20with%20your%20Supabase%20backend%20directly%20from%20your%20MCP%20client.)
  </TabItem>
  <TabItem value="cli" label="goose CLI">
  Use `goose configure` to add a `Remote Extension (Streamable HTTP)` extension type with:
  
  **Endpoint URL**
  ```
  https://mcp.supabase.com/mcp
  ```
  </TabItem>
</Tabs>
:::

## Configuration

### Supabase Remote MCP

The Supabase MCP Server is hosted by Supabase and provides OAuth-based authentication. When you connect, you'll be redirected to supabase.com to sign in to your account.

<Tabs groupId="interface">
  <TabItem value="ui" label="goose Desktop" default>

    <GooseDesktopInstaller
      extensionId="supabase"
      extensionName="Supabase"
      description="Connect your Supabase projects to AI assistants. Manage tables, query data, deploy Edge Functions, and interact with your Supabase backend directly from your MCP client."
      type="http"
      url="https://mcp.supabase.com/mcp"
      envVars={[]}
    />

  </TabItem>
  <TabItem value="cli" label="goose CLI">

    <CLIExtensionInstructions
      name="Supabase"
      description="Connect your Supabase projects to AI assistants. Manage tables, query data, deploy Edge Functions, and interact with your Supabase backend directly from your MCP client."
      type="http"
      url="https://mcp.supabase.com/mcp"
      timeout={300}
      envVars={[]}
    />
    
  </TabItem>
</Tabs>

#### OAuth Flow

When you first connect to the Supabase MCP Server through goose, an OAuth window will open in your browser. Follow the prompts to authorize access to your Supabase projects.

:::note Security Note
The Supabase MCP Server grants access to your database and project management capabilities. Always review LLM-requested actions before execution and ensure you're working in the appropriate environment (development, staging, production).
:::

## Example Usage

### Example 1: List Your Supabase Projects

Get an overview of all your Supabase projects and their configurations.

#### goose Prompt

```
List all my Supabase projects
```

#### goose Output

:::note Desktop

```
Here are your Supabase projects:

1. my project (ID: abc123xyz789defg)
   - Region: US West 1
   - Created: October 30, 2025
   - Status: ACTIVE_HEALTHY
   - Database: PostgreSQL 17.6.1.031 (Engine v17)
   - Host: db.abc123xyz789defg.supabase.co

2. new project (ID: jkl456mno012pqrs)
   - Region: US West 1
   - Created: October 31, 2025
   - Status: INACTIVE
   - Database: PostgreSQL 17.6.1.032 (Engine v17)
   - Host: db.jkl456mno012pqrs.supabase.co

Would you like more detailed information about any specific project?
```

:::

### Example 2: Query Database Tables

Discover and query tables within your Supabase project.

#### goose Prompt

```
What tables exist in my project?
```

#### goose Output

:::note Desktop

```
Tables in my project (public schema):

1. users
   - RLS Enabled: Yes
   - Row count: 150
   - Columns: id (uuid), email (text), name (text), created_at (timestamptz), role (text)
   - Primary Key: id
   - Foreign Keys: Referenced by posts.user_id, profiles.user_id

2. posts
   - RLS Enabled: Yes
   - Row count: 342
   - Columns: id (uuid), user_id (uuid), title (text), content (text), created_at (timestamptz)
   - Primary Key: id
   - Foreign Keys: References users.id, referenced by comments.post_id

3. comments
   - RLS Enabled: Yes
   - Row count: 1089
   - Columns: id (uuid), post_id (uuid), user_id (uuid), content (text), created_at (timestamptz)
   - Primary Key: id
   - Foreign Keys: References posts.id, references users.id

4. profiles
   - RLS Enabled: Yes
   - Row count: 150
   - Columns: id (uuid), user_id (uuid), bio (text), avatar_url (text), updated_at (timestamptz)
   - Primary Key: id
   - Foreign Keys: References users.id

5. categories
   - RLS Enabled: No
   - Row count: 12
   - Columns: id (uuid), name (text), description (text), created_at (timestamptz)
   - Primary Key: id

Would you like to query data from any of these tables?
```

:::

### Example 3: Deploy an Edge Function

Deploy and manage Supabase Edge Functions directly through goose.

#### goose Prompt

```
Deploy a new Edge Function called 'hello-world' that returns a simple JSON response
```

#### goose Output

:::note Desktop

```
Edge Function deployed successfully!

Function Name: hello-world
Version: 1
Status: ACTIVE
Function ID: c4e03748-3e72-45e2-bdca-03f7e4800874
Project: my project (tabowbdrhvtlvzvfyinw)
Entrypoint: index.ts
JWT Verification: Enabled
Created: November 8, 2025

Function code:
~~~typescript
Deno.serve(async (req) => {
  return new Response(
    JSON.stringify({ message: "Hello World!" }),
    { headers: { "Content-Type": "application/json" } },
  )
})
~~~

Endpoint:
https://tabowbdrhvtlvzvfyinw.supabase.co/functions/v1/hello-world

You can test the function by sending a request to the endpoint above.
```

:::

## Additional Resources

For more information about the Supabase MCP Server and its capabilities, visit:
- [Supabase MCP Server GitHub Repository](https://github.com/supabase-community/supabase-mcp)
- [Supabase Documentation](https://supabase.com/docs)
- [Supabase Dashboard](https://supabase.com/dashboard)
