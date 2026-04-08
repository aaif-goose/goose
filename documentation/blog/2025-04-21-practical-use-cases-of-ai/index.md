---
title: 11 Practical Ways I Use AI Agents Without Losing My Authenticity
description: From conference planning to prepping podcasts, here's how I use AI Agents built on MCP for everyday tasks.
authors: 
    - rizel
---

![mcp use cases](mcp-use-cases.png)

"Stop using AI," reads yet another viral post. I get it. It's frustrating to review a colleague's auto-generated work, filled with AI's classic giveaways like generic code comments and phrases like "In today's fast-paced world..."

Still, AI plays a pivotal role in my career. I don't rely on AI to do my work, but I use it to help me brainstorm and work more efficiently. 
The introduction of [Model Context Protocol (MCP)](https://modelcontextprotocol.io) has made this even easier. MCP is an open standard that gives AI tools the context they need to be useful in the real world. It enables AI agents to interact with APIs, apps, and systems in a structured way. I use [Codename goose](/), an open source AI agent built on MCP.

Here are 11 real ways I use AI Agents without sacrificing authenticity, creativity, or quality:

<!--truncate-->

## 1. 🙌🏿 Hands-Free Coding

### Use Case

I spoke to Goose instead of typing, using my voice as input to write code or run tasks.

### Why It's Useful

I have a lot of "my brain has the idea but my hands are full" moments. Whether I'm nursing my baby or recovering from carpal tunnel, this provides an accessible way for me to capture my thoughts without typing.

Sidenote: I met an AI enthusiast at a meetup who said he sometimes gets coding ideas while driving. He's exploring using his voice to vibe code on the go. Stay safe out there. Don't code and drive! 🚗⛑️

### How to Try It

1. Follow [this tutorial](/docs/mcp/speech-mcp)   
2. Enable the [`Speech`](https://github.com/Kvadratni/speech-mcp) and [`Developer`](/extensions/detail?id=developer) extensions
3. Prompt Goose:
    > I'd like to speak instead of typing.

<iframe class="aspect-ratio" src="https://www.youtube.com/embed/rurAp_WzOiY" title="YouTube video player" frameborder="0" allow="accelerometer; autoplay; clipboard-write; encrypted-media; gyroscope; picture-in-picture; web-share" allowfullscreen></iframe>

## 2. 🎤 Prepping Podcast Agendas

### Use Case

I gave Goose a YouTube video of a guest's conference talk. Then, I prompted Goose to create a transcript and generate thoughtful interview questions.

### Why It's Useful

I want guests to feel like I actually know their work, even if I don't have hours to prep. This lets me ask smarter questions and run a better show.

### How to Try It

1. Follow [this tutorial](/docs/mcp/youtube-transcript-mcp)
2. Enable the [`YouTube Transcript`](https://github.com/jkawamoto/mcp-youtube-transcript) and [`Developer`](/extensions/detail?id=developer) extensions
3. Prompt Goose:
   > Generate a transcript for this video https://www.youtube.com/watch?v=dQw4w9WgXcQ, then create relevant interview questions based on the content

---

## 3. 🖼 Resize Images 

### Use Case

Speaker management platforms often have different image requirements for headshots. I used to spend an embarrassingly amount of time trying to resize my photo without ruining the aspect ratio. Now, I just ask Goose to do it.

### Why It's Useful

It saves me from wrestling with random online tools or bloated design apps. I get a clean, correctly sized image in seconds, and it looks exactly how I want it to.

### How to Try It

1. Enable the [`Developer`](/extensions/detail?id=developer) extension
2. Prompt Goose:
   > Resize this image (~/Downloads/image.png) to 1000x1000 pixels. Maintain the aspect ratio and image quality.

---

## 4. 📝 Resume Review Against Job Listings

### Use Case

I've used Goose to compare my current resume to job listings I came across.

### Why It's Useful

I'm not currently looking for a job, but I like to stay prepared. My strategy involves keeping my resume current and competitive. I do this by comparing my current resume to job listings, but I don't have to do this manually anymore. Instead, Goose can quickly point out my strengths and weaknesses for specific job listings. This approach could help hiring managers review resumes faster as well.

### How to Try It

1. Follow [this tutorial](/docs/mcp/pdf-mcp)
2. Enable the [`PDF Reader`](https://github.com/michaelneale/mcp-read-pdf) extension
3. Prompt Goose:
   > Read the resume at ~/Downloads/resume.pdf and evaluate how well this candidate aligns with the following role requirements:
   >   - 5+ years of backend development experience
   >   - Strong system design and distributed systems knowledge
   >   - Cloud infrastructure experience (AWS preferred)
   >   - Prior experience leading technical projects or teams
   >   - Bonus: familiarity with LLMs or AI/ML tools
   >
   > Score each one out of 5, give supporting evidence, and summarize with a final fit rating.

<iframe class="aspect-ratio" src="https://www.youtube.com/embed/EJf2_iZfaWk" title="YouTube video player" frameborder="0" allow="accelerometer; autoplay; clipboard-write; encrypted-media; gyroscope; picture-in-picture; web-share" allowfullscreen></iframe>
---

## 5. 🧠 Understanding Idioms

### Use Case

I've asked Goose to explain idioms or references that I didn't understand.  

### Why It's Useful

Because I wasn't born in America and I'm neurodivergent, I sometimes take idioms literally or misinterpret them. Instead of risking embarrassment at work, I quietly ask Goose to translate.

### How to Try It

1. Enable the [`Developer`](/extensions/detail?id=developer) extension
2. Prompt Goose:
   > What does this phrase mean: "Who does Vegas have as the favorite?"

---

## 6. 📊 Querying a Relational Database

### Use Case

I asked Goose for insights about my data using natural language, and it wrote a Common Table Expression for me.

### Why It's Useful

SQL can get complex with joins, stored procedures, and subqueries. Goose helps me move faster and avoid errors by handling the query logic for me. 

### How to Try It

1. Follow [this tutorial](/docs/mcp/postgres-mcp)
2. Enable the [`PostgreSQL`](https://github.com/bytebase/dbhub/) and [`Developer`](/extensions/detail?id=developer) extensions
3. Prompt Goose:
   > Find my top 3 blog posts by average weekly views over the past 90 days. Include title, URL, average weekly views, and whether they were promoted on social.

<iframe class="aspect-ratio" src="https://www.youtube.com/embed/PZlYQ5IthYM" title="YouTube video player" frameborder="0" allow="accelerometer; autoplay; clipboard-write; encrypted-media; gyroscope; picture-in-picture; web-share" allowfullscreen></iframe>
---

## 7. 🗓 Planning My Conference Speaking Strategy

### Use Case

I've used Goose to analyze historical conference data so I could plan smarter for upcoming CFP deadlines.

### Why It's Useful

I tend to overbook myself or get anxious that I won't get accepted, so I apply to everything. Then I end up getting accepted to all of them and say yes without thinking, which leads to poor planning and rushed talks. With Goose, I can analyze patterns in CFP timelines and make more intentional choices.

### How to Try It

1. Follow [this tutorial](/docs/mcp/agentql-mcp)
2. Enable the [`AgentQL`](https://github.com/tinyfish-io/agentql-mcp) extension
3. Prompt Goose:
   > I'm a tech conference speaker planning my 2025-2026 submissions. 
   > Extract for developer conferences (attendance > 500) occurring between 2022-2024:
   > - Conference name
   > - Conference dates
   > - CFP timeline 
   >
   > To identify:
   > - Consistent monthly patterns
   > - Whether conferences stick to same months yearly
   > - If CFP windows are consistent year-to-year
   > - Any shifts in traditional timing
   >
   > Structure results as JSON

---

## 8. 🐞 Tracking Down a Buggy Commit

### Use Case

A feature broke, but I had made so many commits, I couldn't tell which one introduced the bug. I asked Goose to help me run `git bisect`, so we could identify the problematic code.

### Why It's Useful

The hardest part of debugging is often just figuring out where to look. Git bisect makes that faster, and Goose walked me through the process without needing to memorize the steps.

### How to Try It

1. Install the [Git CLI](https://git-scm.com/book/en/v2/Getting-Started-Installing-Git)
2. Enable the [`Developer`](/extensions/detail?id=developer) extension
3. Prompt Goose:
   > I don't know when I introduced a bug. Can you walk me through using git bisect to find the commit that caused it?

---

## 9. 👩🏾‍🏫 Learning New Technologies

### Use Case

I like to keep up with the latest technologies. Since MCP servers are popular, I used Goose's tutorial extension to walk through building my own MCP server.

### Why It's Useful

In addition to generating code, AI agents can help you learn how to code. Goose includes a built-in tutorial extension designed to walk users through technical concepts in a hands-on way.

### How to Try It

1. Follow [this tutorial](/docs/mcp/tutorial-mcp)
3. Prompt Goose:
   > I'd like to learn how to build an extension or MCP server for Goose

---

## 10. 💼 Comparing Regulatory Documentation

### Use Case

I didn't do this myself, but I was impressed to learn that a community member used Goose to compare proposed and final versions of regulatory documents.

### Why It's Useful

Legal documents are often dense and repetitive. Goose can highlight what actually changed, helping users quickly spot how updates impact compliance or obligations.

### How to Try It

1. Enable the [`Computer Controller`](/extensions/detail?id=computercontroller) extension
2. Prompt Goose:
   > Highlight the differences between these two versions of FinCEN's Investment Adviser AML regulations:
   >
   > Proposed version (2015): https://www.federalregister.gov/documents/2015/09/01/2015-21318/anti-money-laundering-program-and-suspicious-activity-report-filing-requirements-for-registered
   > 
   > Final version (2024): https://www.federalregister.gov/documents/2024/09/04/2024-19260/financial-crimes-enforcement-network-anti-money-launderingcountering-the-financing-of-terrorism
   >
   > Focus on key changes in requirements for investment advisers' AML/CFT programs and how they affect compliance obligations.

---

## 11. 🛠 Prototyping Ideas Quickly

### Use Case

I used Goose to build a working prototype and see the full application live in action.

### Why It's Useful

It's fast, functional, and lets me validate whether an idea is worth pursuing without spending hours coding from scratch.

### How to Try It

1. Enable the [`Developer`](/extensions/detail?id=developer) extension  
2. Prompt Goose:  
   > Build a JavaScript webcam app with real-time filters

🎥 **See it live:**  
Watch The Great Goose Off where we challenged Goose to create creative apps from scratch, like:  
- A Goose-shaped drawing tool  
- A purposely chaotic authentication flow

You'll see ideas go from prompt to prototype in one session.

<iframe class="aspect-ratio" src="https://www.youtube.com/embed/OsA3qhns7dg" title="YouTube video player" frameborder="0" allow="accelerometer; autoplay; clipboard-write; encrypted-media; gyroscope; picture-in-picture; web-share" allowfullscreen></iframe>

---

## Looking for more examples?

This blog post included just a few of the ways I use Goose. If you're curious about what else it can do, check out the [Prompt Library](/prompt-library) or just ask:

What are 5 useful things you can help me with today?

Let goose surprise you. ✨


<head>
  <meta property="og:title" content="11 Practical Ways I Use AI Agents Without Losing My Authenticity" />
  <meta property="og:type" content="article" />
  <meta property="og:url" content="https://goose-docs.ai/blog/2025/04/21/practical-use-cases-of-ai" />
  <meta property="og:description" content="From conference planning to prepping podcasts, here's how I use AI Agents built on MCP for everyday tasks." />
  <meta property="og:image" content="https://goose-docs.ai/assets/images/mcp-use-cases-758ecc959d6334783257fc9d6329e1f2.png" />
  <meta name="twitter:card" content="summary_large_image" />
  <meta property="twitter:domain" content="goose-docs.ai" />
  <meta name="twitter:title" content="11 Practical Ways I Use AI Agents Without Losing My Authenticity" />
  <meta name="twitter:description" content="From conference planning to prepping podcasts, here's how I use AI Agents built on MCP for everyday tasks." />
  <meta name="twitter:image" content="https://goose-docs.ai/assets/images/mcp-use-cases-758ecc959d6334783257fc9d6329e1f2.png" />
</head>