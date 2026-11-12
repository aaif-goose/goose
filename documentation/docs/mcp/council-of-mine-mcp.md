---
title: Council of Mine Extension (Retired)
sidebar_label: Council of Mine (Retired)
description: Council of Mine relied on MCP sampling and is not compatible with current goose releases
---

:::warning Retired extension
Council of Mine relied on MCP sampling for its debate workflow. [Sampling was deprecated](https://modelcontextprotocol.io/specification/2026-07-28/deprecated) in the 2026-07-28 MCP specification, and goose no longer advertises or handles `sampling/createMessage` requests. Do not install this extension for use with current goose releases.
:::

Council of Mine remains available as a [historical example](https://github.com/block/mcp-council-of-mine) of an MCP server built around sampling. MCP servers that need model inference should integrate directly with an LLM provider API.
