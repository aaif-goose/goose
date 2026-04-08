---
sidebar_position: 54
title: MCP Sampling Extensions
sidebar_label: MCP Sampling
description: Transforms MCP servers into intelligent agents that can think, analyze, and make decisions using goose's AI
---

import Tabs from '@theme/Tabs';
import TabItem from '@theme/TabItem';

MCP Sampling can transform extensions from simple data providers into intelligent agents. Instead of just returning raw information for goose to interpret, extensions can leverage goose's AI capabilities to provide expert-level guidance, perform contextual analysis, and create entirely new interaction patterns.

This feature is automatically enabled in goose, no configuration required! Any MCP server extension that supports sampling will automatically have access to the LLM that goose is using. This means:
- goose users can get more targeted responses tailored to the extension's specific capabilities
- developers can add sampling support to their MCP servers to provide enhanced capabilities in goose

Try out the [Council of Mine](/docs/mcp/council-of-mine-mcp) extension to see MCP sampling in action!

:::info
[MCP Sampling](https://modelcontextprotocol.io/specification/draft/client/sampling) is a feature in the Model Context Protocol.
:::

## How MCP Sampling Works

MCP Sampling enables extensions to ask goose's AI for help with their tasks. When an extension needs to analyze data, make intelligent decisions, or understand natural language, it can send a "sampling" request to ask for AI assistance. goose processes the request using its AI capabilities and returns the response to the extension.

This feature allows extensions to provide more specialized contextual responses or novel interaction patterns. The following example shows how a database extension could provide expert-level diagnostics by combining its domain knowledge with goose's AI analysis:

<Tabs>
  <TabItem value="without" label="Without Sampling">
    1. You ask goose: "What's wrong with my database performance?"

    2. goose calls the database tool
    
    3. The database tool returns raw metrics to goose:
       ```
       Query times: 2.3s, 1.8s, 5.2s, 0.3s, 8.1s
       Table sizes: users (1M rows), orders (5M rows)
       Indexes: 3 on users, 1 on orders
       ```

    4. goose responds to you with general recommendations:
       ```
       Your database seems slow. Some queries are taking over 5 seconds. You might need more indexes.
       ```

  </TabItem>
  <TabItem value="with" label="With Sampling">
    1. You ask goose: "What's wrong with my database performance?"

    2. goose calls the database tool
    
    3. The database tool gets raw metrics:
       ```
       Query times: 2.3s, 1.8s, 5.2s, 0.3s, 8.1s
       Table sizes: users (1M rows), orders (5M rows)
       Indexes: 3 on users, 1 on orders
       ```

       Then, the tool:
       - Uses its domain expertise (query patterns, table relationships, database type) to ask goose's AI: "Given these metrics and knowing the JOIN patterns in this PostgreSQL database, what's the issue?"
       - Returns an AI-enhanced response to goose
  
    4. goose responds to you with targeted recommendations:
       ```
       Your orders table is missing an index on customer_id which is causing the 5-8 second delays in your JOIN queries. The slow queries all involve customer lookups. Run: `CREATE INDEX idx_orders_customer ON orders(customer_id);`
       ```

  </TabItem>
</Tabs>

### Use Cases

MCP Sampling enables powerful capabilities like:
- **Smart documentation tools** that explain code in context
- **Intelligent search** that filters and ranks results  
- **Database analyzers** that provide specific optimization recommendations
- **Multi-perspective analysis** where extensions generate and synthesize multiple AI viewpoints

## For Extension Developers

Want to add MCP Sampling to your own extensions? See our [Building Custom Extensions](/docs/tutorials/custom-extensions) tutorial to learn more about how MCP servers can leverage goose's AI capabilities.

## Additional Resources

import ContentCardCarousel from '@site/src/components/ContentCardCarousel';
import mcpSampling from '@site/blog/2025-12-04-mcp-sampling/mcp-sampling.png';

<ContentCardCarousel
  items={[
    {
      type: 'blog',
      title: 'MCP Sampling: When Your Tools Need to Think',
      description: 'Learn how MCP Sampling lets your tools call the AI instead of the other way around.',
      thumbnailUrl: mcpSampling,
      linkUrl: '/blog/2025/12/04/mcp-sampling',
      date: '2025-12-04',
      duration: '6 min read'
    }
  ]}
/>
