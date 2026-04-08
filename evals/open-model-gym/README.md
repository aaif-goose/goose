# Open Model Gym

Run agent tests across a matrix of **models × runners × scenarios**. 

It isn't hard for any agent to do ok with opus, but lets scale things in the other direction. What do we have to break things down to.

<img width="1768" height="1133" alt="image" src="https://github.com/user-attachments/assets/29915659-ee6b-4a8b-ba5e-58420b168b43" />

## Quick Start

```bash
just install   # one-time setup
just run       # run full matrix (3 reps each)
just report    # view results
```

## How It Works

The test harness runs every combination of models, runners, and scenarios defined in your matrix. Each test runs multiple times (default 3) and keeps the **worst result** — if a test fails even once, it's marked failed. This catches flaky passes.

## Configuration

Edit `config.yaml` to define your test matrix:

### Models

LLMs to test against. Supports any provider (Anthropic, OpenAI, Ollama, etc.):

```yaml
models:
  - name: opus
    provider: anthropic
    model: claude-opus-4-5-20251101

  - name: qwen3-coder
    provider: ollama
    model: qwen3-coder:64k

  - name: gpt4
    provider: openai
    model: gpt-4-turbo
```

### Runners

Agent frameworks that execute the tests. Each runner has its own binary, type, and configuration:

```yaml
runners:
  # Goose agent with extensions
  - name: goose-full
    type: goose
    bin: goose                    # path to binary (can be absolute)
    extensions: [developer, todo, skills]
    stdio:
      - node mcp-harness/dist/index.js

  # OpenCode agent
  - name: opencode
    type: opencode
    bin: opencode                 # path to binary
    stdio:
      - node mcp-harness/dist/index.js

  # Custom goose binary path
  - name: goose-dev
    type: goose
    bin: /path/to/my/goose-dev
    extensions: [developer]
```

**Supported runner types:**
- `goose` — [Goose](https://github.com/aaif-goose/goose) agent framework
- `opencode` — [OpenCode](https://opencode.ai) agent framework
- `pi` — [Pi](https://github.com/badlogic/pi-mono) coding agent

## Runner Details

Each runner has different setup requirements, MCP integration methods, and session handling.

### Goose

[Goose](https://github.com/aaif-goose/goose) is an open-source coding agent with built-in MCP support.

**Setup:** Install via `brew install goose` or from source.

**MCP Integration:** Native support. The harness writes a `config.yaml` to an isolated `.goose-root/` directory with extensions and MCP servers:

```yaml
extensions:
  developer:
    enabled: true
  mcp_harness:
    type: stdio
    enabled: true
    cmd: node
    args: [mcp-harness/dist/index.js]
```

**Session Handling:** Uses `--name <session>` for named sessions, `--resume` to continue:
- Turn 1: `goose run -i <prompt> --name <session>`
- Turn 2+: `goose run -i <prompt> --name <session> --resume`
- Single-turn: `goose run -i <prompt> --no-session`

### OpenCode

[OpenCode](https://opencode.ai) is a terminal-based coding agent.

**Setup:** Install via their website or package manager.

**MCP Integration:** Native support. The harness writes an `opencode.json` config to the workdir:

```json
{
  "mcp": {
    "harness": {
      "type": "local",
      "command": ["node", "mcp-harness/dist/index.js"],
      "enabled": true
    }
  },
  "model": "anthropic/claude-opus-4-5-20251101"
}
```

**Session Handling:** Uses `--continue` to resume the last session in the working directory:
- Turn 1: `opencode run "<prompt>"`
- Turn 2+: `opencode run --continue "<prompt>"`

⚠️ OpenCode doesn't support named sessions, so multi-turn scenarios exclude it.

### Pi

[Pi](https://github.com/badlogic/pi-mono) is a lightweight coding agent that requires an adapter for MCP support.

**Setup:**
```bash
# Install Pi
npm install -g @anthropic/pi   # or from source

# Install the MCP adapter (required for MCP tools)
pi install npm:pi-mcp-adapter
```

The `just install` recipe auto-installs pi-mcp-adapter if missing.

**MCP Integration:** Via [pi-mcp-adapter](https://github.com/nicobailon/pi-mcp-adapter). The harness dynamically writes a `.pi-mcp.json` config to the workdir:

```json
{
  "mcpServers": {
    "harness": {
      "command": "node",
      "args": ["mcp-harness/dist/index.js"],
      "lifecycle": "eager",
      "env": { "MCP_HARNESS_LOG": "<workdir>/tool-calls.log" }
    }
  },
  "settings": { "directTools": true }
}
```

Key settings:
- `directTools: true` — Registers MCP tools directly in Pi's tool list (no wrapper)
- `lifecycle: "eager"` — Connects to MCP servers at startup

**Model Configuration:** Pi requires custom models (like Ollama) to be defined in `models.json`. The harness automatically generates this config in an isolated `.pi-root/` directory and sets `PI_CODING_AGENT_DIR` to use it:

```json
{
  "providers": {
    "ollama": {
      "baseUrl": "http://localhost:11434/v1",
      "api": "openai-completions",
      "apiKey": "ollama",
      "models": [{ "id": "model-name", "name": "Model Name", ... }]
    }
  }
}
```

The harness copies `auth.json` from your real Pi config (`~/.pi/agent/`) so API keys work.

**Session Handling:** Uses `--session <path>` for file-based sessions, `--continue` to resume:
- Turn 1: `pi -p --session <path> "<prompt>"`
- Turn 2+: `pi -p --continue --session <path> "<prompt>"`
- Single-turn: `pi -p --no-session "<prompt>"`

The `-p` flag runs Pi in non-interactive "print" mode for automation

### Matrix

Define which scenarios run against which models/runners:

```yaml
matrix:
  - scenario: file-editing
    models: [opus, qwen3-coder]      # omit to run all models
    runners: [goose-full, opencode]  # omit to run all runners

  - scenario: everyday-app-automation
    # runs against ALL models and ALL runners
```

## Scenarios

Scenarios live in `suite/scenarios/` as YAML files:

```yaml
name: file-editing
description: Create and edit files
prompt: |
  1. Create joke.md containing a short joke
  2. Edit hello.rs to add a debug function

setup:
  hello.rs: |
    fn main() { println!("Hello!"); }

validate:
  - type: file_exists
    path: joke.md
  - type: file_matches
    path: hello.rs
    regex: "fn\\s+debug"
```

### Validation Rules

| Rule | Description |
|------|-------------|
| `file_exists` | File exists at path |
| `file_not_empty` | File exists and has content |
| `file_contains` | File contains literal string |
| `file_matches` | File matches regex pattern |
| `command_succeeds` | Shell command exits 0 |
| `tool_called` | MCP tool was called with matching args (regex supported) |

**Tool call validation example:**
```yaml
validate:
  - type: tool_called
    tool: slack_search_messages
    args:
      query: /quarterly.?review/    # regex pattern
  - type: tool_called
    tool: jira_create_issue
    args:
      summary: /Q1.*Review/
      description: /David Brown/
```

## MCP Harness

Mock MCP server providing simulated tools for testing agent tool-use without hitting real APIs.

```bash
cd mcp-harness && npm install && npm run build
```

**Available tools:** gdrive, sheets, salesforce, slack, calendar, gmail, jira, github

Each tool returns realistic mock data. Tool calls are logged to `tool-calls.log` in the workdir for validation.

## Commands

| Command | Description |
|---------|-------------|
| `just run` | Full test run (3 reps each, worst kept) |
| `just test` | Quick run (1 rep each) |
| `just scenario <name>` | Run specific scenario |
| `just agent <name>` | Run specific agent |
| `just report` | Open HTML results |

### CLI Flags

```bash
# Filter by scenario, model, or runner
npx tsx src/runner.ts --scenario=file-editing --model=opus --runner=goose

# Control repetition count
npx tsx src/runner.ts --run-count=5

# Don't auto-open browser
npx tsx src/runner.ts --no-open
```

## Output

- `report.html` — Live-updating HTML matrix showing pass/fail status, duration, and validation details
- `logs/` — Full agent output logs for each run
