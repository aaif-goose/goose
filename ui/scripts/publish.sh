#!/usr/bin/env bash
set -euo pipefail

# Publishes @aaif/goose-acp, @aaif/goose, and all native binary packages to npm.
#
# Usage:
#   ./ui/scripts/publish.sh         # publish all (dry-run)
#   ./ui/scripts/publish.sh --real   # publish for real
#
# Prerequisites:
#   - npm login to the @aaif scope
#   - Native binaries built via npm run build:native:all in ui/acp

REPO_ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
ACP_DIR="${REPO_ROOT}/ui/acp"
NATIVE_DIR="${REPO_ROOT}/ui/goose-binary"
TEXT_DIR="${REPO_ROOT}/ui/text"

DRY_RUN="--dry-run"
if [[ "${1:-}" == "--real" ]]; then
  DRY_RUN=""
  echo "==> Publishing for real"
else
  echo "==> Dry run (pass --real to publish)"
fi

# Build and publish @aaif/goose-acp first (dependency of @aaif/goose)
echo "==> Building @aaif/goose-acp"
(cd "${ACP_DIR}" && npm run build)

echo "==> Publishing @aaif/goose-acp"
(cd "${ACP_DIR}" && npm publish --access public ${DRY_RUN})

# Build @aaif/goose
echo "==> Building @aaif/goose"
(cd "${TEXT_DIR}" && npm run build)

NATIVE_PACKAGES=(
  "goose-binary-darwin-arm64"
  "goose-binary-darwin-x64"
  "goose-binary-linux-arm64"
  "goose-binary-linux-x64"
  "goose-binary-win32-x64"
)

# Publish native binary packages
for pkg in "${NATIVE_PACKAGES[@]}"; do
  pkg_dir="${NATIVE_DIR}/${pkg}"

  if [ ! -f "${pkg_dir}/bin/goose" ] && [ ! -f "${pkg_dir}/bin/goose.exe" ]; then
    echo "    SKIP ${pkg} (no binary found — run npm run build:native:all in ui/acp first)"
    continue
  fi

  echo "==> Publishing @aaif/${pkg}"
  (cd "${pkg_dir}" && npm publish --access public ${DRY_RUN})
done

# Publish the main package
echo "==> Publishing @aaif/goose"
(cd "${TEXT_DIR}" && npm publish --access public ${DRY_RUN})

echo "==> Done"
