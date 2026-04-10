---
title: "goose Lands MCP Apps"
description: "goose ships early support for the draft MCP Apps specification, aligning with the emerging standard for interactive UIs in MCP."
authors:
  - aharvard
image: /img/blog/goose-lands-mcp-apps-header-image.png
---

![Retro 1980s hardware lab with three CRT monitors displaying "goose Lands MCP Apps" in glowing green text, with a small goose figurine on the desk](/img/blog/goose-lands-mcp-apps-header-image.png)

The MCP ecosystem is standardizing how servers deliver interactive UIs to hosts, and goose is an early adopter. Today we're shipping support for the draft MCP Apps specification ([SEP-1865](https://github.com/modelcontextprotocol/ext-apps/blob/main/specification/draft/apps.mdx)), bringing goose in line with the emerging standard, as other hosts like Claude and ChatGPT move toward adoption.

<!-- truncate -->

## What's Shipping

This release ([v1.19.0](https://github.com/aaif-goose/goose/releases/tag/v1.19.0)) brings a minimal-but-functional implementation of MCP Apps:

- Discovery of MCP App resources connected to tools
- HTML content rendering in sandboxed iframes
- Basic message relay between the UI and the MCP server

Extension authors can now build MCP Apps that work across goose and any host that adopts the standard.

## What is MCP Apps?

MCP Apps lets MCP servers present interactive HTML UIs (forms, dashboards, visualizations) directly inside a host. Build once, run everywhere.

It's a draft specification ([SEP-1865](https://github.com/modelcontextprotocol/modelcontextprotocol/pull/1865)) that builds on [MCP-UI](https://mcpui.dev) and the [OpenAI Apps SDK](https://developers.openai.com/apps-sdk/), led by [Ido Salomon](https://x.com/idosal1) and [Liad Yosef](https://x.com/liadyosef) with contributions from Anthropic and OpenAI.

goose has been part of this from early on. We've [shipped MCP-UI support](/blog/2025/08/11/mcp-ui-post-browser-world), participated in spec conversations, and are now implementing MCP Apps so extension authors have a real host to build against while the standard matures.

## This is Experimental

MCP Apps is still a draft. Our implementation is intentionally minimal and subject to change. Expect sharp edges and breaking changes. We're shipping now so authors can try it, give feedback, and help the community converge on the right primitives.

**What's not included yet:**
- Full parity with every feature in the draft spec
- Advanced capabilities (camera, sensors)
- Persistent app windows outside of conversations

## The MCP-UI Transition

MCP-UI isn't going away overnight. We'll keep supporting it while the community finalizes MCP Apps, and there's an [adapter path](https://mcpui.dev/guide/mcp-apps) to ease migration. We'll share a deprecation timeline once the MCP Apps extension is formally accepted.

## Try it

- **Get started:** Update goose and point it at an MCP server that returns App resources
- **Read the spec:** [github.com/modelcontextprotocol/ext-apps](https://github.com/modelcontextprotocol/ext-apps)
- **Join the conversation:** [goose GitHub discussion](https://github.com/aaif-goose/goose/discussions/6069) · [MCP Contributors Discord](https://discord.gg/6CSzBmMkjX)

If you build or port an app, we want to hear from you. File issues, share demos, tell us what's broken. Early feedback shapes what comes next.

<head>
  <meta property="og:title" content="goose Lands MCP Apps" />
  <meta property="og:type" content="article" />
  <meta property="og:url" content="https://goose-docs.ai/blog/2026/01/06/mcp-apps" />
  <meta property="og:description" content="goose ships early support for the draft MCP Apps specification, aligning with the emerging standard for interactive UIs in MCP." />
<meta property="og:image" content="http://goose-docs.ai/assets/images/goose-lands-mcp-apps-header-image-eb1f899d6de24f21cc2c45e46727f11d.png" />
  <meta name="twitter:card" content="summary_large_image" />
  <meta property="twitter:domain" content="goose-docs.ai" />
  <meta name="twitter:title" content="goose Lands MCP Apps" />
  <meta name="twitter:description" content="goose ships early support for the draft MCP Apps specification, aligning with the emerging standard for interactive UIs in MCP." />
  <meta name="twitter:image" content="http://goose-docs.ai/assets/images/goose-lands-mcp-apps-header-image-eb1f899d6de24f21cc2c45e46727f11d.png" />
</head>
