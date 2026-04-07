---
title: Orchestrating 6 Subagents to Build a Collaborative API Playground for Kids
description: Delegating backend, frontend, docs, and tests so six subagents could build collaborative API tool for kids
authors: 
    - rizel
---

![built by subagents](built-by-subagents.png)

I built Postman meets Google Docs for 10-year-olds.

*Cue record scratch.*

*Cue freeze frame.*

*Cue movie cliché.*

You're probably wondering how I got here.


<!--truncate-->

Before I explain, it’s better if I just show you.

👉Try it yourself: https://api-playground-production.up.railway.app/ 

![api playground](api-playground.png)

It’s a collaborative API testing playground where kids can run sample requests, get playful error messages, and see live responses in real time. Everyone in the session sees the API response together, turning the experience of solo debugging into multiplayer coding. And it looks like a literal playground.

I was inspired to build this after attending our company’s Bring Your Kids to Work Day. I didn’t bring my kid because she’s still an infant, but I attended to support my teammate Adewale Abati, who led a virtual session introducing kids to Goose. They used it to build comics, games, and music apps that were fun, imaginative, and genuinely impressive.

I decided to create a digital resource that teaches foundational concepts like APIs in a way that feels inviting instead of intimidating. Traditional API testing tools are powerful, but for a kid just starting out, they can be confusing and unclear.

**The wild part is that I let Goose and six subagents bring this idea to life.**

## Meet the Subagents

[Subagents](/docs/guides/subagents) are individual AI instances that take on specific tasks. Each one runs in its own session, which helps preserve the main context window and keeps your primary Goose conversation uncluttered and focused on high-level orchestration. I think of subagents as temporary teammates. Goose assigns each subagent a job and deallocates it when the work is complete.

For this project, I turned my subagents into an on-demand dev squad, and I assigned them the following roles:

* **Backend Developer** - Build the WebSocket server for real-time collaboration  
* **Frontend Developer** - Create the collaborative web UI  
* **Conflict Resolution Engineer** - Handle simultaneous user edits  
* **Documentation Writer** - Create a beginner-friendly README  
* **API Sample Curator** - Build example collections with fun public APIs  
* **Test Engineer** - Write a simple test suite

Sidenote: It felt like I was assembling the Avengers.
![avengers](avengers.gif)

:::note
As of version 1.10.0, subagents are no longer experimental and don't require enabling any feature flags.
:::

## Instructing My Team

There are a few ways to create subagents in Goose. You can use natural language prompts, define them through [recipes](/docs/guides/recipes/), or even spin up [external subagents](/docs/guides/subagents/#external-subagents) like Codex or Claude Code.

I took the natural language prompt approach because it felt convenient to directly configure a subagent through one prompt. Here’s the prompt I used:
  
```
Build a real-time collaborative API testing platform using 3 AI subagents working sequentially - like "Google Docs for Postman" where teams can test APIs together, but for kids. Make it so errors and results are explained in a way that kids can understand and the design is kid friendly using metaphors. 

3 Sequential Subagents 

- Subagent 1: Create a WebSocket backend server that handles API request execution (GET/POST/PUT/DELETE with headers, body, auth) AND real-time collaboration features (multiple users, shared collections, live updates). 

- Subagent 2: Build a conflict resolution system for when multiple users edit the same API request simultaneously, plus response formatting and request history management. 

- Subagent 3: Create the collaborative web UI using HTML, CSS, and vanilla JavaScript with API testing interface (URL input, method selection, headers, request body) that shows live user cursors, real-time updates, and shared results when anyone runs a test. 

3 other subagents should work in parallel developing a readme, api collections and, a simple test suite. 

- Subagent 4: Create a beginner friendly README

- Subagent 5: Create a sample api collection and examples with 2-3 read to try example requests. Use safe, fun public apis like dog facts and joke api

- Subagent 6: Create a simple test suite 

Final result should be a working web app where multiple people can test APIs together, see each other's requests and responses instantly, and collaborate without conflicts. Use HTML/CSS/JS for the frontend, no frameworks. 

Set the time out to 9 minutes.
```

:::note TLDR
Goose lets you run subagents in parallel or sequentially. I chose a hybrid approach instructing Goose to run the first subagents sequentially (since their tasks relied on the previous step) and the last three subagents in parallel (since they only needed the core app to exist). 

I also set the timeout to 9 minutes, giving the subagents more time than the default 5 minutes to accomplish their tasks.
:::
 
The subagents delivered a working collaborative API playground.  The functionality was solid, but I noticed the visual design was inconsistent. It used so many colors and fonts. I wanted it to look kid friendly, but not like a kid made it!

## My Parallel Prompt Fail

After the agents completed the initial task, I proceeded with a follow-up prompt asking Goose to spawn five more subagents to work in parallel, each responsible for a different UI component: the header, request builder, tab layout, and collaboration panel. I figured that having the subagents execute the work in parallel would get the job done faster.

But the result of this prompt made the app look worse! Each subagent brought its own interpretation of what "kid-friendly" meant. The header had a gaming-like design with black and purple colors, the tabs used Comic Sans while the rest of the app didn't, and the panels used a glassmorphic design. 

This happened because each subagent wasn't aware of the other subagents' plan. They all ran in parallel without any shared design vision.

## A Better Prompt Strategy

This time, I took a different approach.I told Goose to spin up one subagent to analyze the UI and come up with a shared design plan. Once the plan was ready, Goose could then spawn four more subagents to implement the plan in parallel.

```
Can you take a look at the UI? The color scheme is all over the place. I want it to be unified but also have a playground theme like a real-life playground. Not just the colors but the elements as well.

I want to use CSS to create grass and trees and a full visual space. For the panels, background, buttons, and text—every single element. Detailed.

Have one subagent analyze the UI and decide what should be updated to feel cohesive and playful. It will create a plan.

After that, four subagents will carry out the plan.
```

The first subagent came back with a creative design direction: transform the interface into a vibrant outdoor playground using bright greens, sunny yellows, and elements like swings, slides, and trees.

Here’s an excerpt of the plan:

```
Core Visual Concept:

Transform the API testing interface into a vibrant outdoor playground where kids can "play" with APIs like playground equipment. Think bright sunny day, green grass, colorful playground equipment, and friendly cartoon-style elements.

🎨 Color Palette & Visual Elements

- Grass Green: #4CAF50, #66BB6A, #81C784 (various grass shades)
- Sky Blue: #2196F3, #42A5F5, #64B5F6 (clear sky)
- Sunshine Yellow: #FFC107, #FFD54F, #FFEB3B (sun and highlights)
- Playground Red: #F44336, #EF5350 (slides, swings)
- Tree Brown: #8D6E63, #A1887F (tree trunks, wooden elements)
- Flower Colors: #E91E63, #9C27B0, #FF5722 (decorative flowers)
```

Then, it split implementation into four phases between the four remaining subagents:

```
Phase 1: Foundation (Area 1)
- Create base playground environment
- Implement sky, grass, and tree elements

Phase 2: Equipment (Area 2)
- Transform main panels into playground equipment

Phase 3: Interactions (Area 3)
- Convert buttons and form elements
- Add micro-animations and hover effects

Phase 4: Content (Area 4)
- Update typography and fonts
- Rewrite copy with playground metaphors
```

The result was a much more cohesive, playful interface that actually looked like a digital playground. Having Goose coordinate subagents based on a shared design plan worked way better than running them loose in parallel.

## Final Thoughts

This was my first experience with subagents, and I learned that:

* Sequential execution works better when one task builds on another.   
* Parallel execution works when tasks are independent or follow a shared plan  
* Use subagents for complex projects with independent tasks you can delegate.  
* You can let Goose do the planning for you. You don’t have to micromanage every step.

I loved that instead of managing every detail, I could assign focused jobs and let Goose coordinate the flow. 

The next experiment I want to try is using external subagents, which would allow me to delegate one-off tasks to tools like Claude Code or Codex.

What will you build with subagents?

[Download Goose](/)

[Learn about subagents](/docs/guides/subagents)

<head>
  <meta property="og:title" content="Orchestrating 6 Subagents to Build a Collaborative API Playground for Kids" />
  <meta property="og:type" content="article" />
  <meta property="og:url" content="https://goose-docs.ai/blog/2025/07/21/orchestrating-subagents" />
  <meta property="og:description" content="Delegating backend, frontend, docs, and tests so six subagents could build collaborative API tool for kids." />
  <meta property="og:image" content="https://goose-docs.ai/assets/images/built-by-subagents-869a01d4b147ebdb54334dcc22dc521e.png" />
  <meta name="twitter:card" content="summary_large_image" />
  <meta property="twitter:domain" content="goose-docs.ai" />
  <meta name="twitter:title" content="Orchestrating 6 Subagents to Build a Collaborative API Playground for Kids" />
  <meta name="twitter:description" content="Delegating backend, frontend, docs, and tests so six subagents could build collaborative API tool for kids." />
  <meta name="twitter:image" content="https://goose-docs.ai/assets/images/built-by-subagents-869a01d4b147ebdb54334dcc22dc521e.png" />
</head>
