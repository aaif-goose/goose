---
sidebar_position: 3
title: Code Mode
sidebar_label: Code Mode
description: Understanding how Code Mode handles tool discovery and tool calling
---

import Tabs from '@theme/Tabs';
import TabItem from '@theme/TabItem';

Code Mode is a method of interacting with MCP tools programmatically instead of calling them directly. Code Mode is particularly useful when working with many enabled extensions, as it can help manage context window usage more efficiently.

:::info
This functionality requires the built-in [Code Mode extension](/docs/mcp/code-mode-mcp) to be enabled.
:::

Code Mode controls how tools are discovered and called:

- Tools from enabled extensions are discovered on-demand and loaded into context as needed
- Multiple tool calls are batched in one execution
- Intermediate results are chained (output from one tool as input to the next)

## How Code Mode Works

The [Code Mode extension](/docs/mcp/code-mode-mcp) is an MCP server that uses the MCP protocol to expose three foundational meta-tools. When Code Mode is enabled, goose switches to Code Mode. For every request, the LLM writes JavaScript code that goose executes in a fresh goose-owned [Boa](https://boajs.dev/) context to:

- Discover available tools from your enabled extensions (if needed)
- Learn how to work with the tools it needs for the current task
- Call those tools programmatically to complete the task

### Traditional vs. Code Mode Tool Calling

Traditional MCP tool calling and Code Mode are two different approaches to the same goal: giving goose access to tools.

| Aspect | Traditional | Code Mode |
|--------|------------------|-----------|
| **Tool Discovery** | All tools from enabled extensions, for example:<br/>• `shell`<br/>• `write`<br/>• `github__list_issues`<br/>• `github__get_pull_request`<br/>• `slack__send_message`<br/>• ... *potentially many more* | Code Mode extension's meta-tools:<br/>• `list_functions`<br/>• `get_function_details`<br/>• `execute_javascript`<br/><br/>The LLM uses these tools to discover tools from other enabled extensions as needed |
| **Tool Calling** | • Sequential tool calls<br/>• Each result sent to the LLM before the next call | • May require tool discovery calls<br/>• Multiple tool calls batched in one execution<br/>• Intermediate results are chained and processed locally |
| **Context Window** | Every LLM call includes all tool definitions from enabled extensions | Every LLM call includes the 3 meta-tool definitions, plus any tool definitions previously discovered in the session |
| **Best For** | • 1-3 enabled extensions<br/>• Simple tasks using 1-2 tools | • 5+ extensions<br/>• Well-defined multi-step workflows |

:::info Text-Only Results
Code Mode only supports text content from tool results. Images, binary data, and other content types are ignored.
:::

## JavaScript interface

Call `list_functions` to discover exact tool names and short descriptions, then
`get_function_details` with `{"functions": ["extension__first_tool"]}` to inspect
original JSON input and output schemas. Recursive schemas remain JSON data.
Use the returned names exactly, including unprefixed names for first-class tools.

Pass JavaScript to `execute_javascript` with an `async function run()` entry point:

```javascript
async function run() {
  const results = await Promise.all([
    tools.call("extension__first_tool", { query: "example" }),
    tools.call("extension__second_tool", { limit: 5 })
  ]);
  return results.map(result => result.summary);
}
```

Await tool calls and batch independent operations. Return or log only information
needed for the next step. Tool arguments must be JSON objects; results must be
JSON values or `undefined`. Cyclic objects, functions, symbols, and BigInt are
unsupported. Console output, JSON depth and item counts, JavaScript instructions,
loop iterations, and recursion are bounded.

The runtime provides JavaScript built-ins, `tools.call`, and captured
`console.log`, `info`, `warn`, `error`, and `debug` methods. It has no direct
filesystem, shell, network, environment, or package access. Tool calls retain
the extension lease, permission flow, cancellation, and mutation handling used
by goose. Only assistant-visible text results reach the script.

Each execution uses a separate worker and context. The extension timeout and
cancellation stop execution and cancel in-flight tool calls. JavaScript state is
discarded between calls. Optional `tool_graph` metadata remains available for
desktop display; saved TypeScript calls can still be displayed.

## Additional Resources

import ContentCardCarousel from '@site/src/components/ContentCardCarousel';
import gooseCodeMode from '@site/blog/2025-12-15-code-mode-mcp/header-image.jpg';
import notMcpReplacement from '@site/blog/2025-12-21-code-mode-doesnt-replace-mcp/header-image.png';

<ContentCardCarousel
  items={[
    {
      type: 'blog',
      title: 'Code Mode for MCP',
      description: 'Learn about the code execution approach to MCP tool calling.',
      thumbnailUrl: gooseCodeMode,
      linkUrl: '/blog/2025/12/15/code-mode-mcp',
      date: '2025-12-15',
      duration: '5 min read'
    },
    {
      type: 'blog',
      title: 'Code Mode Doesn\'t Replace MCP',
      description: 'Understanding how Code Mode and MCP work together.',
      thumbnailUrl: notMcpReplacement,
      linkUrl: '/blog/2025/12/21/code-mode-doesnt-replace-mcp',
      date: '2025-12-21',
      duration: '8 min read'
    }
  ]}
/>
