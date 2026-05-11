#!/usr/bin/env bash
# Tiny hook handler that loudly announces every event goose fires at it.
#
# Goose pipes the event payload as JSON on stdin and sets PLUGIN_ROOT in the
# environment. We tee a copy of the payload into a log file inside the plugin
# directory so you can see the full structure later.
set -euo pipefail

label="${1:-event}"
log="${PLUGIN_ROOT:-.}/last-event.log"
payload="$(cat)"

event_name="$(printf '%s' "$payload" | sed -n 's/.*"event":"\([^"]*\)".*/\1/p')"
tool_name="$(printf '%s' "$payload" | sed -n 's/.*"tool_name":"\([^"]*\)".*/\1/p')"

emoji() {
  case "$1" in
    start)     printf '\xf0\x9f\x9a\x80' ;;  # rocket
    prompt)    printf '\xf0\x9f\x92\xac' ;;  # speech balloon
    pre-tool)  printf '\xe2\x9a\xa1' ;;      # zap
    post-tool) printf '\xe2\x9c\x85' ;;      # check
    *)         printf '\xf0\x9f\x94\x94' ;;  # bell
  esac
}

icon="$(emoji "$label")"
suffix=""
[ -n "$tool_name" ] && suffix=" tool=$tool_name"

# Both stderr (so it shows up alongside goose tracing) and a log file.
printf '%s [hello-hooks] %s%s\n' "$icon" "${event_name:-$label}" "$suffix" 1>&2

{
  printf -- '---- %s @ %s ----\n' "${event_name:-$label}" "$(date -u '+%Y-%m-%dT%H:%M:%SZ')"
  printf '%s\n' "$payload"
} >> "$log"
