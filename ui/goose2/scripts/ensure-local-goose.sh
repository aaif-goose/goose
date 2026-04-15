#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat <<'EOF'
Usage: ensure-local-goose.sh [--print-bin | --check-bin]

Syncs and builds a dedicated local goose checkout for goose2 development.

Environment variables:
  GOOSE_DEV_MODE             auto|required (default: auto)
  GOOSE_DEV_ROOT             path to the shared goose2 dev cache root
                             (default: platform cache dir under home)
  GOOSE_DEV_REPO             path to the managed goose checkout
                             (default: $GOOSE_DEV_ROOT/goose)
  GOOSE_DEV_STAMP_FILE       path to the shared build stamp file
                             (default: $GOOSE_DEV_ROOT/stamp.env)
  GOOSE_DEV_CLONE_URL        git clone URL for the managed goose checkout
                             (default: https://github.com/block/goose.git)
  GOOSE_DEV_REMOTE           git remote to sync from (default: origin)
  GOOSE_DEV_BRANCH           preferred branch to use (default: baxen/goose2)
  GOOSE_DEV_FALLBACK_BRANCH  fallback branch when the preferred branch does
                             not exist remotely (default: main)
  GOOSE_DEV_ALLOW_DIRTY      1 to allow syncing/building a dirty checkout
EOF
}

action="build"
print_bin=0
while [[ $# -gt 0 ]]; do
  case "$1" in
    --print-bin)
      print_bin=1
      shift
      ;;
    --check-bin)
      action="check"
      print_bin=1
      shift
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    *)
      echo "Unknown argument: $1" >&2
      usage >&2
      exit 1
      ;;
  esac
done

mode="${GOOSE_DEV_MODE:-auto}"
clone_url="${GOOSE_DEV_CLONE_URL:-https://github.com/block/goose.git}"
remote="${GOOSE_DEV_REMOTE:-origin}"
preferred_branch="${GOOSE_DEV_BRANCH:-baxen/goose2}"
fallback_branch="${GOOSE_DEV_FALLBACK_BRANCH:-main}"
allow_dirty="${GOOSE_DEV_ALLOW_DIRTY:-0}"

log() {
  echo "[goose-dev] $*" >&2
}

fail_or_skip() {
  local message="$1"
  if [[ "${mode}" == "required" ]]; then
    echo "${message}" >&2
    exit 1
  fi
  log "${message}"
  # In check mode, exit 2 so callers (e.g. just dev) can detect "not ready"
  # and block instead of silently continuing without a goose binary.
  if [[ "${action}" == "check" ]]; then
    exit 2
  fi
  exit 0
}

default_goose_dev_root() {
  if [[ -n "${XDG_CACHE_HOME:-}" ]]; then
    printf '%s/goose2-dev\n' "${XDG_CACHE_HOME}"
    return
  fi

  case "$(uname -s)" in
    Darwin)
      printf '%s/Library/Caches/goose2-dev\n' "${HOME}"
      ;;
    *)
      printf '%s/.cache/goose2-dev\n' "${HOME}"
      ;;
  esac
}

goose_dev_root="${GOOSE_DEV_ROOT:-$(default_goose_dev_root)}"
goose_repo="${GOOSE_DEV_REPO:-${goose_dev_root}/goose}"
stamp_file="${GOOSE_DEV_STAMP_FILE:-${goose_dev_root}/stamp.env}"
bin_path="${goose_repo}/target/debug/goose"

resolve_remote_head() {
  local branch_name="$1"
  git -C "${goose_repo}" ls-remote --heads "${remote}" "${branch_name}" 2>/dev/null | awk 'NR == 1 { print $1 }'
}

resolve_branch() {
  local resolved_branch="${preferred_branch}"
  local resolved_head
  resolved_head="$(resolve_remote_head "${resolved_branch}")"

  if [[ -z "${resolved_head}" && "${resolved_branch}" != "${fallback_branch}" ]]; then
    log "Remote branch ${remote}/${resolved_branch} not found; falling back to ${remote}/${fallback_branch}."
    resolved_branch="${fallback_branch}"
    resolved_head="$(resolve_remote_head "${resolved_branch}")"
  fi

  if [[ -z "${resolved_head}" ]]; then
    if [[ "${mode}" == "required" ]]; then
      echo "Could not resolve ${remote}/${resolved_branch} for managed goose checkout at ${goose_repo}." >&2
      return 1
    fi
    log "Could not resolve ${remote}/${resolved_branch} for managed goose checkout at ${goose_repo}."
    return 2
  fi

  RESOLVED_BRANCH="${resolved_branch}"
  RESOLVED_REMOTE_HEAD="${resolved_head}"
  return 0
}

write_stamp() {
  local branch_name="$1"
  local commit_sha="$2"

  mkdir -p "$(dirname "${stamp_file}")"
  {
    printf 'STAMP_REPO=%q\n' "${goose_repo}"
    printf 'STAMP_BRANCH=%q\n' "${branch_name}"
    printf 'STAMP_COMMIT=%q\n' "${commit_sha}"
    printf 'STAMP_BIN=%q\n' "${bin_path}"
  } >"${stamp_file}"
}

ensure_checkout_exists() {
  if [[ -d "${goose_repo}/.git" ]]; then
    return 0
  fi

  if [[ "${action}" == "check" ]]; then
    fail_or_skip "Managed goose checkout not found at ${goose_repo}. Rerun just setup."
  fi

  log "Cloning managed goose checkout into ${goose_repo}."
  mkdir -p "$(dirname "${goose_repo}")"
  git clone "${clone_url}" "${goose_repo}" >/dev/null 2>&1 || {
    fail_or_skip "Failed to clone managed goose checkout from ${clone_url} into ${goose_repo}."
  }
}

ensure_checkout_exists

if [[ "${allow_dirty}" != "1" ]]; then
  if [[ -n "$(git -C "${goose_repo}" status --porcelain)" ]]; then
    fail_or_skip "Managed goose checkout at ${goose_repo} is dirty. Use a dedicated checkout or set GOOSE_DEV_ALLOW_DIRTY=1."
  fi
fi

if resolve_branch; then
  branch="${RESOLVED_BRANCH}"
  remote_head="${RESOLVED_REMOTE_HEAD}"
else
  resolve_branch_status=$?
  case "${resolve_branch_status}" in
    1)
      exit 1
      ;;
    2)
      exit 0
      ;;
    *)
      echo "Unexpected resolve_branch status: ${resolve_branch_status}" >&2
      exit 1
      ;;
  esac
fi

if [[ "${action}" == "check" ]]; then
  if [[ ! -f "${stamp_file}" ]]; then
    fail_or_skip "Managed goose checkout is configured, but no local goose build stamp was found. Rerun just setup."
  fi

  # shellcheck disable=SC1090
  source "${stamp_file}"

  if [[ "${STAMP_REPO:-}" != "${goose_repo}" ]]; then
    fail_or_skip "Managed goose checkout changed since the last local goose build. Rerun just setup."
  fi

  if [[ "${STAMP_BRANCH:-}" != "${branch}" ]]; then
    fail_or_skip "Managed goose branch is now ${branch}, but the local goose build was prepared for ${STAMP_BRANCH:-unknown}. Rerun just setup."
  fi

  if [[ ! -x "${STAMP_BIN:-}" ]]; then
    fail_or_skip "Local goose binary was not found at ${STAMP_BIN:-unknown}. Rerun just setup."
  fi

  local_head="$(git -C "${goose_repo}" rev-parse HEAD)"
  if [[ "${STAMP_COMMIT:-}" != "${local_head}" ]]; then
    fail_or_skip "Managed goose checkout changed after the last local build. Rerun just setup."
  fi

  if [[ "${STAMP_COMMIT:-}" != "${remote_head}" ]]; then
    fail_or_skip "Managed goose checkout is behind ${remote}/${branch}. Rerun just setup."
  fi

  if [[ "${print_bin}" == "1" ]]; then
    printf '%s\n' "${STAMP_BIN}"
  fi
  exit 0
fi

git -C "${goose_repo}" fetch "${remote}" "${branch}" >/dev/null 2>&1

remote_ref="refs/remotes/${remote}/${branch}"
if ! git -C "${goose_repo}" show-ref --verify --quiet "${remote_ref}"; then
  fail_or_skip "Fetched ${remote}/${branch}, but ${remote_ref} is not available in ${goose_repo}."
fi

if git -C "${goose_repo}" show-ref --verify --quiet "refs/heads/${branch}"; then
  git -C "${goose_repo}" checkout "${branch}" >/dev/null 2>&1
else
  git -C "${goose_repo}" checkout -b "${branch}" --track "${remote}/${branch}" >/dev/null 2>&1
fi

# Reset to the remote head. This is a managed build-only checkout, so we
# always want to match the remote exactly. A plain `pull --ff-only` would
# fail when the remote branch has been force-pushed (rebased/amended).
git -C "${goose_repo}" reset --hard "${remote}/${branch}" >/dev/null 2>&1

log "Building goose from ${goose_repo} on ${branch}."
(
  cd "${goose_repo}"
  cargo build -p goose-cli --bin goose
)

if [[ -n "$(git -C "${goose_repo}" status --porcelain -- Cargo.lock)" ]]; then
  # Cargo may refresh the lockfile while compiling a freshly synced checkout.
  # This managed repo is only a build source for goose2, so restore the tracked
  # lockfile to keep the checkout clean for later preflight checks.
  git -C "${goose_repo}" checkout -- Cargo.lock
fi

if [[ ! -x "${bin_path}" ]]; then
  echo "Expected goose binary at ${bin_path}, but it was not built successfully." >&2
  exit 1
fi

write_stamp "${branch}" "$(git -C "${goose_repo}" rev-parse HEAD)"

log "Local goose binary ready at ${bin_path}."
if [[ "${print_bin}" == "1" ]]; then
  printf '%s\n' "${bin_path}"
fi
