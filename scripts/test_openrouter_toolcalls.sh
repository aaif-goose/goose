#!/usr/bin/env bash
set -euo pipefail

show_usage() {
  echo "Usage: $0 [options]"
  echo ""
  echo "Options:"
  echo "  -n, --count NUM          Number of OpenRouter models to test (default: 10)"
  echo "  -m, --models MODELS      Comma-separated model list. Skips fetching models."
  echo "  -o, --output-dir DIR     Directory for logs (default: ./openrouter-toolcall-results)"
  echo "  -s, --sort SORT          OpenRouter model sort (default: top-weekly)"
  echo "      --run-timeout SEC    Kill a model run after this many seconds (default: 180, 0 disables)"
  echo "  -h, --help               Show this help message"
  echo ""
  echo "Environment:"
  echo "  OPENROUTER_API_KEY       Required by goose's OpenRouter provider"
  echo "  OPENROUTER_HOST          Optional OpenRouter host (default: https://openrouter.ai)"
  echo "  GOOSE_BIN                Optional goose binary path"
  echo "  SKIP_BUILD               Skip cargo build when set"
  echo ""
  echo "Examples:"
  echo "  $0 --count 5"
  echo "  $0 --models 'anthropic/claude-sonnet-4.5,google/gemini-2.5-flash'"
}

MODEL_COUNT=10
MODEL_LIST=""
OUTPUT_DIR="./openrouter-toolcall-results"
MODEL_SORT="top-weekly"
RUN_TIMEOUT=180

while [[ $# -gt 0 ]]; do
  case "$1" in
    -n|--count)
      MODEL_COUNT="$2"
      shift 2
      ;;
    -m|--models)
      MODEL_LIST="$2"
      shift 2
      ;;
    -o|--output-dir)
      OUTPUT_DIR="$2"
      shift 2
      ;;
    -s|--sort)
      MODEL_SORT="$2"
      shift 2
      ;;
    --run-timeout)
      RUN_TIMEOUT="$2"
      shift 2
      ;;
    -h|--help)
      show_usage
      exit 0
      ;;
    *)
      echo "Error: Unknown option: $1"
      show_usage
      exit 1
      ;;
  esac
done

if [[ -z "${OPENROUTER_API_KEY:-}" ]]; then
  echo "Error: OPENROUTER_API_KEY must be set"
  exit 1
fi

if ! command -v jq >/dev/null 2>&1; then
  echo "Error: jq is required"
  exit 1
fi

if ! command -v curl >/dev/null 2>&1; then
  echo "Error: curl is required"
  exit 1
fi

if ! command -v uv >/dev/null 2>&1; then
  echo "Error: uv is required to run the temporary FastMCP server"
  exit 1
fi

if ! [[ "$RUN_TIMEOUT" =~ ^[0-9]+$ ]]; then
  echo "Error: --run-timeout must be a non-negative integer"
  exit 1
fi

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

if [[ -z "${SKIP_BUILD:-}" && -z "${GOOSE_BIN:-}" ]]; then
  echo "Building goose..."
  (cd "$REPO_ROOT" && cargo build --bin goose)
  echo ""
fi

GOOSE_BIN="${GOOSE_BIN:-$REPO_ROOT/target/debug/goose}"
if [[ ! -x "$GOOSE_BIN" ]]; then
  echo "Error: goose binary not found or not executable: $GOOSE_BIN"
  exit 1
fi

mkdir -p "$OUTPUT_DIR"

MODELS=()
if [[ -n "$MODEL_LIST" ]]; then
  IFS=',' read -ra MODELS <<< "$MODEL_LIST"
else
  OPENROUTER_HOST="${OPENROUTER_HOST:-https://openrouter.ai}"
  MODELS_URL="$OPENROUTER_HOST/api/v1/models?supported_parameters=tools&sort=$MODEL_SORT"

  echo "Fetching OpenRouter models: $MODELS_URL"
  MODELS_JSON=$(curl --fail --silent --show-error --max-time 30 "$MODELS_URL")
  while IFS= read -r model; do
    MODELS+=("$model")
  done < <(jq -r --argjson limit "$MODEL_COUNT" '.data[:$limit][] | .id' <<< "$MODELS_JSON")
fi

if [[ ${#MODELS[@]} -eq 0 ]]; then
  echo "Error: no models found"
  exit 1
fi

TESTDIR=$(mktemp -d)
trap 'rm -rf "$TESTDIR"' EXIT

cat > "$TESTDIR/weather.py" << 'EOF'
from typing import Annotated
from fastmcp import FastMCP

mcp = FastMCP("weather")

@mcp.tool
def get_weather(
    location: Annotated[str, "City or place to check"],
) -> Annotated[str, "Weather report"]:
    """Get the current weather for a location."""
    return f"GOOSE_TOOL_CALL_OK: The weather in {location} is 68 F and clear."
EOF

cat > "$TESTDIR/recipe.yaml" << 'EOF'
title: OpenRouter Tool Call Test
description: Test a model can call a simple MCP tool through goose
prompt: Use the get_weather tool to check the weather in San Francisco. Do not answer from memory.
extensions:
  - name: weather
    cmd: uv
    args:
      - run
      - --with
      - fastmcp==2.14.4
      - fastmcp
      - run
      - weather.py
    type: stdio
EOF

RESULTS=()
OVERALL_SUCCESS=true

summarize_error() {
  awk '
    /Ran into this error:/ {
      sub(/^.*Ran into this error: /, "")
      print
      found = 1
      exit
    }
    /Request failed:/ {
      sub(/^.*Request failed: /, "Request failed: ")
      print
      found = 1
      exit
    }
    /Provider error:/ {
      sub(/^.*Provider error: /, "Provider error: ")
      print
      found = 1
      exit
    }
    END { if (!found) exit 1 }
  ' "$1"
}

run_model() {
  local model="$1"

  if [[ "$RUN_TIMEOUT" -eq 0 ]]; then
    GOOSE_MODE=auto GOOSE_PROVIDER=openrouter GOOSE_MODEL="$model" \
      "$GOOSE_BIN" run --no-profile --max-turns 4 --recipe recipe.yaml
    return $?
  fi

  perl -e '
    my $timeout = shift;
    my $pid = fork();
    die "fork failed: $!" unless defined $pid;
    if ($pid == 0) {
      exec @ARGV;
      die "exec failed: $!";
    }
    local $SIG{ALRM} = sub {
      kill "TERM", $pid;
      sleep 2;
      kill "KILL", $pid;
      exit 124;
    };
    alarm $timeout;
    waitpid($pid, 0);
    my $status = $?;
    alarm 0;
    exit($status & 127 ? 128 + ($status & 127) : $status >> 8);
  ' \
    "$RUN_TIMEOUT" \
    env GOOSE_MODE=auto GOOSE_PROVIDER=openrouter GOOSE_MODEL="$model" \
    "$GOOSE_BIN" run --no-profile --max-turns 4 --recipe recipe.yaml
}

echo "Testing ${#MODELS[@]} OpenRouter model(s)"
echo ""

for model in "${MODELS[@]}"; do
  safe_model=$(echo "$model" | tr '/:' '__' | tr -cd '[:alnum:]_.-')
  log_file="$OUTPUT_DIR/$safe_model.log"

  echo "=========================================================="
  echo "Model: $model"
  echo "Log:   $log_file"
  echo "=========================================================="

  if (cd "$TESTDIR" && run_model "$model" 2>&1) | tee "$log_file"; then
    if error_summary=$(summarize_error "$log_file"); then
      echo "✗ Goose reported an error for $model"
      echo "  $error_summary"
      RESULTS+=("✗ $model - $error_summary")
      OVERALL_SUCCESS=false
    elif grep -qE "(get_weather \| weather)|(▸.*get_weather.*weather)" "$log_file" && \
      grep -Fq "GOOSE_TOOL_CALL_OK:" "$log_file"; then
      echo "✓ Tool call passed for $model"
      RESULTS+=("✓ $model")
    elif grep -qE "(get_weather \| weather)|(▸.*get_weather.*weather)" "$log_file"; then
      echo "✗ Tool call did not return a successful result for $model"
      RESULTS+=("✗ $model - no successful get_weather result found")
      OVERALL_SUCCESS=false
    else
      echo "✗ Tool call not found for $model"
      RESULTS+=("✗ $model - no get_weather call found")
      OVERALL_SUCCESS=false
    fi
  else
    run_status=${PIPESTATUS[0]}
    echo "✗ Goose run failed for $model"
    if [[ "$run_status" -eq 124 ]]; then
      RESULTS+=("✗ $model - run timed out")
    elif error_summary=$(summarize_error "$log_file"); then
      echo "  $error_summary"
      RESULTS+=("✗ $model - $error_summary")
    else
      RESULTS+=("✗ $model - goose run failed")
    fi
    OVERALL_SUCCESS=false
  fi

  echo ""
done

echo "=== Test Summary ==="
for result in "${RESULTS[@]}"; do
  echo "$result"
done

if [[ "$OVERALL_SUCCESS" = false ]]; then
  echo ""
  echo "Some OpenRouter tool call tests failed."
  exit 1
fi

echo ""
echo "All OpenRouter tool call tests passed."
