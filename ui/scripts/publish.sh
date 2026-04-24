#!/usr/bin/env bash
set -euo pipefail

# Builds and publishes all @aaif npm packages:
#   @aaif/goose-sdk            — ACP TypeScript SDK
#   @aaif/goose-binary-*       — platform-specific goose CLI binaries
#   @aaif/goose                — TUI that depends on the above
#
# Linux binaries are built inside Docker containers on their native arch.
# macOS binaries are built natively (requires macOS host with Rust).
# Windows is skipped (no cross-compilation support yet).
#
# Usage:
#   ./ui/scripts/publish.sh          # dry run
#   ./ui/scripts/publish.sh --real   # publish to npmjs.org
#
# Requires:
#   - Docker with buildx (linux/amd64 + linux/arm64)
#   - Rust toolchain with aarch64-apple-darwin and x86_64-apple-darwin targets
#   - pnpm
#   - NPM_PUBLISH_TOKEN env var (or ~/.npm-publish-token file)

REPO_ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
NATIVE_DIR="${REPO_ROOT}/ui/goose-binary"
SDK_DIR="${REPO_ROOT}/ui/sdk"
TEXT_DIR="${REPO_ROOT}/ui/text"
REGISTRY="https://registry.npmjs.org"
DOCKER_IMAGE="rust:1.92-bookworm"

DRY_RUN=true
if [[ "${1:-}" == "--real" ]]; then
  DRY_RUN=false
  echo "==> REAL publish to ${REGISTRY}"
else
  echo "==> Dry run (pass --real to publish for real)"
fi

# Resolve NPM token
if [[ -z "${NPM_PUBLISH_TOKEN:-}" ]]; then
  TOKEN_FILE="${HOME}/.npm-publish-token"
  if [[ -f "${TOKEN_FILE}" ]]; then
    NPM_PUBLISH_TOKEN="$(cat "${TOKEN_FILE}")"
    export NPM_PUBLISH_TOKEN
    echo "==> Loaded NPM_PUBLISH_TOKEN from ${TOKEN_FILE}"
  else
    echo "ERROR: NPM_PUBLISH_TOKEN not set and ${TOKEN_FILE} not found" >&2
    exit 1
  fi
fi

# ---------------------------------------------------------------------------
# Step 1: Build macOS binaries natively
# ---------------------------------------------------------------------------
build_macos() {
  local platform="$1" target="$2"
  local pkg_dir="${NATIVE_DIR}/goose-binary-${platform}/bin"

  echo "==> Building goose for ${platform} (${target}) natively"
  cargo build --release --target "${target}" --bin goose --manifest-path "${REPO_ROOT}/Cargo.toml"

  mkdir -p "${pkg_dir}"
  cp "${REPO_ROOT}/target/${target}/release/goose" "${pkg_dir}/goose"
  chmod +x "${pkg_dir}/goose"
  echo "    ✅ ${pkg_dir}/goose"
}

build_macos darwin-arm64 aarch64-apple-darwin
build_macos darwin-x64   x86_64-apple-darwin

# ---------------------------------------------------------------------------
# Step 2: Build Linux binaries in Docker on native arch
# ---------------------------------------------------------------------------
DOCKER_BUILD_SCRIPT='#!/bin/bash
set -euo pipefail
apt-get update -qq
apt-get install -y -qq --no-install-recommends \
  build-essential cmake pkg-config libssl-dev libdbus-1-dev \
  libclang-dev protobuf-compiler libprotobuf-dev ca-certificates >/dev/null 2>&1
echo "==> Compiling goose (this takes a while)..."
cargo build --release --bin goose
cp /build/target/release/goose /output/goose
echo "==> Done"
'

build_linux_docker() {
  local platform="$1" docker_platform="$2"
  local pkg_dir="${NATIVE_DIR}/goose-binary-${platform}/bin"

  echo "==> Building goose for ${platform} in Docker (${docker_platform})"

  mkdir -p "${pkg_dir}"

  # Create a temporary directory with only what cargo needs.
  # This avoids sending multi-GB Docker contexts.
  local ctx
  ctx="$(mktemp -d)"
  trap "rm -rf '${ctx}'" RETURN 2>/dev/null || true

  # Copy Rust source
  rsync -a --delete \
    --exclude='target/' \
    --exclude='.git/' \
    --exclude='node_modules/' \
    --exclude='documentation/' \
    --exclude='ui/desktop/' \
    --exclude='ui/goose-binary/*/bin/' \
    --exclude='evals/' \
    --exclude='.hermit/' \
    --exclude='*.jsonl' \
    --exclude='bench_results/' \
    "${REPO_ROOT}/" "${ctx}/"

  # Write a minimal Dockerfile into the context
  cat > "${ctx}/Dockerfile.npm-build" <<'DEOF'
FROM rust:1.92-bookworm
RUN apt-get update -qq && \
    apt-get install -y -qq --no-install-recommends \
      build-essential cmake pkg-config libssl-dev libdbus-1-dev \
      libclang-dev protobuf-compiler libprotobuf-dev ca-certificates >/dev/null 2>&1 && \
    rm -rf /var/lib/apt/lists/*
WORKDIR /build
COPY . .
RUN mkdir -p /output && \
    cargo build --release --bin goose && \
    cp target/release/goose /output/goose
DEOF

  # Build in Docker and extract the binary
  local iid="goose-npm-build-${platform}-$$"
  docker build \
    --platform "${docker_platform}" \
    -f "${ctx}/Dockerfile.npm-build" \
    -t "${iid}" \
    "${ctx}"

  # Extract binary from the image
  local cid
  cid="$(docker create --platform "${docker_platform}" "${iid}" /bin/true)"
  docker cp "${cid}:/output/goose" "${pkg_dir}/goose"
  docker rm "${cid}" >/dev/null
  docker rmi "${iid}" >/dev/null 2>&1 || true

  rm -rf "${ctx}"

  echo "    ✅ ${pkg_dir}/goose"
}

build_linux_docker linux-x64   linux/amd64
build_linux_docker linux-arm64 linux/arm64

# ---------------------------------------------------------------------------
# Step 3: Verify all binaries
# ---------------------------------------------------------------------------
echo ""
echo "==> Verifying binaries"
for plat in darwin-arm64 darwin-x64 linux-arm64 linux-x64; do
  bin="${NATIVE_DIR}/goose-binary-${plat}/bin/goose"
  if [[ ! -f "${bin}" ]]; then
    echo "    ❌ MISSING: ${bin}"
    exit 1
  fi
  size=$(stat -f%z "${bin}" 2>/dev/null || stat -c%s "${bin}" 2>/dev/null)
  size_mb=$(( size / 1048576 ))
  filetype=$(file -b "${bin}" | head -c 60)
  echo "    ✅ ${plat}: ${size_mb} MB — ${filetype}"
done

# ---------------------------------------------------------------------------
# Step 4: Build TypeScript packages
# ---------------------------------------------------------------------------
echo ""
echo "==> Building @aaif/goose-sdk"
(cd "${SDK_DIR}" && pnpm run build:ts)

echo "==> Building @aaif/goose"
(cd "${TEXT_DIR}" && pnpm run build)

# ---------------------------------------------------------------------------
# Step 5: Publish
# ---------------------------------------------------------------------------
echo ""

PUBLISH_ARGS=(--access public --no-git-checks --registry "${REGISTRY}" --tag latest)
if [[ "${DRY_RUN}" == "true" ]]; then
  PUBLISH_ARGS+=(--dry-run)
fi

# Write a temporary .npmrc for authentication
NPMRC_BAK=""
UI_NPMRC="${REPO_ROOT}/ui/.npmrc"
if [[ -f "${UI_NPMRC}" ]]; then
  NPMRC_BAK="$(cat "${UI_NPMRC}")"
fi

# Append auth token for npmjs.org
if ! grep -q "registry.npmjs.org/:_authToken" "${UI_NPMRC}" 2>/dev/null; then
  echo "//registry.npmjs.org/:_authToken=\${NPM_PUBLISH_TOKEN}" >> "${UI_NPMRC}"
fi

cleanup_npmrc() {
  if [[ -n "${NPMRC_BAK}" ]]; then
    echo "${NPMRC_BAK}" > "${UI_NPMRC}"
  else
    # Remove the line we added
    sed -i.bak '/registry.npmjs.org\/:_authToken/d' "${UI_NPMRC}" && rm -f "${UI_NPMRC}.bak"
  fi
}
trap cleanup_npmrc EXIT

# Publish order matters: dependencies first
echo "==> Publishing @aaif/goose-sdk"
(cd "${REPO_ROOT}/ui" && pnpm publish "${PUBLISH_ARGS[@]}" acp)

echo "==> Publishing native binary packages"
for plat in darwin-arm64 darwin-x64 linux-arm64 linux-x64; do
  pkg="goose-binary/goose-binary-${plat}"
  echo "    Publishing @aaif/goose-binary-${plat}"
  (cd "${REPO_ROOT}/ui" && pnpm publish "${PUBLISH_ARGS[@]}" "${pkg}")
done

echo "==> Publishing @aaif/goose"
(cd "${REPO_ROOT}/ui" && pnpm publish "${PUBLISH_ARGS[@]}" text)

echo ""
if [[ "${DRY_RUN}" == "true" ]]; then
  echo "✅ Dry run complete. Pass --real to publish for real."
else
  echo "✅ All packages published to ${REGISTRY}"
fi
