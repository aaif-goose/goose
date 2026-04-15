---
title: "MCP Explained for Non-Developers"
description: Learn what Model Context Protocol (MCP) is and how anyone can use it to save time on tasks.
authors: 
    - tania
---

![blog cover](mcp_nondevs.png)

MCP this, MCP that, what exactly is it, and can you use them if you're not a developer? 🤔

<!--truncate-->

## What is MCP?

MCP stands for [Model Context Protocol](https://modelcontextprotocol.io/introduction), an open standard created by Anthropic.

Let's say you're looking for ways to use AI at work to become more efficient and save as much time as possible. So you go off and learn about large language models (LLMs) like OpenAI or Claude, and start chatting with one. It's amazing being able to chat with AI and have it instantly answer questions or have it tell you how to do something, but how about getting the AI to do stuff for you?

Now there are AI agents, or AI assistants, that can take actions and make decisions for you. But in order to have your AI agent interact with your systems, like Google Drive, Asana, or Slack, there wasn't a standard way to do it. At least not without figuring it out from scratch each time you needed your AI agent to work with what you need it to work with. That's super tedious.

That's exactly where MCP comes in. Best part is, you don't need to be a developer to start using them! MCP essentially allows you to give AI agents access to your external systems without having to code. You can think of MCP as the connector for a system and your AI agent, or like the USB-C of AI integrations.

## MCP Servers You Should Try Right Now
So what can you connect your AI agent to? MCP Servers! MCP servers give your agent access to your tools. With [over 3000 MCP servers](https://glama.ai/mcp/servers) you can connect to, here is your top list of popular MCP servers you should try:

- **[Google Drive](/docs/mcp/google-drive-mcp)**: File access and search capabilities for Google Drive
- **[YouTube Transcript](/docs/mcp/youtube-transcript-mcp)**: Grab and work with YouTube video transcripts
- **[Google Maps](/docs/mcp/google-maps-mcp)**: Location services, directions, and place details
- **[Tavily Web Search](/docs/mcp/tavily-mcp)**: Web and local search using Tavily's Search API
- **[Asana](/docs/mcp/asana-mcp)**: View asana tasks, projects, workspaces, and/or comments
- **[Speech](/docs/mcp/speech-mcp)**: Real-time voice interaction, audio/video transcription, text-to-speech conversion and more
- **[GitHub](/docs/mcp/github-mcp)**: Tools to read, search, and manage Git repositories
- **[Fetch](/docs/mcp/fetch-mcp)**: Web content fetching and conversion for efficient LLM usage

This quick list should give you an idea of all the ways you can now use AI agents with your workflow. You can also explore community favorites in [handy MCP directories](https://dev.to/techgirl1908/my-favorite-mcp-directories-573n), and learn [how to check MCP servers are safe](/blog/2025/03/26/mcp-security) before installing.

You can also check out these [Goose tutorials](/docs/category/mcp-servers), showing you exactly how you can use some of these popular MCP servers with Goose, or use [Goose's Tutorial extension](/docs/mcp/tutorial-mcp) to get extra help walking you through using or building extensions.

## Example MCP Prompts
Now that you've caught a glimpse of some of the MCP servers that out there, how do you make sure you're using MCPs with AI agents the best you can? This is where prompts come in.

Prompts are ultimately the text you input when interacting with an AI assistant, and prompts can range from super simple questions to detailed instructions! Here are some example prompts you can ask an AI agent like Goose right now that use some of the MCP servers mentioned above:

### Google Maps
```
Google Maps: Track the live GPS location of driver ID #{driver_id}. Query Google Maps for real-time traffic data and adjust the estimated delivery time if delays exceed 5 minutes. If ETA changes, update the customer's live tracker and send an SMS notification. If the delay is greater than 20 minutes, check if another driver within a 1-mile radius can take over the delivery.
```
### YouTube Transcript
```
YouTube Transcript: Get the transcript from this youtube video [link to video]. Then, summarize it into a blog post.
```
### Google Drive
```
I have an important marketing budget review meeting in 30 minutes and I need your help getting prepared. I have several documents in my Google Drive from our previous meetings and planning sessions. Could you help me by:

1. Finding all relevant documents about our marketing budget and performance
2. Giving me a quick summary of our Q1 performance
3. Highlighting the key decisions we need to make about the marketing automation tool and video production
4. Identifying any outstanding action items from our last meeting
```
### Asana
```
Asana: Create a new task in my Asana workspace called 'Review Q4 metrics' and set the due date to next Friday. Then, find all tasks assigned to me that are due this week and summarize them.
```
### GitHub
```
GitHub: Create a new branch called hello-world in my angiejones/goose-demo repository. Update the README.md file to say "this was written by goose" and commit it. Open a pull request with your changes.
```

To see more examples just like this, along with the results you can get, check out this [Prompt Library](https://goose-docs.ai/prompt-library)! This is your central directory for discovering and using effective prompts with Goose.

## The Possibilities Are Endless
While some are developed by official providers, a vast majority of MCP servers you see are actually developed by community members! Plus, because MCP is an open standard, anyone can build an MCP server for any resource. You could even use Goose to help you build one!

Hopefully now, instead of spending hours manually gathering data and creating your next marketing report, or manually sorting through your todo-backlog on a Monday, you will use MCP with Goose and have it done for you in minutes.

*To learn more about using MCP servers and Goose, check out the [Goose documentation](https://goose-docs.ai/docs/category/getting-started), or join the [Block Open Source Discord](https://discord.gg/goose-oss) to connect with other open source community members.*

<head>
  <meta property="og:title" content="MCP Explained for Non-Developers" />
  <meta property="og:type" content="article" />
  <meta property="og:url" content="https://goose-docs.ai/blog/2025/04/01/mcp-nondevs" />
  <meta property="og:description" content="Learn what Model Context Protocol (MCP) is and how anyone can use it to save time on tasks." />
  <meta property="og:image" content="http://goose-docs.ai/assets/images/mcp_nondevs-5ce7f39de923cab01de6e14e5dc06744.png" />
  <meta name="twitter:card" content="summary_large_image" />
  <meta property="twitter:domain" content="goose-docs.ai" />
  <meta name="twitter:title" content="MCP Explained for Non-Developers" />
  <meta name="twitter:description" content="Learn what Model Context Protocol (MCP) is and how anyone can use it to save time on tasks." />
  <meta name="twitter:image" content="http://goose-docs.ai/assets/images/mcp_nondevs-5ce7f39de923cab01de6e14e5dc06744.png" />
</head>