---
sidebar_position: 15
title: VMware Tanzu Platform
description: Connect goose to VMware Tanzu Platform AI Services
---

import Tabs from '@theme/Tabs';
import TabItem from '@theme/TabItem';

# VMware Tanzu Platform

[VMware Tanzu Platform](https://techdocs.broadcom.com/us/en/vmware-tanzu/platform/ai-services/10-3/ai/index.html) provides enterprise-managed LLM access through AI Services. goose connects to VMware Tanzu Platform as an OpenAI-compatible provider, supporting both **single-model** and **multi-model** service plans with streaming enabled by default.

## Prerequisites

- A VMware Tanzu Platform (TAS) foundation with GenAI tile installed and configured
- Access to a CF org/space where the `genai` service is available in the marketplace
- The CF CLI (`cf`) installed and authenticated (`cf login`)
- goose v1.28.0 or later

## Step 1: Check Available Plans

First, verify the `genai` service is available in your marketplace and review the available plans:

```sh
cf marketplace -e genai
```

You will see output similar to:

```
broker: genai-service
   plan                                description                                            free or paid
   tanzu-Qwen3-Coder-30B-A3B-vllm-v1  Access to: Qwen/Qwen3-Coder-30B-A3B-Instruct-FP8.    free
   tanzu-gpt-oss-120b-vllm-v1          Access to: openai/gpt-oss-120b.                        free
   tanzu-all-models                    Access to: Qwen3.5-122B, Qwen3-Coder-30B, gpt-oss...   free
```

Each plan corresponds to a different model or set of models. **Single-model plans** give access to one model. **Multi-model plans** (e.g., `tanzu-all-models`) give access to multiple models behind a single endpoint.

## Step 2: Create a Service Instance

### Option A: Single-Model Plan

Create a service instance using a single-model plan:

```sh
cf create-service genai tanzu-Qwen3-Coder-30B-A3B-vllm-v1 my-qwen-coder --wait
```

### Option B: Multi-Model Plan

Create a service instance using the multi-model plan:

```sh
cf create-service genai tanzu-all-models my-all-models --wait
```

Verify the instance was created:

```sh
cf services
```

## Step 3: Create a Service Key

Create a service key to generate API credentials:

```sh
cf create-service-key my-qwen-coder my-goose-key --wait
```

Then retrieve the credentials:

```sh
cf service-key my-qwen-coder my-goose-key
```

### Single-Model Plan Output

For a single-model plan, the output includes model metadata at the top level:

```json
{
  "credentials": {
    "api_base": "https://genai-proxy.sys.example.com/tanzu-my-model-abc1234/openai",
    "api_key": "eyJhbGciOi...",
    "endpoint": {
      "api_base": "https://genai-proxy.sys.example.com/tanzu-my-model-abc1234",
      "api_key": "eyJhbGciOi...",
      "config_url": "https://genai-proxy.sys.example.com/tanzu-my-model-abc1234/config/v1/endpoint",
      "name": "tanzu-my-model-abc1234"
    },
    "model_capabilities": ["chat", "tools"],
    "model_name": "Qwen/Qwen3-Coder-30B-A3B-Instruct-FP8",
    "wire_format": "openai"
  }
}
```

### Multi-Model Plan Output

For a multi-model plan, the output only contains the endpoint object:

```json
{
  "credentials": {
    "endpoint": {
      "api_base": "https://genai-proxy.sys.example.com/tanzu-all-models-abc1234",
      "api_key": "eyJhbGciOi...",
      "config_url": "https://genai-proxy.sys.example.com/tanzu-all-models-abc1234/config/v1/endpoint",
      "name": "tanzu-all-models-abc1234"
    }
  }
}
```

## Step 4: Identify Your Endpoint and API Key

From the service key output, you need two values from the **`credentials.endpoint`** object:

| Value | JSON Path | Example |
|-------|-----------|---------|
| **Endpoint URL** | `credentials.endpoint.api_base` | `https://genai-proxy.sys.example.com/tanzu-my-model-abc1234` |
| **API Key** | `credentials.endpoint.api_key` | `eyJhbGciOi...` (JWT token) |

:::warning Use `credentials.endpoint.api_base`, not `credentials.api_base`
Single-model plans include a top-level `credentials.api_base` field that has an `/openai` suffix. **Do not use this value.** Always use `credentials.endpoint.api_base` (without `/openai`), because goose automatically appends the correct path.

Using the wrong value would produce a double-path URL like `.../openai/openai/v1/chat/completions`.
:::

## Step 5: Configure goose

<Tabs groupId="interface">
  <TabItem value="ui" label="goose Desktop" default>

  1. Open goose Desktop
  2. Click the sidebar button, then **Settings** > **Models** > **Configure providers**
  3. Find **VMware Tanzu Platform** in the provider list and click **Configure**
  4. Enter your values:
     - **TANZU_AI_ENDPOINT**: Paste the `credentials.endpoint.api_base` URL
     - **TANZU_AI_API_KEY**: Paste the `credentials.endpoint.api_key` JWT token
  5. Click **Submit**
  6. Select a model from the dynamically fetched list

  </TabItem>
  <TabItem value="cli" label="goose CLI">

  ### Option 1: Using `goose configure`

  ```sh
  goose configure
  ```

  1. Select **Configure Providers**
  2. Choose **VMware Tanzu Platform** from the list
  3. Enter your `TANZU_AI_ENDPOINT` when prompted
  4. Enter your `TANZU_AI_API_KEY` when prompted
  5. Select a model from the fetched list

  ### Option 2: Using environment variables

  Set the following environment variables before launching goose:

  ```sh
  export TANZU_AI_ENDPOINT="https://genai-proxy.sys.example.com/tanzu-my-model-abc1234"
  export TANZU_AI_API_KEY="eyJhbGciOi..."
  ```

  Then start goose:

  ```sh
  goose session
  ```

  :::tip
  Add these exports to your shell profile (`~/.bashrc`, `~/.zshrc`, etc.) to persist them across sessions.
  :::

  </TabItem>
</Tabs>

## Step 6: Select a Model

goose dynamically fetches available models from your Tanzu endpoint. After configuring the provider:

- **Single-model plan**: The one available model will be listed (e.g., `Qwen/Qwen3-Coder-30B-A3B-Instruct-FP8`)
- **Multi-model plan**: All models on the plan will be listed, and you can switch between them

To change models later, use **Settings** > **Models** > **Switch models** in Desktop, or run `goose configure` in the CLI.

:::note
Embedding-only models (e.g., `nomic-ai/nomic-embed-text-v2-moe`) will appear in the model list but cannot be used as a chat model.
:::

## Troubleshooting

### "Could not contact provider" / 401 Unauthorized on models endpoint

This means the API key is not being sent correctly. Common causes:

1. **Environment variables not set**: If using goose Desktop, env vars from your shell may not be inherited. Use the Settings UI to configure the provider instead.
2. **Wrong `api_base`**: Make sure you used `credentials.endpoint.api_base` (without `/openai`), not `credentials.api_base`.
3. **Expired API key**: Tanzu API keys are JWT tokens that may expire. Generate a new service key with `cf create-service-key`.

### Verify your endpoint manually

You can test connectivity with curl:

```sh
# Test model discovery
curl -H "Authorization: Bearer $TANZU_AI_API_KEY" \
  "$TANZU_AI_ENDPOINT/openai/v1/models"

# Test chat completions
curl -X POST "$TANZU_AI_ENDPOINT/openai/v1/chat/completions" \
  -H "Authorization: Bearer $TANZU_AI_API_KEY" \
  -H "Content-Type: application/json" \
  -d '{"model":"YOUR_MODEL_NAME","messages":[{"role":"user","content":"hello"}]}'
```

### Streaming

Streaming is enabled by default. If your endpoint does not support streaming, you can disable it by unchecking the **Streaming** checkbox in the provider configuration UI, or by setting the `TANZU_AI_STREAMING` environment variable to `false`.

### Model not found

If the model you selected returns an error, verify available models on your plan:

```sh
curl -H "Authorization: Bearer $TANZU_AI_API_KEY" \
  "$TANZU_AI_ENDPOINT/openai/v1/models"
```

Ensure the model name matches exactly (including the prefix, e.g., `Qwen/Qwen3-Coder-30B-A3B-Instruct-FP8`).

### Cleaning up

To remove a service instance and its keys:

```sh
cf delete-service-key my-qwen-coder my-goose-key -f
cf delete-service my-qwen-coder -f
```
