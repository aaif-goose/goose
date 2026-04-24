---
title: How I Manage Localhost Port Conflicts With an AI Agent
description: Learn how I use Goose, an open source AI agent and MCP client, to manage conflicting ports without breaking my flow.
authors: 
    - rizel
---

![blog cover](hoarders.png)

# How I Manage Localhost Port Conflicts With an AI Agent

## Localhost Ports Hoarding

I'm perpetually drowning in open tabs. Yes, I do need Bluesky, ChatGPT, Claude, Goose, Cursor, Discord, Slack, Netflix, and Google Docs all open at the same time. I've learned that tab management isn't my only vice.

> "Hi, my name is Rizel, and I'm a localhost ports hoarder. 👋🏿"

<!-- truncate -->

It always starts innocently with me running one project that uses localhost:3000. Then a teammate pings me to review their PR. Their project also runs on localhost:3000. Fortunately, modern frameworks are smart enough to automatically bump to ports 3001 or 3002\. Before I know it, 3003, 3004, and 3005 are running too, and my laptop sounds like it’s about to take off as it crawls under the weight of too many processes.

## Traditional Ports Management

Typically, freeing up ports involves running the following commands:

```shell
# Step 1: list all open ports
lsof -i -P | grep LISTEN

# Step 2: find a specific port
lsof -i :3000

# Step 3: kill the process
kill -9 98026
```

Because I don't do this often, I often end up searching for the commands, which derails what I was initially working on.

## Using Goose as My Port Manager

To avoid that interruption, I started offloading the task to [Goose](/), an open source AI agent.

Goose uses its built-in [Developer](/docs/mcp/developer-mcp) MCP server to handle coding-related tasks on my machine. Here's what the interaction looks like:

**Prompt to Goose:** 

```
Can you tell me which ports I have running?
```

**Goose Response:**

```
Development Servers:
- Node.js servers on ports 3000, 3001, 3002
- Port 18030

Database:
- PostgreSQL on port 5432

Development Tools:
- Ollama on port 11434
- Multiple Goose instances (ports 49377, 49899, 63594, 64365)

Applications:
- Okta Verify (port 8769)
- inSync (ports 7010, 49831, 49830)
- Pieces (ports 39300, 62638)
- Rapportd macOS service (port 60151)
```

**Prompt to Goose:** 

```
Kill all the Node.js servers running on localhost.
```

**Goose Response:**

```
The command executed successfully, freeing up ports 3000, 3001, and 3002.
```

## Why Use AI for Simple Tasks

Freeing up ports is something I know how to do. But I use AI tools like Goose for small tasks that create friction and slow me down. I'm not trying to cram more work into the day. My goal is to work efficiently, make a positive impact, and still have time for my personal life.

<head>
  <meta property="og:title" content="How I Manage Localhost Port Conflicts With an AI Agent" />
  <meta property="og:type" content="article" />
  <meta property="og:url" content="https://goose-docs.ai/blog/2025/05/22/manage-local-host-conflicts-with-goose" />
  <meta property="og:description" content="Learn how I use Goose, an open source AI agent, to manage conflicting ports without breaking my flow." />
  <meta property="og:image" content="https://goose-docs.ai/assets/images/hoarders-753809f09399a9e4f734006a8d74218d.png" />
  <meta name="twitter:card" content="summary_large_image" />
  <meta property="twitter:domain" content="goose-docs.ai" />
  <meta name="twitter:title" content="How I Manage Localhost Port Conflicts With an AI Agent" />
  <meta name="twitter:description" content="Learn how I use Goose, an open source AI agent, to manage conflicting ports without breaking my flow." />
  <meta name="twitter:image" content="https://goose-docs.ai/assets/images/hoarders-753809f09399a9e4f734006a8d74218d.png" />
</head>