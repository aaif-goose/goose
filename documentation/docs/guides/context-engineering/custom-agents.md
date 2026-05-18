---
title: Custom Agents
sidebar_position: 3
sidebar_label: Custom Agents
---

Custom agents are reusable goose configurations for specific roles, behaviors, or areas of expertise. Each agent packages a name, system prompt, and optional metadata such as provider, model, and avatar settings so you can quickly switch goose into a specialized mode such as code reviewer, documentation writer, test planner, or release assistant.

Use custom agents when you want the same role or behavior across multiple sessions without retyping the same instructions.

## Create an Agent in goose Desktop

In goose Desktop, open **Agents** from the sidebar and select **New Agent**.

An agent includes:

| Field | Description |
|---|---|
| **Display Name** | The name shown in the Agents page and chat selector. |
| **Avatar** | Optional visual identifier for the agent. |
| **System Prompt** | The instructions goose uses when this agent is active. |
| **Provider** | Optional provider preference for this agent. |
| **Model** | Optional model preference for this agent. |

The system prompt is the most important field. It should describe the agent's role, expectations, constraints, and output style.

```text title="Example system prompt"
You are a senior code reviewer. Review changes for correctness, maintainability, security, and test coverage. Be direct, prioritize issues by severity, and suggest concrete fixes.
```

After saving, the agent appears in the Agents page and can be selected from the chat agent picker.

## Manage Existing Agents

From the Agents page, you can:

- **View** an agent's prompt and model settings
- **Edit** writable agents
- **Duplicate** an agent to create a variation
- **Delete** agents you no longer need
- **Export** an agent as a portable JSON file
- **Import** an agent from a JSON file

Read-only agents, such as built-in agents or agents loaded from read-only roots, can be viewed but cannot be edited directly. Duplicate a read-only agent if you want to customize it.

## Agent Files

Agents are stored as Markdown files with YAML frontmatter. You can create or edit these files directly if you prefer managing agents in your editor.

Global agents are available across goose sessions:

```text
~/.agents/agents/
```

Project agents are available when goose is working in that project:

```text
<project>/.agents/agents/
```

A minimal agent file looks like this:

```markdown title="~/.agents/agents/code-reviewer.md"
---
name: Code Reviewer
description: Reviews code for correctness, maintainability, and risk
model: claude-sonnet-4-20250514
---

You are a senior code reviewer. Review changes for correctness, maintainability, security, and test coverage. Be direct, prioritize issues by severity, and suggest concrete fixes.
```

The frontmatter supports:

| Field | Required | Description |
|---|---:|---|
| `name` | Yes | Display name for the agent. |
| `description` | No | Short summary shown when listing agents. |
| `model` | No | Preferred model for the agent. |
| `provider` | No | Preferred provider for the agent. |
| `avatar` | No | Avatar URL or data URL used by goose Desktop. |

The Markdown body is the agent's system prompt.

:::note Compatibility paths
goose also discovers agents from `.goose/agents/`, `.claude/agents/`, `~/.goose/agents/`, `~/.claude/agents/`, and goose's platform-specific config agents directory. New agents should use `.agents/agents/` or `~/.agents/agents/`.
:::

## Import and Export Agents

Exporting an agent creates a `.agent.json` file that can be shared or backed up. Importing an agent adds it to your global agents list in goose Desktop.

Exported `.agent.json` files include the agent name, description, and system prompt. Provider, model, and avatar settings may not be included in portable exports depending on how the agent was created.

An exported agent uses this shape:

```json title="code-reviewer.agent.json"
{
  "version": 1,
  "type": "agent",
  "name": "Code Reviewer",
  "description": "Reviews code for correctness, maintainability, and risk",
  "content": "You are a senior code reviewer..."
}
```

Imported files can use `.agent.json`, `.persona.json`, or `.json` extensions.

## Use an Agent in Chat

After creating or importing an agent, open a chat and select it from the agent picker. goose applies that agent's system prompt to future messages in the current session. If the agent has a provider preference and that provider is available, goose may switch to that provider when you select the agent.

Selecting an agent does not start a separate isolated session. Existing conversation context remains available, subject to the session's context window and any compaction. The agent's prompt applies going forward; it does not rewrite earlier messages.

Choose the default goose agent when you want normal behavior, and switch to a custom agent when you want a specialized role or repeatable workflow.

## When to Use Agents, Skills, or Recipes

| Use | Best fit |
|---|---|
| Change goose's overall role, tone, or system prompt for a session | Custom agent |
| Teach goose a reusable workflow or domain-specific procedure it can load on demand | [Skill](/docs/guides/context-engineering/using-skills) |
| Package a repeatable task with prompts, settings, extensions, and parameters | [Recipe](/docs/guides/recipes) |
| Delegate work to another isolated goose instance | [Subagent](/docs/guides/subagents) |

Agents define who goose should be for a session. Skills and recipes define what goose should know or do.

### Can custom agents be scheduled to run?

Not directly. Custom agents are reusable roles, not scheduled jobs. To run something on a schedule, create a [recipe](/docs/guides/recipes) and schedule the recipe. If you want the scheduled job to behave like a custom agent, put the agent's instructions into the recipe or have the recipe delegate to that agent.

### Do custom agents have workflows?

No. A custom agent defines who goose should be for a session: its role, behavior, prompt, and optional provider/model metadata. It does not define a step-by-step workflow. Use a recipe when you need repeatable steps, parameters, extension configuration, or scheduled execution.

### Can custom agents use skills?

Yes. A custom agent can use skills that are available in the session. Skills are still discovered and loaded through goose's normal skill behavior, so the agent can use them when your request matches a skill or when you explicitly ask for one.

### Can custom agents run recipes?

A custom agent does not contain or automatically run a recipe. You can start goose with a recipe, ask goose to load or delegate a recipe when the relevant tools are available, or create a recipe that uses custom-agent instructions as part of its workflow.

### Can custom agents use MCP servers?

Yes. Custom agents can use the MCP servers and extensions that are enabled in the current session. The agent file itself does not define a separate MCP server list. If you need a reusable setup with a specific extension set, use a recipe.

### Can custom agents call subagents?

Yes, when delegation tools are available in the session. A selected custom agent can ask goose to delegate work to a subagent just like the default goose agent can. Delegated subagents run in isolated sessions and do not automatically inherit the full parent conversation.

### Can one custom agent call another custom agent?

Yes, through delegation. For example, one custom agent can delegate a task to another custom agent by name if that agent is discoverable. This is useful for one-off collaboration between specialized agents.

For repeatable chains, use a recipe that explicitly defines the sequence, such as delegating first to a reviewer agent, then to a docs agent, then combining the results.