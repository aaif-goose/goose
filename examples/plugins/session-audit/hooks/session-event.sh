#!/usr/bin/env sh
set -e

EVENT_TYPE="${1:-unknown}"
LOG_FILE="${GOOSE_PLUGIN_SESSION_AUDIT_LOG_FILE:-$HOME/.goose/audit.log}"
LOG_LEVEL="${GOOSE_PLUGIN_SESSION_AUDIT_LOG_LEVEL:-standard}"
SESSION_ID="${AGENT_SESSION_ID:-unknown}"
TIMESTAMP="$(date -u +"%Y-%m-%dT%H:%M:%SZ")"

mkdir -p "$(dirname "$LOG_FILE")"

if [ "$LOG_LEVEL" = "minimal" ]; then
    printf '%s [%s] %s\n' "$TIMESTAMP" "$SESSION_ID" "$EVENT_TYPE" >> "$LOG_FILE"
elif [ "$LOG_LEVEL" = "verbose" ]; then
    PAYLOAD="$(cat)"
    printf '%s [%s] %s %s\n' "$TIMESTAMP" "$SESSION_ID" "$EVENT_TYPE" "$PAYLOAD" >> "$LOG_FILE"
else
    PAYLOAD="$(cat)"
    TOOL_NAME="$(printf '%s' "$PAYLOAD" | grep -o '"tool_name":"[^"]*"' | head -1 | cut -d'"' -f4 2>/dev/null || true)"
    if [ -n "$TOOL_NAME" ]; then
        printf '%s [%s] %s tool=%s\n' "$TIMESTAMP" "$SESSION_ID" "$EVENT_TYPE" "$TOOL_NAME" >> "$LOG_FILE"
    else
        printf '%s [%s] %s\n' "$TIMESTAMP" "$SESSION_ID" "$EVENT_TYPE" >> "$LOG_FILE"
    fi
fi
