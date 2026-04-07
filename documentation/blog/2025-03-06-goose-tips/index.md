---
title: 6 Essential Tips for Working with goose
description: Practical tips to help you use goose more effectively and efficiently.
authors: 
    - angie
---

![goose tips](goose-tips.png)

Working with AI agents can sometimes feel unpredictable. After using goose extensively for the last few months, I've compiled a few key tips that will help you get the most out of this tool. No matter your workflow, these guidelines will help you work more efficiently with goose.


<!--truncate-->

## 1. Keep Sessions Focused and Short

One of the most common mistakes users make is trying to accomplish too much in a single session. While it might seem efficient to keep the conversation going, longer sessions can actually hinder goose's performance. 

Every message adds to the context window, which is the amount of conversation history goose can retain at any given time. This history is made up of tokens, the individual pieces of text (words or even parts of words) that goose processes to generate responses. More tokens don’t just increase processing time, they also contribute to LLM usage costs. And once the context window fills up, older messages get pushed out, which can lead to loss of important details or unexpected behavior.

Think of it like keeping too many browser tabs open. Eventually, it impacts performance. Instead, start fresh sessions for distinct tasks. Don't worry about losing context; that's exactly what the [Memory extension](/docs/mcp/memory-mcp) is for. Keeping sessions focused and concise ensures more accurate, relevant responses while also keeping your LLM costs under control.


## 2. Minimize Active Extensions

When it comes to goose extensions, less is often more. It's tempting to enable [every available extension](https://www.pulsemcp.com/servers) just in case (I'm guilty of this!), but this approach can be counterproductive. Each active extension adds to the system prompt, increasing complexity and making goose work harder to decide which tools to use.

Consider this: if you're cooking in a kitchen, having every possible utensil and appliance out on the counter doesn't make you a better chef. It just creates clutter and confusion. The same principle applies here. 

Go ahead and install any extensions that interest you, but [keep them disabled](/docs/getting-started/using-extensions#enablingdisabling-extensions) until you need them. Start with the built-in [Developer extension](/docs/mcp/developer-mcp) enabled, which is surprisingly powerful on its own, and only enable others when you need their specific capabilities. This leads to faster responses, lower token usage, and often more focused solutions.

:::tip Bonus Tip
Before starting a complex task, ask goose about its current capabilities. A simple prompt like "Do you have tools available to work with [specific technology/service]?" can save time and prevent false starts. goose can tell you whether it has the necessary tools for your task, and if not, suggest which extensions you might need to enable. This quick check ensures you have the right tools ready before diving in too deep.
:::

## 3. Teach goose with .goosehints Files


One of goose's most powerful features is its ability to understand context through [.goosehints](/docs/guides/context-engineering/using-goosehints) files, acting like a "README for AI". These hints can be set at both the project and global levels to guide goose’s responses.

At the project level, placing .goosehints files in your directory helps goose understand your structure, conventions, and special considerations. You can even use multiple files - one at the root for overall guidance and others in specific directories for more granular instructions (e.g., frontend styling conventions).

Beyond projects, global .goosehints files (`~/.config/goose/.goosehints`) apply across all sessions, making them perfect for things like:

* Personal coding style preferences
* Favorite tools and workflows
* Standard testing practices
* Documentation conventions
* Git commit message formatting

## 4. Choose the Right Mode for Your Workflow

goose offers [different modes](/docs/guides/goose-permissions) that determine how much autonomy it has when modifying files, using extensions, and performing automated actions. 

* ⚡️ **Auto Mode (Default):** goose can modify, create, and delete files, as well as use extensions, without requiring approval. Best for users who want seamless automation.

* ✅ **Approve Mode:** goose asks for confirmation before making changes. With [Smart Approve](/docs/guides/goose-permissions#permission-modes) enabled, it evaluates risk levels and prompts for high-risk actions while executing safe ones automatically.

* 💬 **Chat Mode:** goose operates in chat-only mode, without modifying files or using extensions. Ideal for users who want AI assistance without automation.

If you’re new to goose or working on a critical project, Approve Mode offers a great balance of automation and oversight. For hands-free workflows, Auto Mode keeps things moving, while Chat Mode is perfect for brainstorming and general AI assistance.

## 5. Guide goose with Step-by-Step Execution

Complex tasks are best handled in stages, and goose excels when you allow it to break problems into manageable steps. Instead of expecting an instant solution, ask goose to generate a step-by-step plan first. Review the plan to ensure it aligns with your goals, then let goose execute each step in sequence.

This structured approach not only improves accuracy but also gives you more control over the process. You can pause, adjust, or refine each step as needed, giving you more control while ensuring better results.

## 6. Refine and Iterate for Better Responses

goose is powerful, but like any AI, it sometimes needs a second pass to get things right. If you don’t get the response you need, try refining your prompt or asking goose to adjust its answer.

Good iteration techniques include:

* Asking goose to explain its reasoning before taking action
* Requesting alternative solutions to compare different approaches
* Asking for a step-by-step breakdown of its thought process
* Rewording prompts to add more detail or constraints

For example, instead of asking, "Help me debug this error," try, "I’m getting a NullPointerException in my Java method. Here’s the stack trace. What could be causing it?" A small tweak in how you ask can dramatically improve the quality of the response.

---

By following these tips, you'll be able to work more effectively with goose, getting better results while using fewer resources. Remember, the goal is to solve problems efficiently and effectively. Whether you're writing code, automating tasks, or managing complex projects, these guidelines will help you make the most of what goose has to offer.

<head>
  <meta property="og:title" content="6 Essential Tips for Working with goose" />
  <meta property="og:type" content="article" />
  <meta property="og:url" content="https://goose-docs.ai/blog/2025/03/06/goose-tips" />
  <meta property="og:description" content="Practical tips to help you use goose more effectively and efficiently." />
  <meta property="og:image" content="https://goose-docs.ai/assets/images/goose-tips-4add28cc7201737dfb468ad11980f070.png" />
  <meta name="twitter:card" content="summary_large_image" />
  <meta property="twitter:domain" content="goose-docs.ai" />
  <meta name="twitter:title" content="6 Essential Tips for Working with goose" />
  <meta name="twitter:description" content="Practical tips to help you use goose more effectively and efficiently." />
  <meta name="twitter:image" content="https://goose-docs.ai/assets/images/goose-tips-4add28cc7201737dfb468ad11980f070.png" />
</head>