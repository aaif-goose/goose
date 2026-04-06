# VMware Tanzu Platform - CLI Testing Guide

## Prerequisites

- goose CLI built from the `feat/tanzu-ai-provider` branch
- A Tanzu AI Services endpoint and API key (single-model or multi-model plan)

## Locate the CLI Binary

**macOS:**
```bash
# If built from source:
export GOOSE_CLI=~/claude/goose-fork/target/release/goose

# Verify:
$GOOSE_CLI --version
```

**Linux:**
```bash
# If installed via .deb:
export GOOSE_CLI=/usr/bin/goose

# If built from source:
export GOOSE_CLI=~/goose-fork/target/release/goose

# Verify:
$GOOSE_CLI --version
```

## Test 1: Configure VMware Tanzu Platform Provider

```bash
goose configure
```

1. Select **Configure Providers**
2. Scroll to / search for **VMware Tanzu Platform**
3. When prompted for **TANZU_AI_ENDPOINT**, enter your endpoint URL:
   - Single-model: `https://genai-proxy.sys.example.com/tanzu-my-model-abc1234`
   - Multi-model: `https://genai-proxy.sys.example.com/tanzu-all-models-abc1234`
4. When prompted for **TANZU_AI_API_KEY**, paste the JWT token from your service key
5. Select a model from the dynamically fetched list

**Expected:** Models are fetched from the endpoint and displayed for selection.

## Test 2: Start a Session (Single-Model Plan)

```bash
export TANZU_AI_ENDPOINT="https://genai-proxy.sys.tas-tdc.kuhn-labs.com/tanzu-Qwen3-Coder-30B-A3B-vllm-v1-f3b0d18"
export TANZU_AI_API_KEY="<your-jwt-token>"

goose session
```

Type a simple prompt:
```
> What is 2 + 2?
```

**Expected:** The model responds with an answer. If streaming is enabled, tokens appear incrementally.

## Test 3: Start a Session (Multi-Model Plan)

```bash
export TANZU_AI_ENDPOINT="https://genai-proxy.sys.tas-tdc.kuhn-labs.com/tanzu-all-models-a8a9e22"
export TANZU_AI_API_KEY="<your-jwt-token>"

goose session
```

**Expected:** Session starts with whichever model was selected during `goose configure`.

## Test 4: Verify Streaming

With streaming enabled (`supports_streaming: true`), responses should appear token-by-token rather than all at once.

```
> Write a short poem about clouds
```

**Expected:** Text streams in progressively, not appearing all at once after a delay.

## Test 5: Verify Dynamic Model Fetching

```bash
goose configure
```

Select **Configure Providers** > **VMware Tanzu Platform**.

**Expected for single-model plan:** One model appears (e.g., `Qwen/Qwen3-Coder-30B-A3B-Instruct-FP8`)
**Expected for multi-model plan:** Multiple models appear (e.g., `Qwen3.5-122B`, `Qwen3-Coder-30B`, `gpt-oss-120b`)

## Test 6: Verify Error Messages

### Missing API Key
```bash
unset TANZU_AI_API_KEY
goose session
```
**Expected:** Clear error message: "Required API key TANZU_AI_API_KEY is not set."

### Missing Endpoint
```bash
unset TANZU_AI_ENDPOINT
goose session
```
**Expected:** Clear error message about TANZU_AI_ENDPOINT not being set.

### Wrong Endpoint
```bash
export TANZU_AI_ENDPOINT="https://genai-proxy.sys.example.com/nonexistent"
export TANZU_AI_API_KEY="invalid-key"
goose session
```
**Expected:** Connection or authentication error, not a crash.

## Test 7: Switch Between Plans

1. Configure with multi-model endpoint, select a model, start a session, verify it works
2. Run `goose configure` again
3. Change TANZU_AI_ENDPOINT to the single-model endpoint
4. Select the single model
5. Start a new session, verify it works

**Expected:** Both plans work without needing to restart goose.

## Quick Curl Verification

Before testing with goose, you can verify endpoints directly:

```bash
# Test models endpoint
curl -s -H "Authorization: Bearer $TANZU_AI_API_KEY" \
  "$TANZU_AI_ENDPOINT/openai/v1/models" | python3 -m json.tool

# Test chat completions
curl -s -X POST "$TANZU_AI_ENDPOINT/openai/v1/chat/completions" \
  -H "Authorization: Bearer $TANZU_AI_API_KEY" \
  -H "Content-Type: application/json" \
  -d '{"model":"Qwen/Qwen3-Coder-30B-A3B-Instruct-FP8","messages":[{"role":"user","content":"hello"}],"max_tokens":10}'

# Test streaming
curl -s -N -X POST "$TANZU_AI_ENDPOINT/openai/v1/chat/completions" \
  -H "Authorization: Bearer $TANZU_AI_API_KEY" \
  -H "Content-Type: application/json" \
  -d '{"model":"Qwen/Qwen3-Coder-30B-A3B-Instruct-FP8","messages":[{"role":"user","content":"hello"}],"max_tokens":10,"stream":true}'
```
