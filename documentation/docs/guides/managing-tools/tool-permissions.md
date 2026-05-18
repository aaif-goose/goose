---
title: Managing Tool Permissions
sidebar_position: 1
sidebar_label: Tool Permissions
---

import Tabs from '@theme/Tabs';
import TabItem from '@theme/TabItem';
import { PanelLeft, Tornado, Settings } from 'lucide-react';

Tool permissions provide fine-grained control over how goose uses different tools within extensions. This guide will help you understand and configure these permissions effectively.

## Understanding Tools and Extensions

Before diving into permissions, let's clarify the key components:

- **Extensions** are packages that add functionality to goose (like Developer, Google Drive, etc.)
- **Tools** are specific functions within each extension that goose can use

For example, the Developer extension includes multiple tools like:

- Text editor tool for file editing
- Shell tool for running commands
- Screen capture tool for taking screenshots
:::warning Performance Optimization
goose performs best with fewer than 25 total tools enabled across all extensions. Consider enabling only the extensions you need for your current task.
:::

## Permission Levels

Tool permissions work alongside [goose permission modes](/docs/guides/managing-tools/goose-permissions). The mode sets the default behavior, while tool permissions let you override the behavior of specific tools.

Each tool can be set to one of three permission levels:

| Permission Level | Description | Best For | Examples |
|-----------------|-------------|-----------|----------|
| **Always Allow** | Tool runs without requiring approval | Safe, read-only operations | • File reading<br></br>• Directory listing<br></br>• Information retrieval |
| **Ask Before** | Requires confirmation | State-changing operations | • File writing/editing<br></br>• System commands<br></br>• Resource creation |
| **Never Allow** | Tool cannot be used | Sensitive operations | • Credential access<br></br>• System-critical files<br></br>• Resource deletion |

## Configuring Tool Permissions

<Tabs groupId="interface">
  <TabItem value="ui" label="goose Desktop" default>
    You can configure fine-grained tool permissions for enabled extensions when using `Manual` or `Smart` approval mode. These rules can be accessed from the mode toggle or `Settings` page.

    <Tabs>
      <TabItem value="toggle" label="Mode Toggle" default>
        1. Click the <Tornado className="inline" size={16} /> button at the bottom of the app
        2. Click the <Settings className="inline" size={16} /> button next to your selected `Manual` or `Smart` mode
        3. Click the extension whose tools you want to configure
        4. Use the dropdown next to each tool to set its permission level
        5. Click `Save Changes`
      </TabItem>
      <TabItem value="settings" label="Settings Page" default>
        1. Click the <PanelLeft className="inline" size={16} /> button in the top-left to open the sidebar
        2. Click the `Settings` button on the sidebar
        3. Click `Chat`
        4. Under `Mode`, click the <Settings className="inline" size={16} /> button next to your selected `Manual` or `Smart` mode
        5. Click the extension whose tools you want to configure
        6. Use the dropdown next to each tool to set its permission level
        7. Click `Save Changes`
      </TabItem>
    </Tabs>
  
  </TabItem>
  <TabItem value="cli" label="goose CLI">

    1. Run the configure command:
    ```sh
    goose configure
    ```

    2. Select `goose settings` from the menu
    ```sh
    ┌ goose-configure
    │
    ◆ What would you like to configure?
    | ○ Configure Providers
    | ○ Add Extension
    | ○ Toggle Extensions
    | ○ Remove Extension
    // highlight-start
    | ● goose settings
    // highlight-end
    └
    ```

    3. Choose `Tool Permission`
    ```sh
    ┌   goose-configure
    │
    ◇  What would you like to configure?
    │  goose settings
    │
    ◆  What setting would you like to configure?
    │  ○ goose mode
    // highlight-start
    │  ● Tool Permission
    // highlight-end
    |  ○ Tool Output
    └
    ```

    4. Select an extension and configure permissions for its tools:
    ```sh
    ┌   goose-configure
    │
    ◇  What setting would you like to configure?
    │  Tool Permission 
    │
    ◇  Choose an extension to configure tools
    │  developer 
    │
    ◇  Choose a tool to update permission
    │  developer__image_processor 
    │
    ◆  Set permission level for tool developer__image_processor, current permission level: Not Set
    │  ○ Always Allow 
     // highlight-start
    │  ● Ask Before (Prompt before executing this tool)
    // highlight-end
    │  ○ Never Allow 
    └
    ```
  </TabItem>
</Tabs>

## Benefits of Permission Management

:::tip
Review and update your tool permissions as your tasks change. You can modify permissions at any time during a session.
:::

There are several reasons to configure tool permissions:

1. **Performance Optimization**
   - Keep total enabled tools under 25 for best performance
   - Disable tools you don't need for your current task
   - Reduce context window usage and improve response quality
   - Prevent tool decision paralysis

2. **Security Control**
   - Restrict access to sensitive operations
   - Prevent accidental file modifications
   - Control system resource usage

3. **Task Focus**
   - Enable only tools needed for current task
   - Help goose make better tool choices
   - Reduce noise in responses

## Example Permission Configuration

### Task-Based Configuration

Configure permissions based on your current task:

```
Development Task:
✓ File reading → Always Allow
✓ Code editing → Ask Before
✓ Test running → Always Allow
✗ System commands → Ask Before

Documentation Task:
✓ File reading → Always Allow
✓ Markdown editing → Always Allow
✗ Code editing → Never Allow
✗ System commands → Never Allow
```