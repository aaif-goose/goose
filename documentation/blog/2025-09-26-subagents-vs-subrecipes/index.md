---
title: "How to Choose Between Subagents and Subrecipes in goose"
description: When you need to break complex work into multiple AI tasks, should you use subagents or subrecipes? Learn the key differences and when to use each approach.
authors: 
    - ian
---

![Subagents vs Subrecipes](subrecipes-vs-subagents.png)

When you're working on complex projects with goose, you'll often need to break work into multiple tasks and run them with AI agents. Goose gives you two powerful ways to do this: [subagents](/docs/guides/subagents/) and [subrecipes](/docs/tutorials/subrecipes-in-parallel/). Both can run multiple AI instances in parallel, but they work  differently. Picking which one to use can be confusing, so we're going to guide you to a decision.

I've been using both approaches, and the choice between them depends on what you're trying to accomplish. Let me break down when to use each method and show you real examples.

<!--truncate-->

## The Core Difference: To Re-Use or Not Re-Use

Subagents are temporary AI instances that you create with natural language in your prompt and tend to be one-off tasks and then disappear.

Subrecipes are pre-written files full of instructions that define reusable workflows you can run repeatedly with customized parameters.

The TL;DR: subagents are for quick, one-off delegation. Subrecipes are for structured, repeatable processes.

Also, both are still in 'experimental' status, so there's always a possibility their features and capabilities will change over time.


## Subagents: Quick and Flexible

Subagents excel when you're in a goose session and just want task delegation without setup overhead. Here's how simple they are to use:

```
Build a simple task management web app doing these 3 tasks in parallel:

- one task writes the backend API code (Node.js/Express with basic CRUD operations for tasks)
- one task writes comprehensive tests for the API endpoints
- one task creates user documentation explaining how to use the API

Each task should work independently and complete their part simultaneously.
```

Goose automatically spawns three independent AI instances, and each works on a different component. You get real-time progress tracking and they all work in parallel from that prompt. You're off and running: no setup, no configuration files, just natural language instructions.

### What Makes Subagents Great

* **Quick setup with natural language** -- No recipe files to write. Just describe what you want in your prompt.
* **Process isolation** -- Failures don't affect your main workflow. Each subagent runs independently.
* **Context preservation** -- Keeps your main chat clean by offloading detailed work to separate instances.
* **Flexible execution** -- Easy to specify parallel or sequential execution in your prompt.
* **External integration** -- Can use external AI agents like Codex or Claude Code.
* **Real-time progress** -- Live monitoring dashboard shows task completion status.

### Subagent Limitations

* **Limited reusability** -- Each subagent is created from scratch. No saved configuration.
* **Common LLM** -- All subagents share the same LLM model as the parent session.
* **Tool restrictions** -- Subagents can't manage extensions, they can only use what the main session already had access to before the subagents were launched.
* **No persistence** -- Configuration isn't saved for future use.

## Subrecipes: Structured and Reusable

Subrecipes solve the reusability problem. While main "parent" recipes can be in YAML or JSON, subrecipes can only be written in YAML format. These files define structured workflows with parameters, validation, extensions to use, and even allow you to pick a different provider/model to use for the work.

For detailed subrecipe examples and implementation guides, check out our [subrecipes blog post](/blog/2025/09/15/subrecipes-in-goose) and [advanced recipe tips](https://www.youtube.com/watch?v=1szmJSKInnU) video on YouTube.

### What Makes Subrecipes Powerful

* **High reusability** -- Recipe files can be shared and version controlled across projects.
* **Structured parameters** -- Type-safe parameter handling with validation and documentation.
* **Template support** -- Dynamic parameter injection using template syntax.
* **Full tool access** -- Subrecipes can use any extensions and tools available to goose.
* **LLM customization** -- Each subrecipe can specify its own LLM model to use.
* **Conditional logic** -- Smart parameter passing based on conversation context.
* **Workflow orchestration** -- Complex multi-step processes with dependencies and execution order control.

### Subrecipe Trade-offs

* **Setup complexity** -- Requires careful YAML file creation and parameter definition.
* **Learning curve** -- Need to understand recipe syntax and structure.
* **File management** -- Must organize and maintain recipe files.


## A Decision Framework

**Use subagents when:**
- You need quick, one-off task delegation
- Tasks can be completed independently and don't need to be repeated

**Use subrecipes when:**
- Building reusable workflows you can share with your team
- Need structured parameter handling

## Common Benefits and Limitations of Both Approaches

Both of these features share some limitations. We've already mentioned the experimental and evolving nature, but there are a few more important ones to note:

* Subrecipes and subagents run their tasks in isolation and do not share state with one another. This isolation helps prevent conflicts and keeps tasks self-contained. If you do need to share information between the processes, you will have to be very explicit about this in your prompts and instructions.

* Neither can spawn additional similar workers: subagents can't create more subagents, and subrecipes can't call other subrecipes. This prevents runaway processes but limits deep nesting.

* Whether you're using subagents or subrecipes, you can only run up to 10 concurrent parallel workers total. This is not user-configurable, but this limit keeps resource usage manageable; this may limit very large-scale parallelism.


## Getting Started

My recommendation: start with subagents to experiment and understand your workflow needs. They're easier to get started with because you don't need to write configuration files first.

Once you identify patterns you want to repeat, you can convert that subagent workflow session into a recipe and subrecipe structure. This gives you the best progression from experimentation to production.

## The Choice is Yours

The choice depends on your specific needs and workflow requirements. Quick tasks that won't be repeated favor subagents. Complex workflows with multiple steps or customization favor subrecipes.

Your workflow requirements should drive the decision.

Share your subagent prompts or subrecipe ideas with us on our [Discord community](https://discord.gg/goose-oss) or [GitHub discussions](https://github.com/aaif-goose/goose/discussions).



<head>
  <meta property="og:title" content="How to Choose Between Subagents and Subrecipes in goose" />
  <meta property="og:type" content="article" />
  <meta property="og:url" content="https://goose-docs.ai/blog/2025-09-26-subagents-vs-subrecipes" />
  <meta property="og:description" content="When you need to break complex work into multiple AI tasks, should you use subagents or subrecipes? Learn the key differences and when to use each approach." />
  <meta property="og:image" content="https://goose-docs.ai/assets/images/subrecipes-vs-subagents-19bca16b86a951e4618be8ab6ce90fb2.png" />
  <meta name="twitter:card" content="summary_large_image" />
  <meta property="twitter:domain" content="goose-docs.ai" />
  <meta name="twitter:title" content="How to Choose Between Subagents and Subrecipes in goose" />
  <meta name="twitter:description" content="When you need to break complex work into multiple AI tasks, should you use subagents or subrecipes? Learn the key differences and when to use each approach." />
  <meta name="twitter:image" content="https://goose-docs.ai/assets/images/subrecipes-vs-subagents-19bca16b86a951e4618be8ab6ce90fb2.png" />
</head>
