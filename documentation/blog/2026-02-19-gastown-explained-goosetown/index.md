---
title: "Gas Town Explained: How to Use Goosetown for Parallel Agentic Engineering"
description: "Learn how Gas Town and Goosetown lead the industrial coding revolution by teaching AI agents to work together in a team. This beginner guide explains the infrastructure we're using to move from talking to one AI to coordinating many agents at once."
authors:
  - rizel
  - tyler
---

![Goosetown](goosetown.png)

On New Year's Day 2026, while many were recovering from the night before, a different kind of hangover took hold of every AI-pilled, chronically online software engineer. Steve Yegge published a new blog post: "[Welcome to Gas Town](https://steve-yegge.medium.com/welcome-to-gas-town-4f25ee16dd04)." Some walked away inspired to finally use their agents optimally; others were just plain confused. If you're like me, you felt a bit of both. 

Yegge's 34 minute post is a sprawling vision filled with futuristic ideas, playful characters, and enough side tangents to make your head spin. But underneath the lore is a massive architectural shift. I want to take a step back and simplify the "Big Idea" for everyone: Gas Town is a philosophy and a proof of concept to help people coordinate multiple agents working together.

<!-- truncate -->

## The Paradigm Shift of Agentic Engineering

Most people use AI agents sequentially. The workflow can look like this:

* **1:00 PM:** You: "goose, build the API endpoint."  
* *[Wait 10 minutes, check back.]*  
* **1:10 PM:** You: "Now build the frontend."  
* *[Wait 10 minutes, check back.]*  
* **1:20 PM:** You: "Now write the tests."  
* *[Wait 10 minutes, check back.]*  
* **1:30 PM:** Project complete.

You've built a project in 30 minutes, which is fast, but you spent most of that time just watching a progress bar. Some engineers started to realize that if we are running one agent, we can run another five at the same time. 

For example, Agent A builds the API, Agent B can start the frontend, Agent C can write tests, and Agent D can investigate a bug in that legacy codebase you've been avoiding.

This is how people are buying their time back. They're getting entire sprints done in an hour by running parallel threads. (Just don't tell your boss because the reward for finishing work is always more work.)

However, since agents don't communicate with each other, this approach introduces new problems:

* **Merge conflicts**: Two agents change the same line in the same file and break everything.  
* **Lost context**: Sessions crash or the agent starts hallucinating because it's been talking too long, and suddenly an hour of "work" vanishes.  
* **Human bottleneck**: You end up constantly checking your phone at a party on the weekend or in bed to see if your agents are still on track making you a babysitter for agents.

## Gas Town Explained

Gas Town is designed to stop the babysitting. It coordinates the distribution of tasks among parallel agents so you don't have to. The system uses:

* **Worktrees**: These automatically put each agent in its own separate workspace so they don't step on each other's toes.  
* **Beads**: It uses beads to track progress. If a session crashes, the next agent session can pick up exactly where the last agent left off.  
* **Communication**: Each agent reports aloud what it's up to or observing, so other agents gain the necessary context. 

This system also introduces a cast of characters:

* **The Mayor**: your main agent interface that coordinates all the other agents  
* **The Polecat(s)**: These are worker agents. They work on separate work trees, and take instruction from the Mayor.  
* **The Witness**: Observes the worker agents, nudges them when they get stuck, and escalates issues to keep the system running

I won't list every single character here (it gets deep), but the takeaway is: Gas Town creates a chain of command with a shared way to communicate.

## Introducing Goosetown

This is exactly the kind of futuristic thinking we're building toward at [goose](https://goose-docs.ai). So the goose team, specifically Tyler Longwell, built our own take on this called [Goosetown](https://github.com/aaif-goose/goosetown).

Goosetown is a multi-agent orchestration layer built on top of goose. Like Gas Town, it coordinates parallel agents. Unlike Gas Town, it's deliberately minimal and built for research-first parallel work.

When you give Goosetown a task, the main agent acts as an Orchestrator, breaking the job into phases: research, build, and review. Then, it spawns parallel delegates to get it done. Each delegate communicates via a shared Town Wall, an append-only log where every agent posts what they're doing and what they've found.

Here's a real Town Wall snippet from a session where parallel researchers converged on a pivot quickly:

* **[10:14] researcher-api-auth** - 🚨 POTENTIAL SHOWSTOPPER: Service callers have EMPTY capabilities. Planned auth path will silently reject every request. This needs a code change, not just config.   
* **[10:14] researcher-endpoints** - 💡 Found: native endpoint already exists with minimal deps. Alternative path viable.   
* **[10:15] researcher-source** - ✅ Done. Confirmed: native path requires zero new dependencies. Recommending pivot.

Goosetown operates on 4 components: [skills](/docs/guides/context-engineering/using-skills), [subagents](/docs/guides/subagents), [beads](https://github.com/steveyegge/beads), and a [gtwall](https://github.com/aaif-goose/goosetown/blob/main/gtwall).

### Skills

[Skills](/docs/guides/context-engineering/using-skills) are Markdown files that describe how to do something like "how to deploy to production." Goosetown uses these to tell each Delegate how to do its specific job. When a Delegate spawns, it's "pre-loaded" with the skill for its role (Orchestrator, Researcher, Writer, Reviewer).

### Subagents

Instead of doing everything in one long conversation that eventually hits a "context cliff," Goosetown uses [subagents](/docs/guides/subagents), ephemeral agent instances. These are triggered by the [summon extension](/docs/mcp/summon-mcp), using `delegate()` to hand off work to a fresh agent instance. They do the work in their own clean context and return a summary, keeping your main session fast and focused.

### Beads

Goosetown uses [Beads](https://github.com/steveyegge/beads) to track progress so work survives crashes. It's a local issue tracker based on Git. The Orchestrator creates issues, delegates update them, and if a session fails, the next agent picks up the "bead" and continues the work.

### gtwall

[gtwall](https://github.com/aaif-goose/goosetown/blob/main/gtwall) is an append-only log that delegates use to communicate and coordinate. All delegates post and read activity.

## A Note from the Creator 

> *Goosetown started as a fun experiment to push the subagent upgrade I was working on for goose to its limits. I had spent some time with Gas Town and thought that a much less sprawling riff on it would be a whimsical way to show off background subagents and how good modern goose is at orchestrating and managing projects at a very high level. I was just totally surprised at how well it worked when I started using Goosetown as my daily driver.*
>
> *Watching the subagents chatter to each other, each one given a personality and task by the orchestrator, was eye-opening. And funny. They rubber-duck and go back and forth just like I do with my colleagues. Even just having one model getting to bounce ideas off itself automatically in the form of different agents with different contexts makes the output better.*
>
> *Running flocks (swarms) of agents is obviously expensive, but the overall quality of the work is higher and much less of my time is required to get it right. Definitely a tradeoff there. goose does help with this by allowing you to set the default subagent model to a less pricey one ahead of time and by allowing your main agent to select the models it wants to use for subagents explicitly, ad-hoc.*
>
> *There are always new and exciting features being added to goose. Refactors and refinements. For Goosetown, I'll continue to make its artifact (memory) system more robust, make communications inside Goosetown flow more smoothly, and keep it just a little silly.*
>
> — Tyler

## Get Started

Ready to try parallel agentic engineering for yourself? [Goosetown](https://github.com/aaif-goose/goosetown) is open source and available on GitHub. Clone the [repo](https://github.com/aaif-goose/goosetown), follow the setup instructions in the README, and you'll be orchestrating multiple agents in no time. If you're new to this workflow, watching the video below is a great way to see what a real session looks like before diving in.

<iframe class="aspect-ratio" src="https://www.youtube.com/embed/H2hJjNmvEEA" title="Rizel's first time using Goosetown" frameborder="0" allow="accelerometer; autoplay; clipboard-write; encrypted-media; gyroscope; picture-in-picture" allowfullscreen></iframe>

<head>
  <meta property="og:title" content="Gas Town Explained: How to Use Goosetown for Parallel Agentic Engineering" />
  <meta property="og:type" content="article" />
  <meta property="og:url" content="https://goose-docs.ai/blog/2026/02/19/gastown-explained-goosetown" />
  <meta property="og:description" content="Learn how Gas Town and Goosetown lead the industrial coding revolution by teaching AI agents to work together in a team. This beginner guide explains the infrastructure we're using to move from talking to one AI to coordinating many agents at once." />
  <meta property="og:image" content="https://goose-docs.ai/assets/images/goosetown-6e1bda1a4bd160c0c01cfc58c118492e.png" />
  <meta name="twitter:card" content="summary_large_image" />
  <meta property="twitter:domain" content="goose-docs.ai" />
  <meta name="twitter:title" content="Gas Town Explained: How to Use Goosetown for Parallel Agentic Engineering" />
  <meta name="twitter:description" content="Learn how Gas Town and Goosetown lead the industrial coding revolution by teaching AI agents to work together in a team." />
  <meta name="twitter:image" content="https://goose-docs.ai/assets/images/goosetown-6e1bda1a4bd160c0c01cfc58c118492e.png" />
</head>
