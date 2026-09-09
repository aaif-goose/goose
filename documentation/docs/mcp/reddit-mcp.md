---
title: Reddit Extension

description: Add Reddit MCP Server as a goose Extension
---

import Tabs from '@theme/Tabs';
import TabItem from '@theme/TabItem';
import YouTubeShortEmbed from '@site/src/components/YouTubeShortEmbed';
import CLIExtensionInstructions from '@site/src/components/CLIExtensionInstructions';
import GooseDesktopInstaller from '@site/src/components/GooseDesktopInstaller';

<YouTubeShortEmbed videoUrl="https://www.youtube.com/embed/VBtFinHieHA" /> 


This tutorial covers how to add the [Reddit MCP Server](https://github.com/adhikasp/mcp-reddit) as a goose extension to fetch trending threads, analyze Reddit post content, and explore subreddit discussions.

:::tip Quick Install
<Tabs groupId="interface">
  <TabItem value="ui" label="goose Desktop" default>
  [Launch the installer](goose://extension?cmd=uvx&arg=--from&arg=git%2Bhttps%3A%2F%2Fgithub.com%2Fadhikasp%2Fmcp-reddit.git&arg=mcp-reddit&id=reddit-mcp&name=Reddit&description=Fetch%20and%20analyze%20Reddit%20content)
  </TabItem>
  <TabItem value="cli" label="goose CLI">
  **Command**
  ```sh
  uvx --from git+https://github.com/adhikasp/mcp-reddit.git mcp-reddit
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
      extensionId="reddit-mcp"
      extensionName="Reddit"
      description="Fetch and analyze Reddit content"
      command="uvx"
      args={["--from", "git+https://github.com/adhikasp/mcp-reddit.git", "mcp-reddit"]}
    />
  </TabItem>

  <TabItem value="cli" label="goose CLI">
    <CLIExtensionInstructions
      name="Reddit MCP"
      description="Fetch and analyze Reddit content"
      command="uvx --from git+https://github.com/adhikasp/mcp-reddit.git mcp-reddit"
      timeout={300}
    />
  </TabItem>

</Tabs>

## Example Usage

In this example, I’ll use goose with the Reddit MCP Server to build a custom news feed for AI topics. goose will pull top AI threads from Reddit and turn them into a scrollable, social media-style feed with images and highlights.

It’s like Reddit, but curated just for you.

### goose Prompt

> goose using the reddit MCP server, create a personalized social feed of the latest AI threads. Include images and format it like a real social media app.



### goose Output

:::note Desktop

I'll help you create a personalized social media feed using reddits threads and subthreads. Lets first check the content we'll be working with, and then create the web app. 

Let's fetch some sample posts from both subreddits to ensure we have the data structure: 


──────────────  Fetch Reddit Hot Threads ─────────────────

Let's create the web app using javascript:

──────────────  Text Editor ─────────────────

I've opened the web app in your default browser. You should now see your personalized social  media feed.

The page will automatically fetch the latest posts from r/ProgrammerAIHumor by default, and you can switch to r/AI by clicking the button at the top. Each post is clickable and will take you to the original Reddit thread in a new tab.

Let me know if you'd like to make any adjustments to:

- The color scheme
- The layout
- The refresh rate
- The number of posts shown

Any other features!

Also, if you're having any issues viewing the page, please let me know and I'll help troubleshoot.
:::
