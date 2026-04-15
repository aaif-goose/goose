---
title: "How to Stop Your AI Agent From Making Unwanted Code Changes"
description: Teach your AI agent how to commit early and often so you can control changes, roll back safely, and keep clean snapshots of your codebase.
authors: 
    - rizel
---

![goose, revert this change!](header-image.png)

AI agents are often described as brilliant, overeager interns. They're desperate to help, but sometimes that enthusiasm leads to changes you never asked for. This is by design: the large language models powering agents are trained to be helpful. But in code, unchecked helpfulness can create chaos. Even with clear instructions and a meticulous plan, you might hear, "Let me just change this too…" A modification that's either unnecessary or, worse, never surfaced for review.

Sure, you can scour `git diff` to find and revert issues. But in a multi-step process touching dozens of files, untangling one small, unwanted change becomes a manual nightmare. I've spent hours combing through 70 files to undo a single "helpful" adjustment. Asking the agent to revert is often futile, as conversational memory isn't a snapshot of your codebase.

<!-- truncate -->

This problem has a classic engineering solution. We commit early and often to create checkpoints, enabling easy rollbacks and clean collaboration. So, why don't we enforce the same discipline on our AI agents? Here’s the workflow I use with [goose](/) to ensure we're creating snapshots of the codebase: 
 

### 1. Set Up Version Control

I set up the [GitHub CLI](https://cli.github.com/) (`gh`). I've found Goose interacts with it flawlessly. The [GitHub MCP Server](/docs/mcp/github-mcp) is a good alternative.

### 2. Branch First

Always start on a new feature branch. Never let an agent commit directly to main.

### 3. Set Rules in a Context File

This is the key. I use a [`.goosehints`](/docs/guides/context-engineering/using-goosehints) or [`AGENTS.md`](/docs/guides/context-engineering/using-goosehints#custom-context-files) file with one critical instruction: 

> "Every time you make a change, make a commit with a clear message." 

This does two things: it automates checkpointing so I don't have to babysit the session, and it captures perfect snapshots in time, turning the git history into an undo stack for the entire collaboration.

### 4. Collaborate with Confidence 

Now I can prompt goose to build, fix, or refactor. If it veers off course or makes a design choice I dislike, I can instantly review the git log or simply say: 

> *"Revert to commit abc123."*

## The Result
Then, when I'm happy with the final changes, I can push my code to remote. By integrating this basic software practice, I replace anxiety with awareness. goose gets to be brilliantly helpful, and I get to stay in control.

Try out this method with [goose](/) to help you build your next project. Your future self (and your git history) will thank you.

<head>
  <meta property="og:title" content="How to Stop Your AI Agent From Making Unwanted Changes" />
  <meta property="og:type" content="article" />
  <meta property="og:url" content="https://goose-docs.ai/blog/2025/12/10/stop-ai-agent-unwanted-changes" />
  <meta property="og:description" content="AI agents are often described as brilliant, overeager interns. Learn how to keep them in control with simple version control practices." />
  <meta property="og:image" content="https://goose-docs.ai/assets/images/header-image-ce224702149226ea0924fac736eef2fa.png" />
  <meta name="twitter:card" content="summary_large_image" />
  <meta property="twitter:domain" content="goose-docs.ai" />
  <meta name="twitter:title" content="How to Stop Your AI Agent From Making Unwanted Changes" />
  <meta name="twitter:description" content="AI agents are often described as brilliant, overeager interns. Learn how to keep them in control with simple version control practices." />
  <meta name="twitter:image" content="https://goose-docs.ai/assets/images/header-image-ce224702149226ea0924fac736eef2fa.png" />
</head>
