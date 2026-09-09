---
sidebar_position: 1
title: Session Management
sidebar_label: Session Management
---
import Tabs from '@theme/Tabs';
import TabItem from '@theme/TabItem';
import { AppWindow, PanelLeft, FolderDot, Paperclip, Copy, Edit2, Trash2, Download, Upload, ChefHat, History } from 'lucide-react';


A session is a single, continuous interaction between you and goose, providing a space to ask questions and prompt action. This guide covers how to manage the session lifecycle.

## Start Session 

:::info First-time setup
In your first session, goose prompts you to [set up an LLM (Large Language Model) provider](/docs/getting-started/installation#set-llm-provider).
:::

<Tabs groupId="interface">
    <TabItem value="ui" label="goose Desktop" default>
        When you open goose, you'll see the session interface ready for use. Just type&mdash;[or speak](/docs/guides/sessions/in-session-actions#voice-dictation "Learn how to enable voice dictation")&mdash;your questions, requests, or instructions directly into the input field, and goose will immediately get to work.

        To start a new chat session:

        1. Click the <PanelLeft className="inline" size={16} /> button in the top-left to open the sidebar
        2. Click `Home` or `Chat` in the sidebar
        3. Send your first prompt from the chat box

        goose Desktop supports working with multiple active chat sessions in the same window. Your new sessions are added to the `Chat` section of the sidebar, so you can quickly switch between your 10 most recent sessions. Just click a session to [continue working](#resume-session) in that session.
        
        To change your working directory, click the <FolderDot className="inline" size={16} /> directory switcher at the bottom of the app.

        :::info Starting Sessions in a New Window
        To start a session in a new window, click the <AppWindow className="inline" size={16} /> button in the top-left. After you send your first prompt, the new session is added to the `Chat` section of the sidebar. 

        On macOS, you can also use the goose dock icon to quickly start sessions:
            - **Drag and drop** a folder onto the goose icon to open a new session in that directory
            - **Right-click** the goose icon and select `New Window` to open a new session in your most recent directory
        :::
        
        #### Keyboard Shortcuts
        
        You can also use keyboard shortcuts to start a new session or manage goose windows.
        
        | Action | macOS | Windows/Linux |
        |--------|-------|---------------|
        | New Session with [Quick Launcher](#quick-launcher) | `Cmd+Option+Shift+G` | `Ctrl+Alt+Shift+G` |
        | New Session in Current Directory | `Cmd+N` | `Ctrl+N` |
        | New Session in Current Directory (Same Window) | `Cmd+T` | `Ctrl+T` |
        | New Session in Different Directory | `Cmd+O` | `Ctrl+O` |
        | Toggle Sidebar | `Cmd+B` | `Ctrl+B` |
        | Open Settings | `Cmd+,` | `Ctrl+,` |
        | Focus goose Window | `Cmd+Option+G` | `Ctrl+Alt+G` |
        | Keep goose Window Always on Top | `Cmd+Shift+T` | `Ctrl+Shift+T` |
        
        #### Customize Keyboard Shortcuts
        
        You can customize these keyboard shortcuts in the **Settings** menu:
        1. Click the <PanelLeft className="inline" size={16} /> button in the top-left to open the sidebar
        2. Click `Settings`
        3. Click the `Keyboard` tab
        
        Changes to global shortcuts (Focus Window, Quick Launcher) take effect immediately. Changes to application shortcuts (New Chat, Settings, etc.) require restarting goose.
        
        #### Quick Launcher
        Start a new session by typing your prompt into a popup:
        1. Press `Cmd+Option+Shift+G` (macOS) or `Ctrl+Alt+Shift+G` (Windows/Linux) to open the popup
        2. Type your prompt and press `Enter`

        The session opens to your most recently opened directory in a new goose window.

    </TabItem>
    <TabItem value="cli" label="goose CLI">
        From your terminal, navigate to the directory from which you'd like to start, and run the [session](/docs/guides/goose-cli-commands#session-options) command:
        ```sh
        goose session 
        ```

    </TabItem>
</Tabs>

## Name Session
<Tabs groupId="interface">
    <TabItem value="ui" label="goose Desktop" default>
        A session's display name is auto-generated based on the context of your initial prompt. Session names help you identify goose sessions so you can switch between active sessions or [resume a session](#resume-session).

        You can edit session names after they're created:

        1. Click <History className="inline" size={16} /> `Session History` in the sidebar
        2. Hover over the session you'd like to rename
        3. Click the <Edit2 className="inline" size={16} /> button that appears on the session card
        4. In the "Edit Session Description" modal that opens:
           - Enter your new session description (up to 200 characters)
           - Press `Enter` to save or `Escape` to cancel
           - Or click the `Save` or `Cancel` buttons
        5. A success toast notification will confirm the change

        Session names appear in the `Chat` section of the sidebar, the `Window` menu, and the Dock (macOS) or taskbar (Windows) menu.

    </TabItem>
    <TabItem value="cli" label="goose CLI">
        goose sessions are given an auto-generated name based on the context of your initial prompt.

        If you'd like to provide a specific session name, you can do so when starting a session. For example, to name your session "react-migration", run:

        ```sh
        goose session --name react-migration
        ```

        If you want to confirm the session name, run:

        ```sh
        goose session list -l 1
        ```

        Example result:
        
        ```text
        Available sessions:
        20260213_9 - react-migration - 2026-02-13 16:20:37 UTC
        ```

        In the output above, `20260213_9` is the session ID. Session IDs use the format `YYYYMMDD_<COUNT>`. Many [goose CLI commands](/docs/guides/goose-cli-commands) let you identify a session by name (`--name` / `-n`) as an alternative to `--session-id`.
    </TabItem>
</Tabs>

:::tip Disable AI-generated Session Naming
Use [`GOOSE_DISABLE_SESSION_NAMING`](/docs/guides/environment-variables#session-management) to keep the default name instead of calling a model to generate one (useful in CI/headless workflows). The default name in goose Desktop is "New Chat" and the default name in goose CLI is "CLI Session".
:::

## Exit Session
Note that sessions are automatically saved when you exit.
<Tabs groupId="interface">
    <TabItem value="ui" label="goose Desktop" default>
    To exit a session, simply close the application.
    </TabItem>    
    <TabItem value="cli" label="goose CLI">
        To exit a session, type `exit`. Alternatively, you exit the session by holding down `Ctrl+C`.

        Your session will be stored in goose's [local SQLite database](/docs/guides/logs#session-records).
    </TabItem>
</Tabs>

## Search Sessions

Search allows you to find specific content within sessions or find specific sessions.

<Tabs groupId="interface">
  <TabItem value="ui" label="goose Desktop" default>

    You can use keyboard shortcuts and search bar buttons to search sessions in goose Desktop.

    | Action | macOS | Windows/Linux |
    |--------|-------|---------------|
    | Open Search | `Cmd+F`  | `Ctrl+F`  |
    | Next Match | `Cmd+G` or `↓` | `Ctrl+G` or `↓` |
    | Previous Match | `Shift+Cmd+G` or `↑` | `Shift+Ctrl+G` or `↑` |
    | Use Selection for Find | `Cmd+E` | n/a |
    | Toggle Case-Sensitivity | `Aa` | `Aa` |
    | Close Search | `Esc` or `X` | `Esc` or `X` |

    :::tip Customize Search Shortcuts
    You can [customize](#keyboard-shortcuts) the Find, Find Next, and Find Previous keyboard shortcuts in **Settings** → **Keyboard** tab.
    :::

    :::info No Regex or operator support
    Using regular expressions or search operators in search text isn't supported.
    :::

    The following scenarios are supported:

    #### Search Within Current Session
    
    To find specific content within your current session:

    1. Use `Cmd+F` to open the search bar
    2. Enter your search term
    3. Use shortcuts and search bar buttons to navigate the results

    #### Search Across Sessions
    
    To search conversation history across all your sessions:

    1. Click <History className="inline" size={16} /> `Session History` in the sidebar
    2. Use `Cmd+F` (or `Ctrl+F`) to open the search bar
    3. Enter your search term
    4. Use keyboard shortcuts and search bar buttons to navigate the results (`Cmd+E` not supported)

    This searches the content of messages in your conversations. The search is limited to the 10 most recent matching messages across sessions. If your search term appears in many messages, the search will only return a subset of sessions.

    :::tip Ask goose directly
    You can also use the built-in [Chat Recall extension](/docs/mcp/chatrecall-mcp) to ask goose to search your conversation history:
    - "Find my earlier conversation about React hooks from last week"
    - "Show me sessions where I worked on database migrations"
    :::

  </TabItem>
  <TabItem value="cli" label="goose CLI">

    #### Search Within Current Session

    Search functionality is provided by your terminal interface. Use the appropriate shortcut for your environment:

    | Terminal | Operating System | Shortcut |
    |----------|-----------------|-----------|
    | iTerm2 | macOS | `Cmd+F` |
    | Terminal.app | macOS | `Cmd+F` |
    | Windows Terminal | Windows | `Ctrl+F` |
    | Linux Terminal | Linux | `Ctrl+F` |

    To find specific content within your current session:

    1. Use the shortcut to open the search bar
    2. Enter your search term
    3. Use shortcuts and search bar buttons to navigate the results

    :::info
    Your specific terminal emulator may use a different keyboard shortcut. Check your terminal's documentation or settings for the search command.
    :::

    #### Search Across All Session Content
    
    To search conversation content across all your sessions, start a goose session and ask directly:

    - "Find my earlier conversation about React hooks from last week"
    - "Show me sessions where I worked on database migrations"

    goose will search your session history and show relevant conversations with context from matching sessions.
    
    :::info
    This functionality requires the built-in [Chatrecall extension](/docs/mcp/chatrecall-mcp) to be enabled.
    :::

    #### Search Session Data Directly
    
    The [`session list`](/docs/guides/goose-cli-commands#session-list-options) subcommand with supported options can be useful for some search operations.

    You can also query the SQLite database directly:

    ```bash
    # Search session descriptions
    sqlite3 ~/.local/share/goose/sessions/sessions.db \
      "SELECT id, description FROM sessions WHERE description LIKE '%your search term%';"

    # Search by working directory
    sqlite3 ~/.local/share/goose/sessions/sessions.db \
      "SELECT id, description, working_dir FROM sessions WHERE working_dir LIKE '%project-name%';"

    # List recent sessions
    sqlite3 ~/.local/share/goose/sessions/sessions.db \
      "SELECT id, description, created_at FROM sessions ORDER BY created_at DESC LIMIT 10;"
    ```

    :::info Session Storage Migration
    Starting with version 1.10.0, goose uses a SQLite database (`sessions.db`) instead of individual `.jsonl` files. Legacy `.jsonl` files remain on disk but are no longer managed by goose.
    :::

  </TabItem>
</Tabs>

## Resume Session

<Tabs groupId="interface">
    <TabItem value="ui" label="goose Desktop" default>
    
    You can switch between active sessions in the sidebar or resume any session from your history.
    
    #### Switch Between Active Sessions

    goose Desktop allows you to switch between multiple chat sessions in the same window. You can start a task in one session, switch to another to do some work, and return to the first session when the task is completed. 

    Your most recent sessions (up to 10) are available in the sidebar for quick access:

    1. Click the <PanelLeft className="inline" size={16} /> button in the top-left to open the sidebar
    2. In the `Chat` section, click any session to switch to it
    
    Visual indicators help you track session status:
    - **Blue spinning icon** - Session is actively processing a request
    - **Green dot** - Session completed a task while you were viewing a different session
    - **Red dot** - Session encountered an error

    In addition, sessions that are started from a recipe display the <ChefHat className="inline" size={16} /> icon.

    :::tip
    You can [rename sessions](#name-session) to make it easier to identify specific sessions.
    :::

    #### Resume from Session History

    To find and resume sessions beyond your 10 most recent:

    1. Click <History className="inline" size={16} /> `Session History` in the sidebar
    2. Find the session you'd like to resume. goose provides [search features](#search-sessions) to help you find the session.
    3. Choose how to resume:
       - Click `Resume` to continue in the current window
       - Click `New Window` to open in a new window

    </TabItem>
    <TabItem value="cli" label="goose CLI">
        To resume your latest session, you can run the following command:

        ```
         goose session -r
        ```

        To resume a specific session, run the following command: 

        ```
        goose session -r --name <name>
        ```
        For example, to resume the session named `react-migration`, you would run:

        ```
        goose session -r --name react-migration
        ```
    </TabItem>
</Tabs>

Sessions created in goose Desktop can be resumed in the CLI and vice versa. All sessions are stored in the [same database](/docs/guides/logs#session-records).

:::tip Create New Sessions for New Tasks
While you can resume sessions, we recommend creating new sessions for new tasks to reduce the chance of [doom spiraling](/docs/troubleshooting/known-issues#stuck-in-a-loop-or-unresponsive).
:::

## Duplicate Sessions

Create a complete copy of any session to reuse configurations, experiment with variations, or preserve important work.

<Tabs groupId="interface">
    <TabItem value="ui" label="goose Desktop" default>
        
        Duplicate a session from the session list:

        1. Click <History className="inline" size={16} /> `Session History` in the sidebar
        2. Find the session you want to duplicate
        3. Hover over the session card to reveal the action buttons
        4. Click the <Copy className="inline" size={16} /> button that appears in the top-right corner

        The duplicated session includes:
        - Complete conversation history
        - All session metadata and settings
        - Provider and model configuration
        - Extension data and configurations
        - Recipe information (if applicable)

        The new session is named the same as the original and appears at the top of your session list.

        :::tip Duplicate vs Fork Session
        - **Duplicate** (Copy button in session list): Creates a complete copy of the entire session. Use this to preserve a working session or reuse its configuration.
        - **[Fork Session](/docs/guides/sessions/in-session-actions#fork-session)** (Edit button on a message): Creates a new session with conversation history up to a specific edited message. Use this when editing a message to explore different approaches from that point.
        :::

    </TabItem>
    <TabItem value="cli" label="goose CLI">
        
        Create a new session by copying all messages from a previous session using the `--fork` flag along with `--resume`.

        ```bash
        # Fork the most recent session
        goose session --resume --fork

        # Fork a specific session by name
        goose session --resume --fork --name my-project

        # Fork a specific session by ID
        goose session --resume --fork --session-id 20251108_3

        # Fork and show message history
        goose session --resume --fork --history
        ```

        The forked session includes:
        - Complete conversation history from the original session
        - All session metadata and settings
        - Provider and model configuration
        - Extension data and configurations
        - Recipe information (if applicable)
    </TabItem>
</Tabs>

## Delete Sessions

<Tabs groupId="interface">
    <TabItem value="ui" label="goose Desktop" default>
        You can delete sessions directly from the Desktop app:

        1. Click <History className="inline" size={16} /> `Session History` in the sidebar
        2. Find the session you want to delete
        3. Hover over the session card to reveal the action buttons
        4. Click the <Trash2 className="inline" size={16} /> button that appears
        5. Confirm the deletion in the modal that appears

        :::warning Permanent deletion
        Deleting a session from goose Desktop will also delete it from the CLI. This action cannot be undone.
        :::

        The session will be immediately removed from your session history and the underlying session record will be deleted from local storage.
    </TabItem>
    <TabItem value="cli" label="goose CLI">
        You can remove sessions using CLI commands. For detailed instructions on session removal, see the [CLI Commands documentation](/docs/guides/goose-cli-commands#session-remove-options).
    </TabItem>
</Tabs>

## Import Sessions

<Tabs groupId="interface">
    <TabItem value="ui" label="goose Desktop" default>
        Import complete sessions from JSON files to restore, share, or migrate sessions between goose instances. Importing creates a new session with a new ID rather than overwriting existing sessions.

        1. Click <History className="inline" size={16} /> `Session History` in the sidebar
        2. Click the <Upload className="inline" size={16} /> `Import Session` button in the top-right corner
        3. Select a `.json` session file that was previously exported from goose
        4. The session will be imported with a new session ID
        5. A success notification will confirm the import

    </TabItem>
    <TabItem value="cli" label="goose CLI">
        Session import is currently only available through the Desktop app.
    </TabItem>
</Tabs>

## Export Sessions

<Tabs groupId="interface">
    <TabItem value="ui" label="goose Desktop" default>
        Export complete sessions as JSON files for backup, sharing, migration, or archival. Exported files preserve all session data including conversation history, metadata, and settings.

        1. Click <History className="inline" size={16} /> `Session History` in the sidebar
        2. Find the session you want to export
        3. Hover over the session card to reveal the action buttons
        4. Click the <Download className="inline" size={16} /> button that appears
        5. The session will be downloaded as a `.json` file named after the session description

    </TabItem>
    <TabItem value="cli" label="goose CLI">
        Export sessions for backup, sharing, migration, or documentation purposes. You can export as JSON files to preserve complete session data including conversation history, metadata, and settings, or as Markdown files to get a formatted, readable version of the conversation.

        From your terminal, run the [`session export`](/docs/guides/goose-cli-commands#session-export-options) subcommand:
        
        ```bash
        goose session export
        ```

    </TabItem>
</Tabs>
