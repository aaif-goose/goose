---
title: "Vibe Coding with Goose and the Speech MCP"
description: "Explore the new Speech MCP server that enables voice-controlled coding and natural conversation with your AI agent"
authors: 
    - adewale
---

![blog cover](vibe-coding.png)

Imagine creating an app just by describing what you want out loud, like you’re talking to a friend. That’s the magic of vibe coding: turning natural language into working code with the help of an AI agent. And while typing a prompt gets the job done, saying it out loud hits different 🔥 The new [Speech MCP server](/docs/mcp/speech-mcp) has quite literally entered the chat.

<!--truncate-->

In a recent [Wild Goose Case livestream](https://www.youtube.com/watch?v=Zey9GHyXlHYe), hosts [Ebony Louis](https://www.linkedin.com/in/ebonylouis/) and [Adewale Abati](https://www.linkedin.com/in/acekyd/) were joined by [Max Novich](https://www.linkedin.com/in/maksym-stepanenko-26404867) from Block's AI tools team, who demonstrated an exciting new extension - the [Speech MCP server](https://github.com/Kvadratni/speech-mcp). 

During the livestream, Max demonstrated this by creating an entire web application using only voice commands - no keyboard or mouse required. This resulted in a vibrant, animated webpage with 3D effects, synthwave aesthetics, and interactive elements, all created through natural conversation with Goose.

<iframe class="aspect-ratio" src="https://www.youtube.com/embed/Zey9GHyXlHY?start=437&end=752" title="YouTube video player" frameborder="0" allow="accelerometer; autoplay; clipboard-write; encrypted-media; gyroscope; picture-in-picture; web-share" referrerpolicy="strict-origin-when-cross-origin" allowfullscreen></iframe>


## The Speech MCP Server

[Speech MCP](https://github.com/Kvadratni/speech-mcp) is an open source MCP server that enables voice interaction with AI agents like Goose. What makes it special is that it runs entirely locally on your machine, making it:

- LLM agnostic
- Privacy-focused
- Cost-effective compared to cloud-based alternatives
- Accessible without internet connectivity

### Key Features

1. **Local Speech Processing**: Uses two main models:
   - Faster Whisper: An efficient method to convert speech to text
   - Coqui TTS: A Japanese-engineered text-to-speech model with 54 natural-sounding voices

2. **Voice Selection**: Choose from 54 different voices with varying characteristics and personalities

3. **Multi-Speaker Narration**: Generate and play conversations between multiple voices

4. **Audio Transcription**: Convert audio/video content to text with timestamps and speaker detection

## Live Demo Highlights

During the demonstration, Max showcased several impressive capabilities:

1. **Voice-Controlled Development**:
   - Created animated text effects
   - Implemented 3D transformations
   - Added synthwave aesthetics with gradients and grids
   - Integrated music controls

2. **System Integration**:
   - Controlled applications like Discord using voice commands
   - Navigated file system and development environment
   - Generated and managed audio content

3. **Natural Interaction**:
   - Fluid conversation with Goose
   - Real-time feedback and adjustments
   - Multi-voice narration for documentation

## Getting Started

To try the Speech MCP server yourself:

1. Install the required audio library (PortAudio):
   ```bash
   # For macOS
   brew install portaudio
   
   # For Linux
   apt-get install portaudio  # or dnf install portaudio
   ```

2. Install the extension directly using the one-click [deep link install](goose://extension?cmd=uvx&&arg=-p&arg=3.10.14&arg=speech-mcp@latest&id=speech_mcp&name=Speech%20Interface&description=Voice%20interaction%20with%20audio%20visualization%20for%20Goose) in Goose


## Join the Development

The Speech MCP server is [open-source](https://github.com/Kvadratni/speech-mcp) and welcomes contributions. You can also connect with Max on [Discord](https://discord.gg/goose-oss) for questions and collaboration.

Voice interactions with AI agents like Goose with the power and tools to act on instructions provides a different kind of vibe that makes the future feel closer than ever. Whether you're interested in vibe coding, accessibility improvements, or just want to feel a bit more like Tony Stark while getting Goose to pull a J.A.R.V.I.S, the Speech MCP server offers a glimpse into the future of human-AI collaboration - and it's available today.

<head>
  <meta property="og:title" content="Vibe Coding with Goose and the Speech MCP" />
  <meta property="og:type" content="article" />
  <meta property="og:url" content="https://goose-docs.ai/blog/2025/03/28/vibe-coding-with-goose" />
  <meta property="og:description" content="Explore the new Speech MCP server that enables voice-controlled coding and natural conversation with your AI agent." />
  <meta property="og:image" content="https://goose-docs.ai/assets/images/vibe-coding-b2efeed37ea43f4773da5f1ff96f4184.png" />
  <meta name="twitter:card" content="summary_large_image" />
  <meta property="twitter:domain" content="goose-docs.ai" />
  <meta name="twitter:title" content="Vibe Coding with Goose and the Speech MCP" />
  <meta name="twitter:description" content="Explore the new Speech MCP server that enables voice-controlled coding and natural conversation with your AI agent." />
  <meta name="twitter:image" content="https://goose-docs.ai/assets/images/vibe-coding-b2efeed37ea43f4773da5f1ff96f4184.png" />
</head>