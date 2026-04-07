---
title: MCPs for Developers Who Think They Don't Need MCPs
description: If you think MCPs are overhyped, here's what you're missing
authors: 
    - angie
---

![blog cover](mcp-for-devs.png) 

Lately, I've seen more developers online starting to side eye MCP. There was a [tweet](https://x.com/ibuildthecloud/status/1990221860018204721) by Darren Shepherd that summed it up well:

> "Most devs were introduced to MCP through coding agents (Cursor, VSCode) and most devs struggle to get value out of MCP in this use case... so they are rejecting MCP because they have a CLI and scripts available to them which are way better for them."

Fair. Most developers were introduced to MCPs through some chat-with-your-code experience, and sometimes it doesn't feel better than just opening your terminal and using the tools you know. But here's the thing...

<!-- truncate -->

**MCPs weren't built just for developers.**

They're not just for IDE copilots or code buddies. At Block, we use MCPs across *everything*, from finance to design to legal to engineering. [I gave a whole talk](https://youtu.be/IDWqWdLESgY?si=Mjoi-MGEPW9sxvmT) on how different teams are using goose, an AI agent. The point is MCP is a protocol. What you build on top of it can serve all kinds of workflows.

But I get it... let's talk about the dev-specific ones that *are* worth your time.


## GitHub: More Than Just the CLI

If your first thought is "why would I use [GitHub MCP](/docs/mcp/github-mcp) when I have the CLI?" I hear you. GitHub's MCP is kind of bloated right now. (They know. They're working on it.)

But also: **you're thinking too local.**

You're imagining a solo dev setup where you're in your terminal, using GitHub CLI to do your thing. And honestly, if all you’re doing is opening a PR or checking issues, you probably should use the CLI.

But the CLI was never meant to coordinate across tools. It’s built for local, linear commands. But what if your GitHub interactions happened *somewhere else* entirely? 

MCP shines when your work touches multiple systems like GitHub, Slack, and Jira without you stitching it together.

Here's a real example from our team:

> Slack thread. Real developers in realtime.
>
> **Dev 1:** I think there's a bug with xyz
>
> **Dev 2:** Let me check... yep, I think you're right.
>
> **Dev 3:** `@goose` is there a bug here?
>
> **goose:** Yep. It's in these lines...[code snippet]
>
> **Dev 3:** Okay `@goose`, open an issue with the details. What solutions would you suggest?
>
> **goose:** Here are 3 suggestions: [code snippets with rationale]
>
> **Dev 1:** I like Option 1
>
> **Dev 2:** me too
>
> **Dev 3:** `@goose`, implement Option 1
>
> **goose:** Done. Here's the PR.

All of that happened *in Slack*. No one opened a browser or a terminal. No one context switched. Issue tracking, triaging, discussing fixes, implementing code in one thread in a 5-minute span.

We've also got teams tagging Linear or Jira tickets and having goose fully implement them. One team had goose do **15 engineering days** worth of work in a single sprint. The team literally ran out of tasks and had to pull from future sprints. Twice!

So yes, GitHub CLI is great. But MCP opens the door to workflows where GitHub isn't the only place where dev work happens. That's a shift worth paying attention to.


## Context7: Docs That Don't Suck

Here's another pain point developers hit: documentation.

You're working with a new library. Or integrating an API. Or wrestling with an open source tool. 

The [Context7 MCP](/docs/mcp/context7-mcp) pulls up-to-date docs, code examples, and guides right into your AI agent's brain. You just ask questions and get answers like:

* "How do I create a payment with the Square SDK?"
* "What's the auth flow for Firebase?"
* "Is this library tree-shakable?"

It doesn't rely on stale LLM training data from two years ago. It scrapes the source of truth *right now*. Giving it updated... say it with me... CONTEXT.

Developer "flow" is real, and every interruption steals precious focus time. This MCP helps you figure out new libraries, troubleshoot integrations, and get unstuck without leaving your IDE. 


## Repomix: Know the Whole Codebase Without Reading It

Imagine you join a new project or want to contribute to an open source one, but it's a huge repo with lots of complexity.

Instead of poking around for hours trying to draw an architectural diagram in your head, you just ask your agent:

> "goose, pack this project up."

It runs [repomix](/docs/mcp/repomix-mcp), which compresses the entire codebase into an AI-optimized file. From there, your convo might go like this:

* "Where's the auth logic?"
* "Show me how API calls work."
* "What uses `UserContext`?"
* "What's the architecture?"
* "What's still a TODO?"

You get direct answers with context, code snippets, summaries, and suggestions. It's like onboarding with a senior dev who already knows everything. Sure, you could grep around and piece things together. But repomix gives you the whole picture - structure, metrics, patterns - compressed and queryable.

And it even works with remote public GitHub repos, so you don't need to clone anything to start exploring.

This is probably my favorite dev MCP. It's a huge time saver for new projects, code reviews, and refactoring.


## Chrome DevTools MCP: Web Testing While You Code

The [Chrome DevTools MCP](/docs/mcp/chrome-devtools-mcp) is a must-have for frontend devs. You're building a new form/widget/page/whatever. Instead of opening your browser, typing stuff in, and clicking around, you just tell your agent:

> "Test my login form on localhost:3000. Try valid and invalid logins. Let me know what happens."

Chrome opens, test runs, screenshots captured, network traffic logged, console errors noted. All done by the agent.

This is gold for frontend devs who want to actually test their work before throwing it over the fence.

---

Could you script all this with CLIs and APIs? Sure, if you want to spend your weekend writing glue code. But why would you want to do that when MCP gives you that power right out of the box... in any MCP client?!

So no, MCPs are not overhyped. They're how you plug AI into everything you use: Slack, GitHub, Jira, Chrome, docs, codebases - and make that stuff work *together* in new ways.

Recently, Anthropic called out the [real issue](https://www.anthropic.com/engineering/advanced-tool-use): most dev setups load tools naively, bloat the context, and confuse the model. It's not the protocol that's broken. It's that most people (and agents) haven't figured out how to use it well yet. Fortunately, goose has - it [manages MCPs by default](/docs/mcp/extension-manager-mcp), enabling and disabling as you need them. 

But I digress.

Step outside the IDE, and that's when you really start to see the magic.

P.S. Happy first birthday, MCP! 🎉

<head>
  <meta property="og:title" content="MCPs for Developers Who Think They Don't Need MCPs" />
  <meta property="og:type" content="article" />
  <meta property="og:url" content="https://goose-docs.ai/blog/2025/11/26/mcp-for-devs" />
  <meta property="og:description" content="If you think MCPs are overhyped, here's what you're missing" />
  <meta property="og:image" content="https://goose-docs.ai/assets/images/mcp-for-devs-0cbea02edffded1a26cec5f19a2a61b1.png" />
  <meta name="twitter:card" content="summary_large_image" />
  <meta property="twitter:domain" content="goose-docs.ai" />
  <meta name="twitter:title" content="MCPs for Developers Who Think They Don't Need MCPs" />
  <meta name="twitter:description" content="If you think MCPs are overhyped, here's what you're missing" />
  <meta name="twitter:image" content="https://goose-docs.ai/assets/images/mcp-for-devs-0cbea02edffded1a26cec5f19a2a61b1.png" />
</head>