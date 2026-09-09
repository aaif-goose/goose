---
title: Cloudflare Extension
description: Add Cloudflare MCP Servers as goose Extensions
unlisted: true
---

import Tabs from '@theme/Tabs';
import TabItem from '@theme/TabItem';

This tutorial covers how to add [Cloudflare's MCP Servers](https://github.com/cloudflare/mcp-server-cloudflare) as goose extensions to manage your Cloudflare infrastructure, debug applications, analyze traffic, and more using natural language.

Cloudflare provides multiple specialized MCP servers for different aspects of their platform, allowing you to interact with Workers, DNS, security features, analytics, and development tools.

:::tip Quick Install

<Tabs groupId="interface">
  <TabItem value="ui" label="goose Desktop" default>
  [Launch the installer](goose://extension?cmd=npx&arg=mcp-remote&arg=https%3A%2F%2Fobservability.mcp.cloudflare.com%2Fmcp&id=cloudflare-observability&name=Cloudflare%20Observability&description=Debug%20and%20get%20insight%20into%20your%20application%27s%20logs%20and%20analytics&env=CLOUDFLARE_API_TOKEN%3DCloudflare%20API%20Token)
  </TabItem>
  <TabItem value="cli" label="goose CLI">
  **Command**
  ```sh
  npx mcp-remote https://observability.mcp.cloudflare.com/mcp
  ```
  </TabItem>
</Tabs>
  **Environment Variable**
  ```
  CLOUDFLARE_API_TOKEN: Your Cloudflare API token with appropriate permissions
  ```
:::

## Available Cloudflare MCP Servers

Cloudflare provides multiple specialized MCP servers for different use cases:

| Server | Description | Use Cases |
|--------|-------------|-----------|
| **Documentation** | Get up-to-date reference information on Cloudflare | API reference, feature documentation, troubleshooting guides |
| **Workers Bindings** | Build Workers applications with storage, AI, and compute primitives | KV storage, R2 buckets, AI models, Durable Objects |
| **Workers Builds** | Get insights and manage your Cloudflare Workers builds | Deployment status, build logs, version management |
| **Observability** | Debug and get insight into your application's logs and analytics | Error tracking, performance monitoring, request analysis |
| **Radar** | Global Internet traffic insights, trends, URL scans, and utilities | Traffic analysis, threat intelligence, URL scanning |
| **Container** | Spin up sandbox development environments | Isolated testing, development containers |
| **Browser Rendering** | Fetch web pages, convert to markdown, take screenshots | Web scraping, content analysis, visual testing |
| **Logpush** | Get quick summaries for Logpush job health | Log management, data pipeline monitoring |
| **AI Gateway** | Search logs, get details about prompts and responses | AI usage analytics, prompt optimization |
| **AutoRAG** | List and search documents on your AutoRAGs | Document retrieval, knowledge base management |
| **Audit Logs** | Query audit logs and generate reports for review | Security monitoring, compliance reporting |
| **DNS Analytics** | Optimize DNS performance and debug issues | DNS troubleshooting, performance optimization |
| **Digital Experience Monitoring** | Get insight on critical applications for your organization | Application performance, user experience monitoring |
| **Cloudflare One CASB** | Identify security misconfigurations for SaaS applications | Security posture, compliance checking |
| **GraphQL** | Get analytics data using Cloudflare's GraphQL API | Custom analytics, data visualization |

## Prerequisites

- A [Cloudflare account](https://dash.cloudflare.com/sign-up)
- [Cloudflare API Token](https://dash.cloudflare.com/profile/api-tokens) with appropriate permissions
- Node.js installed (for `npx` command)

## Configuration

### Step 1: Create API Token

1. Go to [Cloudflare API Tokens](https://dash.cloudflare.com/profile/api-tokens)
2. Click **"Create Token"**
3. Choose **"Custom token"** for specific permissions or **"Global API Key"** for full access
4. Configure permissions based on which MCP servers you plan to use:
   - **Zone:Read** - For DNS, analytics, and general zone information
   - **Zone:Edit** - For making configuration changes
   - **Account:Read** - For account-level resources
   - **Workers:Read/Edit** - For Workers-related servers
   - **Logs:Read** - For observability and audit logs

### Step 2: Add MCP Server to goose

Choose one or more servers based on your needs. Here are the most popular configurations:

#### Observability Server (Recommended for debugging)

<Tabs groupId="interface">
  <TabItem value="ui" label="goose Desktop" default>
  1. [Launch the installer](goose://extension?cmd=npx&arg=mcp-remote&arg=https%3A%2F%2Fobservability.mcp.cloudflare.com%2Fsse&id=cloudflare-observability&name=Cloudflare%20Observability&description=Debug%20and%20get%20insight%20into%20your%20application%27s%20logs%20and%20analytics&env=CLOUDFLARE_API_TOKEN%3DCloudflare%20API%20Token)
  2. Press `Yes` to confirm the installation
  3. Enter your Cloudflare API Token
  4. Click `Save Configuration`
  5. Scroll to the top and click `Exit` from the upper left corner
  </TabItem>
  <TabItem value="cli" label="goose CLI">
  1. Run the `configure` command:
  ```sh
  goose configure
  ```

  2. Choose to add a `Command-line Extension`
  3. Give your extension a name: `cloudflare-observability`
  4. Enter the command: `npx mcp-remote https://observability.mcp.cloudflare.com/mcp`
  5. Set timeout: `300` seconds
  6. Add environment variable:
     - Name: `CLOUDFLARE_API_TOKEN`
     - Value: Your Cloudflare API token

  </TabItem>
</Tabs>

#### Workers Bindings Server (For Workers development)

<Tabs groupId="interface">
  <TabItem value="ui" label="goose Desktop" default>
  [Launch the installer](goose://extension?cmd=npx&arg=mcp-remote&arg=https%3A%2F%2Fbindings.mcp.cloudflare.com%2Fmcp&id=cloudflare-bindings&name=Cloudflare%20Workers%20Bindings&description=Build%20Workers%20applications%20with%20storage%2C%20AI%2C%20and%20compute%20primitives&env=CLOUDFLARE_API_TOKEN%3DCloudflare%20API%20Token)
  </TabItem>
  <TabItem value="cli" label="goose CLI">
  Command: `npx mcp-remote https://bindings.mcp.cloudflare.com/mcp`
  </TabItem>
</Tabs>

#### Radar Server (For traffic insights)

<Tabs groupId="interface">
  <TabItem value="ui" label="goose Desktop" default>
  [Launch the installer](goose://extension?cmd=npx&arg=mcp-remote&arg=https%3A%2F%2Fradar.mcp.cloudflare.com%2Fmcp&id=cloudflare-radar&name=Cloudflare%20Radar&description=Get%20global%20Internet%20traffic%20insights%2C%20trends%2C%20URL%20scans%2C%20and%20other%20utilities&env=CLOUDFLARE_API_TOKEN%3DCloudflare%20API%20Token)
  </TabItem>
  <TabItem value="cli" label="goose CLI">
  Command: `npx mcp-remote https://radar.mcp.cloudflare.com/mcp`
  </TabItem>
</Tabs>

## Example Usage

### Debugging Application Issues

Let's use the Observability server to debug performance issues with a Workers application:

#### goose Prompt
```
I'm seeing high error rates on my Workers application "my-api-worker". Can you help me:
1. Check the recent error logs
2. Identify the most common error types
3. Analyze the performance metrics
4. Suggest optimizations based on the data
```

#### Expected goose Output
```
I'll help you debug the issues with your "my-api-worker" application. Let me gather the observability data to identify the problems.

First, let me check the recent error logs for your worker:

[goose uses the observability MCP server to fetch error logs]

I found several issues with your worker:

**Error Analysis:**
- 45% of errors are 500 Internal Server Errors
- 30% are timeout errors (exceeded 30s CPU limit)
- 25% are memory limit exceeded errors

**Performance Metrics:**
- Average response time: 2.3s (up 150% from last week)
- CPU usage: 85% average (critical threshold)
- Memory usage: 92% average (near limit)

**Root Causes Identified:**
1. Database connection pooling issues causing timeouts
2. Memory leaks in JSON parsing for large payloads
3. Inefficient regex operations in request validation

**Recommended Optimizations:**
1. Implement connection pooling with proper cleanup
2. Stream large JSON payloads instead of loading into memory
3. Replace regex with more efficient string operations
4. Add caching for frequently accessed data

Would you like me to help implement any of these fixes?
```

### Managing Workers Bindings

Using the Workers Bindings server to set up storage and AI capabilities:

#### goose Prompt
```
I need to set up a new Workers project with:
1. KV storage for caching
2. R2 bucket for file uploads
3. AI binding for text analysis
4. Durable Object for real-time features

Can you help me configure these bindings?
```

### Analyzing Traffic with Radar

Using the Radar server for security and traffic analysis:

#### goose Prompt
```
Can you help me analyze the security posture of my domain example.com? I want to:
1. Check for any security threats or malicious traffic
2. Analyze global traffic patterns
3. Scan for vulnerabilities
4. Get recommendations for improving security
```

## Common Use Cases

### 1. Application Debugging
- **Observability Server**: Monitor errors, performance, and user experience
- **Logpush Server**: Analyze log patterns and data pipeline health
- **DNS Analytics**: Debug DNS resolution issues

### 2. Development & Deployment
- **Workers Bindings**: Configure storage, AI, and compute resources
- **Workers Builds**: Monitor deployment status and build health
- **Container Server**: Set up isolated development environments

### 3. Security & Compliance
- **Audit Logs**: Track configuration changes and access patterns
- **Cloudflare One CASB**: Monitor SaaS application security
- **Radar Server**: Threat intelligence and URL scanning

### 4. Analytics & Insights
- **GraphQL Server**: Custom analytics and reporting
- **Digital Experience Monitoring**: Application performance insights
- **AI Gateway**: AI usage analytics and optimization

### 5. Content & Web Management
- **Browser Rendering**: Web scraping and content analysis
- **AutoRAG**: Document management and retrieval
- **Documentation Server**: API reference and troubleshooting

## Best Practices

### Security
- Use scoped API tokens with minimal required permissions
- Regularly rotate API tokens
- Monitor API usage through audit logs
- Set up alerts for unusual activity

### Performance
- Use appropriate timeout values for different operations
- Cache frequently accessed data when possible
- Monitor rate limits and usage quotas
- Implement proper error handling and retries

### Development Workflow
- Start with the Documentation server for API reference
- Use Container server for isolated testing
- Monitor with Observability server during development
- Analyze with Radar server before going live

## Troubleshooting

### Common Issues

**Authentication Errors:**
- Verify API token has correct permissions
- Check token hasn't expired
- Ensure token is properly set in environment variables

**Rate Limiting:**
- Monitor API usage in Cloudflare dashboard
- Implement exponential backoff for retries
- Consider upgrading plan for higher limits

**Connection Issues:**
- Verify network connectivity to Cloudflare APIs
- Check firewall settings
- Ensure proper DNS resolution

### Getting Help

If you encounter issues:

1. Check the [Cloudflare MCP Server repository](https://github.com/cloudflare/mcp-server-cloudflare) for documentation
2. Review [Cloudflare API documentation](https://developers.cloudflare.com/api/)
3. Join our [Discord community](https://discord.gg/n8R5VaWDAn) for support
4. Check [Cloudflare Community](https://community.cloudflare.com/) for platform-specific help

## Next Steps

With Cloudflare MCP servers enabled in goose, you can:

- **Monitor and debug** your applications with natural language queries
- **Manage infrastructure** through conversational commands
- **Analyze security** and performance data effortlessly
- **Automate workflows** across Cloudflare's entire platform

Try starting with the Observability server to get insights into your current applications, then expand to other servers based on your specific needs.
