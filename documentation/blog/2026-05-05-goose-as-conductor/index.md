---
title: "Orchestrate Complex Workflows Across Multiple Agents with goose"
description: "Use goose to coordinate multi-agent workflows — decompose complex tasks, delegate to specialized agents in parallel, and synthesize the results."
authors:
  - adewale
image: /img/blog/goose-conductor.png
---

![goose orchestrating multiple agents like a conductor](/img/blog/goose-conductor.png)

A lot of people prompt their agents one task at a time — "do this, now do that, now do the next thing." It works, but it's slow. Many people also want their agents doing many things at once: researching while coding, reviewing while testing, writing docs while refactoring.

That's what orchestration gives you. Instead of feeding goose one task at a time and waiting, you can have it coordinate multiple agents working in parallel — each focused on a specific piece of the puzzle, with goose keeping everything on track.

<!-- truncate -->

## What is orchestration?

If you've used [subagents](/docs/guides/subagents) before, you already know how to delegate work to independent AI instances. Orchestration is the layer above that — the part that decides _who_ does _what_ and _when_. Instead of one goose session doing everything sequentially, orchestration lets you:

- **Decompose** a complex task into independent subtasks
- **Delegate** each subtask to a separate agent (subagent, ACP provider, or another goose instance)
- **Coordinate** the results — waiting for dependencies, handling failures, merging outputs

Think of it as the difference between a solo developer and a tech lead managing a team.

## How it works

Under the hood, orchestration uses three primitives you can trigger through natural language or recipes:

**`delegate`** — spawns a subagent with specific instructions. Each delegate runs in its own isolated session with its own context. You can control which extensions it has access to, and even which provider or agent it uses.

**`async: true`** — tells goose to run the delegate in the background instead of waiting for it to finish. This is what enables parallelism — you can kick off multiple delegates at once and they all run simultaneously.

**`load(task_id)`** — retrieves the result of a background delegate. goose waits for the task to complete and brings the output back into the main session.

The flow looks like this:

```
You: "Do X, Y, and Z"

goose (orchestrating):
  1. Kicks off delegate for X (async)
  2. Kicks off delegate for Y (async)
  3. Kicks off delegate for Z (async)
  4. Loads result from X
  5. Loads result from Y
  6. Loads result from Z
  7. Synthesizes everything into a final answer
```

Steps 1–3 happen almost instantly. Steps 4–6 wait for each result. The actual work (X, Y, Z) all happens in parallel. That's the speed gain.

## Try it yourself

Here's something you can run right now to see orchestration in action. Ask goose:

```
Create three files in parallel:
- hello.html with a "Hello World" page styled with a blue theme
- goodbye.html with a "Goodbye World" page styled with a red theme
- index.html that links to both pages with a clean navigation bar
```

goose will spin up three subagents simultaneously — one for each file. You'll see them working at the same time in your session, and all three files appear roughly together instead of one after another.

Want something more involved? Try this with a recipe. Create a file called `plan-trip.yaml`:

```yaml
version: 1.0.0
title: Plan Your Trip
description: Get weather and activities for a destination in parallel
instructions: You are a travel planning assistant.
prompt: |
  Run the following subrecipes in parallel to plan my trip:
    - use weather subrecipe to get the weather forecast for Sydney
    - use things-to-do subrecipe to find activities in Sydney
  Then combine the results into a single trip plan.
sub_recipes:
  - name: weather
    path: "./subrecipes/weather.yaml"
  - name: things-to-do
    path: "./subrecipes/things-to-do.yaml"
extensions:
  - type: builtin
    name: developer
    timeout: 300
    bundled: true
```

Run it with `goose run --recipe plan-trip.yaml` and watch goose coordinate two subrecipes running at the same time, then merge their outputs into one trip plan. The [subrecipes in parallel tutorial](/docs/tutorials/subrecipes-in-parallel/) has the full setup including the subrecipe files.

## How it builds on what already exists

Orchestration doesn't replace subagents or subrecipes — it builds on them:

| Layer | What it does |
|-------|-------------|
| [Subagents](/docs/guides/subagents) (delegate) | Spin up independent sub-tasks |
| Async delegates | Run subagents in the background, collect results later |
| [ACP providers](/docs/guides/acp-providers) | Bring in external agents (Claude Code, Codex, Amp) |
| **Orchestration** | Coordinate all of the above into structured workflows |

If subagents are your teammates, orchestration is the project plan that tells them what to do and when.

## Phased workflows: research → build → verify

Not everything can run in parallel. Some workflows have natural phases where later steps depend on earlier results. You can describe this to goose naturally:

```
Build a REST API for the inventory system:
1. First, research the existing data models in src/models/ and the API patterns in src/routes/ (do both at the same time)
2. Then implement the inventory API routes following those patterns
3. Finally, write integration tests and do a security review (both at the same time)
```

goose understands the dependency structure here. Phase 1 runs two research tasks in parallel. Phase 2 waits for those results before building. Phase 3 kicks off two independent verification tasks in parallel.

The key insight: **reads can be parallel, writes should be sequential** (especially if they touch the same files).

## Orchestration + ACP: mixing agents

Here's where things get really interesting. Orchestration works with [ACP providers](/docs/guides/acp-providers), which means you can delegate to entirely different coding agents — not just goose subagents. Claude Code, Codex, and Amp are all available as ACP providers, and each brings its own strengths:

```
goose (orchestrating):
  → delegate to Claude Code: "Refactor this function for clarity"
  → delegate to local subagent: "Write tests for the refactored function"
  → delegate to Codex: "Generate API documentation from the code"
```

You're not locked into one agent for everything. goose acts as the coordinator, dispatching work to whichever agent is best suited for the task — and they all have access to your extensions, so they can use your tools.

## Best practices

A few things to keep in mind when you start orchestrating:

### Partition your writes

Never let two delegates modify the same file. They run independently and will conflict. If two agents need to touch the same file, make them sequential — one finishes before the other starts.

### Parallelize your reads

Research tasks are safe to run in parallel. Multiple agents can read the same codebase, docs, or APIs simultaneously without stepping on each other.

### Be explicit in instructions

Delegates only know what you tell them. They don't share context with each other or with the parent session. If a delegate needs information from a previous phase, pass it explicitly in the instructions.

### Start simple

You don't need to orchestrate everything. A single subagent is fine for straightforward tasks. Orchestration shines when you have genuinely independent work streams or need multiple perspectives on the same problem.

## Get started

If you're already using goose, you can start orchestrating today. The simplest way is just to ask:

```
Do these three things in parallel: [task A], [task B], [task C]
```

goose handles the rest — spinning up delegates, running them concurrently, and bringing the results together.

For reusable workflows, check out the [subrecipes in parallel tutorial](/docs/tutorials/subrecipes-in-parallel/) to build recipes you can share with your team. And if you want to bring in external agents, the [ACP providers guide](/docs/guides/acp-providers) will get you set up with Claude Code, Codex, or Amp.

<head>
  <meta property="og:title" content="Orchestrate Complex Workflows Across Multiple Agents with goose" />
  <meta property="og:type" content="article" />
  <meta property="og:url" content="https://block.github.io/goose/blog/2026/05/05/goose-as-conductor" />
  <meta property="og:description" content="Use goose to coordinate multi-agent workflows — decompose complex tasks, delegate to specialized agents in parallel, and synthesize the results." />
  <meta property="og:image" content="https://block.github.io/goose/img/blog/goose-conductor.png" />
  <meta name="twitter:card" content="summary_large_image" />
  <meta property="twitter:domain" content="block.github.io/goose" />
  <meta name="twitter:title" content="Orchestrate Complex Workflows Across Multiple Agents with goose" />
  <meta name="twitter:description" content="Use goose to coordinate multi-agent workflows — decompose complex tasks, delegate to specialized agents in parallel, and synthesize the results." />
  <meta name="twitter:image" content="https://block.github.io/goose/img/blog/goose-conductor.png" />
</head>
