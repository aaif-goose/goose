---
title: YouTube Transcript Extension
description: Add YouTube Transcript MCP Server as a goose Extension for accessing YouTube video transcripts
---

import Tabs from '@theme/Tabs';
import TabItem from '@theme/TabItem';
import YouTubeShortEmbed from '@site/src/components/YouTubeShortEmbed';
import GooseDesktopInstaller from '@site/src/components/GooseDesktopInstaller';
import CLIExtensionInstructions from '@site/src/components/CLIExtensionInstructions';

<YouTubeShortEmbed videoUrl="https://www.youtube.com/embed/N38u7hZqZJg" />

This tutorial covers how to add the [YouTube Transcript MCP Server](https://github.com/jkawamoto/mcp-youtube-transcript) as a goose extension to enable fetching and working with YouTube video transcripts.

:::tip Quick Install
<Tabs groupId="interface">
  <TabItem value="ui" label="goose Desktop" default>
  [Launch the installer](goose://extension?cmd=uvx&arg=--from&arg=git%2Bhttps%3A%2F%2Fgithub.com%2Fjkawamoto%2Fmcp-youtube-transcript&arg=mcp-youtube-transcript&id=youtube-transcript-mcp&name=YouTube%20Transcript&description=Access%20YouTube%20video%20transcripts)
  </TabItem>
  <TabItem value="cli" label="goose CLI">
  **Command**
  ```sh
  uvx --from git+https://github.com/jkawamoto/mcp-youtube-transcript mcp-youtube-transcript
  ```
  </TabItem>
</Tabs>
:::

## Configuration


:::info
Note that you'll need [uv](https://docs.astral.sh/uv/#installation) installed on your system to run this command, as it uses `uvx`.
:::

<Tabs groupId="interface">
  <TabItem value="ui" label="goose Desktop" default>
  <GooseDesktopInstaller
    extensionId="youtube-transcript-mcp"
    extensionName="YouTube Transcript"
    description="Access YouTube video transcripts"
    command="uvx"
    args={["--from", "git+https://github.com/jkawamoto/mcp-youtube-transcript", "mcp-youtube-transcript"]}
  />
  </TabItem>
  <TabItem value="cli" label="goose CLI">
    <CLIExtensionInstructions
      name="YouTube Transcript"
      description="Access YouTube video transcripts"
      command="uvx --from git+https://github.com/jkawamoto/mcp-youtube-transcript mcp-youtube-transcript"
    />
  </TabItem>
</Tabs>

## Example Usage

The YouTube Transcript extension allows you to fetch and work with transcripts from YouTube videos. You'll need the video ID from the YouTube URL you want to get the transcript for.

### goose Prompt

```
Get me the transcript for this YouTube video: https://www.youtube.com/watch?v=dQw4w9WgXcQ
```

### goose Output

:::note CLI
I'll help you get the transcript for that video. The video ID is "dQw4w9WgXcQ". Let me fetch the transcript for you.

Here's the transcript:

[Transcript content would appear here with timestamps and text]

I've retrieved the transcript for Rick Astley's "Never Gonna Give You Up" music video. The transcript shows the lyrics of the song, though there are some minor transcription errors due to the automated nature of the system. The transcript includes the iconic chorus and verses of this famous 1987 song, which has become one of the most well-known internet memes, often used for "rickrolling."

Would you like me to help you with anything else regarding the video or its transcript?
:::
