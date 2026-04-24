#!/bin/bash

PROVIDER_CONFIG="
openrouter -> google/gemini-2.5-pro|anthropic/claude-sonnet-4.5|qwen/qwen3-coder:exacto|z-ai/glm-4.6:exacto|nvidia/nemotron-3-nano-30b-a3b
xai -> grok-3
openai -> gpt-4o|gpt-4o-mini|gpt-3.5-turbo|gpt-5
anthropic -> claude-sonnet-4-5-20250929|claude-opus-4-5-20251101
google -> gemini-2.5-pro|gemini-2.5-flash|gemini-3-pro-preview|gemini-3-flash-preview
tetrate -> claude-sonnet-4-20250514
databricks -> databricks-claude-sonnet-4|gemini-2-5-flash|gpt-4o
azure_openai -> ${AZURE_OPENAI_DEPLOYMENT_NAME}
aws_bedrock -> us.anthropic.claude-sonnet-4-5-20250929-v1:0
gcp_vertex_ai -> gemini-2.5-pro
snowflake -> claude-sonnet-4-5
venice -> llama-3.3-70b
litellm -> gpt-4o-mini
sagemaker_tgi -> sagemaker-tgi-endpoint
github_copilot -> gpt-4.1
chatgpt_codex -> gpt-5.4
claude-code -> default
cursor-agent -> auto
ollama -> qwen3
"

# Flaky models allowed to fail without blocking PRs.
ALLOWED_FAILURES=(
  "google:gemini-2.5-flash"
  "google:gemini-3-pro-preview"
  "openrouter:nvidia/nemotron-3-nano-30b-a3b"
  "openrouter:qwen/qwen3-coder:exacto"
  "openai:gpt-3.5-turbo"
)

AGENTIC_PROVIDERS=("claude-code" "cursor-agent")

if [ -f .env ]; then
  export $(grep -v '^#' .env | xargs)
fi

build_goose() {
  if [ -z "$SKIP_BUILD" ]; then
    echo "Building goose..." >&2
    cargo build --bin goose >&2
    echo "" >&2
  else
    echo "Skipping build (SKIP_BUILD is set)..." >&2
    echo "" >&2
  fi

  echo "$(pwd)/target/debug/goose"
}

has_env() { [ -n "${!1}" ]; }
has_cmd() { command -v "$1" &>/dev/null; }
has_file() { [ -f "$1" ]; }

is_provider_available() {
  case "$1" in
    openrouter)      has_env OPENROUTER_API_KEY ;;
    xai)             has_env XAI_API_KEY ;;
    openai)          has_env OPENAI_API_KEY ;;
    anthropic)       has_env ANTHROPIC_API_KEY ;;
    google)          has_env GOOGLE_API_KEY ;;
    tetrate)         has_env TETRATE_API_KEY ;;
    databricks)      has_env DATABRICKS_HOST && has_env DATABRICKS_TOKEN ;;
    azure_openai)    has_env AZURE_OPENAI_ENDPOINT && has_env AZURE_OPENAI_DEPLOYMENT_NAME ;;
    aws_bedrock)     has_env AWS_REGION && { has_env AWS_PROFILE || has_env AWS_ACCESS_KEY_ID; } ;;
    gcp_vertex_ai)   has_env GCP_PROJECT_ID ;;
    snowflake)       has_env SNOWFLAKE_HOST && has_env SNOWFLAKE_TOKEN ;;
    venice)          has_env VENICE_API_KEY ;;
    litellm)         has_env LITELLM_API_KEY ;;
    sagemaker_tgi)   has_env SAGEMAKER_ENDPOINT_NAME && has_env AWS_REGION ;;
    github_copilot)  has_env GITHUB_COPILOT_TOKEN || has_file "$HOME/.config/goose/github_copilot_token.json" ;;
    chatgpt_codex)   has_env CHATGPT_CODEX_TOKEN || has_file "$HOME/.config/goose/chatgpt_codex/tokens.json" ;;
    ollama)          has_env OLLAMA_HOST || has_cmd ollama ;;
    claude-code)     has_cmd claude ;;
    cursor-agent)    has_cmd cursor-agent ;;
    *)               return 0 ;;
  esac
}

is_allowed_failure() {
  local key="${1}:${2}"
  for allowed in "${ALLOWED_FAILURES[@]}"; do
    [ "$allowed" = "$key" ] && return 0
  done
  return 1
}

should_skip_provider() {
  [ -z "$SKIP_PROVIDERS" ] && return 1
  IFS=',' read -ra SKIP_LIST <<< "$SKIP_PROVIDERS"
  for skip in "${SKIP_LIST[@]}"; do
    skip=$(echo "$skip" | xargs)
    [ "$skip" = "$1" ] && return 0
  done
  return 1
}

is_agentic_provider() {
  for agentic in "${AGENTIC_PROVIDERS[@]}"; do
    [ "$agentic" = "$1" ] && return 0
  done
  return 1
}

# build_test_cases [--skip-agentic]
build_test_cases() {
  local skip_agentic=false
  [ "$1" = "--skip-agentic" ] && skip_agentic=true

  local providers=()
  while IFS= read -r line; do
    [[ "$line" =~ ^#.*$ || -z "$line" ]] && continue
    local provider="${line%% -> *}"
    if is_provider_available "$provider"; then
      providers+=("$line")
      echo "✓ Including $provider"
    else
      echo "⚠️  Skipping $provider (prerequisites not met)"
    fi
  done <<< "$PROVIDER_CONFIG"
  echo ""

  TEST_CASES=()
  local job_index=0
  for provider_config in "${providers[@]}"; do
    local provider="${provider_config%% -> *}"
    local models_str="${provider_config#* -> }"

    if should_skip_provider "$provider"; then
      echo "⊘ Skipping provider: ${provider} (SKIP_PROVIDERS)"
      continue
    fi

    if [ "$skip_agentic" = true ] && is_agentic_provider "$provider"; then
      echo "⊘ Skipping agentic provider: ${provider}"
      continue
    fi

    IFS='|' read -ra models <<< "$models_str"
    for model in "${models[@]}"; do
      TEST_CASES+=("$provider|$model|$job_index")
      ((job_index++))
    done
  done
}

# run_test_cases <test_fn>
run_test_cases() {
  local test_fn="$1"

  RESULTS_DIR=$(mktemp -d)
  trap 'if [ -n "${RESULTS_DIR:-}" ]; then rm -rf -- "$RESULTS_DIR"; fi; if [ -n "${CLEANUP_DIR:-}" ]; then rm -rf -- "$CLEANUP_DIR"; fi' EXIT
  MAX_PARALLEL=${MAX_PARALLEL:-$(nproc 2>/dev/null || sysctl -n hw.ncpu 2>/dev/null || echo 8)}
  echo "Running ${#TEST_CASES[@]} tests (max $MAX_PARALLEL parallel)"
  echo ""

  local running=0
  for ((i=0; i<${#TEST_CASES[@]}; i++)); do
    IFS='|' read -r provider model idx <<< "${TEST_CASES[$i]}"

    if [ $i -eq 0 ]; then
      # First test runs sequentially to catch early failures
      "$test_fn" "$provider" "$model" "$RESULTS_DIR/result_$idx" "$RESULTS_DIR/output_$idx"
    else
      "$test_fn" "$provider" "$model" "$RESULTS_DIR/result_$idx" "$RESULTS_DIR/output_$idx" &
      ((running++))
      if [ $running -ge $MAX_PARALLEL ]; then
        wait -n 2>/dev/null || wait
        ((running--))
      fi
    fi
  done
  wait
}

report_results() {
  echo ""
  echo "=== Test Results ==="
  echo ""

  RESULTS=()
  HARD_FAILURES=()

  for job in "${TEST_CASES[@]}"; do
    IFS='|' read -r provider model idx <<< "$job"

    echo "Provider: $provider"
    echo "Model: $model"
    echo ""
    cat "$RESULTS_DIR/output_$idx"
    echo ""

    local result_line=""
    [ -f "$RESULTS_DIR/result_$idx" ] && result_line=$(cat "$RESULTS_DIR/result_$idx")
    local status="${result_line%%|*}"
    local msg="${result_line#*|}"

    if [ "$status" = "success" ]; then
      echo "✓ SUCCESS: $msg"
      RESULTS+=("✓ ${provider}: ${model}")
    else
      if is_allowed_failure "$provider" "$model"; then
        echo "⚠ FLAKY: $msg"
        RESULTS+=("⚠ ${provider}: ${model} (flaky)")
      else
        echo "✗ FAILED: $msg"
        RESULTS+=("✗ ${provider}: ${model}")
        HARD_FAILURES+=("${provider}: ${model}")
      fi
    fi
    echo "---"
  done

  echo ""
  echo "=== Test Summary ==="
  for result in "${RESULTS[@]}"; do
    echo "$result"
  done

  if [ ${#HARD_FAILURES[@]} -gt 0 ]; then
    echo ""
    echo "Hard failures (${#HARD_FAILURES[@]}):"
    for failure in "${HARD_FAILURES[@]}"; do
      echo "  - $failure"
    done
    echo ""
    echo "Some tests failed!"
    exit 1
  else
    if echo "${RESULTS[@]}" | grep -q "⚠"; then
      echo ""
      echo "All required tests passed! (some flaky tests failed but are allowed)"
    else
      echo ""
      echo "All tests passed!"
    fi
  fi
}
