#!/usr/bin/env bash
set -euo pipefail

out=${1:?usage: download_models_dev.sh <output file>}
curl -fsSL --max-time 60 https://models.dev/api.json -o "$out"
if ! node -e 'JSON.parse(require("fs").readFileSync(process.argv[1], "utf8"))' "$out" 2>/dev/null; then
  echo "Error: models.dev returned something that isn't JSON" >&2
  exit 1
fi
