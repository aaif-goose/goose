#!/usr/bin/env bash
set -euo pipefail

cargo build -p goose --bin goose-acp \
  --profile lean \
  --no-default-features \
  --features native-tls

binary="target/lean/goose-acp"
bytes=$(wc -c < "$binary" | tr -d '[:space:]')
mib=$(awk -v bytes="$bytes" 'BEGIN { printf "%.2f", bytes / 1024 / 1024 }')
printf '%s: %s bytes (%s MiB)\n' "$binary" "$bytes" "$mib"

if [[ -n "${GOOSE_LEAN_MAX_BYTES:-}" ]] && (( bytes > GOOSE_LEAN_MAX_BYTES )); then
  printf 'lean binary exceeds GOOSE_LEAN_MAX_BYTES=%s\n' "$GOOSE_LEAN_MAX_BYTES" >&2
  exit 1
fi
