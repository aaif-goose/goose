#!/usr/bin/env bash
set -euo pipefail

w=${1:?usage: run_goose.sh <work folder> [VARIABLE=value ...]}
shift
if [ ! -x "$w/bin/goose" ]; then
  echo "Error: no goose at $w/bin/goose" >&2
  exit 1
fi

rm -f "$w/root/state/logs/"llm_request.*.jsonl

while read -r name; do
  case $name in GOOSE_*) unset "$name" ;; esac
done < <(compgen -e)

export HOME="$w/home" GOOSE_PATH_ROOT="$w/root" GOOSE_DISABLE_KEYRING=1
mkdir -p "$w/home" "$w/root" "$w/work"
cd "$w/work"
timeout 120 env "$@" "$w/bin/goose" run --text "hi" --no-session > "$w/last_run.txt" 2>&1
