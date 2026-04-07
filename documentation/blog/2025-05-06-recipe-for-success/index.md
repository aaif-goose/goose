---
title: "A Recipe for Success: Cooking Up Repeatable Agentic Workflows"
description: A fresh look at AI agents, orchestration, and repeatability told through the metaphor of a rat who can cook.
authors: 
    - rizel
---
![blog cover](cookingwithgoose.png)

# A Recipe for Success: Cooking Up Repeatable Agentic Workflows

Ratatouille isn't just a heartwarming (and slightly unhygienic) film about a rat chef. It's also analogous to a popular tech trend: AI agents and the [Model Context Protocol (MCP)](https://modelcontextprotocol.io/).

<!-- truncate -->

---

## The Remy-Linguini Dynamic

If you haven't seen the movie, here's the gist: Remy is an incredible chef with all the know-how, but he's a rat, so no kitchen access. Linguini is a kitchen worker with full access but little cooking skill. Together, they form a symbiotic relationship: Remy hides under Linguini's hat and guides him on what tools to use and when. 

If a customer orders fries, Linguini might freeze, but Remy scopes out the kitchen, sees what's available, and gives step-by-step instructions: 

> _"Grab a knife and a cutting board. Now slice the potato."_ 

Then, Linguini executes the plan.

---

## Traditional AI Agents

Agentic systems work similarly. You have three core components:

* A Large Language Model (LLM)  
* An Agent  
* Tools

The LLM is like Remy; it is full of knowledge and reasoning, but has no hands-on access. The agent is like Linguini; it can take action, but needs guidance.

If a user says, "Write some unit tests," the LLM analyzes the code and replies, 

> _"Looks like JavaScript. Use Jest, create a test file, and import the module."_ 

The agent follows the plan and uses tools like `file.write()` to get it done.

---

## Linguini's Evolution

But Linguini's story doesn't stop there. Even with Remy's guidance, he's still clumsy, unsure how to confidently move through the kitchen. His boss, Chef Skinner, notices something's off. To help him improve, Linguini is paired with Colette, a seasoned cook who shows him how the kitchen works:

* Where tools live
* How stations are organized
* How to move efficiently through space
* When to pivot if something's missing

With Colette's guidance, Linguini understands the kitchen as a system. When a customer orders spaghetti, Remy quickly forms a plan: 

> _"Boil the pasta, sauté the garlic and tomatoes, plate it with basil."_

Instead of mindlessly following orders, Linguini is equipped to orchestrate the entire operation by:

* Heading to the pasta station to get water boiling
* Checking the sauté station for clean pans and fresh garlic
* Grabbing the right tools: colander, ladle, sauté pan
* Finding spare pans or changing flow when needed
* Managing ingredients and backup supplies
* Coordinating timing so everything finishes in sync
* Plating and delivering dishes confidently

---

## Built Different

That's how it works with AI agents that follow the Model Context Protocol(MCP). MCP shifts the agent from passive executor to active orchestrator, making it less reliant on an LLM and more aware of the context in which it's operating.

[Goose](/) is a local, open source AI agent that follows the structure of MCP.

MCP provides a standardized way for agents to interact with external data and services. It has three core components:

* **MCP Host** – the core agent that receives a plan and coordinates the task
* **MCP Client** – a local connector used by the host to talk to external services
* **MCP Servers** – a package of tools, data, or prompts in a structured format. In the Goose ecosystem, we refer to MCP servers as extensions.

This architecture allows Goose to discover tools dynamically, understand how to use them, and orchestrate complex workflows across multiple systems.

---

## Goose as an Orchestrator

When a user prompts Goose to "Gather all discussions about the authentication bug from last week," Goose orchestrates the operation. It coordinates tools, manages execution, and adapts on the fly by:

* Identifying the right MCP servers: Slack, GitHub, PostgreSQL  
* Understanding when a tool isn't working as expected  
* Considering alternative approaches when needed

When something breaks, Goose doesn't panic; it pivots. For example, Goose might determine:

* "Slack search isn't returning last week's messages. Let me try a different date range."  
* "If we still can't access those, the PR comments might have the key points."

---

## Scaling Agentic Workflows with Recipes 

It's been 18 years since the movie came out, and I'd like to imagine that Linguini has surpassed his cooking era and stepped into his mentor era. Instead of training every new cook inefficiently, he's documenting his favorite dishes to make his knowledge shareable and scalable.

Similarly, Goose is a forward-looking AI agent with a solution for scaling knowledge through [recipes](/docs/guides/recipes/session-recipes). Recipes are complete orchestrations you can rerun, remix, or share, passing on knowledge to anyone who needs it.

Sharing a prompt doesn't always recreate the experience; AI is non-deterministic, and people may not have the same extensions or context configured. Recipes solve this by packaging your entire Goose workflow: the extensions, the setup, the goal, and example activities.

**Let's try it:**
The link below is a recipe that lets you choose your favorite platform (GitHub, Bluesky, or Dev.to) and builds a custom, story-driven 404 portfolio page using your public content.

> [Create a 404-style portfolio page with Goose](goose://recipe?config=eyJ2ZXJzaW9uIjoiMS4wLjAiLCJ0aXRsZSI6IjQwNFBvcnRmb2xpbyIsImRlc2NyaXB0aW9uIjoiQ3JlYXRlIHBlcnNvbmFsaXplZCwgY3JlYXRpdmUgNDA0IHBhZ2VzIHVzaW5nIHB1YmxpYyBwcm9maWxlIGRhdGEiLCJpbnN0cnVjdGlvbnMiOiJDcmVhdGUgYW4gZW5nYWdpbmcgNDA0IGVycm9yIHBhZ2UgdGhhdCB0ZWxscyBhIGNyZWF0aXZlIHN0b3J5IHVzaW5nIGEgdXNlcidzIHJlY2VudCBwdWJsaWMgY29udGVudCBmcm9tICoqb25lKiogb2YgdGhlIGZvbGxvd2luZyBwbGF0Zm9ybXM6ICoqR2l0SHViKiosICoqRGV2LnRvKiosIG9yICoqQmx1ZXNreSoqLiBZb3UgZG8gbm90IG5lZWQgdG8gdXNlIGFsbCB0aHJlZeKAlGp1c3QgdGhlIG9uZSBzZWxlY3RlZCBieSB0aGUgdXNlci5cblxuVGhlIHBhZ2Ugc2hvdWxkIGJlIGZ1bGx5IGJ1aWx0IHdpdGggKipIVE1MLCBDU1MsIGFuZCBKYXZhU2NyaXB0KiosIGZlYXR1cmluZzpcblxuKiBSZXNwb25zaXZlIGRlc2lnblxuKiBQZXJzb25hbCBicmFuZGluZyBlbGVtZW50cyAoZS5nLiwgbmFtZSwgaGFuZGxlLCBhdmF0YXIpXG4qIE5hcnJhdGl2ZS1kcml2ZW4gbGF5b3V0IHRoYXQgdHVybnMgdGhlIGVycm9yIGludG8gYW4gb3Bwb3J0dW5pdHkgZm9yIGRpc2NvdmVyeVxuXG5Vc2UgcGxhdGZvcm0tc3BlY2lmaWMgbWV0aG9kcyB0byBmZXRjaCByZWNlbnQgdXNlciBjb250ZW50OlxuXG4qIEZvciAqKkRldi50byoqLCB1c2UgdGhlIFtwdWJsaWMgRGV2LnRvIEFQSV0oaHR0cHM6Ly9kZXZlbG9wZXJzLmZvcmVtLmNvbS9hcGkpIHRvIHJldHJpZXZlIHJlY2VudCBhcnRpY2xlcywgcmVhY3Rpb25zLCBhbmQgcHJvZmlsZSBpbmZvcm1hdGlvbi5cbiogRm9yICoqR2l0SHViKiosIHVzZSB0aGUgR2l0SHViIFJFU1Qgb3IgR3JhcGhRTCBBUEkgdG8gYWNjZXNzIHJlY2VudCByZXBvcywgY29tbWl0cywgYW5kIGNvbnRyaWJ1dGlvbnMuXG4qIEZvciAqKkJsdWVza3kqKiwgdXNlIHB1YmxpYyBmZWVkIGVuZHBvaW50cyBmcm9tIHRoZSBBcHBWaWV3IEFQSSAoZS5nLiwgYGFwcC5ic2t5LmZlZWQuZ2V0QXV0aG9yRmVlZGApIHRvIHB1bGwgcG9zdHMsIHJlcGxpZXMsIG9yIGxpa2VzLlxuXG5JbmNvcnBvcmF0ZSB0aGUgZmV0Y2hlZCBkYXRhIGludG8gYSBjb21wZWxsaW5nIG5hcnJhdGl2ZSAoZS5nLiwg4oCcTG9va3MgbGlrZSB0aGlzIHBhZ2UgaXMgbWlzc2luZywgYnV0IFxcW3VzZXJuYW1lXSBoYXMgYmVlbiBidXN5IeKAnSksIGFuZCBkaXNwbGF5IGl0IHVzaW5nIGVuZ2FnaW5nIHZpc3VhbHMgbGlrZSBjYXJkcywgdGltZWxpbmVzLCBvciBtZWRpYSBlbWJlZHMuXG5cbldyYXAgdGhlIHVzZXLigJlzIGFjdGl2aXR5IGludG8gYSBzdG9yeSDigJQgZm9yIGV4YW1wbGU6XG5cbuKAnFRoaXMgcGFnZSBtYXkgYmUgbG9zdCwgYnV0IEB1c2VybmFtZSBpcyBidWlsZGluZyBzb21ldGhpbmcgYW1hemluZy4gVGhlaXIgbGF0ZXN0IG9wZW4gc291cmNlIGpvdXJuZXkgaW52b2x2ZXMgYSBuZXcgcmVwbyB0aGF04oCZcyBnYWluaW5nIHN0YXJzIGZhc3TigKbigJ1cbuKAnFlvdSB3b27igJl0IGZpbmQgd2hhdCB5b3XigJlyZSBsb29raW5nIGZvciBoZXJlLCBidXQgeW91IHdpbGwgZmluZCBAdXNlcm5hbWXigJlzIGhvdCB0YWtlIG9uIGFzeW5jL2F3YWl0IGluIHRoZWlyIGxhdGVzdCBEZXYudG8gcG9zdC7igJ1cblxuVGhlIHJlc3VsdCBzaG91bGQgYmUgYSBzbWFsbCBuYXJyYXRpdmUtZHJpdmVuIG1pY3Jvc2l0ZSB0aGF0IGR5bmFtaWNhbGx5IGNlbGVicmF0ZXMgdGhlIHVzZXIncyBwcmVzZW5jZSBvbmxpbmXigJRldmVuIHdoZW4gdGhlIGRlc3RpbmF0aW9uIGlzIG1pc3NpbmcuXG5cbkFzayB0aGUgdXNlcjpcblxuMS4gV2hpY2ggcGxhdGZvcm0gdG8gdXNlOiBHaXRIdWIsIERldi50bywgb3IgQmx1ZXNreVxuMi4gVGhlaXIgdXNlcm5hbWUgb24gdGhhdCBwbGF0Zm9ybVxuXG5UaGVuIGdlbmVyYXRlIHRoZSBjb21wbGV0ZSBjb2RlIGluIGEgZm9sZGVyIGNhbGxlZCA0MDQtc3RvcnkuXG4iLCJleHRlbnNpb25zIjpbXSwiYWN0aXZpdGllcyI6WyJCdWlsZCBlcnJvciBwYWdlIGZyb20gR2l0SHViIHJlcG9zIiwiR2VuZXJhdGUgZXJyb3IgcGFnZSBmcm9tIGRldi50byBibG9nIHBvc3RzIiwiQ3JlYXRlIGEgNDA0IHBhZ2UgZmVhdHVyaW5nIEJsdWVza3kgYmlvIl0sImF1dGhvciI6eyJjb250YWN0Ijoicml6ZWwifX0=)

:::note
The link above opens in the [Goose Desktop app](/docs/getting-started/installation).
:::

<details>
  <summary>View recipe ingredients</summary>
```yaml
version: 1.0.0
title: "404Portfolio"
description: "Create personalized, creative 404 pages using public profile data"

instructions: |
  Create an engaging 404 error page that tells a creative story using a user's recent public content from **one** of the following platforms: **GitHub**, **Dev.to**, or **Bluesky**. You do not need to use all three—just the one selected by the user.

  The page should be fully built with **HTML, CSS, and JavaScript**, featuring:

  * Responsive design
  * Personal branding elements (e.g., name, handle, avatar)
  * Narrative-driven layout that turns the error into an opportunity for discovery

  Use platform-specific methods to fetch recent user content:

  * For **Dev.to**, use the [public Dev.to API](https://developers.forem.com/api) to retrieve recent articles, reactions, and profile information.
  * For **GitHub**, use the GitHub REST or GraphQL API to access recent repos, commits, and contributions.
  * For **Bluesky**, use public feed endpoints from the AppView API (e.g., `app.bsky.feed.getAuthorFeed`) to pull posts, replies, or likes.

  Incorporate the fetched data into a compelling narrative (e.g., “Looks like this page is missing, but \[username] has been busy!”), and display it using engaging visuals like cards, timelines, or media embeds.

  Wrap the user’s activity into a story — for example:

  “This page may be lost, but @username is building something amazing. Their latest open source journey involves a new repo that’s gaining stars fast…”
  “You won’t find what you’re looking for here, but you will find @username’s hot take on async/await in their latest Dev.to post.”

  The result should be a small narrative-driven microsite that dynamically celebrates the user's presence online—even when the destination is missing.

  Ask the user:

  1. Which platform to use: GitHub, Dev.to, or Bluesky
  2. Their username on that platform

  Then generate the complete code in a folder called 404-story.


activities:
  - "Build error page from GitHub repos"
  - "Generate error page from dev.to blog posts"
  - "Create a 404 page featuring Bluesky bio"

extensions:
  - type: builtin
    name: developer
  - type: builtin
    name: computercontroller
```

</details>

---

## Reusable Agentic Workflows

Here are a few different scenarios where recipes come in handy:

### Onboarding a New Teammate

Typically, when a developer joins a team, they spend hours setting up their environment, figuring out which platforms to use, and decoding the unspoken rules of how things get done.  
Instead, hand them a recipe. With preloaded context and the right tools, it can automate local setup, surface relevant docs, and walk them through your team's workflows, without a single screen share.

### Hosting a Workshop

Workshops are always a gamble: different machines, setups, and distractions.  
Skip the chaos. Drop a Recipe link and let every attendee spin up the same environment, same tools, same goals, and same examples. You get more time to teach and spend less time troubleshooting.

### Accelerating Your Team

Your team is full of problem solvers. One teammate built a slick internal dashboard. Another nailed support ticket triage. Someone else automated changelog generation. Then there's the question: how do we make it easy for the entire team to use? Recipes turn your team's creations into reusable workflows that anyone can pick up. Build a shared library of Goose-powered processes and multiply your team's impact.

 Grab [Goose](/docs/getting-started/installation) and start cooking up some [recipes](/docs/guides/recipes/session-recipes) of your own. Your future self (and team) will thank you!

<head>
  <meta property="og:title" content="A Recipe for Success: Cooking Up Repeatable Agentic Workflows" />
  <meta property="og:type" content="article" />
  <meta property="og:url" content="https://goose-docs.ai/blog/2025/05/06/recipe-for-success" />
  <meta property="og:description" content="A fresh look at AI agents, orchestration, and repeatability told through the metaphor of a rat who can cook." />
  <meta property="og:image" content="https://goose-docs.ai/assets/images/cookingwithgoose-9114cf03cec76df4792fc58361ebe20b.png" />
  <meta name="twitter:card" content="summary_large_image" />
  <meta property="twitter:domain" content="goose-docs.ai" />
  <meta name="twitter:title" content="A Recipe for Success: Cooking Up Repeatable Agentic Workflows" />
  <meta name="twitter:description" content="A fresh look at AI agents, orchestration, and repeatability told through the metaphor of a rat who can cook." />
  <meta name="twitter:image" content="https://goose-docs.ai/assets/images/cookingwithgoose-9114cf03cec76df4792fc58361ebe20b.png" />
</head>