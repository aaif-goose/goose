#!/bin/bash
set -e

if [ -f .env ]; then
  export $(grep -v '^#' .env | xargs)
fi

if [ -z "$SKIP_BUILD" ]; then
  echo "Building goose..."
  cargo build --bin goose
  echo ""
else
  echo "Skipping build (SKIP_BUILD is set)..."
  echo ""
fi

SCRIPT_DIR=$(pwd)

# Add goose binary to PATH so subagents can find it when spawning
export PATH="$SCRIPT_DIR/target/debug:$PATH"

# Set default provider and model if not already set
# Use fast model for CI to speed up tests
export GOOSE_PROVIDER="${GOOSE_PROVIDER:-anthropic}"
export GOOSE_MODEL="${GOOSE_MODEL:-claude-haiku-5-5}"

echo "Using provider: $GOOSE_PROVIDER"
echo "Using model: $GOOSE_MODEL"
echo ""

TESTDIR=$(mktemp -d)
echo "Created test directory: $TESTDIR"

cp -r "$SCRIPT_DIR/scripts/test-subrecipes-examples/"* "$TESTDIR/"
echo "Copied test recipes from scripts/test-subrecipes-examples"

echo ""
echo "=== Testing Subrecipe Workflow ==="
echo "Recipe: $TESTDIR/project_analyzer.yaml"
echo ""

# Create sample code files for analysis
echo "Creating sample code files for testing..."
cat > "$TESTDIR/sample.rs" << 'EOF'
// TODO: Add error handling
fn calculate(x: i32, y: i32) -> i32 {
    x + y
}

#[test]
fn test_calculate() {
    assert_eq!(calculate(2, 2), 4);
}
EOF

cat > "$TESTDIR/sample.py" << 'EOF'
# FIXME: Optimize this function
def process_data(items):
    """Process a list of items"""
    return [item * 2 for item in items]

def test_process_data():
    assert process_data([1, 2, 3]) == [2, 4, 6]
EOF

cat > "$TESTDIR/README.md" << 'EOF'
# Sample Project
This is a test project for analyzing code patterns.
## TODO
- Add more tests
EOF
echo ""

RESULTS=()

check_recipe_output() {
  local tmpfile=$1
  local mode=$2

  # Check for delegate tool invocation (old: "─── delegate |", new: "▸ delegate")
  if grep -qE "(─── delegate)|(▸.*delegate)" "$tmpfile"; then
    echo "✓ SUCCESS: Delegate tool invoked"
    RESULTS+=("✓ Delegate tool invocation ($mode)")
  else
    echo "✗ FAILED: No evidence of delegate tool invocation"
    RESULTS+=("✗ Delegate tool invocation ($mode)")
  fi

  # Check that both subrecipes were called (shown as "source: <name>" in delegate output)
  if grep -q "source:.*file_stats\|source.*file_stats" "$tmpfile" && grep -q "source:.*code_patterns\|source.*code_patterns" "$tmpfile"; then
    echo "✓ SUCCESS: Both subrecipes (file_stats, code_patterns) found in output"
    RESULTS+=("✓ Both subrecipes present ($mode)")
  else
    echo "✗ FAILED: Not all subrecipes found in output"
    RESULTS+=("✗ Subrecipe names ($mode)")
  fi
}

echo "Running recipe with parallel subrecipes..."
TMPFILE=$(mktemp)
if (cd "$TESTDIR" && "$SCRIPT_DIR/target/debug/goose" run --recipe project_analyzer_parallel.yaml --no-session 2>&1) | tee "$TMPFILE"; then
  echo "✓ SUCCESS: Recipe completed successfully"
  RESULTS+=("✓ Recipe exit code")
  check_recipe_output "$TMPFILE" "parallel"
else
  echo "✗ FAILED: Recipe execution failed"
  RESULTS+=("✗ Recipe exit code")
fi
rm "$TMPFILE"
echo ""

rm -rf "$TESTDIR"

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
