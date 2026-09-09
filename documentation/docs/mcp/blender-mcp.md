---
title: Blender Extension
description: Add Blender MCP Server as a goose Extension
---

import Tabs from '@theme/Tabs';
import TabItem from '@theme/TabItem';
import YouTubeShortEmbed from '@site/src/components/YouTubeShortEmbed';
import GooseDesktopInstaller from '@site/src/components/GooseDesktopInstaller';
import CLIExtensionInstructions from '@site/src/components/CLIExtensionInstructions';

<YouTubeShortEmbed videoUrl="https://www.youtube.com/embed/izoQatKtJ2I" />

This tutorial covers how to add the [Blender MCP Server](https://github.com/ahujasid/blender-mcp) as a goose extension to create 3D scenes, control Blender with natural language, generate models, apply materials, and more.

:::tip Quick Install
<Tabs groupId="interface">
  <TabItem value="ui" label="goose Desktop" default>
  [Launch the installer](goose://extension?cmd=uvx&arg=blender-mcp&id=blender-mcp&name=Blender&description=Blender%203D%20scene%20creation%20integration)
  </TabItem>
  <TabItem value="cli" label="goose CLI">
  **Command**
  ```sh
  uvx blender-mcp
  ```
  </TabItem>
</Tabs>
:::

**Requirement**

Download [Blender Application](https://www.blender.org/download/) and [Blender MCP Addon file](https://github.com/ahujasid/blender-mcp/blob/main/addon.py)

## Configuration

:::info
Note that you'll need [uv](https://docs.astral.sh/uv/#installation) installed on your system to run this command, as it uses `uvx`.
:::

1. Download [Blender Application](https://www.blender.org/download/)
2. Add Blender MCP Addon
    - Download the `addon.py` file from the [Blender MCP repository](https://github.com/ahujasid/blender-mcp/blob/main/addon.py).
    - Open Blender
    - Navigate to `Edit` > `Preferences` > `Add-ons`.
    - Click the down arrow, select `install from disk`, add the `addon.py` file you downloaded.
    - After installing, check the box to enable `Blender MCP`.
3. Start Blender MCP server 
    - In Blender, press N to open the sidebar.
    - Go to the Blender MCP tab.
    - Click `connect to MCP server`

### Add Blender MCP Server

<Tabs groupId="interface">
  <TabItem value="ui" label="goose Desktop" default>
  <GooseDesktopInstaller
    extensionId="blender-mcp"
    extensionName="Blender"
    description="Blender 3D scene creation integration"
    command="uvx"
    args={["blender-mcp"]}
  />
  </TabItem>
  <TabItem value="cli" label="goose CLI">
    <CLIExtensionInstructions
      name="Blender"
      description="Blender 3D scene creation integration"
      command="uvx blender-mcp"
    />
  </TabItem>
</Tabs>

## Example Usage

In this example, I’ll show you how to use goose with the Blender Extension to build an entire 3D campfire scene from scratch. By automating Blender controls through natural language, goose can create objects like logs and fireballs, apply realistic materials, add lighting effects, generate particles for sparks, create subtle smoke, and even animate the fire to gently pulse over time.

In this example, I’ll show you how to use goose with the Blender Extension to build a full 3D campfire scene, including logs, fireballs, lighting, and subtle fire animation, just by describing what you want.

### goose Prompt

> Build a cozy campfire scene. Add a ground plane, place a few logs in a circle, and add a glowing fireball in the middle. Set the lighting to a warm tone like a sunset, and angle the camera for a dramatic shot. Give the logs a rough wood texture and make the fireball emit light.


### goose Output

:::note CLI

I'll help you create a cozy campfire scene in Blender. Let's break this down into steps and use a combination of basic objects, textures from PolyHaven, and proper lighting. First, let's check what we're working with.

─── **Get Scene Info** ───

Let's build the scene step by step:

First, let's remove the default cube and set up our ground plane:

─── **Execute Blender Code** ───

2. Now let's create the logs in a circle:

─── **Execute Blender Code** ───

3. Let's create the glowing fireball in the middle:

─── **Execute Blender Code** ───

Let me fix the fireball material:

─── **Execute Blender Code** ───

4. Finally, let's set up the lighting and camera for a dramatic sunset shot:

─── **Execute Blender Code** ───

I've created a cozy campfire scene with the following elements:

1. A large ground plane with a dark brown material
2. 5 logs arranged in a circle, with a rough wood material
3. A glowing fireball in the center with emission material
4. A point light to create the fire's glow effect
5. Adjusted the main sun light to create a warm sunset atmosphere
6. Positioned the camera for a dramatic angle
7. Set up the scene for better rendering with Cycles

The scene has a warm, cozy atmosphere with the logs arranged naturally around the glowing fireball. The lighting simulates a sunset with warm tones, and the camera is positioned to capture the scene from an interesting angle.

If you'd like to make any adjustments, such as:

- Changing the size or position of any elements
- Adjusting the lighting intensity
- Modifying the camera angle
- Adding more details like small rocks or additional logs

Just let me know and I'll help you refine the scene further!
:::

#### Blender Image

![Blender MCP Scene](/img/blender.png)
