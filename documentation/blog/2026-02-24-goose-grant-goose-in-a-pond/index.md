---
title: "Grant Winner: Goose In A Pond"
description: "Introducing  a privacy-first, local AI home assistant powered by Goose on edge hardware."
authors:
  - angie
---

![blog banner](banner.png)

We launched the [goose grant program](/grants/) awarding $100K grants for developers building the future of agentic AI. We're looking for ambitious, open source projects that push goose into new territory, and today, We're thrilled to introduce one of our grant recipients: **Goose In A Pond**, a project that's taking goose off the desktop and into your home.

<!--truncate-->

## What Is Goose In A Pond?

Goose In A Pond is a fully local, privacy-first smart home assistant built on top of goose. Think of it as what your smart speaker *should* be: an AI assistant that actually runs on your hardware, understands your voice offline, controls your devices, and never sends your data to the cloud.

The project is being built by [Jarida](https://jarida-io.web.app/), a team of five developers based in Nairobi, Kenya, led by [Jerry Ochieng](https://linkedin.com/in/jerry-ochieng). They're taking goose's open source agent framework and deploying it on edge hardware, specifically the NVIDIA Jetson Orin Nano, to create a modular, agentic home hub that you fully own and control.

There's no shortage of smart assistants out there, but most of them share the same problem: they're vendor and cloud dependent, and treat your data like it belongs to someone else. Goose In A Pond flips that entirely.

Here's what makes it stand out:

### Everything Runs Locally

All computation - voice recognition, language modeling, memory, device control - happens on device. No cloud. No external servers. No data leaving your home. The team selected the Jetson Orin Nano as their primary platform, which can run quantized 1–7B parameter language models at 40–70 tokens per second. That's fast enough for natural, conversational interactions without needing an internet connection.

### Offline Voice That Actually Works

One of the coolest parts of this project is the fully offline voice pipeline. Using goose's experimental [Perception extension](https://github.com/michaelneale/goose-perception) as a foundation, the system listens for a wake word ("goose," naturally 🪿), then switches to a higher quality transcription mode to capture what you say. Wake word detection, speech recognition, and text-to-speech all run locally using open source models like Whisper, Vosk, and Coqui TTS.

Their early benchmarks on a Raspberry Pi 5 show usable response times with just a few seconds of delay, comparable to (and sometimes better than) commercial cloud assistants in low. connectivity environments.

### Smart Device Control

For devices with standard APIs (smart bulbs, switches, etc.), Goose In A Pond integrates through protocols like zigbee2mqtt and HTTP/MQTT. But what about all those devices that *don't* have open APIs?

The team has answers for that too:

- **IR blasting** for legacy devices like TVs and air conditioners using GPIO-connected IR LEDs
- **Android sandboxing** via ADB and UIAutomator to automate proprietary apps that don't expose APIs
- **Bluetooth and USB control** for direct communication with sensors and peripherals

They're essentially building goose into a universal remote for your entire home - open or closed ecosystem, it doesn't matter.

### A Self-Improving Assistant

Goose In A Pond isn't just a static tool. Through goose's [Memory extension](/docs/mcp/memory-mcp) and a feedback loop system, it learns your preferences, adapts its behavior, and refines its own prompts over time. The team is also exploring self-refinement techniques where the system analyzes its own session logs to optimize its automation behavior. It's the kind of agent that gets better the more you use it.

### Mobile Companion

The project also includes a mobile companion app called **Goose On The Go**. The idea is to control your home assistant from your phone, whether you're on the couch or away from home. Real-time dashboards, voice and text input, push notifications, and remote command execution, all connecting back to your local goose instance.

## The MCPs They're Building

Part of what makes this project so valuable to the broader goose community is the set of MCP servers and extensions the Jarida team plans to open source:

- **Moonbeam MCP** — An Android UI automation server (think Playwright, but for Android) for controlling smart devices through their apps
- **Local Vision Event Detection MCP** — Processes camera feeds locally for motion detection, pet detection, package arrival, and more
- **Offline ASR / Voice Command MCP** — Fully local voice processing with wake-word detection and command parsing
- **Sensor Data Aggregator MCP** — Collects data from local sensors (temperature, humidity, motion, energy meters) and exposes it via MCP
- **Local Routine / Scheduler MCP** — Define automations with natural language like "At 7am, bring up lights and coffee maker"
- **Privacy Audit / Logs MCP** — Monitor what goose is doing, track device activity, and flag potential privacy concerns
- **Inter-Agent Coordination MCP** — Enable multiple goose agents to coordinate tasks and share context

These extensions won't just power Goose In A Pond, they'll be available for anyone in the goose community to use and build on.

## Meet the Jarida Team

The Jarida team is a group of five graduates from the Catholic University of Eastern Africa who came together around a shared belief: open source isn't just a good way to build software, it's the *right* way. They're young, hungry, and fully committed to this project.

- **Jerry Ochieng** — Team Lead & On-Device Intelligence Lead. Backend engineer, AI enthusiast, and the person who trained an on-device model for Kenyan Sign Language to English.
- **Liz Wangui** — Product & DevRel Lead. Brings marketing experience from Red Bull Kenya and technical chops from backend engineering.
- **Emmanuel Charles** — Backend & Security Lead. ISC² certified with a background in network cloud operations, ensuring everything stays private and secure.
- **Africia Kerubo** — UI/UX & AI Lead. Blending human-centric design with AI to create interfaces that feel natural.
- **Purity Wanjiru** — Mobile, QA & Automations Lead. Cisco Ethical Hacking certified, leading mobile design for Goose On The Go.

They're also mentored by **Obinna Anya**, Senior UX Researcher at Google, and **Harold Nyikal**, Android Growth Lead at Google in Kenya.

## What's Next

The team has a year long roadmap broken into four quarters:

1. **Q1** — Get goose running locally on Jetson hardware with voice input and offline LLM response for basic tasks
2. **Q2** — Smart device control via native and sandboxed methods, plus the mobile companion app
3. **Q3** — Self-improving agent capabilities, home security camera integration, and memory refinement
4. **Q4** — Full open source release with install docs, dev kits, starter templates, demo videos, and community feedback systems

By the end, Goose In A Pond should be something anyone can install, customize, and contribute to.

---

The goose grant program exists to support projects that push goose into places we haven't imagined yet. Goose In A Pond does exactly that. It takes goose from a developer tool on your laptop to a full blown home assistant running on edge hardware - completely local, completely open, completely yours.

We can't wait to see what the Jarida team builds. If you want to follow along, join the [goose community](https://discord.gg/goose-oss) and stay tuned for updates as the project progresses.

And if *you* have a wild idea for what goose could do? The **[goose grant program](/grants/)** might be for you 🪿

<head>
  <meta property="og:title" content="meet goose grant winner: Goose In A Pond" />
  <meta property="og:type" content="article" />
  <meta property="og:url" content="https://goose-docs.ai/blog/2026/02/24/goose-grant-goose-in-a-pond" />
  <meta property="og:description" content="Introducing a privacy-first, local AI home assistant powered by Goose on edge hardware." />
  <meta property="og:image" content="https://goose-docs.ai/assets/images/banner-ae6f66bdec317d7e20264c4a62ad0013.png" />
  <meta name="twitter:card" content="summary_large_image" />
  <meta property="twitter:domain" content="aaif-goose.github.io" />
  <meta name="twitter:title" content="Meet the Goose Grant Winner: Goose In A Pond" />
  <meta name="twitter:description" content="Introducing a privacy-first, local AI home assistant powered by Goose on edge hardware." />
  <meta name="twitter:image" content="https://goose-docs.ai/assets/images/banner-ae6f66bdec317d7e20264c4a62ad0013.png" />
</head>