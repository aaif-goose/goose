---
title: "How I Taught My Agent My Design Taste"
description: "I used Agent Skills and recipes to automate execution so I could study taste, constraint design, feedback loops, and avoid AI smells."
authors: 
    - rizel
image: /img/blog/automate-taste.png
---

![blog cover](automate-taste.png)

Can you automate taste? The short answer is no, you cannot automate taste, but I did make my design preferences legible.

But for those interested in my experiment, I'll share the longer answer: I wanted to participate in [Genuary](https://genuary.art/), the annual challenge where people create one piece of creative coding every day in January. 

My goal here wasn't to "outsource" my creativity. Instead, I wanted to use Genuary as a sandbox to learn agentic engineering workflows. These workflows are becoming the standard for how developers work with technology. To keep my skills sharp, I used [goose](/) to experiment with these workflows in small, daily bursts.

<!--truncate-->

By building a system where goose handles the execution, I could test different architectures side-by-side. This experiment allowed me to determine which parts of an agentic workflow actually add value and which parts I should ditch. I spent a few hours focused on infrastructure to buy myself an entire month of workflow data.

:::tip
[Skills](/docs/guides/context-engineering/using-skills) are reusable sets of instructions and resources that teach goose how to perform specific tasks.
:::

## The Inspiration

I have to give a huge shout-out to my friend [Andrew Zigler](https://www.linkedin.com/posts/andrewzigler_genuary4-genuary2026-activity-7413652312495149056-5jA-). I saw him crushing Genuary and reached out to see how he was doing it. He shared his creations and mentioned he was using a "harness."

I'll admit, I'd been seeing people use that term all December, but I didn't actually know what it meant. Andrew explained: a harness is just the toolbox you build for the model. It's the set of deterministic scripts that wrap the LLM so it can interact with your environment reliably. He had used this approach to solve a different challenge, building a system that could iterate, submit, and verify itself.

He justified that if you spend time upfront working on a spec and establishing constraints. Then, you delegate. Once you have deterministic tools with good logging, the agent is incredibly good at looping until it hits its goal. 

My approach is typically very vanilla, and I lean heavily on prompting, but I was open to experimenting since Andrew was getting such excellent results.

## Harness vs. Skills

Inspired by that conversation, I built two versions of the same workflow to see how they handled the same daily Genuary prompts.

- **Approach 1: Harness + [Recipe](/docs/tutorials/recipes-tutorial)**: This lives in `/genuary`. Following Zig's lead, I wrote a shell script to act as the harness. It handles the scaffolding, creating folders and surfacing the daily prompt, so goose doesn't have to guess where to go. The recipe is about 300 lines long and fully self-contained.
- **Approach 2: Skills + Recipe**: This lives in `/genuary-skills`. This recipe is much leaner because it delegates the "how" to a skill. The skill contains the design philosophy, references, and examples. I wanted to see how the work changed when the agent had to "discover" its instructions in a bundle rather than following a flat script.

I spent one focused session building the entire system: [recipes](https://github.com/blackgirlbytes/genuary2026/blob/main/genuary/genuary.yaml), [skills](https://github.com/blackgirlbytes/genuary2026/blob/main/genuary-skills/.goose/skills/genuary/SKILL.md), harness scripts, templates, and [GitHub Actions](https://github.com/blackgirlbytes/genuary2026/tree/main/.github/workflows). (This happened in the quiet hours of my December break, with my one-year-old sleeping on my lap.) This was about trading short-term effort for long-term leverage. From that point on, the system did the daily work.

## On Taste

The automation was smooth, but when I reviewed the output, I noticed everything looked suspiciously similar.

That's when I started to think about the discourse on how you can't teach an agent "taste." I thought about how I develop taste. I honestly develop taste by:

- Seeing what's cool and copying it.
- Knowing what's overplayed because you've seen it too much.
- Following people with "good taste" and absorbing their patterns.

Obviously, I approached goose about this problem: 

> "I noticed it always does salmon colored circles..i know we said creative..any ideas on how to make sure it thinks outside the box"

![Salmon colored circles - a common AI generated cliché](salmon-circles.png)

goose shared that it was following a p5.js template it retrieved, which included a `fill(255, 100, 100)` (salmon!) value and an ellipse example. Since LLMs anchor heavily on concrete examples, the agent was following the code more than my "creative" instructions.

I removed the salmon circle from the template, but then I took it further: I asked how to ban common AI generated clichés altogether. goose searched discussions, pulled examples, and produced a banned list of patterns that scream "AI-generated."

### BANNED CLICHÉS

| Category | Banned Patterns |
| :---- | :---- |
| Color Crimes | Salmon or coral pink, teal and orange combinations, purple-pink-blue gradients. |
| Composition Crimes | Single centered shapes, perfect symmetry with no variation, generic spirals. |
| The Gold Rule | If it looks like an AI generated output, do not do it. |

### ENCOURAGED PATTERNS

| Category | Encouraged Patterns |
| :---- | :---- |
| Color Wins | HSB mode with shifting hues, complementary palettes, gradients that evolve over time. |
| Composition Wins | Particle systems with emergent behavior, layered depth with transparency, hundreds of elements interacting. |
| Movement Wins | Noise-based flow fields, flocking/swarming, organic growth patterns, breathing with variation. |
| Inspiration Sources | Natural phenomena: starlings murmurating, fireflies, aurora, smoke, water. |
| The Gold Rule | If it sparks joy and someone would want to share it, you're on the right track. |

goose determined this list through pattern recognition. So perhaps, agents can use patterns to reflect my taste, not because they understand beauty, but because I'm explicitly teaching them what I personally respond to.

I showed Andrew my favorite output of the three days: butterflies lining themselves in a Fibonacci sequence.

![Butterflies arranged in a Fibonacci spiral](fibonacci-butterflies.png)

His response was validating:

> "WOW that's an incredible Fibonacci… I'd be really curious to know your aesthetic prompting. Mine leans more pixel art and mathematical color manipulation because I've conditioned it that way… I like that yours leaned softer and tried to not look computer-created… like phone wallpaper practically lol..How did you even get that cool thinned line art on the butterflies? It looks like a base image. It's so cool. Did it draw SVGs? Like where did those come from?"

Because I'd specifically told goose to look at "natural phenomena" and "organic growth," it used Bezier curves for the wings and shifted the colors based on the spiral position to create depth, and a warm amber-to-blue gradient instead of stark black.

## Scaling Visual Feedback Loops

Both workflows use the [Chrome DevTools MCP server](/docs/mcp/chrome-devtools-mcp) so goose can see the output and iterate on it. This created a conflict where multiple instances couldn't use the same Chrome profile. I didn't want a manual step, so I asked the agent if it was possible to run Chrome DevTools in parallel. The solution was assigning separate user data directories.

```yaml
# genuary recipe example
- type: stdio
  name: Chrome Dev Tools
  cmd: npx
  args:
    - -y
    - chrome-devtools-mcp@latest
    - --userDataDir
    - /tmp/genuary-harness-chrome-profile
```

## What I Learned

I automated execution so I could study taste, constraint design, and feedback loops.

The two approaches behaved very differently. The harness-based workflow was more reliable and efficient, but it produced more predictable results. It followed instructions faithfully and optimized for consistency.

The skills-based approach was messier. It surfaced more surprises, made stranger connections, and required more editorial intervention. But the output felt more like a collaboration than a pipeline.

What this reinforced for me is that the "AI vs. human" framing is too simplistic. Automation handles repetition and speed well. Taste still lives in constraint-setting, curation, and deciding what should never happen. I ended up not automating taste. Instead, the end result was a system that made my preferences legible enough to be reflected back to me.

## See the Code

The code and full transcripts live in [my Genuary 2026 repo](https://github.com/blackgirlbytes/genuary2026). Each day folder contains the complete conversation history, including the pitches, iterations, and the back-and-forth between me and the agent. You can also view the creations on the [Genuary 2026 site](https://genuary2026.vercel.app/).

<head>
  <meta property="og:title" content="How I Taught My Agent My Design Taste" />
  <meta property="og:type" content="article" />
  <meta property="og:url" content="https://goose-docs.ai/blog/2026/01/04/how-i-taught-my-agent-my-design-taste" />
  <meta property="og:description" content="I used Agent Skills and recipes to automate execution so I could study taste, constraint design, feedback loops, and avoid AI smells." />
  <meta property="og:image" content="https://goose-docs.ai/assets/images/automate-taste-9a928fdbc3c8e4d335dba61401ede6bc.png" />
  <meta name="twitter:card" content="summary_large_image" />
  <meta property="twitter:domain" content="goose-docs.ai" />
  <meta name="twitter:title" content="How I Taught My Agent My Design Taste" />
  <meta name="twitter:description" content="I used Agent Skills and recipes to automate execution so I could study taste, constraint design, feedback loops, and avoid AI smells." />
  <meta name="twitter:image" content="https://goose-docs.ai/assets/images/automate-taste-9a928fdbc3c8e4d335dba61401ede6bc.png" />
</head>
