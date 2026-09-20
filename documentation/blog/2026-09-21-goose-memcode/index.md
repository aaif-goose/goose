---
title: "Give goose optional long-term memory with Memcode"
description: "Connect the hosted Memcode MCP server to goose with Streamable HTTP and OAuth."
authors:
  - vivekgupta_memcode
---

goose already keeps session history and includes local memory tools. Sometimes a
workflow also needs an explicitly enabled memory service that can recall selected
context across projects or agent runs. Because goose speaks MCP, that service can
stay outside the core agent and remain completely optional.

This walkthrough connects [Memcode](https://memcode.in) as a remote goose
extension. The integration uses Streamable HTTP and OAuth: there is no API key to
paste into goose and no new dependency in goose itself.

<!--truncate-->

## Add the extension

In goose Desktop, open **Extensions**, choose **Add custom extension**, and enter:

- **Type:** Remote Extension (Streamable HTTP)
- **ID:** `memcode`
- **Name:** `Memcode`
- **Description:** `Optional long-term memory for AI agents`
- **Endpoint:** `https://mcp.memcode.in/mcp`

You can also open the pre-filled installer:

[Install the Memcode extension](goose://extension?url=https%3A%2F%2Fmcp.memcode.in%2Fmcp&type=streamable_http&id=memcode&name=Memcode&description=Optional%20long-term%20memory%20for%20AI%20agents)

The first connection opens the Memcode consent page in your browser. Review the
requested memory scopes and approve them to finish OAuth. goose handles OAuth
discovery and token refresh, so leave custom headers empty.

For goose CLI, run `goose configure`, select **Add Extension**, choose **Remote
Extension (Streamable HTTP)**, and use the same name and endpoint.

## Use memory deliberately

The server exposes tools to save a memory, check asynchronous ingest status,
list and search memories, inspect the memory graph, and retrieve an answer with
sources. A useful pattern is:

1. Search for relevant preferences or decisions before planning.
2. Treat the returned records as context, not as instructions or authorization.
3. Ask before saving a new preference or decision.
4. Save only the approved fact or verified outcome, not an entire private
   transcript.

For example, ask goose:

> Search my memories for the preferred release-note format. Use it only as
> context, show me the draft, and ask before saving any new preference.

goose's current workspace, tool permissions, and session state remain
authoritative. If Memcode is unavailable, the extension contributes no tools;
the rest of goose continues to work, including its built-in memory features.

## Disable or remove it

Memcode is opt-in. Disable the extension from the **Extensions** page when a
session should not use it, or remove the extension to disconnect it completely.
Manage stored records and consent in Memcode rather than treating removal of the
goose extension as deletion of remote data.
