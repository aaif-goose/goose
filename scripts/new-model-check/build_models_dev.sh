#!/usr/bin/env bash
set -euo pipefail

out=${1:?usage: build_models_dev.sh <output file>}
out="$(cd "$(dirname "$out")" && pwd)/$(basename "$out")"
repo=anomalyco/models.dev

sha=$(gh run list -R "$repo" -w deploy.yml -s success -L 1 --json headSha --jq '.[].headSha')
if [ -z "$sha" ]; then
  echo "Error: no successful models.dev deploy found" >&2
  exit 1
fi

dir=$(mktemp -d)
trap 'rm -rf "$dir"' EXIT

git clone --quiet "https://github.com/$repo" "$dir"
git -C "$dir" checkout --quiet "$sha"
(cd "$dir" && bun install --frozen-lockfile) >&2
(cd "$dir/packages/web" && bun ./script/build.ts) >&2
cp "$dir/packages/web/dist/_api.json" "$out"
echo "$sha"
