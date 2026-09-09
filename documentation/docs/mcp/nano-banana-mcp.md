---
title: Nano Banana Extension
description: Add Nano Banana MCP Server as a goose Extension
---

import Tabs from '@theme/Tabs';
import TabItem from '@theme/TabItem';
import YouTubeShortEmbed from '@site/src/components/YouTubeShortEmbed';
import CLIExtensionInstructions from '@site/src/components/CLIExtensionInstructions';
import GooseDesktopInstaller from '@site/src/components/GooseDesktopInstaller';


<!--<YouTubeShortEmbed videoUrl="https://www.youtube.com/embed/VIDEO_ID" />-->


This tutorial covers how to add the [Nano Banana MCP Server](https://github.com/ConechoAI/Nano-Banana-MCP) as a goose extension that provides AI image generation and editing capabilities using Google's Gemini Image API.

:::tip Quick Install
<Tabs groupId="interface">
  <TabItem value="ui" label="goose Desktop" default>
  [Launch the installer](goose://extension?cmd=npx&arg=nano-banana-mcp&id=nano-banana-mcp&name=Nano%20Banana&description=Image%20generation%20with%20Gemini)
  </TabItem>
  <TabItem value="cli" label="goose CLI">
  **Command**
  ```sh
  npx nano-banana-mcp
  ```
  </TabItem>
</Tabs>
  **Environment Variable**
  ```
  GEMINI_API_KEY: <YOUR_API_KEY>
  ```
:::

## Configuration

:::info
Note that you'll need [Node.js](https://nodejs.org/) installed on your system to run this command, as it uses `npx`.
:::

<Tabs groupId="interface">
  <TabItem value="ui" label="goose Desktop" default>
    <GooseDesktopInstaller
      extensionId="nano-banana-mcp"
      extensionName="Nano Banana"
      description="Image generation with Gemini"
      type="stdio"
      command="npx"
      args={["nano-banana-mcp"]}
      envVars={[
        { name: "GEMINI_API_KEY", label: "Gemini API Key" }
      ]}
      apiKeyLink="https://aistudio.google.com/api-keys"
      apiKeyLinkText="Gemini API Key"
    />
  </TabItem>
  <TabItem value="cli" label="goose CLI">
    <CLIExtensionInstructions
      name="Nano Banana"
      description="Image generation with Gemini"
      type="stdio"
      command="npx nano-banana-mcp"
      timeout={300}
      envVars={[
        { key: "GEMINI_API_KEY", value: "▪▪▪▪▪▪▪▪▪▪▪▪▪▪▪▪▪▪▪▪▪▪▪▪▪▪▪▪▪▪▪▪▪" }
      ]}
      infoNote={
        <>
          Get your API key from <a href="https://aistudio.google.com/api-keys" target="_blank" rel="noopener noreferrer">
            Google AI Studio
          </a> and paste it in.
        </>
      }
    />
  </TabItem>
</Tabs>

## Example Usage

With Nano Banana, you can generate new images and edit existing images in natural language.  Simply describe what you want created or modified and goose will turn that into a detailed prompt to provide to Nano Banana.

### Image Creation Examples
Create images from scratch by describing what you want:

- "Create an image of a sunset over mountains"
- "Generate a logo for a coffee shop called Bean There"
- "Make a fantasy dragon guarding a treasure hoard"

### Image Iteration Examples
After generating or editing an image, you can keep refining it:

- "Make the sky more purple"
- "Add a cat in the corner"
- "Remove the text"
- "Make it more cartoon-style"

### goose Prompt

> use nano banana to create an image of a fantasy ice mage crafting ice sculptures in a frozen workshop

### goose Output

:::note Desktop

🎨 Image generated with nano-banana (Gemini 2.5 Flash Image)!


Prompt: "A fantasy ice mage crafting ice sculptures in a frozen workshop. The mage wears flowing robes with frost patterns, their hands glowing with blue magical energy as they shape intricate ice sculptures. The workshop is filled with beautiful frozen creations - ice animals, crystalline flowers, and elaborate figurines. Icicles hang from the ceiling, frost covers the stone walls, and magical blue light illuminates the space. Snow gently drifts through the air. Detailed fantasy art style with magical atmosphere."

✨ **Done!** Here's your fantasy ice mage image!

The image turned out beautifully — it features:

- **The Ice Mage**: A hooded figure in elegant deep blue robes with intricate frost and snowflake embroidery, hands glowing with blue magical energy
- **Ice Sculptures**: A stunning unicorn being crafted, a pegasus in the background, crystalline rose bouquets, and small knight figurines
- **The Frozen Workshop**: A magnificent ice cathedral-like space with dramatic icicles hanging from vaulted ceilings, frost-covered pillars, and geometric ice patterns
- **Magical Atmosphere**: Rainbow light refractions, drifting snow particles, and an ethereal blue glow throughout

The image has been saved to:
`generated_imgs/generated-2025-12-04T04-15-47-907Z-3grhec.png`

![](/img/nano-banana-generated-img.png)

Would you like me to make any modifications to this image? I can adjust elements like the mage's appearance, add more sculptures, change the lighting, or tweak any other details using the `continue_editing` feature!

:::
