---
title: "MCP UI: Bringing the Browser into the Agent"
description: "A first look at a UI for agents built on the proposed MCP-UI extension"
authors:
  - mic
---

import Tabs from '@theme/Tabs';
import TabItem from '@theme/TabItem';
import YouTubeShortEmbed from '@site/src/components/YouTubeShortEmbed';
import CLIExtensionInstructions from '@site/src/components/CLIExtensionInstructions';

![blog cover](mcp-ui-shot.png)

Goose recently released support for [MCP-UI](https://mcpui.dev/) which allows MCP servers to suggest and contribute user interface elements back to the agent.

:::warning
MCP-UI is still an [open RFC](https://github.com/modelcontextprotocol-community/working-groups/issues/35) being considering for adoption into the MCP spec. It works as is but may change as the proposal advances.
:::

MCP-UI sits on top of the protocol, but instead of text/markdown being the result, servers can return content that the client can render richly (including interactive GUI content).

<!-- truncate -->

Many everyday activities that agents undertake could benefit from a graphical representation. Sometimes this is done by the agent rendering a GUI on its own (I know I do that a lot), but this allows it to be more intrinsic to extensions for cases where interaction is best done graphically with a human. It also naturally (hence the [Shopify connection](https://shopify.engineering/mcp-ui-breaking-the-text-wall)) works well with commerce applications where you want to see the product!

It is worth taking a minute to watch this MCP server for an airline seat selector demo to get a sense of the capability:

  <video 
    controls 
    class="aspect-ratio"
    poster={require('@site/static/img/mcp-ui-shot.png').default}
    playsInline
  >
    <source src={require('@site/static/videos/mcp-ui.mov').default} type="video/mp4" />
    Your browser does not support the video tag.
  </video>

Essentially, MCP servers are suggesting GUI elements for the client (agent) to render as it sees fit.

## How do I use this

Starting from Goose v1.3.0, you can add MCP-UI as an extension. 

:::tip Add MCP-UI to Goose
<Tabs groupId="interface">
  <TabItem value="ui" label="goose Desktop" default>
  [Launch the installer](goose://extension?type=streamable_http&url=https%3A%2F%2Fmcp-aharvard.netlify.app%2Fmcp&id=mcpuidemo&name=MCP-UI%20Demo&description=Demo%20MCP-UI-enabled%20extension)
  </TabItem>
  <TabItem value="cli" label="goose CLI">
  Use `goose configure` to add a `Remote Extension (Streaming HTTP)` extension type with:

  **Endpoint URL**
  ```
  https://mcp-aharvard.netlify.app/mcp
  ```
  </TabItem>
</Tabs>
:::


Take a look at [MCP-UI demos](https://mcp-aharvard.netlify.app/) provided by Andrew Harvard. You can also check out his GitHub repo which has [samples you can start with](https://github.com/aharvard/mcp_aharvard/tree/main/components).

## The tech behind MCP-UI

At the heart of MCP-UI is an interface for a `UIResource`:

```ts
interface UIResource {
  type: 'resource';
  resource: {
    uri: string;       // e.g., ui://component/id
    // highlight-next-line
    mimeType: 'text/html' | 'text/uri-list' | 'application/vnd.mcp-ui.remote-dom'; // text/html for HTML content, text/uri-list for URL content, application/vnd.mcp-ui.remote-dom for remote-dom content (JavaScript)
    text?: string;      // Inline HTML, external URL, or remote-dom script
    blob?: string;      // Base64-encoded HTML, URL, or remote-dom script
  };
}
```

The `mimeType` is where the action happens. It can be HTML content, for example (in the simplest case).

Another key tech at play here is [Remote DOM](https://github.com/Shopify/remote-dom), which is an open source project from Shopify. It lets you take DOM elements from a sandboxed environment and render them in another one, which is quite useful for agents. This also opens up the possibility that the agent side can render widgets as it needs (i.e., with locally matching styles or design language).


## Possible futures

It's still early days for MCP-UI, so the details may change, but that is part of what makes experimenting with it exciting right now.

MCP-UI will continue to evolve, and may pick up more declarative ways for MCP-UI servers to specify they need forms or widgets of certain types, but without specifying the exact rendering. How nice would it be to be able to specify these components and let the agent render it beautifully, be that in a desktop or mobile client, or even a text UI in a command line!

<head>
  <meta property="og:title" content="MCP UI: Bringing the Browser into the Agent" />
  <meta property="og:type" content="article" />
  <meta property="og:url" content="https://goose-docs.ai/blog/2025/08/11/mcp-ui-post-browser-world" />
  <meta property="og:description" content="A first look at a UI for agents built on the proposed MCP-UI extension" />
  <meta property="og:image" content="https://goose-docs.ai/assets/images/mcp-ui-shot-1b80ebfab25d885a8ead1ca24bb6cf13.png" />
  <meta name="twitter:card" content="summary_large_image" />
  <meta property="twitter:domain" content="goose-docs.ai" />
  <meta name="twitter:title" content="MCP UI: Bringing the Browser into the Agent" />
  <meta name="twitter:description" content="A first look at a UI for agents built on the proposed MCP-UI extension" />
  <meta name="twitter:image" content="https://goose-docs.ai/assets/images/mcp-ui-shot-1b80ebfab25d885a8ead1ca24bb6cf13.png" />
</head>