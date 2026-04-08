---
title: "One Shot Prompting is Dead"
description: "Practical steps and mental models for building context engineered workflows instead of clever prompts."
authors:
  - ebony
---

![One shot prompting is dead](blogbanner.png)

I attended one shot prompting’s funeral.

There were no tears. Just a room full of developers quietly pretending they weren’t taking shots the night before. Because if we’re being honest, everyone saw this coming and couldn’t be happier it was over.

Saying “one shot prompting is dead” isn’t revolutionary. It’s just catching up to what builders have been experiencing for months.

<!-- truncate -->

---

## The blog post that aged faster than oat milk

Last year, I wrote a post about [how to prompt better](https://goose-docs.ai/blog/2025/03/19/better-ai-prompting). I shared tricks, phrasing tips, and even said to add a few “pleases” and “thank yous” and your AI agent would give you the world. At the time it felt cutting edge, because it was. There were livestreams and conference talks entirely about how to prompt better.

Less than a year later, it feels… quaint. Not because prompting stopped mattering, but because prompting stopped being the main character.

The conversation shifted from:

> “How do I coach the model better?”

to

> “What environment am I dropping this model into?”

That’s a completely different problem, and now it has a name. **[Context engineering](https://goose-docs.ai/docs/guides/context-engineering/)**.


---

## The abstraction that broke

One shot prompting worked when agents were party tricks. You crafted a clever prompt, you got a clever answer, and by “clever answer” I mean a fully “working” app, so everyone clapped. But the moment we asked agents to plan, remember, call tools, and operate across multiple steps, the definition of “worked” fell apart.

A single prompt stopped being a solution and became a bottleneck. What matters now isn’t the sentence you type. It’s the system that surrounds it. Prompts didn’t disappear, but they were demoted to one step inside a larger pipeline designed to hold state, plan ahead, and enforce guardrails.

As someone put it in a thread I recently came across:

> “The best model with bad context loses to an average model with great context.”

That line explains the shift. Context is now the advantage.

And this isn’t theoretical. You can see it in how serious agent systems are being built. Projects like [OpenClaw](https://openclaw.ai/) and [Ralph Wiggum loop](https://ghuntley.com/loop/) aren’t chasing clever phrasing. They’re designing environments where context persists, decisions accumulate, and agents can operate across time without resetting every session.

The excitement around these systems isn’t just hype either. It’s relief. Builders have been hungry for real working examples that behave *predictably* over time.

Which leads to the only question that matters ....

---

## How do I actually do this?

When I started building our skills marketplace, one shot prompting alone couldn't cut it. My normal workflow involved researching in one place and implementing in another, and every time I switched tools I had to re-explain the same decisions. Context wasn’t living inside the system. It was living in my head. The agent would forget, I would remember, and the entire session became an exercise in rehydration instead of progress.

Here’s what that loop looked like in practice:

{/* Video Player */}
<div style={{ width: '100%', maxWidth: '800px', margin: '0 auto' }}>
  <video
    controls
    playsInline
    style={{ width: '100%', height: 'auto', display: 'block' }}
  >
    <source src={require('@site/static/videos/contextBlog.mp4').default} type="video/mp4" />
    Your browser does not support the video tag.
  </video>
</div>



> *Even **this** demo is powered by persistent context.*

That was the moment I experimented with [RPI](https://goose-docs.ai/docs/tutorials/rpi). Not because it was trendy, but because the alternative had become tedious.

You don’t have to adopt RPI, or any new pattern, tomorrow to benefit from this. You can simulate the shift in your next session with a small change in how you start.

Before you execute anything, put your agent in chat only mode and run this handoff.

**Step 1: Align on the finish line**

Tell the agent exactly what counts as done.

> “We are shipping: ___  
> Success looks like: ___”

If the finish line feels fuzzy to you this is the time to flesh it out with your agent, if not your session will drift.

**Step 2: Lock in non-negotiables**

Define what is not up for debate.

> “Constraints: ___  
> Architecture we are committing to: ___ ”

This prevents the classic agent spiral where it keeps trying to overengineer the project instead of building it.

**Step 3: Capture persistent context**

Write down the facts that must survive the session.

> “Context that must persist:  
> – ___  
> – ___  
> – ___”

This is research, assumptions, domain knowledge, edge cases, terminology, anything your agent will need to pick up exactly where it left off.

Now save it somewhere accessible:

- a file in the project  
- a context file (goosehints, Cursor rules, etc)
- a memory extension   

Anything that outlives the chat window.

The rule is simple. Context should live in the system, not in your head.

---

## This is good news for people who think beyond code

The interesting part is this shift isn’t just technical. It has a quiet career implication hiding inside it. AI isn’t replacing engineers. It’s replacing workflows that stop at “my code runs, so I’m done.” Context engineering rewards a different mindset, the ability to pick up all these different patterns and utilize them by thinking about how decisions propagate through a system, what persists, and what the downstream effects look like over time.

That’s a muscle I’m actively working on too. And the more I lean into it, the clearer the direction becomes.

---

## The real skill is orchestration

We attended its funeral, but as you can see, prompting isn’t really gone. It just stopped being the workflow.

One shot prompting is still great for demos and exploration. But when the goal is building systems that last longer than a single session, the advantage shifts to how well you design the environment around the model.

The people who thrive in this era won’t be the ones with the cleverest phrasing. They’ll be the ones who know how to orchestrate context so intelligence accumulates instead of resetting.

And honestly, that’s progress.


<head>
  <meta property="og:title" content="One Shot Prompting is Dead" />
  <meta property="og:type" content="article" />
  <meta property="og:url" content="https://goose-docs.ai/blog//2026/02/07/context-engineering" />
  <meta property="og:description" content="Practical steps and mental models for building context-engineered workflows instead of clever prompts." />
  <meta property="og:image" content="https://goose-docs.ai/assets/images/blogbanner-2fa90c93a49496447d38217739242dec.png" />
  <meta name="twitter:card" content="summary_large_image" />
  <meta property="twitter:domain" content="goose-docs.ai" />
  <meta name="twitter:title" content="One Shot Prompting is Dead" />
  <meta name="twitter:description" content="Practical steps and mental models for building context-engineered workflows instead of clever prompts.." />
  <meta name="twitter:image" content="https://goose-docs.ai/assets/images/blogbanner-2fa90c93a49496447d38217739242dec.png" />
</head>