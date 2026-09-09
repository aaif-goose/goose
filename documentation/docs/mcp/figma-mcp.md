---
title: Figma Extension
description: Add Figma Dev Mode MCP Server as a goose Extension
---

import Tabs from '@theme/Tabs';
import TabItem from '@theme/TabItem';
import YouTubeShortEmbed from '@site/src/components/YouTubeShortEmbed';
import CLIExtensionInstructions from '@site/src/components/CLIExtensionInstructions';
import GooseDesktopInstaller from '@site/src/components/GooseDesktopInstaller';
import { PanelLeft } from 'lucide-react';

This tutorial covers how to add the [Figma Dev Mode MCP Server](https://help.figma.com/hc/en-us/articles/32132100833559-Guide-to-the-Dev-Mode-MCP-Server) as a goose extension to enable interaction with Figma files, designs, and components.

:::info
The MCP Server requires a Dev or Full seat on Professional, Organization, or Enterprise Figma plans.
:::

:::tip Quick Install
<Tabs groupId="interface">
  <TabItem value="ui" label="goose Desktop" default>
    [Launch the installer](goose://extension?type=streamable_http&url=http%3A%2F%2F127.0.0.1%3A3845%2Fmcp&id=figma&name=Figma&description=Convert%20Figma%20designs%20into%20code%20and%20extract%20design%20context)
  </TabItem>
  <TabItem value="cli" label="goose CLI">
    Use `goose configure` to add a `Remote Extension (Streamable HTTP)` extension type with:
    
    **Endpoint URL**
    
    ```
    http://127.0.0.1:3845/mcp
    ```
  </TabItem>
</Tabs>

**Required Setup**

The Dev Mode MCP Server must be enabled in the [Figma desktop app](https://www.figma.com/downloads/)
:::

## Configuration

1. Follow the instructions in Figma's [Guide to the Dev Mode MCP Server](https://help.figma.com/hc/en-us/articles/32132100833559-Guide-to-the-Dev-Mode-MCP-Server) to enable the MCP Server.

   Your server will now be running at `http://127.0.0.1:3845/mcp`

   :::info Alternative Setup
   If you don't see a `Preferences` menu in the desktop app as described in the instructions, try this:
   1. Click the `</>` toggle at the bottom of your design file
   2. In the right panel, toggle `Enable MCP server`
   :::

2. Add the Figma extension to goose:
   <Tabs groupId="interface">
     <TabItem value="ui" label="goose Desktop" default>
       <GooseDesktopInstaller
         extensionId="figma"
         extensionName="Figma"
         description="Convert Figma designs into code and extract design context"
         type="http"
         url="http://127.0.0.1:3845/mcp"
       />
     </TabItem>
     <TabItem value="cli" label="goose CLI">
       <CLIExtensionInstructions            
         name="figma"
         description="Convert Figma designs into code and extract design context"
         type="http"
         url="http://127.0.0.1:3845/mcp"
         timeout={300}
       />
     </TabItem>
   </Tabs>

## Example Usage

The Figma extension in goose works alongside the Figma desktop app. You can use the extension to:
- Convert Figma designs into production-ready code (React, Vue, HTML/CSS, etc.)
- Extract design tokens, variables, and generate design system rules
- Capture visual references of your designs

Let's try converting a Figma design into React code. First, copy the link:
  1. Open a design file in the [Figma desktop app](https://www.figma.com/downloads/) where the MCP Server is enabled
  2. In the layers panel, right-click any frame or component and choose `Copy link to selection`

:::tip
You can provide goose with a Figma link or simply select an element in the Figma desktop app.
:::

### goose Prompt

```
Convert this to React: https://www.figma.com/design/B6IFM1qjwxJw5fOYJYKRl5/Buttons-?node-id=351-8&m=dev
```

### goose Output

:::note Desktop

I'll help you convert that Figma design to React. Let me first examine the Figma file to understand the design structure and components.

```
manage_extensions action, extension_name
```

I'll help you convert that Figma design to React. Let me first extract the node ID from the URL and then generate the React code for you.

```
get_code nodeId, clientLanguages, clientFrameworks
```

```
get_image nodeId, clientLanguages, clientFrameworks
```

I've successfully converted your Figma design to React! Here's the generated component:

```jsx
const imgImage1 = "http://localhost:3845/assets/e0e0f6accb741cb6cacec2697cef047a7eea7e30.png";

function Component1() {
  return (
    <div className="relative size-full" data-name="Component 1" data-node-id="351:8">
      <div className="absolute bg-[#d9d9d9] inset-0 rounded-lg" data-node-id="351:2" />
      <div
        className="absolute font-['Inter:Black',_sans-serif] font-black inset-[23.33%_2.56%_22.22%_56.41%] leading-[normal] not-italic text-[#333b43] text-[20px]"
        data-node-id="351:3"
      >
        <p className="block mb-0">ask</p>
        <p className="block">goose</p>
      </div>
      <div
        className="absolute aspect-[92/90] bg-center bg-cover bg-no-repeat left-0 right-[52.82%] rounded-lg top-0"
        data-name="image 1"
        data-node-id="351:6"
        style={{ backgroundImage: `url('${imgImage1}')` }}
      />
    </div>
  );
}

export default function Component2() {
  return (
    <div className="relative size-full" data-name="Component 1" data-node-id="351:8">
      <Component1 />
    </div>
  );
}
```

#### Key Features of the Generated Component:

- **Tailwind CSS Styling**: Uses Tailwind classes for responsive design and styling
- **Image Asset**: The goose logo is loaded from a localhost server as a background image  
- **Typography**: Uses Inter Black font for the "ask goose" text
- **Layout**: Positioned absolutely with precise inset values to match the Figma design
- **Responsive**: Uses relative positioning and percentage-based insets for scalability

#### Component Structure:
- **Background**: Light gray rounded container (#d9d9d9)
- **Logo**: goose silhouette image positioned on the left side
- **Text**: "ask goose" text positioned on the right in dark gray (#333b43)

The component maintains the exact visual hierarchy and styling from your Figma design while being fully responsive and ready to integrate into a React application with Tailwind CSS.

:::
