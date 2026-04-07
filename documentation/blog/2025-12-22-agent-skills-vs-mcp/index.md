---
title: "Did Skills Kill MCP?"
description: An overview of Agent Skills vs MCP
authors: 
    - angie
---

![](skills-vs-mcp.png)

Every time there's a hot new development in AI, Tech Twitter™ declares a casualty.

This week's headline take is **"Skills just killed MCP"**

It sounds bold. It sounds confident. It's also wrong.

<!-- truncate -->

Saying skills killed MCP is about as accurate as saying GitHub Actions killed Bash. Of course, that's not true. Bash is still very much alive, and in fact, doing the actual work. What GitHub Actions changed was expression, not execution. They gave us a better way to describe workflows. A cleaner, more shareable way to say, "Here's how we build, test, and deploy." Under the hood, the same shell commands are still running. YAML organized execution, it didn't replace it.

That's pretty much the relationship between [Skills](/docs/guides/context-engineering/using-skills/) and MCP.

Once you see it that way, the "Skills killed MCP" take kind of collapses on its own.

MCP is where **capability** lives. It's what allows an AI agent to actually do things instead of just talking about them. When an agent can run shell commands, edit files, call APIs, query databases, read from drives, store or retrieve memory, or pull live data, that's MCP at work. MCP Servers are code. They run as services and expose callable tools. If an agent needs to interact with the real world in any meaningful way, MCP is almost certainly involved.

For example, if an agent needs to query the GitHub API, send a Slack message, or fetch production metrics, that requires real integrations, real permissions, and real execution. Instructions alone can't do that.

Skills live at a different layer. Skills are about process and knowledge. They're markdown files that encode how work should be done. They capture team conventions, workflows, and domain expertise. A Skill might describe how deployments should happen, how code reviews are handled, or how incidents are triaged. This is institutional knowledge made explicit.

For example, here's an example Skill that teaches an agent how to integrate with a Square account:

```md
---
name: square-integration
description: How to integrate with our Square account
---

# Square Integration

## Authentication
- Test key: Use `SQUARE_TEST_KEY` from `.env.test`
- Production key: In 1Password under "Square Production"

## Common Operations

### Create a customer
const customer = await squareup.customers.create({
  email: user.email,
  metadata: { userId: user.id }
});


### Handle webhooks
Always verify webhook signatures. See `src/webhooks/square.js` for our handler pattern.

## Error Handling
- `card_declined`: Show user-friendly message, suggest different payment method
- `rate_limit`: Implement exponential backoff
- `invalid_request`: Log full error, likely a bug in our code
```

Skills can include things that look executable. I think this is where some of the confusion comes from.  A Skill might show code snippets, reference scripts, or even bundle supporting files like templates or a script. That can make it feel like the Skill itself is doing the work.

But it isn't.

Even when a Skill folder includes runnable files, the Skill is not the thing executing them. The agent executes those files by calling tools provided elsewhere, like a shell tool exposed via the [Developer MCP Server](/docs/mcp/developer-mcp). The Skill packages guidance and assets together, but the capability to run code, access the network, or modify systems comes from tools, which can be exposed via MCP.

This is exactly how GitHub Actions works. A workflow file can reference scripts, commands, and reusable actions. It can look powerful. But the YAML doesn't execute anything. The runner does. Without a runner, the workflow is just a plan.

Skills describe the workflow. MCP provides the runner.

That's why saying Skills replace MCP doesn't make sense. Skills without MCP are well written instructions. MCP without Skills is raw power with no guidance. One tells the agent what should happen. The other makes it possible for anything to happen at all.

Put simply, MCP gives agents abilities. Skills teach agents how to use those abilities well. Bash still runs the commands. GitHub Actions still defines the workflow. Same system, different layers, no murders involved.

If anything, the existence of both is a good sign. It means the ecosystem is maturing. We're no longer arguing about whether agents should have tools or instructions. We're building systems that assume you need both.

That's progress, not replacement.


<head>
  <meta property="og:title" content="Did Skills Kill MCP?" />
  <meta property="og:type" content="article" />
  <meta property="og:url" content="https://goose-docs.ai/blog/2025/12/22/agent-skills-vs-mcp" />
  <meta property="og:description" content="An overview of Agent Skills vs MCP" />
  <meta property="og:image" content="https://goose-docs.ai/assets/images/skills-vs-mcp-f2d83cbf65b3ddb4f9294470ab653355.png" />
  <meta name="twitter:card" content="summary_large_image" />
  <meta property="twitter:domain" content="goose-docs.ai" />
  <meta name="twitter:title" content="Did Skills Kill MCP?" />
  <meta name="twitter:description" content="An overview of Agent Skills vs MCP" />
  <meta name="twitter:image" content="https://goose-docs.ai/assets/images/skills-vs-mcp-f2d83cbf65b3ddb4f9294470ab653355.png" />
</head>