---
title: "How I Used RPI to Build an OpenClaw Alternative"
description: "Learn how I built a minimal, personal AI agent using goose and the RPI method."
authors:
  - rizel
---

![How I Used RPI to Build an OpenClaw Alternative](blogbanner.png)

Everyone on Tech Twitter has been buying Mac Minis, so they could run a local agentic tool called [OpenClaw](https://openclaw.ai/). OpenClaw is a messaging-based AI assistant that connects to platforms such as Discord and Telegram allowing you to interact with an AI agent through DMs or @mentions. Under the hood, it uses an agent called Pi to execute tasks, browse the web, write code, and more.

Seeing the hype made me want to get my hands dirty. I wanted to see if I could build a lite version for myself. I wanted something minimal that used [goose](https://github.com/aaif-goose/goose) as the engine instead of Pi. I tentatively dubbed it AltOpenClaw.

<!-- truncate -->

## Choosing RPI

My usual move is to just jump in, start breaking things, and refactor as I go. I actually prefer the back and forth conversation with an agent because it helps me learn how the project works in real time. But when I tried that here, I hit a wall fast. goose did not naturally know what OpenClaw was, and it kept hallucinating how to use its own backend. It would forget context mid-conversation or suggest API calls that simply did not exist.

I realized I needed to change my approach. While I love the iterative learning process, I needed a way to give the agent a better foundation so our pair programming sessions actually made progress. I decided to try the [RPI method (Research, Plan, Implement)](/docs/tutorials/rpi). This is a framework introduced by [HumanLayer](https://humanlayer.dev/) that trades raw speed for predictability. It is built into goose as a series of recipes. Since I did not fully understand the technical landscape myself, this investment in structure felt like the right move to help us both get on the same page.

---

### Research

First, I needed goose to understand what I was building and whether it was even possible. I kicked things off with a detailed research prompt:

```
/research_codebase topic="learn what openclaw is, how people use it, 
and how it works. learn if goose can actually be used as a backend 
or if that's not yet possible; understand the port issues especially 
if you have an instance of goose that's running to help you build 
an agent that uses goose as a backend. learn if there will be any 
auth issues"
```

goose spawned multiple parallel subagents to investigate.

**Key findings from the research:**

* **OpenClaw uses its own embedded agent runtime (Pi)**, not goose. This meant there was no existing integration to copy.
* **goose CAN be used as a backend!** The `goosed` server exposes a full HTTP API.
* **Port conflicts are manageable.** We just needed to run on a different port with `GOOSE_PORT=3001`.
* **Authentication is simple.** We could pass a secret key in the `X-Secret-Key` header.

The research also mapped out all the relevant API endpoints, such as `POST /sessions` to create a new session and `POST /sessions/{id}/reply` to handle the actual messaging.

---

### Plan

With the research complete, I asked goose to create an implementation plan. This is where we defined the personality and security of the bot:

```
/create_plan ticket-or-context="I want to build a Discord MCP server 
for goose that replicates the popular features of OpenClaw but with 
better security. Core Features: Users can DM the bot or @ it in a 
channel to give goose tasks. goose responds in Discord with results. 
Security requirements: Allowlist (only specific Discord user IDs can 
interact), Approval flow (before goose executes any tool/action, the 
bot posts what it wants to do and waits for user approval), 
Non-allowlisted users get a polite 'you don't have access'"
```

goose analyzed the requirements and produced a detailed plan with four phases:

1. Phase 1: Project Setup (Discord.js skeleton and allowlist)
2. Phase 2: goose HTTP Client (Connecting to the API and handling SSE streaming)
3. Phase 3: Tool Approval Flow (The UI for ✅/❌ reactions)
4. Phase 4: Polish & Error Handling (Slash commands and session management)

I liked this phased approach because it gave us less to debug at each step. We could handle features in chunks rather than trying to fix everything at once.

---

### Implement

With the plan in place, I gave the signal to start building:

```
/implement_plan start building
```

The first two phases were surprisingly smooth. Within an hour, the bot was online and I could actually DM it. Seeing a Discord message trigger a goose session for the first time was a massive win.

First, we tested if AltOpenClaw could respond to me with a joke!

![First successful message to the bot](first-message.png)

However, as every developer knows, it was not all perfect. We still ran into some classic real-world hurdles during implementation:

* The SSE (Server-Sent Events) format was different than we expected. We spent a good chunk of time debugging why the messages were not appearing until we realized the event structure was nested deeper than anticipated.
* My local path did not have npm properly mapped, which led to a brief detour.
* Discord has a strict limit on message length. If goose wrote a long script, the bot would just crash. We had to implement a chunking system on the fly.

Currently, the tool approval feature is still a work in progress. I actually got so excited that the core part of the project was working that I sat down to write this post before finishing the UI for the reactions.


## The Takeaway

The RPI method felt like a superpower, even if it didn't magically delete every bug from the project. There is a big difference between fighting a hallucination and fighting a real technical challenge.

When I didn't use RPI, goose hallucinated nonexistent endpoints and tried to build a complex MCP server when a simple HTTP API was all we needed. Those are the kinds of bugs that waste hours because you are chasing ghosts.

![Before RPI: Debugging failures and hallucinations](failure-screenshot.png)

Instead, RPI helped us clear the conceptual fog so we could focus on real implementation details like SSE parsing and character limits.

By forcing the agent to research first, it built up the context it was missing. It is a bit slower at the start (which I barely have patience for), but it turns the agent into a much more capable partner for that back and forth learning process I enjoy.

I even had AltOpenClaw push its own [repository](https://github.com/blackgirlbytes/discord-goose-bot) to GitHub.

![AltOpenClaw in action, completing a task](altopenclaw-action.png)

## Try It Out

If you want more reliability from your agent, give the [RPI recipes](/docs/tutorials/rpi) in goose a shot:

* `/research_codebase`
* `/create_plan`
* `/implement_plan`
* `/iterate_plan`

Happy hacking!

<head>
  <meta property="og:title" content="How I Used RPI to Build an OpenClaw Alternative" />
  <meta property="og:type" content="article" />
  <meta property="og:url" content="https://goose-docs.ai/blog/2026/02/06/rpi-openclaw-alternative" />
  <meta property="og:description" content="Learn how I built a minimal, personal AI agent using goose and the RPI method." />
  <meta property="og:image" content="https://goose-docs.ai/assets/images/blogbanner-7c71d1a80441079767f7fd25b9e27385.png" />
  <meta name="twitter:card" content="summary_large_image" />
  <meta property="twitter:domain" content="goose-docs.ai" />
  <meta name="twitter:title" content="How I Used RPI to Build an OpenClaw Alternative" />
  <meta name="twitter:description" content="Learn how I built a minimal, personal AI agent using goose and the RPI method." />
  <meta name="twitter:image" content="https://goose-docs.ai/assets/images/blogbanner-7c71d1a80441079767f7fd25b9e27385.png" />
</head>
