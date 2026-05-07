---
title: "Orchestrate Complex Workflows Across Multiple Agents with goose"
description: "With v1.29, goose gained orchestration support — the ability to coordinate multi-agent workflows where goose acts as the conductor, not just a solo performer."
authors:
  - adewale
image: /img/blog/goose-conductor.png
---

![goose orchestrating multiple agents like a conductor](/img/blog/goose-conductor.png)

Most AI agents work alone. You give them a task, they do it, you get a result. But real-world workflows are rarely that simple. You might need one agent to research, another to code, another to review, and a coordinator to tie it all together.

With v1.29, goose gained orchestration support — the ability to coordinate multi-agent workflows where goose acts as the conductor, not just a solo performer.

<!-- truncate -->

## What is orchestration?

If you've used [subagents](/docs/guides/subagents) before, you already know how to delegate work to independent AI instances. Orchestration is the layer above that. Instead of one goose session doing everything sequentially, orchestration lets you:

- **Decompose** a complex task into independent subtasks
- **Delegate** each subtask to a separate agent (subagent, ACP provider, or another goose instance)
- **Coordinate** the results — waiting for dependencies, handling failures, merging outputs
- **Manage sessions** across the orchestrated workflow

Think of it as the difference between a solo developer and a tech lead managing a team.

## How it builds on what already exists

Orchestration doesn't replace subagents or subrecipes — it builds on them. Here's how the pieces fit together:

| Feature | What it does | Since |
|---------|-------------|-------|
| [Subagents](/docs/guides/subagents) (delegate) | Spin up independent sub-tasks | v1.24 |
| Async delegates | Run subagents in background, load results later | v1.24 |
| [ACP providers](/docs/tutorials/acp-providers) | Use external agents (Claude Code, Codex, Gemini) | v1.28 |
| **Orchestration** | Coordinate all of the above in structured workflows | v1.29 |

If subagents are your teammates, orchestration is the project plan that tells them what to do and when.

## Example: orchestrated code review pipeline

Here's a real pattern I've been using — running a multi-dimensional code review where three subagents analyze different aspects of the same code in parallel:

```
Review the authentication module — check security, performance, and test coverage.
```

Behind the scenes, goose orchestrates this as:

```
goose (orchestrator):
  → delegate(async: true, instructions: "Security audit of auth module —
      check for injection, token handling, session management")
  → delegate(async: true, instructions: "Performance review of auth module —
      identify N+1 queries, unnecessary allocations, slow paths")
  → delegate(async: true, instructions: "Test coverage analysis of auth module —
      identify untested paths and suggest new test cases")

  [All three run in parallel]

  → load(task_1) — security findings
  → load(task_2) — performance findings
  → load(task_3) — coverage findings

  → Synthesize into unified review report
```

Each subagent focuses on one dimension without being distracted by the others. The orchestrator waits for all three, then synthesizes a unified report. You get three expert reviews in the time it takes to do one.

## Example: research → build → verify

Not everything can run in parallel. Some workflows have natural phases where later steps depend on earlier results. Orchestration handles this with sequential coordination:

```
Build a REST API for the inventory system based on our existing data models.
```

goose breaks this into phases:

```
goose (orchestrator):

  Phase 1 — Research (parallel):
    → delegate: "Analyze the existing data models in src/models/"
    → delegate: "Review the existing API patterns in src/routes/"

  Phase 2 — Build (sequential, depends on Phase 1):
    → delegate: "Implement the inventory API routes following the patterns found"

  Phase 3 — Verify (parallel):
    → delegate: "Write integration tests for the new inventory API"
    → delegate: "Review the implementation for security issues"
```

Phase 2 waits until Phase 1 completes because it needs the research results. Phase 3 runs its two tasks in parallel because they're independent of each other (but both depend on Phase 2).

## Orchestration + ACP: mixing models

One of the most powerful aspects of orchestration in v1.29 is that it works with [ACP providers](/docs/tutorials/acp-providers). This means you can orchestrate across different AI models — each chosen for what it's best at:

```
goose (orchestrator):
  → delegate to Claude Code: "Refactor this function for clarity"
  → delegate to local subagent: "Write tests for the refactored function"
  → delegate to Gemini: "Generate API documentation from the code"
```

You're not locked into one model for everything. The orchestrator picks the right tool for each job.

## Best practices

After using orchestration extensively, here's what I've learned works best:

### Partition your writes

Never let two delegates modify the same file. They run independently and will conflict. If two agents need to touch the same file, make them sequential — one finishes before the other starts.

### Parallelize your reads

Research tasks are safe to run in parallel. Multiple agents can read the same codebase, docs, or APIs simultaneously without stepping on each other.

### Be explicit in instructions

Delegates only know what you tell them. They don't share context with each other or with the parent session. If a delegate needs information from a previous phase, you need to pass it explicitly.

### Use async for independence

If tasks don't depend on each other, run them with `async: true` and `load()` the results when you need them. This is faster and keeps the orchestrator free to manage the workflow.

### Start simple

You don't need to orchestrate everything. A single subagent is fine for straightforward tasks. Orchestration shines when you have genuinely independent work streams or need multiple perspectives on the same problem.

## Orchestration vs. just using subagents

You could already do multi-step workflows with subagents before v1.29. So what does orchestration actually add?

- **Structured coordination patterns** — The orchestrator understands phases, dependencies, and parallel vs. sequential execution as first-class concepts.
- **Session management** — Better visibility into what's happening across the workflow. Delegate logs now show in the UI ([#7519](https://github.com/block/goose/pull/7519)).
- **Foundation for advanced patterns** — Pipelines, DAGs, retry logic, and conditional branching become possible as orchestration matures.

It's the difference between manually managing a bunch of background tasks and having a proper workflow engine.

## What's next

Orchestration in v1.29 is the foundation. The patterns will get richer over time — think conditional branching (if the security review finds critical issues, skip deployment), retry logic (if a delegate fails, try again with different instructions), and richer inter-agent communication.

For now, the core loop of decompose → delegate → coordinate → synthesize is already powerful enough to transform how you work with goose on complex tasks.

## Get started

If you're already using subagents, you're most of the way there. Orchestration is about being intentional with how you structure multi-agent work:

1. **Identify independent subtasks** in your workflow
2. **Decide what can run in parallel** vs. what has dependencies
3. **Write clear, self-contained instructions** for each delegate
4. **Let goose coordinate** — it'll manage the async execution and result collection

Check out the [subagents guide](/docs/guides/subagents) if you're new to delegation, or the [subrecipes tutorial](/docs/tutorials/subrecipes-in-parallel/) if you want reusable orchestration patterns you can share with your team.

<head>
  <meta property="og:title" content="Orchestrate Complex Workflows Across Multiple Agents with goose" />
  <meta property="og:type" content="article" />
  <meta property="og:url" content="https://block.github.io/goose/blog/2026/05/05/goose-as-conductor" />
  <meta property="og:description" content="With v1.29, goose gained orchestration support — the ability to coordinate multi-agent workflows where goose acts as the conductor, not just a solo performer." />
  <meta property="og:image" content="https://block.github.io/goose/img/blog/goose-conductor.png" />
  <meta name="twitter:card" content="summary_large_image" />
  <meta property="twitter:domain" content="block.github.io/goose" />
  <meta name="twitter:title" content="Orchestrate Complex Workflows Across Multiple Agents with goose" />
  <meta name="twitter:description" content="With v1.29, goose gained orchestration support — the ability to coordinate multi-agent workflows where goose acts as the conductor, not just a solo performer." />
  <meta name="twitter:image" content="https://block.github.io/goose/img/blog/goose-conductor.png" />
</head>
