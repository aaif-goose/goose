#!/usr/bin/env bash
set -euo pipefail

VITE_PORT="${1:-${VITE_PORT:-}}"
if [[ -z "${VITE_PORT}" ]]; then
    echo "VITE_PORT is required" >&2
    exit 1
fi

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_DIR="$(cd "${SCRIPT_DIR}/.." && pwd)"

PID=$(lsof -ti :"${VITE_PORT}" 2>/dev/null | head -1 || true)
if [[ -n "${PID}" ]]; then
    PROC_ARGS="$(ps -p "${PID}" -o args= 2>/dev/null || true)"
    PROC_CWD="$(lsof -a -p "${PID}" -d cwd -Fn 2>/dev/null | sed -n 's/^n//p' | head -1)"
    if [[ "${PROC_CWD}" == "${PROJECT_DIR}" && "${PROC_ARGS}" == *"--port ${VITE_PORT} --strictPort"* ]]; then
        echo "Reusing existing Goose2 Vite dev server on port ${VITE_PORT} (PID ${PID})"
        exit 0
    fi

    PROC_NAME="$(ps -p "${PID}" -o comm= 2>/dev/null || true)"
    echo "Port ${VITE_PORT} is already in use by '${PROC_NAME}' (PID ${PID})." >&2
    echo "Run 'just goose2 kill' if it is a stale Goose2 dev server, or rerun with VITE_PORT=<free-port> just goose2 dev." >&2
    exit 1
fi

cd "${PROJECT_DIR}"
exec pnpm exec vite --port "${VITE_PORT}" --strictPort
