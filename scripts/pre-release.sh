#!/bin/bash
set -euo pipefail

REPO="${GOOSE_GITHUB_REPO:-aaif-goose/goose}"
DEST="$HOME/Downloads"
TMPDIR=$(mktemp -d)
PLIST=$(mktemp /tmp/entitlements.XXXXXX)
trap "rm -rf '$TMPDIR' '$PLIST'" EXIT

# Find release PR
if [[ $# -gt 0 ]]; then
    SEARCH="chore(release): release version $1"
else
    SEARCH="chore(release): release version"
fi

PR=$(gh pr list --repo "$REPO" --search "$SEARCH in:title" --state all --limit 1 --json number,title)
PR_NUMBER=$(echo "$PR" | jq -r '.[0].number // empty')

if [[ -z "$PR_NUMBER" ]]; then
    echo "No matching release PR found in $REPO (search: \"$SEARCH\")."
    exit 1
fi

VERSION=$(echo "$PR" | jq -r '.[0].title // empty' | grep -oE '[0-9]+\.[0-9]+\.[0-9]+' || true)
echo "Found PR #$PR_NUMBER - version $VERSION"

# Grab the last nightly.link download URL from PR comments
DOWNLOAD_URL=$(gh api "repos/$REPO/issues/$PR_NUMBER/comments" \
    --jq '[.[].body | select(test("https://nightly\\.link/[^)]+\\.zip")) | capture("(?<url>https://nightly\\.link/[^)]+\\.zip)") | .url] | last // empty')

if [[ -z "$DOWNLOAD_URL" ]]; then
    echo "No download link found in PR comments."
    exit 1
fi
echo "Downloading $DOWNLOAD_URL"

# Download artifact — try nightly.link first, fall back to gh CLI
if ! curl -fSL --connect-timeout 10 -o "$TMPDIR/goose.zip" "$DOWNLOAD_URL" 2>/dev/null; then
    echo "nightly.link unavailable, falling back to GitHub API..."
    RUN_ID=$(echo "$DOWNLOAD_URL" | grep -oE 'actions/runs/[0-9]+' | cut -d/ -f3)
    ARTIFACT_NAME=$(echo "$DOWNLOAD_URL" | sed 's/\.zip$//' | xargs basename)
    ARTIFACT_ID=$(gh api "repos/$REPO/actions/runs/$RUN_ID/artifacts" \
        --jq ".artifacts[] | select(.name==\"$ARTIFACT_NAME\") | .id")
    if [[ -z "$ARTIFACT_ID" ]]; then
        echo "Could not find artifact '$ARTIFACT_NAME' for run $RUN_ID"
        exit 1
    fi
    gh api "repos/$REPO/actions/artifacts/$ARTIFACT_ID/zip" > "$TMPDIR/goose.zip"
fi
echo "Done."

unzip -o -q "$TMPDIR/goose.zip" -d "$TMPDIR/extracted"

INNER_ZIP=$(find "$TMPDIR/extracted" -name "*.zip" | head -1)
if [[ -n "$INNER_ZIP" ]]; then
    unzip -o -q "$INNER_ZIP" -d "$TMPDIR/extracted"
fi

echo "unzipped"

APP=$(find "$TMPDIR/extracted" -name "*.app" -maxdepth 2 | head -1)
if [[ -z "$APP" ]]; then
    echo "No .app found in archive."
    exit 1
fi

APP_NAME="Goose ${VERSION}.app"
rm -rf "$DEST/$APP_NAME"
cp -R "$APP" "$DEST/$APP_NAME"
APP_PATH="$DEST/$APP_NAME"

# Remove quarantine and sign with entitlements
xattr -r -d com.apple.quarantine "$APP_PATH" 2>/dev/null || true

cat > "$PLIST" << 'EOF'
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>com.apple.security.cs.allow-jit</key><true/>
    <key>com.apple.security.cs.allow-unsigned-executable-memory</key><true/>
    <key>com.apple.security.device.audio-input</key><true/>
    <key>com.apple.security.device.camera</key><true/>
    <key>com.apple.security.network.client</key><true/>
    <key>com.apple.security.network.server</key><true/>
</dict>
</plist>
EOF

codesign --force --deep --sign - --entitlements "$PLIST" "$APP_PATH"

echo ""
echo "Ready: $APP_PATH"
echo "  open \"$APP_PATH\""
