---
draft: false
title: "Connecting AI Agents to Your Systems with MCP"
date: 2024-12-10
authors:
  - angie
---

![mcp](goose-mcp.png)

Open standards are a critical ingredient for interoperable systems. They have enabled most of the technologies that we all rely on. The ability to connect to the internet no matter where we are relies on open standards such as Wi-Fi, TCP/IP and DNS. When you receive an email in your Gmail account from an Outlook sender, it's the use of open standards like SMTP, IMAP, and POP3 that makes this seamless. One of the most transformative technologies of our lifetime - the internet - enables anyone to have their web page accessible to the entire world thanks to the HTTP and HTML standards.

We're in the early days of a new era in tech, one where companies are innovating and building practical AI solutions for the masses. To ensure the longevity of this technology, open standards will be essential in guiding the development of AI tools so that the diverse systems built by various companies can work together seamlessly.

<!-- truncate -->

:::warning goose Beta Version
This post was written about a beta version of goose and the commands and flow may have changed.
:::

### The MCP Open Standard

Anthropic is leading the charge with the [Model Context Protocol (MCP)](https://modelcontextprotocol.io), an open standard that enables large language model (LLM) applications to connect with external systems, providing the necessary context for more informed and relevant AI interactions. 

This is a game changer for AI agents such as [goose](https://goose-docs.ai/), which can perform tasks autonomously - a significant leap beyond chatbots that only provide step-by-step instructions. However, to unlock the full potential of these AI agents, we need a standard method for connecting them to external data sources. MCP provides this foundation.

With MCP's standardized APIs and endpoints, goose can integrate seamlessly into your systems, enhancing its ability to perform complex tasks like debugging, writing code, and running commands directly in your environment. 

![goose Framework](goose-framework-1.0.png)

### What's Possible

Without MCP, every [goose toolkit](https://goose-docs.ai/plugins/using-toolkits.html) developer would need to implement bespoke integrations with every system they need to connect to. Not only is this tedious and repetitive, but it delays the fun stuff.

Let's take a simple GitHub workflow, for example. goose interacts directly with the GitHub API using custom scripts or configurations. Developers must configure goose to authenticate with GitHub and specify endpoints for actions like fetching open pull requests or adding comments. Each integration requires manual setup and custom coding to handle authentication tokens, error handling, and API updates.

MCP simplifies the process by providing a standardized interface for accessing GitHub as a resource. goose, acting as an [MCP client](https://modelcontextprotocol.io/clients), requests the necessary information (e.g., list of open pull requests) from an [MCP server](https://modelcontextprotocol.io/quickstart#general-architecture) configured to expose GitHub's capabilities. The MCP server handles authentication and communication with GitHub, abstracting away the complexity of API interactions. goose can then focus on tasks like providing a detailed review comment or suggesting code changes.

### Join the Ecosystem

As MCP adoption expands, so does goose’s potential to deliver even more powerful solutions for your organization. By [integrating goose](https://goose-docs.ai/) into your workflows and [embracing MCP](https://modelcontextprotocol.io/introduction), you’re not just enhancing your own systems, you’re contributing to the growth of an ecosystem that makes AI tools more interoperable, efficient, and impactful.



<head>
  <meta charset="UTF-8" />
  <title>Connecting AI Agents to Your Systems with MCP</title>
  <meta name="description" content="goose" />
  <meta name="keywords" content="MCP, Anthropic, AI Open Standards" />


  <!-- HTML Meta Tags -->
  <title>Connecting AI Agents to Your Systems with MCP</title>
  <meta name="description" content="Learn how MCP standardizes integrations and fosters an ecosystem for the future of AI-enabled tools." />

  <!-- Facebook Meta Tags -->
  <meta property="og:url" content="https://goose-docs.ai/blog/2024/12/10/connecting-ai-agents-to-your-systems-with-mcp" />
  <meta property="og:type" content="website" />
  <meta property="og:title" content="Connecting AI Agents to Your Systems with MCP" />
  <meta property="og:description" content="Learn how MCP standardizes integrations and fosters an ecosystem for the future of AI-enabled tools." />
  <meta property="og:image" content="https://goose-docs.ai/assets/images/goose-mcp-34a5252d18d18dff26157d673f7af779.png" />

  <!-- Twitter Meta Tags -->
  <meta name="twitter:card" content="summary_large_image" />
  <meta property="twitter:domain" content="aaif-goose.github.io" />
  <meta property="twitter:url" content="https://goose-docs.ai/blog/2024/12/10/connecting-ai-agents-to-your-systems-with-mcp" />
  <meta name="twitter:title" content="Connecting AI Agents to Your Systems with MCP" />
  <meta name="twitter:description" content="Learn how MCP standardizes integrations and fosters an ecosystem for the future of AI-enabled tools." />
  <meta name="twitter:image" content="https://goose-docs.ai/assets/images/goose-mcp-34a5252d18d18dff26157d673f7af779.png" />
</head>

