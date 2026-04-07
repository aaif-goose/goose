---
title: How to Successfully Migrate Your App with an AI Agent
description: A step-by-step prompt strategy for AI-assisted code migration with real examples from a Next.js refactor
authors: 
    - rizel
---

![How to Successfully Migrate Your App with an AI Agent](migrate-app-ai-agent.png)

"Migrate my app from x language to y language." You hit enter, watch your AI agent spin its wheels, and eventually every success story you've heard feels like a carefully orchestrated lie.

Most failures have less to do with the agent's capability and more to do with poor prompt and context strategy. Think about it: if someone dropped you into a complex, unfamiliar codebase and said "migrate this," you'd be lost without a plan. You'd need to explore the code, ask questions about its structure, and break the work into manageable steps.

Your AI agent needs the same approach: guided exploration, strategic questions, and decomposed tasks.

<!--truncate-->

I recently put this approach into practice with [goose](/), migrating a legacy LLM credit provisioning system split across two repositories (React/Vite frontend and Node/Express backend) into a unified Next.js framework.

## Why I Needed to Refactor

I originally built the app to distribute LLM API credits at a Boston meetup. It was a quick prototype that experienced unexpected adoption, exposing fundamental architectural problems. (And I have shiny toy syndrome, so I struggled to return to the app to improve it). I wanted to make the following improvements:

- Email-based provisioning
- Dynamic credit allocation per event
- Analytics infrastructure
- Admin panel

I hopped on a livestream to tackle this huge refactor but seconds in I realized I realistically could not do this all in one hour. I focused on consolidating the fragmented codebase first. Two repositories (React/Vite frontend, Express backend) needed to become one monolithic Next.js application. But simply telling goose "Convert to Next.js" wouldn't work without proper context building.

## My Prompt Strategy

### Building a Shared Mental Model

Before I instructed goose to write any code, I prioritized helping it understand the codebase systematically with the following prompt:

> Can you get a lay of the land for the two projects found here and how they communicate?

goose employed the [analyze tool](/docs/mcp/developer-mcp#developer-extension-tools) to generate a high-level architectural flow. (The analyze tool is part of the [Developer extension](/docs/mcp/developer-mcp), an [MCP server](https://modelcontextprotocol.io/docs/learn/server-concepts) that's built into goose).

```
User Browser
    ↓
[goose-access-gateway] (React SPA)
    ↓ (HTTPS REST API)
[goose-hacknight-backend] (Express API)
    ↓ (HTTPS REST API)
[OpenRouter API] (Third-party service)
```

It also shared all the various endpoints and how to run the repos. This mapping served dual purposes: establishing the agent's contextual foundation and refreshing my own mental model of an eight-month-old implementation.

### Defining the Scope

With the landscape mapped, I needed to prevent scope creep. I deliberately focused the agent's attention on the frontend to avoid chaotic, uncontrolled changes across the entire codebase.

> Tell me the commands to run the frontend project.

Yes, I could have found the commands in the package.json, but asking goose to do it served a purpose: it grounded goose in the actual project setup and prevented it from hallucinating commands or ports.

:::tip Pro tip
I always have goose tell me what commands to run (like npm run dev) rather than executing them itself. Long-running or blocking commands can halt goose's process.
:::

### Verification Driven Development

One major pitfall of AI-assisted coding is that agents cannot validate their code beyond syntactic correctness. 

To address this, I enabled the [Chrome Dev Tools extension](/docs/mcp/chrome-devtools-mcp), granting the agent browser-level inspection capabilities: DOM manipulation verification, CSS property validation, and performance profiling. This extension gave goose "eyes", which meant I could give it my most ambitious prompt yet:

> I have the frontend running right now on localhost:8080. I want to take this UI design and start from scratch a bit. I need all new logic especially for the backend. Can we create a new directory and create a Next.js project and for now let's just do the frontend, but don't add any of the API calls or anything. We just want to retain the design of the frontend page. Please recreate that. Use the Chrome Dev Tools extension to see how the UI looks so you can copy it and use the to do extension to help you plan. If there are interactive commands or you can run an install or something like that just tell me to do it...and give me the details of what I need to run.

This was a huge prompt, so let's break down what each part accomplished:

- **Isolation:** create a new directory
- **Scope:** just do the frontend, but don't add any of the API calls
- **Verification:** Use the Chrome Dev Tools extension to see how the UI looks
- **Planning:** use the to do extension to help you plan
- **Interaction:** just tell me to do it...and give me the details of what I need to run

:::note
In retrospect, the instruction regarding blocking commands should have been codified in [persistent context files](/docs/guides/context-engineering/using-goosehints) ([AGENTS.md](https://agents.md/) or [goosehints](/docs/guides/context-engineering/using-goosehints)) rather than inline prompts.
:::

But, I was so happy that goose generated a pixel perfect recreation of the app. 

### Task Decomposition

The agent's successful, perfect recreation of the UI was largely due to the [Todo extension](/docs/mcp/todo-mcp), an MCP server that's built into goose. I find that this extension helps prevent scope drift, where agents autonomously expand into adjacent functionality after completing an objective.

The to do list included items like:

- Copy logo assets from old project
- Create glass-morphism card component
- Add logo with fade in animation
- Verify theme toggle works

When I ran the app locally, I did encounter a Tailwind CSS v4/v3 syntax error, but goose used the Chrome Dev Tools extension and the Todo extension to quickly fix it.

### Automated Version Control

Because my UI was pixel-perfect, I felt confident enough to introduce some backend logic, but I knew introducing this level of complexity would require granular version control. When an agent makes a dozen changes, it's easy to end up with unwanted code buried in the history. Manually tracking and reverting these changes is tedious. 

To solve this problem, I instituted an automated commit policy by adding a persistent directive to my .goosehints file:

> Every time you make a change, make a commit using the GitHub CLI or the GitHub MCP Server.

### Pattern Replication

The final step was to add the backend logic for emailing API keys. Instead of asking goose to invent this from scratch, I had it learn from a known-working system: a separate app with similar provisioning logic.

I gave goose the following prompt:

> There's a recipe cookbook. To submit people have to open up a PR and then it sends them an email with an API key. Are you able to find the logic where it sends the API key?

Once it analyzed that code, I gave the final instruction:

> Use what you learned from the recipe project logic to make this happen in goose-credits... send the API key to their email using the SendGrid API.

This "copy-and-adapt" strategy was incredibly effective. goose successfully implemented the necessary API routes and clearly identified the environment variables I needed to supply. I manually added those variables. I didn't give them to goose for security purposes.

## The Lesson

I shared my messy, tedious conversation with goose (using Claude Sonnet 4.5) so that readers can confidently ditch one-shot prompts for complex tasks and work incrementally with agents. Just like coding, collaborating with an agent requires patience, but it doesn't have to feel stressful. 

I hope this clarifies how to converse with an agent and accomplish complex tasks like migrations. If you want to see this in action, you're in luck; below is a VOD livestream of me navigating the project in real-time.

<iframe class="aspect-ratio" src="https://www.youtube.com/embed/zGyXfA3kKTk" title="How to Successfully Migrate Your App with an AI Agent" frameborder="0" allow="accelerometer; autoplay; clipboard-write; encrypted-media; gyroscope; picture-in-picture" allowfullscreen></iframe>

*Ready to try AI-assisted migration with goose? Get started with our [quickstart guide](/docs/quickstart) and share your experience in our [Discord community](http://discord.gg/goose-oss).*


<head>
  <meta property="og:title" content="How to Successfully Migrate Your App with an AI Agent" />
  <meta property="og:type" content="article" />
  <meta property="og:url" content="https://goose-docs.ai/blog/2025/11/17/migrate-app-with-ai-agent" />
  <meta property="og:description" content="A step-by-step prompt strategy for AI-assisted code migration with real examples from a Next.js refactor" />
  <meta property="og:image" content="https://goose-docs.ai/assets/images/migrate-app-ai-agent-e8e3dcddf74909b6f84f85c8c776aaed.png" />
  <meta name="twitter:card" content="summary_large_image" />
  <meta property="twitter:domain" content="goose-docs.ai" />
  <meta name="twitter:title" content="How to Successfully Migrate Your App with an AI Agent" />
  <meta name="twitter:description" content="A step-by-step prompt strategy for AI-assisted code migration with real examples from a Next.js refactor" />
  <meta name="twitter:image" content="https://goose-docs.ai/assets/images/migrate-app-ai-agent-e8e3dcddf74909b6f84f85c8c776aaed.png"/>
</head>
