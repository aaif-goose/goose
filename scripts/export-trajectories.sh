#!/usr/bin/env bash
#
# Export goose sessions as JSON transcripts for trajectory normalization.
#
# Each exported file is one self-contained goose session export: the session
# record plus its `conversation` message array, exactly as produced by
# `goose session export --format json`. That file is the transcript contract
# consumed by the `goose` source in https://github.com/letta-ai/trajectory:
#
#   normalizeTranscript({ source: "goose", transcript: fileContents })
#
# This script only enumerates sessions and calls the existing export command;
# it never reads or writes the session database directly.

set -euo pipefail

OUTPUT_DIR="./goose-trajectories"
LIMIT=""
WORKING_DIR=""
GOOSE_BIN="${GOOSE_BIN:-goose}"
FORCE=false

usage() {
    cat <<'EOF'
Usage: ./scripts/export-trajectories.sh [options]

Export goose sessions to one JSON transcript per session, for use as a
trajectory source.

Options:
  -o, --output-dir DIR    Directory for exported transcripts (default: ./goose-trajectories)
  -l, --limit N           Export only the N most recently updated sessions
  -w, --working-dir DIR   Only export sessions whose working directory matches DIR
  -f, --force             Re-export sessions that already have an output file
  -h, --help              Show this help

Examples:
  # Export the 100 most recent sessions
  ./scripts/export-trajectories.sh --limit 100

  # Export every session for one project into a chosen directory
  ./scripts/export-trajectories.sh --working-dir ~/Development/goose -o /tmp/corpus

Then normalize them with trajectory:
  npm install @letta-ai/trajectory
  node -e '
    const { normalizeTranscript } = require("@letta-ai/trajectory");
    const fs = require("fs");
    const transcript = fs.readFileSync(process.argv[1], "utf8");
    const { records, diagnostics } = normalizeTranscript({ source: "goose", transcript });
    console.log(JSON.stringify({ records, diagnostics }, null, 2));
  ' ./goose-trajectories/<session-id>.json
EOF
}

while [[ $# -gt 0 ]]; do
    case "$1" in
        -o|--output-dir)
            OUTPUT_DIR="$2"
            shift 2
            ;;
        -l|--limit)
            LIMIT="$2"
            shift 2
            ;;
        -w|--working-dir)
            WORKING_DIR="$2"
            shift 2
            ;;
        -f|--force)
            FORCE=true
            shift
            ;;
        -h|--help)
            usage
            exit 0
            ;;
        *)
            echo "Unknown option: $1" >&2
            usage >&2
            exit 1
            ;;
    esac
done

if ! command -v "${GOOSE_BIN}" >/dev/null 2>&1; then
    echo "error: '${GOOSE_BIN}' not found on PATH. Set GOOSE_BIN to the goose binary." >&2
    exit 1
fi

if ! command -v jq >/dev/null 2>&1; then
    echo "error: 'jq' is required to parse the session list." >&2
    exit 1
fi

mkdir -p "${OUTPUT_DIR}"

list_args=(session list --format json)
if [[ -n "${LIMIT}" ]]; then
    list_args+=(--limit "${LIMIT}")
fi
if [[ -n "${WORKING_DIR}" ]]; then
    list_args+=(--working_dir "${WORKING_DIR}")
fi

echo "Listing goose sessions..." >&2
sessions_json="$("${GOOSE_BIN}" "${list_args[@]}")"

# `goose session list --format json` returns an array of session summaries.
# Accept either a bare array or an object wrapping one under `sessions`.
# Read into an array without `mapfile`, which macOS's bash 3.2 lacks.
session_ids=()
while IFS= read -r session_id; do
    [[ -n "${session_id}" ]] && session_ids+=("${session_id}")
done < <(
    printf '%s' "${sessions_json}" |
        jq -r 'if type == "array" then . else (.sessions // []) end | .[] | .id // empty'
)

if [[ ${#session_ids[@]} -eq 0 ]]; then
    echo "No sessions found." >&2
    exit 0
fi

exported=0
skipped=0
failed=0

for session_id in "${session_ids[@]}"; do
    out="${OUTPUT_DIR}/${session_id}.json"

    if [[ -f "${out}" && "${FORCE}" != true ]]; then
        skipped=$((skipped + 1))
        continue
    fi

    if "${GOOSE_BIN}" session export --id "${session_id}" --format json --output "${out}" >/dev/null 2>&1; then
        exported=$((exported + 1))
    else
        # A session can fail to export while it is actively being written, or
        # when it holds no messages. Skip it rather than aborting the corpus.
        echo "warn: failed to export session ${session_id}" >&2
        rm -f "${out}"
        failed=$((failed + 1))
    fi
done

echo "Exported ${exported} session(s) to ${OUTPUT_DIR}" >&2
if [[ ${skipped} -gt 0 ]]; then
    echo "Skipped ${skipped} already-exported session(s); pass --force to re-export." >&2
fi
if [[ ${failed} -gt 0 ]]; then
    echo "Failed to export ${failed} session(s)." >&2
fi
