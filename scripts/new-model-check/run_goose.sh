#!/usr/bin/env bash
set -euo pipefail

w=${1:?usage: run_goose.sh <work folder> [VARIABLE=value ...]}
shift
if [ ! -x "$w/bin/goose" ]; then
  echo "Error: no goose at $w/bin/goose" >&2
  exit 1
fi

rm -f "$w/root/state/logs/"llm_request.*.jsonl

keep=()
for name in PATH TMPDIR SSL_CERT_FILE SSL_CERT_DIR \
  HTTP_PROXY HTTPS_PROXY NO_PROXY ALL_PROXY http_proxy https_proxy no_proxy all_proxy; do
  if [ -n "${!name:-}" ]; then
    keep+=("$name=${!name}")
  fi
done

mkdir -p "$w/home" "$w/root" "$w/work"
cd "$w/work"
timeout 120 env -i "${keep[@]}" \
  HOME="$w/home" GOOSE_PATH_ROOT="$w/root" GOOSE_DISABLE_KEYRING=1 "$@" \
  "$w/bin/goose" run --text "hi" --no-session > "$w/last_run.txt" 2>&1
