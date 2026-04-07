---
title: ElevenLabs Extension
description: Add ElevenLabs MCP Server as a goose Extension
---

import Tabs from '@theme/Tabs';
import TabItem from '@theme/TabItem';
import YouTubeShortEmbed from '@site/src/components/YouTubeShortEmbed';
import GooseDesktopInstaller from '@site/src/components/GooseDesktopInstaller';
import CLIExtensionInstructions from '@site/src/components/CLIExtensionInstructions';

<YouTubeShortEmbed videoUrl="https://www.youtube.com/embed/1Z8XtjQ9El0" />


This tutorial covers how to add the [ElevenLabs MCP Server](https://github.com/elevenlabs/elevenlabs-mcp) as a goose extension to enable AI-powered voice generation, voice cloning, audio editing, and speech-to-text transcription.

:::tip Quick Install
<Tabs groupId="interface">
  <TabItem value="ui" label="goose Desktop" default>
  [Launch the installer](goose://extension?cmd=uvx&arg=elevenlabs-mcp&id=elevenlabs&name=ElevenLabs&description=ElevenLabs%20voice%20synthesis%20server&env=ELEVENLABS_API_KEY)
  </TabItem>
  <TabItem value="cli" label="goose CLI">
  **Command**
  ```sh
  uvx elevenlabs-mcp
  ```
  </TabItem>
</Tabs>

  **Environment Variable**
  ```
  ELEVENLABS_API_KEY: <YOUR_API_KEY>
  ```
:::

## Configuration

:::info
Note that you'll need [uv](https://docs.astral.sh/uv/#installation) installed on your system to run this command, as it uses `uvx`.
:::

<Tabs groupId="interface">
  <TabItem value="ui" label="goose Desktop" default>
    <GooseDesktopInstaller
      extensionId="elevenlabs"
      extensionName="ElevenLabs"
      description="ElevenLabs voice synthesis server"
      command="uvx"
      args={["elevenlabs-mcp"]}
      envVars={[
        { name: "ELEVENLABS_API_KEY", label: "ElevenLabs API Key" }
      ]}
      apiKeyLink="https://elevenlabs.io/app/settings/api-keys"
      apiKeyLinkText="ElevenLabs API Key"
    />
  </TabItem>
  <TabItem value="cli" label="goose CLI">
    <CLIExtensionInstructions
      name="ElevenLabs"
      description="ElevenLabs voice synthesis server"
      command="uvx elevenlabs-mcp"
      envVars={[
        { key: "ELEVENLABS_API_KEY", value: "▪▪▪▪▪▪▪▪▪▪▪▪▪▪▪▪▪▪▪▪▪▪▪▪▪▪▪▪▪▪▪▪▪▪▪▪▪▪▪" }
      ]}
      infoNote={
        <>
          Obtain your <a href="https://elevenlabs.io/app/settings/api-keys" target="_blank" rel="noopener noreferrer">ElevenLabs API Key</a> and paste it in.
        </>
      }
    />
  </TabItem>
</Tabs>

## Example Usage

In this example, I’ll show you how to use goose with the ElevenLabs Extension to create AI-generated voiceovers for a YouTube Short. goose will take a sample script I provided, generate a narrated version using different AI voices, and seamlessly switch tones mid-script to match the content flow.

By connecting to the ElevenLabs MCP server, goose can transform plain text into natural-sounding speech, offering multiple voice styles and character options — all without any manual recording or editing.

### goose Prompt

> Hey goose, create a script for me for my youtube short video, I want there to be two different voices. The first voice should cut me off and be a human narrator style and then switch to a casual AI tone after I read the prompt. Here's an example of a YT short script I've done in the past: 

Wait… Within Seconds, goose performed Security Audits Across Multiple Projects?! 🔥

Lets, plug & play to find out how 

Let’s provide goose with the command it needs to connect to the Filesystem MCP server extension…

Now lets play 
prompt:  "Hey goose, I need to perform a security audit across multiple projects. Let's check for…🔹 Hardcoded Credentials – API keys, passwords, and secrets left in the code.🔹 SQL Injection Risks – Unsafe queries that could expose data.🔹 Insecure Cryptographic Practices – Weak encryption methods that put data at risk.AND🔹 Exposed Config Files – Sensitive information that shouldn't be public.🔹 Outdated Dependencies – Security vulnerabilities in third-party libraries."

Go goose, go goose!

✅ goose scanned the entire codebase across 3 different projects, identified security risks, generated a detailed report with fixes and provided me with step by step instructions on how I can test and verify these code fixes! 

If that’s not amazing idk what is …

🚀 to get started visit goose-docs.ai_


### goose Output

:::note Desktop

I'll create your YouTube script for you using the given script as reference. 

───────────── Text To Speech ─────────────

───────────── Text To Speech ─────────────

───────────── Text To Speech ─────────────

───────────── Text To Speech ─────────────

───────────── Text To Speech ─────────────

───────────── Play Audio  ────────────────

───────────── Play Audio  ────────────────

The script has been created and read aloud using the specified voices and style. The audio files have been saved to your desktop.

Press play and hear it for yourself! 🔊

<audio controls>
  <source src="/audio/elevenlabs-mcp-demo.mp3" type="audio/mpeg" />
  Your browser does not support the audio element.
</audio>

:::