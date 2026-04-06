#!/bin/bash
# Provider smoke tests - code execution mode (JS batching)

LIB_DIR="$(cd "$(dirname "$0")" && pwd)"
source "$LIB_DIR/test_providers_lib.sh"

echo "Mode: code_execution (JS batching)"
echo ""

# --- Setup ---

GOOSE_BIN=$(build_goose)
BUILTINS="memory,code_execution"

# --- Test case ---

run_test() {
  local provider="$1" model="$2" result_file="$3" output_file="$4"
  local testdir=$(mktemp -d)

  local prompt="Store a memory with category 'test' and data 'hello world', then retrieve all memories from category 'test'."

  # Run goose
  (
    export GOOSE_PROVIDER="$provider"
    export GOOSE_MODEL="$model"
    cd "$testdir" && "$GOOSE_BIN" run --text "$prompt" --with-builtin "$BUILTINS" 2>&1
  ) > "$output_file" 2>&1

  # Matches: "execute_typescript | code_execution", "get_function_details | code_execution",
  #           "tool call | execute", "tool calls | execute" (old format)
  #           "▸ execute N tool call" (new format with tool_graph)
  #           "▸ execute_typescript" (plain tool name in output)
  if grep -qE "(execute_typescript \| code_execution)|(get_function_details \| code_execution)|(tool calls? \| execute)|(▸.*execute.*tool call)|(▸ execute_typescript)" "$output_file"; then
    echo "success|code_execution tool called" > "$result_file"
  else
    echo "failure|no code_execution tool calls found" > "$result_file"
  fi

  rm -rf "$testdir"
}

build_test_cases --skip-agentic
run_test_cases run_test
report_results
