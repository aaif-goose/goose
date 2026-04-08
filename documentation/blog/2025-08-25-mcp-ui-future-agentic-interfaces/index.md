---
title: "MCP-UI: The Future of Agentic Interfaces"
description: Discover how MCP-UI is revolutionizing AI agent interactions by bringing rich, interactive web components directly into agent conversations, making AI more accessible and intuitive for everyone.
authors: 
    - ebony
---


![mcp ui](mcpui-goose.png)

The days of endless text walls in AI agent conversations are numbered. What if instead of reading through paragraphs of product descriptions, you could browse a beautiful, interactive catalog? What if booking a flight seat could be as simple as clicking on your preferred spot on a visual seat map? This isn't science fiction. It's happening right now with MCP-UI.

In a recent [Wild Goose Case episode](https://www.youtube.com/live/GS-kmreZDgU), we dove deep into MCP-UI with its creators Ido Salomon and Liad Yosef from Monday.com, alongside Block's own Andrew Harvard, to explore how this groundbreaking technology is reshaping the future of agentic interfaces.

<!-- truncate -->

## The Problem with Text-Only Interfaces

Let's be honest, we've all been there. You ask an AI agent to help you shop for shoes, and you get back a wall of text with product names, prices, and descriptions. Then you have to copy and paste URLs, open multiple tabs, and basically do all the work yourself. It defeats the purpose of having an AI assistant in the first place.

As Ido put it during our conversation: "I think everyone did something had a bunch of text and were like this is terrible why do I have to type all of that and kind of rage quit chat GPT."

The reality is that text-based interfaces work fine for early adopters and technical users, but they're not the future. They're certainly not going to work for everyone – including our moms, who are increasingly using AI assistants but shouldn't have to navigate complex text responses.

## Enter MCP-UI: Bridging the Gap

MCP-UI (Model Context Protocol User Interface) represents a fundamental shift in how we think about AI agent interactions. Instead of forcing users to consume everything through text, MCP-UI enables rich, interactive web components to be embedded directly into agent conversations.

The core philosophy is brilliant in its simplicity: **Why throw away decades of web UI/UX expertise when we can enhance it with AI?**

As Liad explained: "We have more than a decade of human targeted interfaces on the web that are built and perfected for the human cognitive limitations and needs, and it doesn't make sense that agents will make us get rid of all of that."

## How MCP-UI Works

At its heart, MCP-UI is both a protocol and an SDK that enables:

1. **Rich UI Components**: Instead of text descriptions, you get interactive catalogs, seat maps, booking forms, and more
2. **Brand Preservation**: Companies like Shopify keep their branding and user experience intact
3. **Seamless Integration**: UI components communicate with agents through secure, sandboxed iframes
4. **Cross-Platform Compatibility**: The same UI can work across different AI agents and platforms

The magic happens through embedded resources in the MCP specification. When you interact with an MCP server that supports UI, instead of just returning text, it can return rich UI components that render directly in your agent interface.

## Real-World Examples in Action

During our demo, we saw some incredible examples of MCP-UI in action:

### Shopping Made Visual
Instead of reading through product descriptions, users saw a beautiful Shopify catalog with images, prices, and interactive elements. Clicking on items added them to a cart, just like a regular e-commerce experience, but embedded seamlessly in the AI conversation.

### Travel Planning Reimagined
We watched as users could select airplane seats by clicking on a visual seat map, then have the agent automatically look up weather information for their destination cities, all without leaving the conversation or typing additional commands.

### Restaurant Discovery
The demo showed how users could browse local restaurants with rich cards showing photos, ratings, and menus, then place orders directly through interactive interfaces, all while maintaining the conversational flow with the AI agent.

## The Technical Foundation

From a technical perspective, MCP-UI prioritizes security and isolation. UI components are rendered in sandboxed iframes that can only communicate with the host through post messages. This ensures that third-party UI code can't access or manipulate the parent application.

The current implementation supports several content types:
- **External URLs**: Existing web apps embedded in iframes
- **Raw HTML**: Custom HTML components with CSS and JavaScript
- **Remote DOM**: UI rendered in separate workers for enhanced security

For developers, getting started is surprisingly simple. As Andrew demonstrated, you can begin with something as basic as:

```javascript
return createUIResource({
  type: 'html',
  content: '<h1>Hello World</h1>'
});
```

## The Stakeholder Ecosystem

MCP-UI's success depends on several key stakeholders:

1. **Agent Developers** (like the Goose team): Need to implement MCP-UI support in their platforms
2. **MCP Server Developers**: Build the UI components and integrate them with existing services
3. **Service Providers** (like Shopify, Square): Create rich interfaces for their platforms
4. **End Users**: Benefit from more intuitive and visual AI interactions

The beauty of this approach is that it creates a network effect. Once implemented, a Shopify MCP-UI component works across all compatible agents – from Goose to VS Code extensions to future mobile AI assistants.

## Looking Ahead: The Future is Visual

The implications of MCP-UI extend far beyond just prettier interfaces. We're looking at:

### Accessibility Revolution
As Ido noted: "What's more accessible than an agent that knows you and builds the UI for your preferences? You don't need to rely on every web app in the world to support or build that."

### Generative UI
Future versions might move beyond static HTML to AI-generated interfaces tailored to individual users' needs, preferences, and accessibility requirements.

### Multi-Modal Experiences
The protocol isn't limited to visual interfaces – it could extend to voice interactions, mobile native components, or even entirely new interaction paradigms we haven't imagined yet.

### Cross-Platform Standardization
Instead of every company building separate integrations for each AI platform, MCP-UI creates a standard that works everywhere.

## The Adoption Challenge

The technology is ready, but adoption is the next frontier. As the team emphasized, this is now more of an adoption problem than a technical one. The good news? Major players are already on board. Shopify has launched MCP support for all their stores, providing a massive real-world testing ground for commerce experiences. And of course, MCP-UI is supported in Goose.

For developers interested in contributing, the focus is on:
- Building real applications that solve actual user problems
- Expanding the specification based on real-world needs
- Creating better tooling and SDKs to lower the barrier to entry

## Getting Started

If you're excited about MCP-UI and want to start experimenting, here's where to begin:

1. **Check out the documentation**: The MCP-UI team has created [comprehensive docs and examples](https://mcpui.dev/)
2. **Join the community**: There's an active [Discord server](https://discord.gg/4ww9QnJgCp) for collaboration and discussion
3. **Start simple**: Take an existing MCP server and add UI resources to it
4. **Experiment with Goose**: Try out [MCP-UI in Goose](/docs/guides/interactive-chat/mcp-ui) to see it in action

## The Bottom Line

MCP-UI represents a fundamental shift from text-heavy AI interactions to rich, visual, and intuitive experiences.

When you're browsing products, you want to see images and interact with catalogs. When you're booking travel, you want visual seat maps and itineraries. When you're managing your calendar, you want familiar interface patterns that just work.

MCP-UI makes all of this possible while preserving the conversational nature of AI agents. It's the bridge between the web as we know it and the agentic future we're building.

The future of interacting with AI relies on smarter interfaces. And with MCP-UI, that future is already here.

---

*Want to see MCP-UI in action? Check out the full Wild Goose Chase episode below and join our community to start building the future of agentic interfaces.*

<iframe src="https://www.youtube.com/embed/GS-kmreZDgU" title="MCP-UI: The Future of Agentic Interfaces" frameborder="0" allow="accelerometer; autoplay; clipboard-write; encrypted-media; gyroscope; picture-in-picture" allowfullscreen class="aspect-ratio"></iframe>

*Join the conversation on [Discord](https://discord.gg/goose-oss) and follow our progress as we continue pushing the boundaries of what's possible with AI agents.*

<head>
  <meta property="og:title" content="MCP-UI: The Future of Agentic Interfaces" />
  <meta property="og:type" content="article" />
  <meta property="og:url" content="https://goose-docs.ai/blog/2025/08/25/mcp-ui-future-agentic-interfaces" />
  <meta property="og:description" content="Discover how MCP-UI is revolutionizing AI agent interactions by bringing rich, interactive web components directly into agent conversations, making AI more accessible and intuitive for everyone." />
  <meta property="og:image" content="https://goose-docs.ai/assets/images/mcpui-goose-44f248ede0eb5d2e0bddccf76e98b07e.png" />
  <meta name="twitter:card" content="summary_large_image" />
  <meta property="twitter:domain" content="goose-docs.ai" />
  <meta name="twitter:title" content="MCP-UI: The Future of Agentic Interfaces" />
  <meta name="twitter:description" content="Discover how MCP-UI is revolutionizing AI agent interactions by bringing rich, interactive web components directly into agent conversations, making AI more accessible and intuitive for everyone." />
  <meta name="twitter:image" content="https://goose-docs.ai/assets/images/mcpui-goose-44f248ede0eb5d2e0bddccf76e98b07e.png" />
</head>