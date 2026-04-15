---
title: JetBrains Extension
description: Use JetBrains MCP Server as a goose Extension
---

import Tabs from '@theme/Tabs';
import TabItem from '@theme/TabItem';
import YouTubeShortEmbed from '@site/src/components/YouTubeShortEmbed';
import CLIExtensionInstructions from '@site/src/components/CLIExtensionInstructions';
import { PanelLeft } from 'lucide-react';

<YouTubeShortEmbed videoUrl="https://www.youtube.com/embed/1fP5elf9qQM" />

This tutorial covers how to add the JetBrains extension to integrate with any JetBrains IDE. While goose can use the [Developer extension](/docs/mcp/developer-mcp) for developer-centric tasks, the JetBrains extension provides a more integrated and project-aware way to work with code.

## Configuration

**Important**: The configuration steps depend on your IDE version. You can find the version in `[IDE Name] > About` (macOS) or `Help > About` (Windows/Linux).

<Tabs groupId="ideVersion">
  <TabItem value="2026.1" label="2026.1 and later" default>

    Versions 2026.1 and later expose a streamable HTTP endpoint directly. No plugin install or npm required.

    :::tip Quick Install
    <Tabs groupId="interface">
      <TabItem value="ui" label="goose Desktop" default>
      Use `Add custom extension` in Settings → Extensions with the URL from `Copy HTTP Stream Config` in your IDE.
      </TabItem>
      <TabItem value="cli" label="goose CLI">
      Use `goose configure` with the URL from `Copy HTTP Stream Config` in your IDE.
      </TabItem>
    </Tabs>
    :::

    <br/>
    Configure the extension using your IDE's built-in HTTP stream endpoint:

    1. Get your IDE-specific URL:

       1. Go to `Settings > Tools > MCP Server` in your IDE
       2. Check `Enable MCP Server` and confirm the permissions dialog if shown
       3. Click `Copy HTTP Stream Config`
       4. Note the `url` value (e.g. `http://127.0.0.1:64344/stream`) — the port is auto-assigned and varies per IDE instance

    2. Add the JetBrains extension to goose using the URL from the config:

       <Tabs groupId="interface">
         <TabItem value="ui" label="goose Desktop" default>
           1. Click the <PanelLeft className="inline" size={16} /> button in the top-left to open the sidebar
           2. Click `Extensions` on the sidebar
           3. Click `Add custom extension`
           4. On the `Add custom extension` modal, enter the following:
              - **Extension Name**: JetBrains
              - **Type**: Streamable HTTP
              - **Description**: Integrate goose with any JetBrains IDE
              - **URL**: Paste the `url` from `Copy HTTP Stream Config`
           5. Click `Add Extension` to save the extension
           6. Navigate to the chat
         </TabItem>
         <TabItem value="cli" label="goose CLI">
           <CLIExtensionInstructions
             name="jetbrains"
             description="Integrate goose with any JetBrains IDE"
             type="http"
             url="http://127.0.0.1:<PORT>/stream"
             timeout={300}
             commandNote={
               <>
                 Paste the <code>url</code> from <code>Copy HTTP Stream Config</code>. The port varies per IDE instance.
               </>
             }
           />
         </TabItem>
       </Tabs>
  </TabItem>
  <TabItem value="2025.2" label="2025.2 – 2025.x">

    Versions 2025.2 through 2025.x have built-in MCP server support using stdio transport. See your IDE's documentation for more details (e.g. [MCP Server](https://www.jetbrains.com/help/idea/mcp-server.html) for IntelliJ IDEA).

    :::tip Quick Install
    <Tabs groupId="interface">
      <TabItem value="ui" label="goose Desktop" default>
      Use `Add custom extension` in Settings → Extensions with the command from `Copy Stdio Config` in your IDE.
      </TabItem>
      <TabItem value="cli" label="goose CLI">
      Use `goose configure` with the command from `Copy Stdio Config` in your IDE.
      </TabItem>
    </Tabs>
    :::

    <br/>
    Configure the extension using your IDE's built-in MCP server support:

    1. Get your IDE-specific config:

       1. Go to `Settings > Tools > MCP Server` in your IDE
       2. If needed, click `Enable MCP Server` to enable the MCP server
       3. Click `Copy Stdio Config`
       4. Click `OK` to save your changes and start the server
       5. Copy the `command`, `args`, and `env` values from the config

    2. Add the JetBrains extension to goose using the command from the config:

       <Tabs groupId="interface">
         <TabItem value="ui" label="goose Desktop" default>
           1. Click the <PanelLeft className="inline" size={16} /> button in the top-left to open the sidebar
           2. Click `Extensions` on the sidebar
           3. Click `Add custom extension`
           4. On the `Add custom extension` modal, enter the following:
              - **Extension Name**: JetBrains
              - **Type**: STDIO
              - **Description**: Integrate goose with any JetBrains IDE
              - **Command**: Combine the `command` and `args` from your IDE's Stdio config into a single command string
              - **Environment Variables**: Add `IJ_MCP_SERVER_PORT` with the port value from the `env` section of your copied Stdio config
           5. Click `Add Extension` to save the extension
           6. Navigate to the chat
         </TabItem>
         <TabItem value="cli" label="goose CLI">
           <CLIExtensionInstructions
             name="jetbrains"
             description="Integrate goose with any JetBrains IDE"
             type="stdio"
             command="YOUR_COMMAND_AND_ARGS_FROM_IDE"
             timeout={300}
             envVars={[{ key: "IJ_MCP_SERVER_PORT", value: "YOUR_PORT_FROM_IDE" }]}
             commandNote={
               <>
                 Combine <code>command</code> and <code>args</code> into a single string. The port value comes from the <code>env</code> section of your copied Stdio config.
               </>
             }
           />
         </TabItem>
       </Tabs>
  </TabItem>
</Tabs>

## Example Usage

In this example, I'm going to upgrade a Java project to the latest LTS version.

<Tabs groupId="interface">
  <TabItem value="ui" label="goose Desktop" default>
   1. Open [IntelliJ](https://www.jetbrains.com/idea/download) (JetBrains' Java and Kotlin IDE)
   2. Open a new session in goose Desktop
   :::note
   You will interact with two separate apps: the goose Desktop app and the IntelliJ IDE.
   :::

  </TabItem>
  <TabItem value="cli" label="goose CLI">

  1. Open [IntelliJ](https://www.jetbrains.com/idea/download) (JetBrains' Java and Kotlin IDE)
  2. Open a terminal within your IDE and start a new goose session:

  ```sh
  goose session
  ```

  </TabItem>
</Tabs>

:::tip
When using goose to refactor code, it's recommended to commit your code to version control _before_ making changes. This way, you have a clean snapshot to compare against, making it easy to review goose's modifications, revert if needed, and track improvements.
:::

### goose Prompt
```
using the JetBrains IDE tools, upgrade this project to use the latest Java LTS version and refactor the codebase to use newer Java features where beneficial.
```

After 2 minutes, goose was done with the upgrade! 🎉

### goose Output

```md
I'll make several improvements to modernize this codebase to use Java 21 (latest LTS) features. Here are the changes I'll implement:

1. Update the `pom.xml` to use Java 21
2. Use newer Java features

[... code changes ...]

Here's a summary of the modernization changes made:

1. `pom.xml`:
   - Set Java version to 21
   - Enabled preview features
   - Updated all dependencies to their latest stable versions

2. `AccountsOverviewPage.java`:
   - Made the class `final` since it extends the sealed `Page` class
   - Updated stream collection to use `.toList()` instead of `collect(Collectors.toList())`
   - Made fields final for better immutability

3. `Page.java`:
   - Converted to a sealed class
   - Restricted which classes can extend it using `permits`
   - Made fields protected and final where appropriate
   - Added pattern matching for exception handling
   - Implemented Optional for null safety
   - Used String Templates for error messages
   - Added functional programming concepts with Supplier
   - Enhanced error handling with specific exception types

4. `LoginComponent.java`:
   - Added a private record for credentials
   - Implemented pattern matching with record patterns
   - Made fields final for better immutability

5. `AccountTests.java`:
   - Implemented structured concurrency using `StructuredTaskScope`
   - Used parallel execution for UI and API tasks
   - Improved error handling with structured task management
   - Used pattern matching and type inference with `var`

6. `BaseTests.java`:
   - Implemented structured concurrency for setup tasks
   - Improved resource management
   - Better exception handling
   - Used modern Duration API

7. `APIUtil.java`:
   - Implemented String Templates (Java 21 preview feature)
   - Used modern text blocks

8. `Customers.java`:
   - Added sealed interface for customer types
   - Implemented pattern matching for switch expressions
   - Added pattern matching for instanceof
   - Enhanced type safety with records

Key modern Java features used:
- String Templates (Java 21 Preview)
- Pattern Matching for switch (Java 21)
- Sealed Classes and Interfaces (Java 17)
- Records (Java 16)
- Pattern Matching for instanceof (Java 16)
- Text Blocks (Java 15)
- Switch Expressions (Java 14)

The codebase is now using the latest Java LTS version (21) and incorporates many modern Java features that improve:
- Type safety
- Null safety
- Code readability
- Error handling
- Performance
```




Using the JetBrains extension provided several advantages and made it easier for goose to:
- navigate through the codebase
- track changes
- make consistent modifications across files
- verify the content of files before and after changes

