---
sidebar_position: 31
title: Tool Shim
sidebar_label: Tool Shim
---

import Tabs from '@theme/Tabs';
import TabItem from '@theme/TabItem';

:::warning Experimental Feature
The tool shim is an experimental feature. Configuration options and behavior may change in future releases.
:::

Some language models don't natively support tool/function calling, or intermittently output tool calls as plaintext instead of structured API responses. The tool shim detects these text-based tool call formats and converts them into proper tool calls that goose can execute.

## When to enable

Enable the tool shim when:

- Tools stop working mid-session — the model calls a tool but goose doesn't execute it
- The model outputs plaintext like `functions.shell:0 <|tool_call_argument_begin|> {...}` instead of using the tool API
- You're using a local model (Ollama, llama.cpp) that doesn't have native tool calling support
- Your OpenAI-compatible provider routes to models that mix reasoning tags (`<think>`) with tool calls, causing parsing failures

Most locally-hosted models and some cloud models that weren't fine-tuned for structured tool calling will need the shim.

## How it works

The shim intercepts model responses and converts any text-based tool call formats into structured tool calls that goose can execute. It requires a separate **interpreter model** — by default, goose uses Ollama for this. The interpreter model is independent of whichever provider you use for your main conversation.

## Configuration

### Enable the shim

```bash
export GOOSE_TOOLSHIM=true
```

### Ollama backend (default)

Ollama must be installed and running. The default interpreter model is `mistral-nemo`.

```bash
# Pull the default interpreter model
ollama pull mistral-nemo

# Optional: use a different interpreter model
export GOOSE_TOOLSHIM_OLLAMA_MODEL=llama3.2
```

### Local backend (llama.cpp / built-in inference)

If you're running goose with the built-in local inference backend, you can use it as the interpreter instead of a separate Ollama instance. A model name is required — set either `GOOSE_TOOLSHIM_MODEL` or the `LOCAL_LLM_MODEL` config key, otherwise goose will error on startup:

```bash
export GOOSE_TOOLSHIM_BACKEND=local
export GOOSE_TOOLSHIM_MODEL=my-model-name
```

Valid values for `GOOSE_TOOLSHIM_BACKEND`: `ollama` (default), `local`, `llama.cpp`.

## Usage examples

<Tabs>
  <TabItem value="ollama-primary" label="Ollama as primary provider" default>

  ```bash
  GOOSE_TOOLSHIM=true goose session
  ```

  Uses `mistral-nemo` as the interpreter. Override with `GOOSE_TOOLSHIM_OLLAMA_MODEL` if needed.

  </TabItem>
  <TabItem value="custom-provider" label="Custom OpenAI-compatible provider">

  ```bash
  GOOSE_TOOLSHIM=true \
  GOOSE_TOOLSHIM_OLLAMA_MODEL=llama3.2 \
  goose session
  ```

  Your primary provider can be anything (Bedrock, a custom router, etc.). The shim uses Ollama locally as the interpreter regardless of which provider you're talking to.

  </TabItem>
  <TabItem value="local-backend" label="Built-in local inference">

  ```bash
  GOOSE_TOOLSHIM=true \
  GOOSE_TOOLSHIM_BACKEND=local \
  GOOSE_TOOLSHIM_MODEL=my-model-name \
  goose session
  ```

  Uses goose's built-in llama.cpp backend as the interpreter. `GOOSE_TOOLSHIM_MODEL` (or `LOCAL_LLM_MODEL` in config) is required — startup fails if neither is set.

  </TabItem>
</Tabs>

## Environment variable reference

| Variable | Description | Default |
|----------|-------------|---------|
| `GOOSE_TOOLSHIM` | Enable the tool shim (`true` or `1`) | `false` |
| `GOOSE_TOOLSHIM_BACKEND` | Interpreter backend: `ollama`, `local`, or `llama.cpp` | `ollama` |
| `GOOSE_TOOLSHIM_OLLAMA_MODEL` | Ollama model used as the interpreter | `mistral-nemo` |
| `GOOSE_TOOLSHIM_MODEL` | Model name for the local interpreter backend (required if using `local` backend and `LOCAL_LLM_MODEL` config is not set) | — |

## Troubleshooting

**Tools suddenly stop working in the middle of a session**

The model may have switched from native tool calls to a text-based format. Enable `GOOSE_TOOLSHIM=true` and restart.

**The shim is enabled but tools still don't execute**

Check that your interpreter backend is reachable:
- Ollama: run `ollama list` to confirm it's running and the interpreter model is pulled.
- Local: confirm local inference is configured and a model is set.

**Interpreter calls are slow**

Switch to a smaller, faster Ollama model:
```bash
export GOOSE_TOOLSHIM_OLLAMA_MODEL=qwen2.5:3b
```

**Model outputs reasoning before tool calls (`<think>` tags)**

Some reasoning models mix thinking tags with tool calls, causing parsing failures. The shim handles this automatically — enable it and the reasoning content is stripped from the final message.
