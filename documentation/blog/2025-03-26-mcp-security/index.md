---
title: "How to Determine If An MCP Server Is Safe"
description: Before you plug your agent into just any MCP server, here's how to check if it's actually safe.
authors: 
    - ebony
---

![blog cover](mcpsafety.png)

# How I Vet MCP Servers Before Plugging Them In

[Model Context Protocol (MCP)](https://www.anthropic.com/news/model-context-protocol) servers are everywhere right now. Last time I checked there were **3,000 and counting**. Every day, a new one pops up, letting AI agents like Goose access files, query your Google Drive, search the web, and unlock all kinds of amazing integrations.

<!--truncate-->

And just when I thought things couldn’t get any crazier, Zapier blessed us with an MCP server. That means your agent can now tap into over 8,000+ integrations.

So trust me, I know it’s super tempting to want to plug your AI agent into everything and just _see_ what happens.

But hold on a minute, we can’t afford to skip over security.

When you connect to an MCP server, you’re giving it access to your workflows, most times even your data. And a lot of these servers are community built, with little to no governance.

## Here’s What I Do Before I Trust an MCP Server

Any time I’m checking out a new MCP server to plug into Goose, I start with **[Glama.ai](https://glama.ai/mcp/servers)**.

Glama is an all-in-one AI workspace, and it maintains one of the **most comprehensive and security-aware MCP server directories** that I've seen. The servers listed are either community built or created by the actual companies behind the tools, like **Azure** or **JetBrains**.

Each server gets a **report card**, so at a glance you can quickly assess whether it’s solid or a little sketchy.

## What Glama Scores

Here’s what Glama grades servers on:

- ✅ **Security** – Checks for known vulnerabilities in the server or its dependencies  
- ✅ **License** – Confirms it’s using a permissive open source license  
- ✅ **Quality** – Indicates whether the server is running and functions as expected

You’ll also see helpful context like how many tools the server exposes, whether it has a README file, when it was last updated, and whether it supports live previews through the MCP inspector tool.

Glama doesn't just perform these checks once, they **revaluate servers regularly**, so if something breaks or a vulnerability gets introduced, the score updates automatically.

Here’s an example of a solid server: the **YouTube MCP server**, which lets Goose download and process videos to create summaries and transcripts.

![YouTube MCP Score](youtubeMcp.png)

>_All A’s across the board—**security, license, and quality**._  

That’s exactly the kind of score I look for before I plug Goose into any server.

So please, **check before you connect**.

A quick glance at an MCP directory like Glama can save you from crying on your office floor later. However, once you’ve done your homework?

**Have fun. Plug your agent in. Break things (safely). And vibe code with peace of mind.**

<head>
  <meta property="og:title" content="How to Determine If An MCP Server Is Safe" />
  <meta property="og:type" content="article" />
  <meta property="og:url" content="https://goose-docs.ai/blog/2025/03/26/mcp-security" />
  <meta property="og:description" content="Before you plug your AI agent into just any MCP server, here's how to check if it's actually safe." />
  <meta property="og:image" content="http://goose-docs.ai/assets/images/mcpsafety-87eb7ace7163a5edbe068ff75b79a199.png" />
  <meta name="twitter:card" content="summary_large_image" />
  <meta property="twitter:domain" content="goose-docs.ai" />
  <meta name="twitter:title" content="How to Determine If An MCP Server Is Safe" />
  <meta name="twitter:description" content="Before you plug your agent into just any MCP server, here's how to check if it's actually safe." />
  <meta name="twitter:image" content="http://goose-docs.ai/assets/images/mcpsafety-87eb7ace7163a5edbe068ff75b79a199.png" />
</head>