---
title: "Code Mode Doesn't Replace MCP (Here's What It Actually Does)"
description: Code Mode isn't killing MCP. It makes it better. A practical look at how Code Mode works with MCP to solve tool bloat and performance issues in agents.
authors:
    - rizel
---

![blog cover](header-image.png)

One day, we will tell our kids we used to have to wait for agents, but they won't know that world because the agents in their day would be so fast. I joked about this with Nick Cooper, an MCP Steering Committee Member from OpenAI, and Bradley Axen, the creator of [goose](/). They both chuckled at the thought because they understand exactly how clunky and experimental our current "dial-up era" of agentic workflows can feel. 

[Model Context Protocol (MCP)](https://modelcontextprotocol.io/) has moved the needle by introducing a new norm: the ability to connect agents to everyday apps. However, the experience isn't perfect. We are still figuring out how to balance the power of these tools with the technical constraints of the models themselves.

<!-- truncate -->

---

## The "Too Many Extensions" Problem

(Quick note: In [goose](/), we call MCP servers "extensions." I'll use "extensions" from here on out.)

Many people write off MCP because they experience lag or instability, often without realizing they've fallen into the trap of "tool bloat." Admittedly, there's a lot of "don't do this" advice so you can have a good experience. For example, a best practice that the goose team and power users follow is: don't turn on too many extensions at once. Otherwise, your sessions will degrade quicker, you'll see more hallucinations, and task execution may be slower.

I've seen first-time users turn on a bunch of extensions in excitement. "This is so cool. I'm going to need it to access GitHub, Vercel, Slack, my database..." They are effectively flooding the agent's context window with hundreds of tokens worth of tool definitions. Each tool call requires the model to hold all those definitions in its "active memory", which leads to a noticeable degradation in performance. The agent becomes slower, begins to hallucinate details that aren't there, and eventually starts throwing errors, leading the frustrated user to conclude that the platform isn't ready for prime time. 

## Making Extensions Dynamic

The goose team initially combatted this by adding [dynamic extensions](/docs/getting-started/using-extensions/#automatically-enabled-extensions), which allow the system to keep most tools dormant until the agent specifically identifies a need for them. While this was a massive step toward efficiency, it remained a somewhat hidden feature that many casual users rarely discovered. I spent plenty of time watching people operate with a huge list of active extensions, cringing as I realized they were wasting tokens on extensions and tools they weren't even using.

## Code Mode Explained

[Code Mode](/blog/2025/12/15/code-mode-mcp) resolves the issue of extension bloat by taking this idea of limiting tools a step further. I first learned about this concept from a [Cloudflare blog post](https://blog.cloudflare.com/code-mode/) where they proposed agents should write JavaScript or TypeScript that decides which tools to call and how, and then runs that logic in one execution instead of calling tools one step at a time. Instead of forcing the LLM to memorize a hundred different tool definitions, you provide it with just three foundational tools: `search_modules`, `read_module`, and `execute_code`. The agent then learns to find what it needs on the fly and writes a custom script to chain those actions together in a single execution.

## Code Mode Doesn't Replace MCP

When the concept of Code Mode landed on socials, many people claimed it was a replacement for MCP. Actually, Code Mode still uses MCP under the hood. The tools it discovers and executes are still MCP tools. Think of it like HTTP and REST: HTTP is the underlying protocol that makes communication possible, while REST is an architectural pattern built on top of it. Similarly, MCP is the protocol that standardizes how agents connect to tools, and Code Mode is a pattern for how agents interact with those tools more efficiently. In fact, the goose ecosystem actually treats Code Mode as an MCP server (extension).

### How goose Implemented Code Mode

[goose](/) took a unique approach by making [Code Mode](/blog/2025/12/15/code-mode-mcp) itself an extension called the Code Mode extension. When active, it wraps your other extensions and exposes them as JavaScript modules, allowing the LLM to see only three tools instead of eighty.

When the agent needs to perform a complex task, it writes a script that looks something like this:

```javascript
import { shell, text_editor } from "developer";

const branch = shell({ command: "git branch --show-current" });
const commits = shell({ command: "git log -3 --oneline" });
const packageJson = text_editor({ path: "package.json", command: "view" });
const version = JSON.parse(packageJson).version;

text_editor({ 
  path: "LOG.md", 
  command: "write", 
  file_text: `# Log\n\nBranch: ${branch}\n\nCommits:\n${commits}\n\nVersion: ${version}` 
});
```

## Code Mode vs. No Code Mode

In addition to reading about Code Mode, I had to try it out, so I could really understand how it works. So, I conducted an experiment where I compared my experience with Code Mode and without Code Mode. I used Claude Opus 4.5, enabled eight different extensions, and gave the agent a straightforward, but multi-step prompt to see how it handled the load:

> "Create a LOG.md file with the current git branch, last 3 commits, and the version from package.json"

### Without Code Mode

When I ran this test with Code Mode disabled, goose successfully performed five separate tool calls to gather the data and write the file. However, because all eight extensions had their full definitions loaded into the context, this relatively simple task consumed 16% of my total context window. This demonstrates the clear scalability issues of standard workflows, as the system becomes increasingly unstable and prone to failure when you aren't using Code Mode.

### With Code Mode

When I toggled Code Mode on and ran the exact same prompt, the experience changed completely. The agent used its discovery tools to find the necessary modules and wrote a single, unified JavaScript script to handle the entire workflow at once. In this scenario, only 3% of the context window was used.

This means I can have a longer session before the model's performance begins to degrade or it begins to hallucinate under the weight of too many tools. 

## The Value of Code Mode

This exercise cleared up a few misconceptions I had about Code Mode's behavior in goose.

* **I thought it would make tasks execute faster:** Code Mode doesn't necessarily speed up task execution; in fact, I noticed additional round-trips because the LLM has to discover tools and write JavaScript before it can act.
* **I thought it was for every task:** If you are only using one or two tools, the overhead of writing and executing code might actually be more work than just calling the tool directly.

However, Code Mode shines when goose: 
- Has too many extensions enabled
- Needs to perform multi-step orchestration
- Needs to stay coherent over a long-running session

Therefore, it doesn't make sense for me to use Code Mode when:
- I only have 1-2 extensions enabled
- The task is single-step
- Speed matters more than context longevity

---

## Improving Code Mode Support in goose

The cool part is Code Mode is only getting better. The team is currently refining Code Mode following its release in goose v1.17.0 (December 2025):

- [Better UX](https://github.com/aaif-goose/goose/pull/6205) - showing what tools are being called instead of raw JavaScript
- [Better reliability](https://github.com/aaif-goose/goose/pull/6177) - improving type signatures so LLMs get the code right the first time
- [More capabilities](https://github.com/aaif-goose/goose/pull/6160) - enabling subagents to work inside Code Mode

Code Mode helps us take a step forward in building agents that can scale to handle all your tools without falling apart. I love seeing how MCP is evolving, and I can't wait for the day I tell my children that agents weren't always this limitless and that we actually used to have to ration our tools just to get a simple task done.

---

*Ready to try Code Mode? Enable the "Code Mode" extension in [goose](/docs/quickstart) v1.17.0 or later. Join our [Discord](https://discord.gg/goose-oss) to share your experience!*

<head>
  <meta property="og:title" content="Code Mode Doesn't Replace MCP (Here's What It Actually Does)" />
  <meta property="og:type" content="article" />
  <meta property="og:url" content="https://goose-docs.ai/blog/2025/12/21/code-mode-doesnt-replace-mcp" />
  <meta property="og:description" content="Code Mode isn't killing MCP. It makes it better. A practical look at how Code Mode works with MCP to solve tool bloat and performance issues in agents." />
  <meta property="og:image" content="https://goose-docs.ai/assets/images/header-image-c7b1f3556c63058f53eeb740bdaffa3b.png" />
  <meta name="twitter:card" content="summary_large_image" />
  <meta property="twitter:domain" content="goose-docs.ai" />
  <meta name="twitter:title" content="Code Mode Doesn't Replace MCP (Here's What It Actually Does)" />
  <meta name="twitter:description" content="Code Mode isn't killing MCP. It makes it better. A practical look at how Code Mode works with MCP to solve tool bloat and performance issues in agents." />
  <meta name="twitter:image" content="https://goose-docs.ai/assets/images/header-image-c7b1f3556c63058f53eeb740bdaffa3b.png" />
  <meta name="keywords" content="goose, MCP, Model Context Protocol, Code Mode, AI agents, extensions, tool bloat, context window, JavaScript, developer tools" />
</head>
