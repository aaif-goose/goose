---
title: AnyAPI Extension
description: Add AnyAPI MCP Server as a goose Extension
---

import Tabs from '@theme/Tabs';
import TabItem from '@theme/TabItem';
import CLIExtensionInstructions from '@site/src/components/CLIExtensionInstructions';
import GooseDesktopInstaller from '@site/src/components/GooseDesktopInstaller';

This tutorial covers how to add the [AnyAPI MCP Server](https://getanyapi.com/docs/mcp-server) as a goose extension to reach hundreds of scraping and data APIs - social platforms, search results, maps, and general web extraction - through a single remote server.

AnyAPI is a hosted gateway, so there is nothing to install and no package to run. goose connects to the remote endpoint and you authorize it in the browser. Every API is priced per request in US dollars, and a failed call is not charged.

## Configuration

:::tip Quick Install
<Tabs groupId="interface">
  <TabItem value="ui" label="goose Desktop" default>
  [Launch the installer](goose://extension?type=streamable_http&url=https%3A%2F%2Fapi.getanyapi.com%2Fmcp&id=anyapi&name=AnyAPI&description=Hundreds%20of%20scraping%20and%20data%20APIs%20through%20one%20gateway%20-%20one%20key%2C%20USD%20pay-per-request%2C%20automatic%20failover)
  </TabItem>
  <TabItem value="cli" label="goose CLI">
  Use `goose configure` to add a `Remote Extension (Streamable HTTP)` extension type with:

  **Endpoint URL**
  ```
  https://api.getanyapi.com/mcp
  ```
  </TabItem>
</Tabs>
:::

:::info OAUTH FLOW
An OAuth window will open in your browser. Follow the prompts to authorize access to your AnyAPI wallet. New accounts start with free trial credit, so you can run calls before adding funds.
:::

<Tabs groupId="interface">
  <TabItem value="ui" label="goose Desktop" default>
    <GooseDesktopInstaller
      extensionId="anyapi"
      extensionName="AnyAPI"
      description="Hundreds of scraping and data APIs through one gateway - one key, USD pay-per-request, automatic failover"
      type="http"
      url="https://api.getanyapi.com/mcp"
    />
  </TabItem>
  <TabItem value="cli" label="goose CLI">
    <CLIExtensionInstructions
      name="anyapi"
      description="Hundreds of scraping and data APIs through one gateway - one key, USD pay-per-request, automatic failover"
      type="http"
      url="https://api.getanyapi.com/mcp"
      timeout={300}
    />
  </TabItem>
</Tabs>

For all setup and configuration options, see the [official AnyAPI MCP documentation](https://getanyapi.com/docs/mcp-server).

## Available Tools

| Tool | What it does |
|---|---|
| `list_apis` | Browse the catalog, optionally filtered by category. |
| `search_apis` | Ranked search over the catalog by meaning or keyword. |
| `get_api` | Full definition of one API, including its input and output JSON Schema and its price. |
| `quote_api` | Validate an input and return its exact USD price without running or charging anything. |
| `run_api` | Execute an API and return normalized output plus the amount charged. |
| `read_result` | Reshape a cached result without paying to run it again. |
| `get_balance` | Remaining USD balance on the wallet. |

Discovery, quoting, and reading a cached result are free. Only `run_api` is charged, and only when it succeeds.

## Example Usage 1

Find out which APIs exist for a job before committing to one, and what each would cost.

### goose Prompt

```
What APIs can get me the transcript of a TikTok video, and what do they cost?
```

### goose Output

```
I searched the AnyAPI catalog for TikTok transcript APIs. There are 45 relevant matches; the two that
directly transcribe a video are:

1. TikTok Video Transcript (tiktok.video_transcript)
   - Fetches the spoken-word transcript of a TikTok video by URL
   - $2.00 /1k req, with automatic failover to a second provider

2. TikTok Video Transcript, Audio (tiktok.video_transcript_full)
   - Transcribes the audio directly, with timed segments, speaker labels, and per-word confidence
   - For videos TikTok publishes no subtitle track for
   - $17.60 /1k req

If the video has a published subtitle track, the first one is the cheaper choice. The audio version is
what you want when it does not.
```

## Example Usage 2

Check the exact price of a call before you make it. Quoting is free and also validates the input.

### goose Prompt

```
Before you run anything, what would one Google search for "best coffee grinder" cost me?
```

### goose Output

```
I quoted it without running it. For google.search with that query:

- Likely charge: $0.0009
- Ceiling reserved: $0.00126

Pricing is flat at $0.0009 per call. The ceiling is what gets held during the request; you are charged
the actual amount, and nothing at all if the call fails.
```

## Example Usage 3

Run the call once you have picked an API. The response reports what it cost.

### goose Prompt

```
Get me the top Google results for "best coffee grinder".
```

### goose Output

```
I ran google.search through AnyAPI. It returned 8 organic results; here are the first five:

1. Best Budget Coffee Grinders for 2026
2. Coffee Grinders: A Buying Guide (and a Strong Suggestion)
3. What's the Best Coffee Grinder to Buy Right Now?
4. Your Coffee Will Be Better If You Get the Right Grinder
5. Top Rated Coffee and Espresso Grinders

That call cost $0.0009, taken from the wallet balance. There is a cursor in the response if you want
the next page of results.
```
