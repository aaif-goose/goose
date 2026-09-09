---
title: Excalidraw Extension
description: Add Excalidraw MCP App as a goose Extension
---

import Tabs from '@theme/Tabs';
import TabItem from '@theme/TabItem';
import YouTubeShortEmbed from '@site/src/components/YouTubeShortEmbed';
import CLIExtensionInstructions from '@site/src/components/CLIExtensionInstructions';
import GooseDesktopInstaller from '@site/src/components/GooseDesktopInstaller';

<YouTubeShortEmbed videoUrl="https://www.youtube.com/embed/REc5IvWvI5s" />

This tutorial covers how to add the [Excalidraw MCP App](https://github.com/excalidraw/excalidraw-mcp) as a goose extension that enables goose to draw hand sketched Excalidraw diagrams in real time.

:::tip Quick Install
<Tabs groupId="interface">
  <TabItem value="ui" label="goose Desktop" default>
   [Launch the installer](goose://extension?cmd=http&id=excalidraw&name=Excalidraw&url=https%3A%2F%2Fexcalidraw-mcp-app.vercel.app%2Fmcp&description=Excalidraw%20MCP%20App%20for%20AI-powered%20diagramming)
  </TabItem>
  <TabItem value="cli" label="goose CLI">
  Add a `Remote Extension (Streamable HTTP)` extension type with:

  **Endpoint URL**
  ```
  https://excalidraw-mcp-app.vercel.app/mcp
  ```
  </TabItem>
</Tabs>
:::

## Configuration
These steps configure the Remote MCP Server. For other deployment options, see the [Excalidraw MCP App documentation](https://github.com/excalidraw/excalidraw-mcp).

<Tabs groupId="interface">
  <TabItem value="ui" label="goose Desktop" default>
    <GooseDesktopInstaller
      extensionId="Excalidraw"
      extensionName="Excalidraw MCP App"
      description="Excalidraw MCP App for diagramming"
      type="http"
      url="https://excalidraw-mcp-app.vercel.app/mcp"
      envVars={[]}
    />

  </TabItem>
  <TabItem value="cli" label="goose CLI">
    <CLIExtensionInstructions
      name="Excalidraw"
      description="Excalidraw MCP App for diagramming"
      type="http"
      url="https://excalidraw-mcp-app.vercel.app/mcp"
      timeout={300}
    />

  </TabItem>
</Tabs>

## Example Usage

In this example, we use the Excalidraw MCP App to have Goose visualize its own automation pipeline in real time. This demonstrates how Goose can connect to live tools, reason about workflows, and generate structured diagrams element by element.

### goose Prompt
```
Hey Goose review my video automation recipe and create a visual of the automation pipeline for me. I want clean lines and clear labels.
```

### goose Output

```

─── reading /Users/ebonyl/Desktop/plug-and-play-video.yaml ──────────────────────────

─── Create View elements: [ {"type":"cameraUpdate","width":600}, ...] ──────────────────────────

```
![excalidraw image](/img/excalidrawImage.png)
