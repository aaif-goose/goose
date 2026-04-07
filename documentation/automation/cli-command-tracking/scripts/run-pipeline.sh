#!/bin/bash
# End-to-end pipeline for CLI command tracking
# Usage: ./run-pipeline.sh [old_version] [new_version]
# Example: ./run-pipeline.sh v1.17.0 v1.19.0
#
# Version detection:
# - If old_version not provided: uses the second-most-recent release tag
# - If new_version not provided: uses the most recent release tag (or RELEASE_TAG env var)
# - HEAD is only used when explicitly passed for testing unreleased changes

set -e

GOOSE_REPO=${GOOSE_REPO:-"$HOME/Development/goose"}

# Function to get release tags using gh CLI
get_latest_release() {
    if command -v gh &> /dev/null; then
        gh release list --repo aaif-goose/goose --limit 1 --json tagName --jq '.[0].tagName' 2>/dev/null
    else
        # Fallback: get latest version tag from git
        cd "$GOOSE_REPO" && git tag --sort=-v:refname | grep -E '^v[0-9]+\.[0-9]+\.[0-9]+$' | head -1
    fi
}

get_previous_release() {
    if command -v gh &> /dev/null; then
        gh release list --repo aaif-goose/goose --limit 2 --json tagName --jq '.[].tagName' 2>/dev/null | sed -n '2p'
    else
        # Fallback: get second-latest version tag from git
        cd "$GOOSE_REPO" && git tag --sort=-v:refname | grep -E '^v[0-9]+\.[0-9]+\.[0-9]+$' | sed -n '2p'
    fi
}

# Determine versions
if [ -n "$1" ]; then
    OLD_VERSION="$1"
else
    OLD_VERSION=$(get_previous_release)
    if [ -z "$OLD_VERSION" ]; then
        echo "Error: Could not determine previous release version" >&2
        exit 1
    fi
fi

if [ -n "$2" ]; then
    NEW_VERSION="$2"
elif [ -n "$RELEASE_TAG" ]; then
    # Used by GitHub Actions release trigger
    NEW_VERSION="$RELEASE_TAG"
else
    NEW_VERSION=$(get_latest_release)
    if [ -z "$NEW_VERSION" ]; then
        echo "Error: Could not determine latest release version" >&2
        exit 1
    fi
fi

echo "=========================================="
echo "CLI Command Documentation Pipeline"
echo "=========================================="
echo "Old Version: $OLD_VERSION"
echo "New Version: $NEW_VERSION"
echo ""

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"

# Change to output directory
OUTPUT_DIR="$SCRIPT_DIR/../output"
mkdir -p "$OUTPUT_DIR"
cd "$OUTPUT_DIR"

# Use a per-run temp directory for logs to avoid collisions
LOG_DIR=$(mktemp -d)
trap 'rm -rf "$LOG_DIR"' EXIT

echo "Step 1: Extracting CLI structure from $OLD_VERSION..."
if ! ../scripts/extract-cli-structure.sh "$OLD_VERSION" > old-cli-structure.json 2>"$LOG_DIR/extract-old.log"; then
    echo "✗ Failed to extract CLI structure from $OLD_VERSION" >&2
    echo "Error output:" >&2
    cat "$LOG_DIR/extract-old.log" >&2
    exit 1
fi
echo "✓ Extracted $(jq '.commands | length' old-cli-structure.json) commands"

echo ""
echo "Step 2: Extracting CLI structure from $NEW_VERSION..."
if ! ../scripts/extract-cli-structure.sh "$NEW_VERSION" > new-cli-structure.json 2>"$LOG_DIR/extract-new.log"; then
    echo "✗ Failed to extract CLI structure from $NEW_VERSION" >&2
    echo "Error output:" >&2
    cat "$LOG_DIR/extract-new.log" >&2
    exit 1
fi
echo "✓ Extracted $(jq '.commands | length' new-cli-structure.json) commands"

echo ""
echo "Step 3: Comparing CLI structures..."
python3 ../scripts/diff-cli-structures.py old-cli-structure.json new-cli-structure.json > cli-changes.json 2>"$LOG_DIR/diff.log"

HAS_CHANGES=$(jq -r '.has_changes' cli-changes.json)
echo "✓ Comparison complete. Has changes: $HAS_CHANGES"

if [ "$HAS_CHANGES" = "true" ]; then
    echo ""
    echo "Changes detected:"
    echo "  - Commands added: $(jq '.summary.commands_added' cli-changes.json)"
    echo "  - Commands removed: $(jq '.summary.commands_removed' cli-changes.json)"
    echo "  - Commands modified: $(jq '.summary.commands_modified' cli-changes.json)"
    echo "  - Breaking changes: $(jq '.summary.breaking_changes' cli-changes.json)"
    
    echo ""
    echo "Step 4: Synthesizing CLI changes documentation..."
    
    # Run goose and capture output, filtering out session logs
    goose run --recipe ../recipes/synthesize-cli-changes.yaml 2>&1 | \
        sed -E 's/\x1B\[[0-9;]*[mK]//g' | \
        grep -v "^starting session" | \
        grep -v "^    session id:" | \
        grep -v "^    working directory:" | \
        grep -v "^─── text_editor" | \
        grep -v "^path:" | \
        grep -v "^command:" | \
        grep -v "^Closing session" | \
        grep -v "^Loading recipe:" | \
        grep -v "^Description:" | \
        cat -s > cli-changes.md.tmp

    # If the pipeline fails, surface the goose error (grep can exit 1 when it matches nothing)
    if [ ${PIPESTATUS[0]} -ne 0 ]; then
        echo "✗ Failed to synthesize CLI changes (goose run failed)" >&2
        exit 1
    fi
    
    # Check if we got meaningful content
    if [ -s cli-changes.md.tmp ] && grep -q "# CLI Command Changes" cli-changes.md.tmp; then
        mv cli-changes.md.tmp cli-changes.md
        echo "✓ Generated cli-changes.md ($(wc -l < cli-changes.md) lines)"
    elif [ -f cli-changes.md ] && [ -s cli-changes.md ]; then
        # File was written directly by goose
        rm -f cli-changes.md.tmp
        echo "✓ Generated cli-changes.md ($(wc -l < cli-changes.md) lines)"
    else
        echo "✗ Failed to generate cli-changes.md"
        rm -f cli-changes.md.tmp
        exit 1
    fi
    
    echo ""
    echo "Step 5: Updating CLI commands documentation..."
    
    # Set environment variables for the update recipe
    export CLI_COMMANDS_PATH="${GOOSE_REPO}/documentation/docs/guides/goose-cli-commands.md"
    
    # Run the update recipe
    goose run --recipe ../recipes/update-cli-commands.yaml 2>&1 | \
        sed -E 's/\x1B\[[0-9;]*[mK]//g' | \
        grep -v "^starting session" | \
        grep -v "^    session id:" | \
        grep -v "^    working directory:" | \
        grep -v "^─── text_editor" | \
        grep -v "^path:" | \
        grep -v "^command:" | \
        grep -v "^Closing session" | \
        grep -v "^Loading recipe:" | \
        grep -v "^Description:" | \
        cat -s

    if [ ${PIPESTATUS[0]} -ne 0 ]; then
        echo "✗ Failed to update documentation (goose run failed)" >&2
        exit 1
    fi
    
    echo "✓ Documentation update complete"
    
    echo ""
    echo "=========================================="
    echo "Pipeline Complete!"
    echo "=========================================="
    echo ""
    echo "Output files:"
    echo "  - old-cli-structure.json"
    echo "  - new-cli-structure.json"
    echo "  - cli-changes.json"
    echo "  - cli-changes.md"
    echo ""
    echo "Review the changes to the CLI commands documentation."
else
    echo ""
    echo "=========================================="
    echo "No Changes Detected"
    echo "=========================================="
    echo ""
    echo "No CLI changes between $OLD_VERSION and $NEW_VERSION."
    echo "Documentation update not needed."
fi
