---
title: Netlify Extension
description: Add Netlify MCP Server as a goose Extension
---

import Tabs from '@theme/Tabs';
import TabItem from '@theme/TabItem';
import YouTubeShortEmbed from '@site/src/components/YouTubeShortEmbed';
import GooseDesktopInstaller from '@site/src/components/GooseDesktopInstaller';
import CLIExtensionInstructions from '@site/src/components/CLIExtensionInstructions';

<YouTubeShortEmbed videoUrl="https://www.youtube.com/embed/iSUNmxOf6gw" />

This tutorial covers how to add the [Netlify MCP Server](https://github.com/netlify/netlify-mcp) as a goose extension to build, deploy, and manage Netlify sites.

:::tip Quick Install
<Tabs groupId="interface">
  <TabItem value="ui" label="goose Desktop" default>
  [Launch the installer](goose://extension?cmd=npx&arg=-y&arg=%40netlify%2Fmcp&id=netlify-mcp&name=Netlify&description=Build%2C%20deploy%2C%20and%20manage%20sites%20with%20Netlify%27s%20official%20MCP%20server.)
  </TabItem>
  <TabItem value="cli" label="goose CLI">
  **Command**
  ```sh
  npx -y @netlify/mcp
  ```
  </TabItem>
</Tabs>
:::

## Configuration

Make sure that you are signed in to your Netlify account:

```bash
netlify login
```

### Add Netlify MCP Server

:::info
Note that you'll need [Node.js](https://nodejs.org/) installed on your system to run this command, as it uses `npx`.
:::

<Tabs groupId="interface">
  <TabItem value="ui" label="goose Desktop" default>
  <GooseDesktopInstaller
    extensionId="netlify-mcp"
    extensionName="Netlify"
    description="Build, deploy, and manage sites with Netlify's official MCP server."
    command="npx"
    args={["-y", "@netlify/mcp"]}
  />
  </TabItem>
  <TabItem value="cli" label="goose CLI">
    <CLIExtensionInstructions
      name="Netlify"
      description="Build, deploy, and manage sites with Netlify's official MCP server"
      command="npx -y @netlify/mcp"
    />
  </TabItem>
</Tabs>

## Example Usage

In this example, I’ll show you how to use goose with the Netlify Extension to deploy a web application.

### goose Prompt

> Can you deploy my app to Netlify via my GitHub repo?

### goose Output

:::note CLI

<details>
    <summary>Tool Calls</summary>
    I'll help you deploy your tic-tac-toe app to Netlify from your GitHub repository. Let me first check your Netlify account and then set up the deployment.

    ─── netlify-user-services | netlify ──────────────────────────

    selectSchema: <br/>
        &nbsp;&nbsp;&nbsp;&nbsp;operation: get-user <br/>
        &nbsp;&nbsp;&nbsp;&nbsp;params:

    Great! I can see you're logged into Netlify with your GitHub account connected. Now let me create a new Netlify site and deploy your tic-tac-toe app from your GitHub repository.

    ─── netlify-project-services | netlify ──────────────────────────

    selectSchema:<br/>
        &nbsp;&nbsp;&nbsp;&nbsp;operation: create-new-project<br/>
        &nbsp;&nbsp;&nbsp;&nbsp;params:<br/>
            &nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;name: tic-tac-toe-game

    Let me try a different name for the project:

    ─── netlify-project-services | netlify ──────────────────────────

    selectSchema:<br/>
        &nbsp;&nbsp;&nbsp;&nbsp;operation: create-new-project<br/>
        &nbsp;&nbsp;&nbsp;&nbsp;params:<br/>
            &nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;name: janedoe-tic-tac-toe

    Perfect! I've created a new Netlify site called "janedoe-tic-tac-toe". Now I need to deploy your code from the GitHub repository. Let me deploy the site:

    ─── netlify-deploy-services | netlify ──────────────────────────

    selectSchema:<br/>
        &nbsp;&nbsp;&nbsp;&nbsp;operation: deploy-site<br/>
        &nbsp;&nbsp;&nbsp;&nbsp;params:<br/>
            &nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;deployDirectory: /Users/janedoe/Development/tic-tac-toe<br/>
            &nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;siteId: 1731e49e-1d0a-49a3-ad95-975b550f324a

    Let me check the deployment status and get the live URL for your site:

    ─── netlify-project-services | netlify ──────────────────────────

    selectSchema:<br/>
        &nbsp;&nbsp;&nbsp;&nbsp;operation: get-project<br/>
        &nbsp;&nbsp;&nbsp;&nbsp;params:<br/>
            &nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;siteId: 1731e49e-1d0a-49a3-ad95-975b550f324a
</details>
:::

🎉 **Success! Your tic-tac-toe game has been deployed to Netlify!**

## Deployment Details:

- **Live Site URL**: http://janedoe-tic-tac-toe.netlify.app
- **Netlify Project Dashboard**: https://app.netlify.com/projects/janedoe-tic-tac-toe
- **Site ID**: 1731e49e-1d0a-49a3-ad95-975b550f324a

## What was deployed:

✅ **HTML**: Your main game interface

✅ **CSS**: Modern styling with animations and responsive design

✅ **JavaScript**: Complete game logic and interactivity

✅ **README**: Project documentation

## Next Steps:

1. **Visit your live site**: http://janedoe-tic-tac-toe.netlify.app
2. **Test the game**: Make sure everything works as expected
3. **Share with others**: Your game is now live on the web!

## Future Updates:

Since your code is deployed from your local directory, if you want to make changes:

1. Update your local files
2. Push changes to your GitHub repository
3. You can either:
   - Redeploy manually using the same command
   - Set up continuous deployment from GitHub (which would auto-deploy when you push changes)

Would you like me to help you set up continuous deployment from your GitHub repository so that future changes are automatically deployed?

Your tic-tac-toe game is now live and ready to play! 🎮
