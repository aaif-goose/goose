---
title: Research → Plan → Implement Pattern
description: How to use RPI, a context engineering technique, on complex software projects
---

import CodeBlock from '@theme/CodeBlock';
import researchDoc from '../../static/files/thoughts/research/2025-12-22-llm-tool-selection-strategy.raw';
import planDoc from '../../static/files/thoughts/plans/2025-12-23-remove-tool-selection-strategy.raw';


Most people use AI agents by jumping straight to execution: "refactor this code", "remove this feature", "add this new feature". While sometimes this works well, especially for smaller changes or codebases, it often falls apart on complex changes. 

**[RPI (Research, Plan, Implement)](https://github.com/humanlayer/advanced-context-engineering-for-coding-agents/blob/main/ace-fca.md)**, introduced by HumanLayer, proposes a different way of working with AI agents. This approach trades speed for clarity, predictability, and correctness.

This tutorial walks through how RPI works via a real demonstration. By the end, you should be able to run this same workflow on your own codebase.

## Prerequisites

<details>
<summary>1. Import RPI Recipes</summary>

Copy the snippet below and paste it in your terminal. This will download the main RPI recipes and their subrecipes and save them into the global recipe directory.

```sh
mkdir -p ~/.config/goose/recipes/subrecipes

curl -sL https://raw.githubusercontent.com/aaif-goose/goose/main/documentation/src/pages/recipes/data/recipes/rpi-research.yaml -o ~/.config/goose/recipes/rpi-research.yaml
curl -sL https://raw.githubusercontent.com/aaif-goose/goose/main/documentation/src/pages/recipes/data/recipes/rpi-plan.yaml -o ~/.config/goose/recipes/rpi-plan.yaml
curl -sL https://raw.githubusercontent.com/aaif-goose/goose/main/documentation/src/pages/recipes/data/recipes/rpi-implement.yaml -o ~/.config/goose/recipes/rpi-implement.yaml
curl -sL https://raw.githubusercontent.com/aaif-goose/goose/main/documentation/src/pages/recipes/data/recipes/rpi-iterate.yaml -o ~/.config/goose/recipes/rpi-iterate.yaml

curl -sL https://raw.githubusercontent.com/aaif-goose/goose/main/documentation/src/pages/recipes/data/recipes/subrecipes/rpi-codebase-locator.yaml -o ~/.config/goose/recipes/subrecipes/rpi-codebase-locator.yaml
curl -sL https://raw.githubusercontent.com/aaif-goose/goose/main/documentation/src/pages/recipes/data/recipes/subrecipes/rpi-codebase-analyzer.yaml -o ~/.config/goose/recipes/subrecipes/rpi-codebase-analyzer.yaml
curl -sL https://raw.githubusercontent.com/aaif-goose/goose/main/documentation/src/pages/recipes/data/recipes/subrecipes/rpi-pattern-finder.yaml -o ~/.config/goose/recipes/subrecipes/rpi-pattern-finder.yaml
```
</details>

<details>
<summary>2. Add Custom Slash Commands</summary>

Now that the recipes are imported, to quickly invoke them in-session [add custom slash commands](/docs/guides/context-engineering/slash-commands) for each of the following recipes:

| Recipe | Slash Command |
|--------|---------------|
| RPI Research Codebase | `research_codebase`|
| RPI Create Plan | `create_plan` |
| RPI Implement Plan | `implement_plan` |
| RPI Iterate | `iterate_plan` |
</details>

## RPI Workflow

In goose, we use a structured RPI workflow using recipes to systematically tackle complex codebase changes. The workflow consists of slash commands that guide goose through disciplined phases of work:

1. `research_codebase` – Document what exists today. No opinions.
2. `create_plan` - Design the change with clear phases and success criteria.
3. `implement_plan` - Execute the plan step by step with verification.
4. `iterate_plan` – (optional) Adjust the plan if necessary.

```md
┌─────────────────────────────────────────────────────────────────────────────-┐
│                           RPI WORKFLOW                                       │
├─────────────────────────────────────────────────────────────────────────────-┤
│                                                                              │
│  /research_codebase "topic"                                                  │
│       │                                                                      │
│       ├──► Spawns parallel sub-agents:                                       │
│       │    • find_files (rpi-codebase-locator)                               │
│       │    • analyze_code (rpi-codebase-analyzer)                            │
│       │    • find_patterns (rpi-pattern-finder)                              │
│       │                                                                      │
│       └──► Output: thoughts/research/YYYY-MM-DD-HHmm-topic.md                │
│                                                                              │
│  /create_plan "feature/task"                                                 │
│       │                                                                      │
│       ├──► Reads research docs                                               │
│       ├──► Asks clarifying questions                                         │
│       ├──► Proposes design options                                           │
│       │                                                                      │
│       └──► Output: thoughts/plans/YYYY-MM-DD-HHmm-description.md             │
│                                                                              │
│  /implement_plan "plan path"                                                 │
│       │                                                                      │
│       ├──► Executes phase by phase                                           │
│       ├──► Runs verification after each phase                                │
│       ├──► Updates checkboxes in plan                                        │
│       │                                                                      │
│       └──► Working code                                                      │
│                                                                              │
│  /iterate_plan "plan path" + feedback                                        │
│       │                                                                      │
│       ├──► Researches only what changed                                      │
│       ├──► Updates the plan surgically                                       │
│       │                                                                      │
│       └──► Updated plan                                                      │
│                                                                              │
└─────────────────────────────────────────────────────────────────────────────-┘
```

All RPI outputs live in a predictable place:

```md
thoughts/
├── research/
│   └── YYYY-MM-DD-HHmm-topic.md
└── plans/
    └── YYYY-MM-DD-HHmm-description.md
```

## The Task

In this tutorial, I want to remove an existing feature from a large codebase.

This isn't a small change. The feature touches:

- Core Rust code
- TypeScript
- Configuration
- Tests
- Documentation

This is the kind of task where agents often struggle, not because they're incapable, but because the work spans too much context to safely "just do it."

So instead of jumping to implementation, we'll use RPI.

## Session 1: Research

The concept of planning before implementation has become a widely accepted practice. However, planning without research can lead to assumptions that come back to bite you. So, in RPI, we begin with research.

I start the prompt with the `/research_codebase` command followed by a topic written in natural language

```
/research_codebase "look through the cloned goose repo and research how the LLM Tool Discovery is implemented"
```


This command invokes the **[RPI Research Codebase](https://raw.githubusercontent.com/aaif-goose/goose/refs/heads/main/documentation/src/pages/recipes/data/recipes/rpi-research.yaml)** recipe, whose job is very strict:

- Document what exists
- Do not suggest changes
- Do not critique
- Do not plan

With this recipe, goose automatically spawns three parallel subagents:

- `find_files`: Uses the codebase locator to figure out where relevant files live.
- `analyze_code`: Reads those files fully and documents how they work.
- `find_patterns`: Looks for similar features or conventions elsewhere in the repo.

These subagents run independently and report back. You don't have to orchestrate that yourself.

:::info A course correction
After goose began researching, I noticed that it was researching "tool discovery" in general. But I only wanted to remove a specific feature called Tool Selection Strategy. So I stopped goose and reran research with a more accurate topic.

That wasn't a failure. In fact, this is exactly why research exists. Had I told goose to "remove the LLM Tool Discovery feature", it may have removed our other tool discovery methods as well. Fortunately, catching these types of mistakes early is cheap and easy to recover from.
:::

The output from the `/research_codebase` session was a detailed research document:

<details>
<summary>./thoughts/research/2025-12-22-llm-tool-selection-strategy.md</summary>

<CodeBlock language="markdown">
{researchDoc}
</CodeBlock>

</details>

This is a large, structured file that includes:
- Git metadata
- File and line references
- Flow descriptions
- Key components
- Open questions

Think of it as a technical map of the feature as it exists today. Nothing was changed yet, and that's intentional. The only goal was shared understanding.

As the human in the loop, be sure to review the research! This will inform the plan, so you want to make sure this is accurate.

## Session 2: Plan

Once research is complete, you move to planning. 

:::tip Sessions
It's important to do each phase in a new session to keep the LLM laser focused on only the task at hand. One goal per session!
:::

```
/create_plan a removal of the Tool Selection Strategy feature
```

The **[RPI Create Plan](https://raw.githubusercontent.com/aaif-goose/goose/refs/heads/main/documentation/src/pages/recipes/data/recipes/rpi-plan.yaml)** recipe starts by reading the research document goose created.

Then it did three key things:

1. **Asked clarifying questions**

    For example:
    - Full removal vs deprecation?
    - How should config cleanup behave?
    - Should OpenAPI artifacts be regenerated?
    - Where do related tests live?

2. **Presented design options**
    
    Where there were multiple reasonable approaches, goose laid them out and asked me to choose.

3. **Produced a phased implementation plan**


The output was a detailed plan:

<details>
<summary>thoughts/plans/2025-12-23-remove-tool-selection-strategy.md</summary>

<CodeBlock language="markdown">
{planDoc}
</CodeBlock>

</details>

This plan includes:
- 10 explicit phases
- Exact file paths
- Code snippets showing what to remove
- Automated success criteria
- Manual verification steps
- Checkboxes for tracking progress

At this point, the plan became the source of truth. The key shift here is that we've moved from understanding to decision making, but we're still not touching code.

The plan is explicit enough that someone else could execute it. That's not an accident. Remember that the implementation will be in a fresh new session, so the plan must have enough context to actually execute it.

Again, you as the human need to step in here to review the plan and make sure it's solid. If there's anything amiss, instead of starting over you can run the **[RPI Iterate Plan](https://raw.githubusercontent.com/aaif-goose/goose/refs/heads/main/documentation/src/pages/recipes/data/recipes/rpi-iterate.yaml)** plan (`/iterate_plan`) with details on what's wrong. goose will then read the existing plan, research only what needs rethinking, propose targeted updates, and edit the plan accordingly.

## Session 3: Implement

Only after research and planning are complete should you move to implementation. Pass in the plan document.

```
/implement_plan thoughts/plans/2025-12-23-remove-tool-selection-strategy.md
```

The **[RPI Implement Plan](https://raw.githubusercontent.com/aaif-goose/goose/refs/heads/main/documentation/src/pages/recipes/data/recipes/rpi-implement.yaml)** recipe is intentionally boring. In fact, I fell asleep while goose was running it. Implementation should feel mechanical. If it feels creative, something upstream is missing. But knowing that you have a rock solid plan, I advise you to go do something else with your time while goose works (unless there are manual steps in the plan).

It will read the plan completely, execute the phases in order, run verification after each phase, and update the checkboxes directly in the plan file as it goes.

That last bit was really helpful because my context window filled up partway through and goose was able to compact and pick up right where it left off because of the status updates in the plan.

## Final Result

For 10 phases of work that spanned 32 files, the Research phase took 9 minutes, the Plan phase took 4 minutes, and the Implement phase took 39 minutes. So in total, this took just shy of an hour... 52 minutes to be exact. This included goose working and testing as well as me answering questions.

Definitely not a fast process. BUT! When I put up [this PR](https://github.com/aaif-goose/goose/pull/6250), the build passed and the separate Code Review Agent didn't have a single comment. That's just how well done the work was.

Had I done this without AI, it would have likely taken me several hours of work as the feature was complex and deeply integrated. And had I had AI jump straight to implementation, I have no doubt it would have surely drifted and messed something up.

So while RPI is slower than having AI get right to work, the quality is top notch. A very worthy tradeoff.


## When to Use RPI

For basic tasks, RPI may be overkill. Especially because it's not a quick process. However, if you need to do a complex task that spans multiple files, it's a great choice.

You can use RPI for:
- Refactors
- Migrations
- Feature additions
- Large upgrades
- Incident cleanup
- Documentation overhauls

Try it for yourself!

