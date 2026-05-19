---
sidebar_position: 2
title: In-Session Actions
sidebar_label: In-Session Actions
---
import Tabs from '@theme/Tabs';
import TabItem from '@theme/TabItem';
import { PanelLeft, Paperclip, Edit2, Send, GripVertical, X, ChevronUp, ChevronDown, FolderDot, Puzzle, Bot, Tornado } from 'lucide-react';

goose provides features you can use to manage conversations and share information during sessions.

## Edit Message

Edit your previously sent messages to refine conversations, correct course, or try different approaches.

**Example Message Flow:**

Your original conversation has five messages. After editing message 3, all message and response context from messages 4 and 5 is deleted.

```
┌─────┐   ┌─────┐   ┌─────┐   ┌─────┐   ┌─────┐
│  1  │ → │  2  │ → │  3  │ → │  4  │ → │  5  │
└─────┘   └─────┘   └─────┘   └─────┘   └─────┘
           
                   Edit here
                       ↓
┌─────┐   ┌─────┐   ┌─────┐    continue from here in
│  1  │ → │  2  │ → │  3  │ →  current session, or
└─────┘   └─────┘   └─────┘    copy to new session
```

You choose to continue working in the current session or create a fork in a new session:

- [Edit in Place](#edit-in-place) to update the current session and delete all message and response context after the edited message. This lets you start your conversation over from a given point.
- [Fork Session](#fork-session) to create a new session with all conversation history prior to and including the edited message. This lets you explore different approaches while preserving your original conversation.

### Edit in Place

Edit in Place gives you complete control over the conversation history by overwriting all context that follows the edited message. Your change can be as simple as fixing a path in your last message or completely starting over from a given point. 

Editing in place is useful when:

- You realize a prompt you sent was unclear or incomplete
- goose misunderstood your intent and went in the wrong direction

<Tabs groupId="interface">
    <TabItem value="ui" label="goose Desktop" default>

        1. Hover over any of your previous messages
        2. Click the <Edit2 className="inline" size={16} /> `Edit` button that appears
        3. Make your changes in the inline editor
        4. Click `Edit in Place` to save your changes and reprompt goose

        goose removes all conversation history after the edited message and responds contextually from that point.

        :::warning Deleted Context
        With Edit in Place, subsequent conversation history is permanently deleted from the session and removed from goose's context. Use this option only if you don't need goose to remember the context that follows the edited message.
        :::

    </TabItem>
    <TabItem value="cli" label="goose CLI">
        Message editing options are not available in the goose CLI.
    </TabItem>
</Tabs>

### Fork Session

Fork Session creates a new session with your edited message while preserving the original conversation. You can experiment with variations and compare results while keeping the original session as a reference point.

Forking sessions is useful to:
- Compare different approaches to the same problem side-by-side
- Test how different prompts affect goose's responses

<Tabs groupId="interface">
    <TabItem value="ui" label="goose Desktop" default>
        1. Hover over any of your previous messages
        2. Click the <Edit2 className="inline" size={16} /> `Edit` button that appears
        3. Make your changes in the inline editor
        4. Click `Fork Session` to save your changes and start a new session (or use `Cmd+Enter` (macOS) or `Ctrl+Enter` (Windows/Linux))

        goose creates a new session with conversation history up to and including your edited message. The new session is named "(edited)" and the original session remains unchanged.

        :::tip Fork vs Duplicate
        - **Fork Session** (Edit button on a message): Creates a new session with history up to a specific edited message. Use this to explore different approaches from a particular point in the conversation.
        - **[Duplicate Session](/docs/guides/sessions/session-management#duplicate-sessions)** (Copy button in session list): Creates a complete copy of the entire session. Use this to preserve a working session or reuse its configuration.
        :::
</TabItem>
    <TabItem value="cli" label="goose CLI">
    Message editing is not available in the goose CLI, but you can [duplicate entire sessions](/docs/guides/sessions/session-management#duplicate-sessions) using the `--fork` flag.
    </TabItem>
</Tabs>

### Editing Scenario Tips

- **Iterative Prompt Refinement**: Start with a basic prompt, then edit and refine based on goose's response. This often works better than trying to craft the perfect prompt from the start.
- **When to Edit vs. Interrupt**: Editing earlier messages when a conversation has gone off track can be more effective than trying to correct course using new messages or [interruptions](#interrupt-task). By editing messages, you rewrite history. With interruptions, you only affect the conversation from the current message onwards.
- **Preserving Progress**: Use Fork Session when you've made good progress but want to try a different approach. This way you can always return to the original if the new direction doesn't work out.

## Queue Messages

Queue messages while goose is processing a task to manage your workflow. This is useful when:

- You want to prepare next steps while goose is working
- You have a sequence of related tasks to complete
- You're using [voice dictation](#voice-dictation) and need to capture thoughts quickly

:::tip
goose may perform better when complex tasks are split into subtasks, a technique called [*prompt chaining*](https://www.promptingguide.ai/techniques/prompt_chaining). This structured approach can both improve accuracy and give you more control over the process.
:::

<Tabs groupId="interface">
    <TabItem value="ui" label="goose Desktop" default>
      Add a message to the queue:
      1. While goose is processing a response, type your next message
      2. Press `Enter` to add it to the queue (or interrupts if using [interruption keywords](#interrupt-task))
      
      Queued messages appear as numbered cards showing the queue order. The first message in the queue is automatically sent when goose finishes each response.
      
      :::info Related Features
      - In general, pressing `Enter` while goose is processing a task queues the message, but clicking `Send` sends the task immediately and [interrupts the task](#interrupt-task)
      - When you type common interrupt keywords like "stop", "wait", or "hold on" in a queued message, goose pauses until you enter or send the next message and then continues processing the queue
      :::

      #### Queue Management Controls
    
      Queued messages run automatically in order as goose finishes each task, but you can manage the queue:
      - **Edit a message**: Click the message text to reveal the edit controls, then type your change and click `Save`
      - **Reorder messages**: Hover over the message card to reveal the <GripVertical className="inline" size={16} /> button, then grab it and drag the message up or down
      - **Send a message**: Click the <Send className="inline" size={16} /> button to send a message immediately and interrupt the current task
      - **Delete a message**: Click the <X className="inline" size={16} /> button to delete the message
      - **Clear the queue**: Click `Clear All` on the **Message Queue** card
      - **Collapse or expand the queue**: Click the <ChevronUp className="inline" size={16} /> or <ChevronDown className="inline" size={16} /> button on the **Message Queue** card

      #### Example Message Flow

      **Without queuing:**

      You send: "Can you refactor our authentication code to support OAuth 2.0 and add proper error handling? Also include unit tests for the OAuth flow, update the API documentation to reflect these changes, and create a migration script to help existing users transition to the new system."

      This approach might lead to overwhelming responses where important details get missed or tasks are handled superficially. Even sending a single prompt with clear sequential steps doesn't allow goose to focus on each task individually or build context progressively.

      **With queuing:**

      1. You send: "Refactor the authentication code to support OAuth 2.0"
      2. While goose is working, you queue the following messages:
         - "And add proper error handling"
         - "Add unit tests for the OAuth flow"
         - "Update the API documentation" 
         - "Create migration script for existing users"
      
      Each task builds on the previous one.

    </TabItem>
    <TabItem value="cli" label="goose CLI">
        Message queuing is not available in the goose CLI.
    </TabItem>
</Tabs>

## Interrupt Task

Interrupt goose while it's processing a task to take control of the conversation. This is useful when:

- goose is heading in the wrong direction
- You realize you need to add important context  
- You want to switch to a completely different task

<Tabs groupId="interface">
    <TabItem value="ui" label="goose Desktop" default>
        
        There are two ways to interrupt a task:

        #### Send interruption keyword
        1. Type a prompt that includes common interruption keywords like `stop`, `wait`, `hold on`, `actually`, or `instead`. Using keywords alone or at the beginning of sentences works best for reliable detection.
        2. Click `Send`
        
        goose stops processing the current task and asks for more information.
        
        #### Provide immediate redirection
        1. Type a prompt with more context and clarification or that changes direction. For example:
           - "I forgot to mention this is for a mobile app"
           - "Let's focus on React instead of TypeScript"
        2. Click `Send`
        
        goose stops processing the current task and pivots to the new request context.
        
        :::info Related features
        - Clicking `Send` while goose is processing a task interrupts the task but pressing `Enter` [queues the message](#queue-messages)
        - Typing a stop or pause keyword in a queued message also stops goose from processing the current task
        - You can also [edit a sent message](#edit-message) to provide more context and clarification or change direction during a session
        :::

        <details>
          <summary>Interruption Keywords List</summary>

          **High-priority keywords** (interrupt in any context):
          ```
          stop, halt, cease, quit, end, abort, cancel, wait, hold, pause, hold on, wait up, hold up
          ```

          **Medium-priority keywords** (interrupt only as exact matches or at beginning of sentences):
          ```
          no, nope, nah, wrong, incorrect, not right, actually, instead, rather, better idea, change of plans, nevermind, never mind, forget it, ignore that, disregard
          ```

          **Detection Rules**:
          - **Exact match** (100% confidence): Word/phrase matches exactly and always interrupts
          - **Beginning of sentence** (very high confidence): Word/phrase starts your message and always interrupts  
          - **Short messages only** (high confidence): In messages ≤20 characters, only high-priority keywords interrupt
          - **Case insensitive**: All detection is case-insensitive

          **Examples**:
          - ✅ "stop" interrupts (exact match)
          - ✅ "Wait, I meant something else" interrupts (beginning of sentence)
          - ✅ "no" interrupts (short message, high-priority)
          - ❌ "actually" in short message doesn't interrupt (medium-priority in short message)
          - ✅ "Actually, let's try React instead" interrupts (beginning of sentence)

        </details>

    </TabItem>
    <TabItem value="cli" label="goose CLI">
        1. Press `Ctrl+C` to interrupt the current task
        2. Type your prompt that provides more context or changes direction  
        3. Press `Enter`

        goose responds contextually to your new request.
        
    </TabItem>
</Tabs>

## Voice Dictation
Speak to goose directly instead of typing your prompts.

<Tabs groupId="interface">
    <TabItem value="ui" label="goose Desktop" default>
    To enable voice dictation:
        1. Click the <PanelLeft className="inline" size={16} /> button in the top-left to open the sidebar
        2. Click `Settings` in the sidebar
        3. Click the `Chat` tab
        4. For `Voice Dictation Provider`, select your provider from the dropdown:
           - Local — On-device transcription using a local whisper model, no API key needed. On first use, you'll be prompted to download the model.
           - [Elevenlabs](https://elevenlabs.io/)
           - [Groq](https://groq.com/)
           - [OpenAI](https://platform.openai.com/api-keys)
        5. If prompted, enter your API key for the provider you chose

    To use voice dictation:
        1. Click your chat session under `Chat` in the sidebar
        2. Click the microphone button on the right of the chat box and begin speaking
        3. To send your message, do one of the following:
           - Say "submit" to send the message and continue recording your next one. To stop recording, click the microphone button.
           - Click the microphone button to stop recording, then click `Send` or press `Enter`. This option allows you to edit the message before sending.
        
        The first time you use voice dictation, goose will request access to your microphone. While recording, you'll see `Listening` and `Transcribing` status indicators. goose transcribes your speech during natural pauses and adds the text to the chat box. 

        **If you don't see the microphone**, check the [models you have configured](/docs/getting-started/providers.md). For example, using OpenAI as the voice dictation provider requires that you have an OpenAI model configured in goose, even if using another LLM provider for chat.

       #### Important Notes
        * You can record up to 50MB of audio
        * ElevenLabs, Groq, and OpenAI send audio recordings to their servers for processing. The local provider processes audio entirely on your device—no data leaves your computer.
        * Voice input is appended to any existing text in the text input field
        * Recordings are not stored locally after transcription
        * To disable voice dictation, select `Disabled` from the provider dropdown

  </TabItem>
    <TabItem value="cli" label="goose CLI">
        Voice dictation is not available in the goose CLI. 
    </TabItem>
</Tabs>

## Spellcheck

Spellcheck is enabled by default in the goose Desktop chat input field.

<Tabs groupId="interface">
    <TabItem value="ui" label="goose Desktop" default>
    To disable or re-enable spellcheck:
        1. Click the <PanelLeft className="inline" size={16} /> button in the top-left to open the sidebar
        2. Click `Settings` in the sidebar
        3. Click `Chat`
        4. Scroll down to `Enable Spellcheck` and toggle it on or off
        5. Restart goose for the change to take effect
        
    </TabItem>
    <TabItem value="cli" label="goose CLI">
        Spellcheck is not available in the goose CLI.
    </TabItem>
</Tabs>

## Share Files in Session

Provide goose with context from your codebase, documents, and other files to get more relevant and accurate assistance.

<Tabs groupId="interface">
    <TabItem value="ui" label="goose Desktop" default>
        Share files with goose in several ways:

        1. **Drag and Drop**: Simply drag files from your computer's file explorer/finder and drop them anywhere in the chat window. The file paths will be automatically added to your message.

        2. **File Browser**: Click the <Paperclip className="inline" size={16} /> button at the bottom of the app to open your system's file browser and select files

        3. **Manual Path**: Type or paste the file path directly into the chat input

        4. **Quick File Search**: Use the [`@` shortcut key](/docs/guides/file-management#quick-file-search-in-goose-desktop) to quickly find and include files
    </TabItem>
    <TabItem value="cli" label="goose CLI">
        You can reference files by their paths directly in your messages. Since you're already in a terminal, you can use standard shell commands to help with file paths:

        ```bash
        # Reference a specific file
        What does this code do? ./src/main.rs

        # Use tab completion
        Can you explain the function in ./src/lib<tab>

        # Use shell expansion
        Review these test files: ./tests/*.rs
        ```
    </TabItem>
</Tabs>

## Mid-Session Changes

You can change some settings during a session and they will take effect immediately, rather than requiring you to start a new session. This gives you more control over context and capabilities while you're interacting with goose.

<Tabs groupId="interface">
  <TabItem value="ui" label="goose Desktop" default>

  Use the toolbar at the bottom of the app to change supported settings mid-session:

  | Setting | Toolbar Item | Persistence* |
  |---------|--------------|-------------|
  | **Working Directory** | <FolderDot className="inline" size={16} /> directory switcher | New sessions (after restart) |
  | [**Enabled Extensions**](/docs/getting-started/using-extensions#change-extensions-mid-session) | <Puzzle className="inline" size={16} /> icon | Current session only |
  | [**Model**](/docs/getting-started/providers#configure-provider-and-model) | <Bot className="inline" size={16} /> model switcher | New sessions |
  | [**goose Mode**](/docs/guides/managing-tools/goose-permissions#configuring-goose-mode) | <Tornado className="inline" size={16} /> mode switcher | New sessions |

  </TabItem>
  <TabItem value="cli" label="goose CLI">

  Use the slash commands to change supported settings mid-session:

  | Setting | Slash Command | Persistence* |
  |---------|--------------|-------------|
  | [**Enabled Extensions**](/docs/getting-started/using-extensions#change-extensions-mid-session) | `/extension` or `/builtin` | Current session only |
  | [**goose Mode**](/docs/guides/managing-tools/goose-permissions#configuring-goose-mode) | `/mode [options]` | New sessions |

  :::info
  The CLI supports [additional slash commands](/docs/guides/goose-cli-commands#slash-commands) but doesn't support mid-session changes to the working directory or model.
  :::

  </TabItem>
</Tabs>

*Persistence indicates whether changes apply to your current session only or also carry over to new sessions
