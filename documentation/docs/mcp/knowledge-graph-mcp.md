---
title: Knowledge Graph Memory Extension
description: Add Knowledge Graph Memory MCP Server as a goose Extension
---

import Tabs from '@theme/Tabs';
import TabItem from '@theme/TabItem';
import YouTubeShortEmbed from '@site/src/components/YouTubeShortEmbed';
import GooseDesktopInstaller from '@site/src/components/GooseDesktopInstaller';
import CLIExtensionInstructions from '@site/src/components/CLIExtensionInstructions';

<YouTubeShortEmbed videoUrl="https://www.youtube.com/embed/PF6hpDaI9Mc" />


This tutorial covers how to add the [Knowledge Graph Memory MCP Server](https://github.com/modelcontextprotocol/servers/tree/main/src/memory) as a goose extension. This enables goose to analyze relationships, detect patterns, and gain a deeper understanding of your data. The knowledge graph builds on the [memory extension](/docs/mcp/memory-mcp) by mapping complex relationships between concepts and providing persistent memory across goose sessions.

:::tip Quick Install
<Tabs groupId="interface">
  <TabItem value="ui" label="goose Desktop" default>
  [Launch the installer](goose://extension?cmd=npx&arg=-y&arg=%40modelcontextprotocol%2Fserver-memory&id=knowledge_graph_memory&name=Knowledge%20Graph%20Memory&description=Graph-based%20memory%20system%20for%20persistent%20knowledge%20storage)
  </TabItem>
  <TabItem value="cli" label="goose CLI">
  **Command**
  ```sh
  npx -y @modelcontextprotocol/server-memory
  ```
  </TabItem>
</Tabs>
:::

## Configuration

:::info
Note that you'll need [Node.js](https://nodejs.org/) installed on your system to run this command, as it uses `npx`.
:::

<Tabs groupId="interface">
  <TabItem value="ui" label="goose Desktop" default>
  <GooseDesktopInstaller
    extensionId="knowledge_graph_memory"
    extensionName="Knowledge Graph Memory"
    description="Graph-based memory system for persistent knowledge storage"
    command="npx"
    args={["-y", "@modelcontextprotocol/server-memory"]}
  />
  </TabItem>
  <TabItem value="cli" label="goose CLI">
    <CLIExtensionInstructions
      name="Knowledge Graph Memory"
      description="Graph-based memory system for persistent knowledge storage"
      command="npx -y @modelcontextprotocol/server-memory"
    />
  </TabItem>
</Tabs>

## Example Usage

In this example, I'll show you how goose can become an intelligent security reviewer by using connected knowledge patterns to detect and analyze vulnerabilities. goose will be able to understand the relationship between security issues, their impacts, and mitigations.

This means goose doesn't just spot issues - it understands how vulnerabilities connect to real world impacts and can suggest comprehensive solutions.

### Step 1: Teach goose About Security Patterns

#### goose Prompt #1

```text
goose, learn these security vulnerability patterns and their relationships:

1. SQL Injection relates to:

   - Unvalidated database inputs
   - Data theft risks
   - Parameterized query solutions

2. XSS Vulnerabilities connect to:

   - Unescaped user output
   - Session hijacking risks
   - Content sanitization fixes

3. Authentication Weaknesses link to:

   - Session management
   - Account compromise
   - JWT-based solutions
```

#### goose Output

```text
I'll store this security knowledge in the Knowledge Graph Memory with proper entities and relationships

[.... Create Entities .....]

Now, let's create the relationships between these security concepts:

[.... Create Relations .....]

Let me verify what's been stored:

[... Read Graph ...]

I've now stored your security knowledge in the graph with:

1. Vulnerability Entities:

   - SQL Injection (with observations about unvalidated inputs and data theft)
   - XSS Vulnerabilities (with observations about unescaped output and session risks)
   - Authentication Weaknesses (with observations about session management)

2. Solution Entities:

   - Parameterized Queries
   - Content Sanitization
   - JWT Authentication

3. Connected Relationships:

   - Vulnerabilities -> their potential impacts
   - Solutions -> the vulnerabilities they mitigate

The knowledge is now structured in a way that I can:

   - Trace vulnerability impact chains
   - Identify appropriate solutions for each vulnerability
   - Understand how different security issues are related
```

### Step 2:  Use Connected Knowledge for Code Review

#### goose Prompt #2

Review this API endpoint for security issues:

```js
app.post('/api/login', (req, res) => {
    const { username, password } = req.body;
    const query = `SELECT * FROM users WHERE username = '${username}'`;
    db.query(query, (err, result) => {
        if (err) throw err;
        if (result.length > 0) {
            res.cookie('user', username);
            res.json({ success: true });
        }
    });
});
```

#### goose Output

```text
I'll analyze this code using our connected knowledge graph to trace security vulnerabilities and their implications:

[.... Search Nodes ... Open Nodes ...]

Based on our knowledge graph analysis, I've found multiple critical security issues:

1. SQL Injection Vulnerability:
   - Issue: Using string concatenation in query WHERE username = '${username}'
   - Impact Chain: SQL Injection -> Data Theft
   - Risk Level: Critical
   - Solution (from parameterized_queries): Use parameterized queries instead

2. Authentication Weakness:
   - Issue: Using basic cookie-based session res.cookie('user', username)
   - Impact Chain: Auth Weakness -> Account Compromise
   - Risk Level: High
   - Solution (from jwt_auth): Implement JWT with proper expiration
```

Here's the secure implementation based on our stored patterns:

```js
app.post('/api/login', async (req, res) => {
    try {
        if (!req.body.username || !req.body.password) {
            return res.status(400).json({
                success: false,
                error: 'Username and password required'
            });
        }

        const query = 'SELECT * FROM users WHERE username = ?';
        const result = await db.query(query, [req.body.username]);

        if (!result.length) {
            return res.status(401).json({
                success: false,
                error: 'Invalid credentials'
            });
        }

        const token = jwt.sign(
            { userId: result[0].id },
            process.env.JWT_SECRET,
            { expiresIn: '1h' }
        );

        res.json({
            success: true,
            token: token
        });
    } catch (error) {
        res.status(500).json({
            success: false,
            error: 'Internal server error'
        });
    }
});
```

```text
The fixes address the vulnerability chains our knowledge graph identified:

1. SQL Injection -> Data Theft: Mitigated by parameterized queries
2. Auth Weakness -> Account Compromise: Mitigated by JWT implementation

Additional security improvements based on connected patterns:

- Added input validation
- Proper error handling
- No exposure of internal errors
- Token-based instead of cookie-based auth
- Expiration time on authentication

Would you like me to:

1. Explain any of these security relationships in more detail?
2. Show how to implement additional security patterns?
3. Analyze another endpoint?
```
