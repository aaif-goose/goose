---
title: "Why Tool Descriptions Aren’t Enough"
description: "I thought better tool descriptions would solve everything. They didn’t. Here’s what finally made MCP sampling click for me."
authors: 
    - ebony
---
![blog banner](blogbanner.png)


The first question I had when I heard about MCP sampling was:

> *“Can’t I just write better tool descriptions and tell the tool it’s an expert?”*

<!--truncate-->

Because honestly, that’s what I was already doing.

If a tool wasn’t behaving how I expected, I’d tweak the wording. Add more detail. Clarify intent. Be more explicit. And sure, that helped a little.

But something still felt off.

The tools still weren’t really *thinking*. They were fetching data, returning text, and leaving all the heavy reasoning to my LLM. That’s when I realized the issue wasn’t my descriptions. It was how the system actually worked under the hood.

That’s where [MCP sampling](https://goose-docs.ai/docs/guides/mcp-sampling/) came in.
Not as a magic feature, but as a different way of structuring how tools and the LLM actually collaborate.

## What actually changed my understanding

Once I realized the issue wasn’t my tool descriptions but how the system itself was structured, I needed a clearer way to understand the difference.

This is the distinction that helped it click for me:

> Tool descriptions influence how a tool is used
> Sampling changes how a tool participates in reasoning

That might still sound a little abstract, so I mapped it out visually below.

![without sampling](without-mcp.png)


Without sampling, the tool mostly acts like a messenger. It fetches data, returns content, and all the real reasoning happens at the top level in the LLM.

![with sampling](with-mcp.png)


With sampling, the behavior changes. The tool gathers its data, then uses the same LLM you already configured in Goose to ask a targeted question from its own context before returning anything. Instead of just passing information upward, it’s now contributing to the thinking.

It’s the same model and the same agent, but the behavior changes completely.

## Where Council of Mine fits in

Seeing the flow change helped me understand sampling conceptually. [Council of Mine](https://github.com/aaif-goose/mcp-council-of-mine) helped me understand it viscerally.

It’s not MCP sampling itself. It’s an example of what becomes possible once sampling exists.

Instead of making a single request to the LLM, Council of Mine uses sampling repeatedly and intentionally. Each perspective is its own conversation with the same LLM, framed by a different point of view. Those responses are then compared, debated, and synthesized into a final answer.

The server handles the orchestration. The LLM does the reasoning. Sampling is what allows that back-and-forth to happen at all.

What made this click for me was watching one question turn into multiple independent perspectives, then seeing how those perspectives shaped the final output. It took sampling from an abstract idea to something concrete.

## What I landed on

Good tool descriptions still matter. This isn’t a replacement for them.

But on their own, they won’t get you to truly agentic behavior. Descriptions shape behavior at the surface. Sampling changes how the reasoning itself is structured.

That distinction was the missing piece for me. And once I could actually see the flow, everything else started to make more sense.

If this helped make things click, I’d recommend trying the [Council of Mine extension](https://goose-docs.ai/docs/mcp/council-of-mine-mcp) for yourself. It’s one of the clearest ways to see MCP sampling in action.

<head>
  <meta property="og:title" content="Why Tool Descriptions Aren’t Enough" />
  <meta property="og:type" content="article" />
  <meta property="og:url" content="https://goose-docs.ai/blog/2026/01/15/why-tool-descriptions-arent-enough" />
  <meta property="og:description" content="I thought better tool descriptions would solve everything. They didn’t. Here’s what finally made MCP sampling click for me." />
  <meta property="og:image" content="https://goose-docs.ai/assets/images/blogbanner-97fb5e20248b53e838888082ac9f5860.png" />
  <meta name="twitter:card" content="summary_large_image" />
  <meta property="twitter:domain" content="goose-docs.ai" />
  <meta name="twitter:title" content="Why Tool Descriptions Aren’t Enough" />
  <meta name="twitter:description" content="I thought better tool descriptions would solve everything. They didn’t. Here’s what finally made MCP sampling click for me." />
  <meta name="twitter:image" content="https://goose-docs.ai/assets/images/blogbanner-97fb5e20248b53e838888082ac9f5860.png" />
</head>
