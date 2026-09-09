#!/bin/bash
set -e

if [ -z "$SKIP_BUILD" ]; then
  echo "Building goose..."
  cargo build --bin goose
  echo ""
else
  echo "Skipping build (SKIP_BUILD is set)..."
  echo ""
fi

SCRIPT_DIR=$(pwd)
GOOSE_BIN="$SCRIPT_DIR/target/debug/goose"

TEST_PROVIDER=${GOOSE_PROVIDER:-anthropic}
TEST_MODEL=${GOOSE_MODEL:-claude-haiku-4-5-20251001}

RESULTS=()

TESTDIR=$(mktemp -d)

cat > "$TESTDIR/test_mcp.py" << 'EOF'
from typing import Annotated
from fastmcp import FastMCP

mcp = FastMCP("test_server")

@mcp.tool
def add(
    a: Annotated[float, "First number"],
    b: Annotated[float, "Second number"],
) -> Annotated[float, "Sum of the two numbers"]:
    """Add two numbers."""
    return a + b
EOF

cat > "$TESTDIR/recipe.yaml" << 'EOF'
title: FastMCP Test
description: Test that FastMCP servers with stderr banners work
prompt: Use the add tool to calculate 42 + 58
extensions:
  - name: test_mcp
    cmd: uv
    args:
      - run
      - --with
      - fastmcp
      - fastmcp
      - run
      - test_mcp.py
    type: stdio
EOF

TMPFILE=$(mktemp)
(cd "$TESTDIR" && GOOSE_PROVIDER="$TEST_PROVIDER" GOOSE_MODEL="$TEST_MODEL" \
    "$GOOSE_BIN" run --recipe recipe.yaml 2>&1) | tee "$TMPFILE"

if grep -qE "(add \| test_mcp)|(▸.*add.*test_mcp)" "$TMPFILE" && grep -q "100" "$TMPFILE"; then
    echo "✓ FastMCP stderr test passed"
    RESULTS+=("✓ FastMCP stderr")
else
    echo "✗ FastMCP stderr test failed"
    RESULTS+=("✗ FastMCP stderr")
fi

rm "$TMPFILE"
rm -rf "$TESTDIR"
echo ""

echo "=== Test Summary ==="
for result in "${RESULTS[@]}"; do
  echo "$result"
done

if echo "${RESULTS[@]}" | grep -q "✗"; then
  echo ""
  echo "Some tests failed!"
  exit 1
else
  echo ""
  echo "All tests passed!"
fi
