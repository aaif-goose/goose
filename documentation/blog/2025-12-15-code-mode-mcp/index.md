---
title: "Code Mode MCP in goose"
description: An emerging approach to MCP tool calling gets an open source implementation in goose
authors:
    - alexhancock
---

![code mode MCP in goose!](header-image.jpg)

# Code Mode MCP

There is an emerging approach to MCP tool calling referred to as "sandbox mode" or "code mode". These ideas were initially
presented by Cloudflare in their [Code Mode: the better way to use MCP](https://blog.cloudflare.com/code-mode/) post and Anthropic
in their [Code execution with MCP: Building more efficient agents](https://www.anthropic.com/engineering/code-execution-with-mcp)
posts. Since the approach and the benefits are clearly laid out in those posts I will summarize them here.

<!-- truncate -->

## The approach

### Summary

* Instead of exposing the tools directly to the model, an MCP client application can:
    * Generate a programmatic interface to these same tools (typically JS or TS powered)
    * Provide a limited set of tools to the model (search available modules/tool source code, read the source code for a tool, and then a tool to execute some code)
    * Run the code the model generates to call the programmatic API in a sandboxed environment for safety

### Benefits

* The model can progressively discover relevant tools, without all server and tool definitions in the context window from the beginning
* The model can chain tool call results into inputs to further tool calls without the intermediate results needing to flow back to the model - this saves on tokens and avoids exposing potentially sensitive data to the model unnecessarily
* The models pre-training datasets have made them very efficient at analyzing large programmatic APIs and writing code to call them, as compared to having been trained only on contrived examples of MCP tool calling

## In goose

In v1.17.0 of goose, we've introduced an open source implementation of this idea in a new platform extension called: Code Mode.
Our implementation generates a JavaScript interface representing the connected MCP tools and then lets the model write code to run
against it in [boa](https://github.com/boa-dev/boa) which is an embeddable JavaScript engine. One neat feature of boa we were able
to take advantage of was the concept of [NativeFunction](https://docs.rs/boa_engine/latest/boa_engine/native_function/struct.NativeFunction.html).

In boa, a `NativeFunction` is something which exposes a function in the embedded JavaScript environment which calls back into a
natively implemented rust function. This is perfect for the calls originating in JS and then routing the tool call to the underlying
MCP server with ease!

## Help us evaluate it

Our hope is that we improve tool calling performance and handling of large numbers of tools in goose, but
also provide an open source implementation of this emerging approach.

* Try out the feature by enabling the ["Code Mode" extension](https://github.com/aaif-goose/goose/blob/main/crates/goose/src/agents/platform_extensions/code_execution.rs) in v1.17.0 or later of goose by clicking extensions on the left side of the desktop app or running `goose configure` on cli
* Please give us feedback on how it works for you by joining our [discord](https://discord.gg/goose-oss).

Kudos to my colleague [Mic Neale](https://github.com/michaelneale) for collaborating with me on the implementation!

<head>
  <meta property="og:title" content="Code Mode MCP in goose" />
  <meta property="og:type" content="article" />
  <meta property="og:url" content="https://goose-docs.ai/blog/2025/12/15/code-mode-mcp-in-goose" />
  <meta property="og:description" content="An emerging approach to MCP tool calling gets an open source implementation in goose" />
  <meta property="og:image" content="https://goose-docs.ai/assets/images/header-image-1fa39f1d26aea7722e2c10fc424804f5.jpg" />
  <meta name="twitter:card" content="summary_large_image" />
  <meta property="twitter:domain" content="goose-docs.ai" />
  <meta name="twitter:title" content="Code Mode MCP in goose" />
  <meta name="twitter:description" content="An emerging approach to MCP tool calling gets an open source implementation in goose" />
  <meta name="twitter:image" content="https://goose-docs.ai/assets/images/header-image-1fa39f1d26aea7722e2c10fc424804f5.jpg" />
</head>
