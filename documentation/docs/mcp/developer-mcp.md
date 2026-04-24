---
title: Developer Extension
description: Use Developer MCP Server as a goose Extension
---

import Tabs from '@theme/Tabs';
import TabItem from '@theme/TabItem';
import YouTubeShortEmbed from '@site/src/components/YouTubeShortEmbed';
import GooseBuiltinInstaller from '@site/src/components/GooseBuiltinInstaller';
import { Tornado } from 'lucide-react';

<YouTubeShortEmbed videoUrl="https://www.youtube.com/embed/on_p-LeIrak" />

The Developer extension allows goose to automate developer-centric tasks such as file editing, shell command execution, and project setup. It also provides tools for [enhanced code editing](/docs/guides/enhanced-code-editing) and [codebase analysis](/docs/guides/codebase-analysis).

This tutorial will cover enabling and using the Developer MCP Server, which is a built-in goose extension. 


## Configuration

:::info
The Developer extension is already enabled by default when goose is installed.
:::

<Tabs groupId="interface">
  <TabItem value="ui" label="goose Desktop" default>
  <GooseBuiltinInstaller
    extensionName="Developer"
    description="Automate developer-centric tasks like file editing and shell commands"
  />
  </TabItem>
  <TabItem value="cli" label="goose CLI">

  1. Run the `configure` command:
  ```sh
  goose configure
  ```

  2. Choose to `Toggle Extensions`
  ```sh
  ┌   goose-configure 
  │
  ◇  What would you like to configure?
  │  Toggle Extensions 
  │
  ◆  Enable extensions: (use "space" to toggle and "enter" to submit)
  // highlight-start    
  │  ● developer
  // highlight-end  
  └  Extension settings updated successfully
  ```
  </TabItem>
</Tabs>

## Example Usage

In this example, I'm going to have goose automate setting up my JavaScript developer environment with Express, Mongoose, Nodemon, Dotenv and initialize Git.


<Tabs groupId="interface">
  <TabItem value="ui" label="goose Desktop" default>
   1. Open a new session in goose Desktop
  </TabItem>
  <TabItem value="cli" label="goose CLI">

  1. Open a terminal and start a new goose session:

  ```sh
  goose session
  ```

  </TabItem>
</Tabs>

### goose Prompt
```
set up a new JavaScript project with Express, Mongoose, Nodemon, and Dotenv? Fetch the latest package versions, generate a README with those versions, and initialize Git
```

After 1 minute, goose was done setting up my entire dev environment! 🎉

### goose Output

```md
I'll help you set up a new JavaScript project with those dependencies. I'll break this down into steps:

1. First, let's create a new directory and initialize the project
2. Then install the dependencies
3. Create a detailed README
4. Initialize git
5. Create some starter files
6. Let's begin:

[... shell commands, files created ...]

Great! I've set up a complete Express project with MongoDB and Dotenv integrations:

1. Created a new project directory and initialized npm
2. Installed Express, Mongoose, and dotenv dependencies
3. set up the basic project structure with directories for:
    - config/
    - models/
    - routes/ 
    - controllers/
4. Created essential files:
    - index.js (main server file)
    - .env (environment variables)
    - .ignore (with common node.js ignores)
5. Added a basic user model with basic name and email fields
6. Created a user controller with two methods:
    - getUsers (GET all users)
    - createUser (POST new user)
7. Added user routes under `/api/routes`
8. Updated package.json with start scripts
9. Created a comprehensive README.md with:
    - Installation scripts
    - Project structure
    - Available scripts
    - Environment variables 
    - Latest package versions

The project structure is:

my-express-app/
├── config/         # Configuration files
├── models/        # MongoDB models
│   └── User.js    # User model
├── routes/        # API routes
│   └── userRoutes.js
├── controllers/   # Route controllers
│   └── userController.js
├── .env           # Environment variables
├── .gitignore     # Git ignore file
├── index.js       # Entry point
└── package.json   # Project dependencies and scripts

To start developing you can:

1. Run `npm install` to install dependencies
2. start the server with `npm run dev` 

The API will be available at:
    - `http://localhost:3000/api/users` (GET to get all users)
    - `http://localhost:3000/api/users` (POST to create a new user)

To test out the API you can use tools like postman or curl, for example, to create a new user:

curl x POST http://localhost:3000/api/users \
-H "Content-Type: application/json" \
-d '{"name": "Ebony Louis" , "email": "ebony@example.com"}'

Would you like me to add any additional features or make other improvements to the setup?
```

## Environment Variables in Shell Commands

Shell commands executed by the `shell` tool inherit the environment of the running goose process. This typically includes:
- System variables like `PATH`, `HOME`, and `USER`
- Environment variables present in the process that launched goose (for example, your terminal's environment when you start goose from a shell)
- Session-specific variables injected by goose, such as `AGENT_SESSION_ID` for [session-isolated workflows](/docs/guides/environment-variables#using-session-ids-in-workflows)

This enables workflows that depend on environment configuration, such as authenticated CLI operations and build processes.

:::info
goose Desktop or launcher-based starts may use a different environment and may not load your shell startup files.
:::

:::warning Sensitive Information
Environment variables may contain sensitive values like API keys and tokens (e.g., `GITHUB_TOKEN`, `AWS_ACCESS_KEY_ID`).
:::

## Configuring Access Controls

By default, goose can run system commands with your user privileges and edit any accessible file **without your approval**. This is because goose runs in Autonomous permission mode by default and has access to the Developer extension's shell and file editing tools. While this configuration allows goose to work quickly and independently, there's potential for unexpected outcomes. Understanding the available access control features can help you configure goose to match your comfort level and specific needs.

:::tip
See the [Quick Setup Example](#quick-setup-example) below for some ways to configure more control over goose's behavior.
:::

### Developer Extension Tools

The Developer extension provides these tools:

| Tool | Description | Use Cases | Risk Level |
|------|-------------|-----------|------------|
| `shell` | Execute shell commands | Running tests, installing packages, git operations | ⚠️ High<br />Can run any system command with your user privileges |
| `text_editor` | Read, write, and edit files | Code refactoring, creating files, updating configs | ⚠️ High<br />Can modify any accessible file |
| `analyze` | Analyze code structure | Understanding codebase, finding dependencies | ✅ Low<br />Read-only code analysis |
| `screen_capture` | Take screenshots | Debugging UI issues, documenting state | ✅ Low<br />Visual information only |
| `image_processor` | Process and resize images | Optimizing assets, format conversion | ✅ Low<br />Image manipulation only |

### Access Control Features

You can layer multiple controls to match your risk tolerance and workflow:

- **[goose Permission Modes](/docs/guides/goose-permissions)** control when goose asks for approval:

  | Mode | Description | Use Cases |
  |------|-------------|-----------|
  | Autonomous<br />CLI: `auto` | No approval required | Best for experienced users in safe environments |
  | Manual Approval<br />CLI: `approve` | Review every action | Recommended for sensitive work or when you want maximum control |
  | Smart Approval<br />CLI: `smart_approve` | AI decides what needs review | Balanced approach |
  | Chat Only<br />CLI: `chat` | Disable all tools | For maximum security and models that don't support tool-calling |

- **[Tool Permissions](/docs/guides/managing-tools/tool-permissions)** let you set `Always allow`, `Ask before`, and `Never allow` permissions for individual extension tools when in Manual Approval or Smart Approval modes

- **[.gooseignore files](/docs/guides/using-gooseignore)** restrict which files and directories goose can access (`.gitignore` files are fallback)

:::tip Changing Modes In-Session
You can change goose permission modes during a session without restarting:
- **CLI**: Use the `/mode` command (e.g. `/mode approve`)
- **Desktop**: Use the <Tornado className="inline" size={16} /> mode selector button in the bottom menu
:::

#### Quick Setup Example

You might want more control over goose's operations when working with sensitive systems, exploring unfamiliar codebases, using untrusted models, or simply preferring to review actions before execution.

Here's an example configuration that enables oversight:

1. **Set the [permission mode](/docs/guides/goose-permissions)** to Smart Approval or Manual Approval:
   ```yaml
   # ~/.config/goose/config.yaml
   GOOSE_MODE: smart_approve  # or approve
   ```

2. **Create a [`.gooseignore` file](/docs/guides/using-gooseignore)** in your project to protect sensitive files:
   ```
   .env*
   secrets.*
   *.key
   *.pem
   .git/
   ```

3. **Configure [tool permissions](/docs/guides/managing-tools/tool-permissions)** based on your needs

As you become more comfortable with goose's behavior, you can adjust these settings to reduce friction while maintaining appropriate safeguards for your environment.

:::info
Also see the [Security Guide](/docs/guides/security/) for information about using goose safely.
:::

## Additional Resources

import ContentCardCarousel from '@site/src/components/ContentCardCarousel';

<ContentCardCarousel
  items={[
    {
      type: 'topic',
      title: 'Enhanced Code Editing with AI Models',
      description: 'Use AI models to intelligently apply code changes',
      linkUrl: '/docs/guides/enhanced-code-editing'
    },
    {
      type: 'topic',
      title: 'Codebase Analysis',
      description: 'Understand codebases with semantic analysis and call graphs',
      linkUrl: '/docs/guides/codebase-analysis'
    }
  ]}
/>
