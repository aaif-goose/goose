#!/bin/bash
# Extract CLI command structure from goose at a specific version
# Usage: ./extract-cli-structure.sh <version>
# Example: ./extract-cli-structure.sh v1.15.0
#
# For tagged releases (v*), downloads pre-built binary from GitHub releases.
# For HEAD or non-release refs, builds from source.

set -e
set -o pipefail

VERSION=${1:-"HEAD"}
GOOSE_REPO=${GOOSE_REPO:-"$HOME/Development/goose"}
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"

# Create a temporary directory
TEMP_DIR=$(mktemp -d)
trap "rm -rf $TEMP_DIR" EXIT

# Check if version is a release tag (starts with 'v' followed by numbers)
is_release_tag() {
    [[ "$1" =~ ^v[0-9]+\.[0-9]+\.[0-9]+$ ]]
}

# Download pre-built binary for a release version
download_release_binary() {
    local version=$1
    local safe_version=${version//\//-}
    local bin_dir="$TEMP_DIR/bin"
    mkdir -p "$bin_dir"
    
    echo "Downloading goose $version from GitHub releases..." >&2
    
    # Use the official download script with custom bin dir and specific version
    curl -fsSL "https://github.com/aaif-goose/goose/releases/download/stable/download_cli.sh" | \
        CONFIGURE=false GOOSE_BIN_DIR="$bin_dir" GOOSE_VERSION="$version" bash >&2 2>&1 || {
        echo "Error: Failed to download goose $version" >&2
        return 1
    }
    
    echo "$bin_dir/goose"
}

# Build goose from source
build_from_source() {
    local version=$1
    local safe_version=${version//\//-}
    
    if [ ! -d "$GOOSE_REPO" ]; then
        echo "Error: GOOSE_REPO directory not found: $GOOSE_REPO" >&2
        exit 1
    fi
    
    cd "$GOOSE_REPO"
    
    if [ "$version" = "HEAD" ]; then
        echo "Building goose from HEAD..." >&2
        cargo build --release --quiet >&2 2>&1 || {
            echo "Error: Failed to build goose from HEAD" >&2
            return 1
        }
        echo "$GOOSE_REPO/target/release/goose"
    else
        # Verify version exists
        if ! git rev-parse "$version" >/dev/null 2>&1; then
            echo "Error: Version $version not found in git history" >&2
            return 1
        fi
        
        echo "Building goose from $version..." >&2
        
        # Create a worktree for the version
        local worktree_dir="$TEMP_DIR/goose-$safe_version"
        git worktree add --quiet "$worktree_dir" "$version" >&2 2>&1 || {
            echo "Error: Failed to create worktree for $version" >&2
            return 1
        }
        
        cd "$worktree_dir"
        cargo build --release --quiet >&2 2>&1 || {
            echo "Error: Failed to build goose from $version" >&2
            cd "$GOOSE_REPO"
            git worktree remove "$worktree_dir" 2>/dev/null || true
            return 1
        }
        
        # Clean up worktree but keep the binary accessible
        local bin_path="$worktree_dir/target/release/goose"
        local temp_bin="$TEMP_DIR/goose-$safe_version-bin"
        cp "$bin_path" "$temp_bin"
        
        cd "$GOOSE_REPO"
        git worktree remove "$worktree_dir" 2>/dev/null || true
        
        echo "$temp_bin"
    fi
}

# Get the goose binary
if is_release_tag "$VERSION"; then
    GOOSE_BIN=$(download_release_binary "$VERSION")
else
    GOOSE_BIN=$(build_from_source "$VERSION")
fi

if [ -z "$GOOSE_BIN" ] || [ ! -x "$GOOSE_BIN" ]; then
    echo "Error: Goose binary not found or not executable" >&2
    exit 1
fi

echo "Using binary: $GOOSE_BIN" >&2
echo "Binary version: $($GOOSE_BIN --version 2>&1)" >&2

# Run the Python extraction script
python3 "$SCRIPT_DIR/extract-cli-structure.py" "$GOOSE_BIN" "$VERSION"
