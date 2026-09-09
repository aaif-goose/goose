#!/usr/bin/env bash
set -euo pipefail

show_usage() {
  echo "Usage: $0 [options]"
  echo ""
  echo "Options:"
  echo "  -n, --top-n NUM          Number of recommended models to test (default: 3)"
  echo "  -m, --models MODELS      Comma-separated download ids. Skips search."
  echo "  -o, --output-dir DIR     Directory for logs (default: ./local-model-smoke-results)"
  echo "      --ram-gb NUM         Override RAM passed to goose lm search"
  echo "      --instruction TEXT   Prompt to send to each model"
  echo "      --repo-prefix TEXT   Forwarded to goose lm search"
  echo "      --repo-suffix TEXT   Forwarded to goose lm search"
  echo "      --quant TEXT         Forwarded to goose lm search"
  echo "      --download-retries N Retry model downloads after HF rate limits (default: 3)"
  echo "      --retry-delay SEC    Initial retry delay for HF rate limits (default: 60)"
  echo "      --run-timeout SEC    Kill a model run after this many seconds (default: 600, 0 disables)"
  echo "      --keep-downloads     Do not delete models after testing"
  echo "  -h, --help               Show this help message"
  echo ""
  echo "Environment:"
  echo "  GOOSE_BIN                Optional goose binary path"
  echo "  SKIP_BUILD               Skip cargo build when set"
}

TOP_N=3
OUTPUT_DIR="./local-model-smoke-results"
MODEL_LIST=""
RAM_GB=""
INSTRUCTION="Say hello in one short sentence. Do not use tools."
REPO_PREFIX="unsloth/"
REPO_SUFFIX=""
QUANT="Q4"
DOWNLOAD_RETRIES=3
RETRY_DELAY=60
RUN_TIMEOUT=600
KEEP_DOWNLOADS=false

while [[ $# -gt 0 ]]; do
  case "$1" in
    -n|--top-n)
      TOP_N="$2"
      shift 2
      ;;
    -o|--output-dir)
      OUTPUT_DIR="$2"
      shift 2
      ;;
    -m|--models)
      MODEL_LIST="$2"
      shift 2
      ;;
    --ram-gb)
      RAM_GB="$2"
      shift 2
      ;;
    --instruction)
      INSTRUCTION="$2"
      shift 2
      ;;
    --repo-prefix)
      REPO_PREFIX="$2"
      shift 2
      ;;
    --repo-suffix)
      REPO_SUFFIX="$2"
      shift 2
      ;;
    --quant)
      QUANT="$2"
      shift 2
      ;;
    --download-retries)
      DOWNLOAD_RETRIES="$2"
      shift 2
      ;;
    --retry-delay)
      RETRY_DELAY="$2"
      shift 2
      ;;
    --run-timeout)
      RUN_TIMEOUT="$2"
      shift 2
      ;;
    --keep-downloads)
      KEEP_DOWNLOADS=true
      shift
      ;;
    -h|--help)
      show_usage
      exit 0
      ;;
    *)
      echo "Error: Unknown option: $1"
      show_usage
      exit 1
      ;;
  esac
done

if ! [[ "$TOP_N" =~ ^[0-9]+$ ]] || [[ "$TOP_N" -eq 0 ]]; then
  echo "Error: --top-n must be a positive integer"
  exit 1
fi

if ! [[ "$DOWNLOAD_RETRIES" =~ ^[0-9]+$ ]]; then
  echo "Error: --download-retries must be a non-negative integer"
  exit 1
fi

if ! [[ "$RETRY_DELAY" =~ ^[0-9]+$ ]]; then
  echo "Error: --retry-delay must be a non-negative integer"
  exit 1
fi

if ! [[ "$RUN_TIMEOUT" =~ ^[0-9]+$ ]]; then
  echo "Error: --run-timeout must be a non-negative integer"
  exit 1
fi

if ! command -v jq >/dev/null 2>&1; then
  echo "Error: jq is required"
  exit 1
fi

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

if [[ -z "${SKIP_BUILD:-}" && -z "${GOOSE_BIN:-}" ]]; then
  echo "Building goose..."
  (cd "$REPO_ROOT" && cargo build -p goose-cli --features local-inference --bin goose)
  echo ""
fi

GOOSE_BIN="${GOOSE_BIN:-$REPO_ROOT/target/debug/goose}"
if [[ ! -x "$GOOSE_BIN" ]]; then
  echo "Error: goose binary not found or not executable: $GOOSE_BIN"
  exit 1
fi

mkdir -p "$OUTPUT_DIR"

EXISTING_MODELS_FILE="$OUTPUT_DIR/existing-models.txt"
RESULTS_FILE="$OUTPUT_DIR/results.tsv"
"$GOOSE_BIN" lm list | awk 'NR > 2 && $4 == "✓" { print $1 }' > "$EXISTING_MODELS_FILE"
printf "status\tmodel_id\tdetail\n" > "$RESULTS_FILE"

TEMP_HF_CACHE_ROOT=""
TEMP_MODELS=()

cleanup_temp_models() {
  local model_id
  local cleanup_failed=false

  for model_id in "${TEMP_MODELS[@]}"; do
    if ! "$GOOSE_BIN" lm delete "$model_id" >/dev/null 2>&1; then
      cleanup_failed=true
    fi
  done

  if [[ "$cleanup_failed" = true ]]; then
    echo "Warning: could not unregister all temporary models; cache retained at $TEMP_HF_CACHE_ROOT" >&2
    return
  fi

  if [[ -n "$TEMP_HF_CACHE_ROOT" && -d "$TEMP_HF_CACHE_ROOT" ]]; then
    rm -rf -- "$TEMP_HF_CACHE_ROOT"
  fi
}

untrack_temp_model() {
  local target="$1"
  local model_id
  local remaining=()

  for model_id in "${TEMP_MODELS[@]}"; do
    if [[ "$model_id" != "$target" ]]; then
      remaining+=("$model_id")
    fi
  done
  TEMP_MODELS=("${remaining[@]}")
}

if [[ "$KEEP_DOWNLOADS" = false ]]; then
  TEMP_HF_CACHE_ROOT=$(mktemp -d)
fi
trap cleanup_temp_models EXIT

MODELS=()
if [[ -n "$MODEL_LIST" ]]; then
  IFS=',' read -ra REQUESTED_MODELS <<< "$MODEL_LIST"
  for model in "${REQUESTED_MODELS[@]}"; do
    repo="${model%%:*}"
    variant="${model#*:}"
    if [[ "$variant" = "$model" ]]; then
      variant="manual"
    fi
    MODELS+=("$repo"$'\t'"$model"$'\t'"$model"$'\t'"$variant"$'\t'"0")
  done
else
  search_query="${REPO_PREFIX%/}"
  SEARCH_ARGS=(lm search)
  if [[ -n "$search_query" ]]; then
    SEARCH_ARGS+=("$search_query")
  fi
  SEARCH_ARGS+=(--limit "$TOP_N" --json)
  if [[ -n "$RAM_GB" ]]; then
    SEARCH_ARGS+=(--ram-gb "$RAM_GB")
  fi
  if [[ -n "$REPO_PREFIX" ]]; then
    SEARCH_ARGS+=(--repo-prefix "$REPO_PREFIX")
  fi
  if [[ -n "$REPO_SUFFIX" ]]; then
    SEARCH_ARGS+=(--repo-suffix "$REPO_SUFFIX")
  fi
  if [[ -n "$QUANT" ]]; then
    SEARCH_ARGS+=(--quant "$QUANT")
  fi

  SEARCH_JSON="$OUTPUT_DIR/search.json"
  echo "Finding recommended local models..."
  "$GOOSE_BIN" "${SEARCH_ARGS[@]}" > "$SEARCH_JSON"

  while IFS= read -r model_row; do
    MODELS+=("$model_row")
  done < <(
    jq -r --argjson limit "$TOP_N" '
      [.[] | select(.recommended_variant != null)]
      | .[:$limit][]
      | [
          .repo_id,
          .recommended_variant.model_id,
          .recommended_variant.download_id,
          .recommended_variant.label,
          (.recommended_variant.size_bytes | tostring)
        ]
      | @tsv
    ' "$SEARCH_JSON"
  )
fi

if [[ ${#MODELS[@]} -eq 0 ]]; then
  echo "No recommended models found."
  exit 1
fi

RESULTS=()
OVERALL_SUCCESS=true

record_result() {
  local status="$1"
  local model_id="$2"
  local detail="$3"
  RESULTS+=("$status $model_id${detail:+ - $detail}")
  printf "%s\t%s\t%s\n" "$status" "$model_id" "$detail" >> "$RESULTS_FILE"
}

summarize_goose_error() {
  awk '
    /Ran into this error:/ {
      sub(/^.*Ran into this error: /, "")
      print
      found = 1
      exit
    }
    /Request failed:/ {
      sub(/^.*Request failed: /, "Request failed: ")
      print
      found = 1
      exit
    }
    /Provider error:/ {
      sub(/^.*Provider error: /, "Provider error: ")
      print
      found = 1
      exit
    }
    END { if (!found) exit 1 }
  ' "$1"
}

download_once() {
  local download_id="$1"
  local cache_root="$2"

  if [[ -n "$cache_root" ]]; then
    HF_HUB_CACHE="$cache_root/hub" \
      HF_XET_CACHE="$cache_root/xet" \
      "$GOOSE_BIN" lm download "$download_id"
  else
    "$GOOSE_BIN" lm download "$download_id"
  fi
}

download_model() {
  local download_id="$1"
  local log_file="$2"
  local cache_root="$3"
  local attempt=1
  local delay="$RETRY_DELAY"

  while true; do
    : > "$log_file"
    if download_once "$download_id" "$cache_root" 2>&1 | tee "$log_file"; then
      return 0
    fi

    if ! grep -q "429 Too Many Requests" "$log_file"; then
      return 1
    fi

    if [[ "$attempt" -gt "$DOWNLOAD_RETRIES" ]]; then
      return 2
    fi

    echo "Hugging Face rate limit hit. Retrying in ${delay}s ($attempt/$DOWNLOAD_RETRIES)..."
    sleep "$delay"
    attempt=$((attempt + 1))
    if [[ "$delay" -gt 0 ]]; then
      delay=$((delay * 2))
    fi
  done
}

run_model() {
  local model_id="$1"
  local log_file="$2"

  if [[ "$RUN_TIMEOUT" -eq 0 ]]; then
    GOOSE_MODE=auto GOOSE_PROVIDER=local GOOSE_MODEL="$model_id" \
      "$GOOSE_BIN" run --no-profile --text "$INSTRUCTION" 2>&1 | tee "$log_file"
    return "${PIPESTATUS[0]}"
  fi

  perl -e '
    my $timeout = shift;
    my $pid = fork();
    die "fork failed: $!" unless defined $pid;
    if ($pid == 0) {
      exec @ARGV;
      die "exec failed: $!";
    }
    local $SIG{ALRM} = sub {
      kill "TERM", $pid;
      sleep 2;
      kill "KILL", $pid;
      exit 124;
    };
    alarm $timeout;
    waitpid($pid, 0);
    my $status = $?;
    alarm 0;
    exit($status & 127 ? 128 + ($status & 127) : $status >> 8);
  ' \
    "$RUN_TIMEOUT" \
    env GOOSE_MODE=auto GOOSE_PROVIDER=local GOOSE_MODEL="$model_id" \
    "$GOOSE_BIN" run --no-profile --text "$INSTRUCTION" 2>&1 | tee "$log_file"
  return "${PIPESTATUS[0]}"
}

echo "Testing ${#MODELS[@]} model(s)"
echo ""

for row in "${MODELS[@]}"; do
  IFS=$'\t' read -r repo_id model_id download_id label size_bytes <<< "$row"
  safe_model=$(echo "$model_id" | tr '/:' '__' | tr -cd '[:alnum:]_.-')
  download_log="$OUTPUT_DIR/$safe_model.download.log"
  run_log="$OUTPUT_DIR/$safe_model.run.log"
  delete_log="$OUTPUT_DIR/$safe_model.delete.log"
  size_gb=$(awk "BEGIN { printf \"%.1f\", $size_bytes / 1024 / 1024 / 1024 }")

  echo "=========================================================="
  echo "Model:    $model_id"
  echo "Repo:     $repo_id"
  echo "Variant:  $label (${size_gb}GB)"
  echo "=========================================================="

  existed_before=false
  if grep -Fxq "$model_id" "$EXISTING_MODELS_FILE"; then
    existed_before=true
  fi

  downloaded=false
  if [[ "$existed_before" = true ]]; then
    echo "Using pre-existing download for $model_id"
    downloaded=true
  else
    if [[ -n "$TEMP_HF_CACHE_ROOT" ]]; then
      TEMP_MODELS+=("$model_id")
    fi
    set +e
    download_model "$download_id" "$download_log" "$TEMP_HF_CACHE_ROOT"
    download_status=$?
    set -e
    if [[ "$download_status" -eq 0 ]]; then
      downloaded=true
    elif [[ "$download_status" -eq 2 ]]; then
      echo "Download rate limited for $model_id"
      record_result "FAIL" "$model_id" "Hugging Face rate limited"
      OVERALL_SUCCESS=false
    else
      echo "Download failed for $model_id"
      record_result "FAIL" "$model_id" "download failed"
      OVERALL_SUCCESS=false
    fi
  fi

  if [[ "$downloaded" = true ]]; then
    set +e
    run_model "$model_id" "$run_log"
    run_status=$?
    set -e

    if [[ ! -s "$run_log" ]]; then
      echo "Run produced no output for $model_id"
      record_result "FAIL" "$model_id" "empty output"
      OVERALL_SUCCESS=false
    elif [[ "$run_status" -eq 124 || "$run_status" -eq 142 ]]; then
      echo "Run timed out after ${RUN_TIMEOUT}s for $model_id"
      record_result "FAIL" "$model_id" "run timed out"
      OVERALL_SUCCESS=false
    elif error_summary=$(summarize_goose_error "$run_log"); then
      echo "Goose reported an error for $model_id"
      echo "  $error_summary"
      record_result "FAIL" "$model_id" "$error_summary"
      OVERALL_SUCCESS=false
    elif [[ "$run_status" -eq 0 ]]; then
      echo "Run passed for $model_id"
      record_result "PASS" "$model_id" ""
    else
      echo "Run replied but exited with status $run_status for $model_id"
      record_result "FAIL" "$model_id" "replied but exited $run_status"
      OVERALL_SUCCESS=false
    fi
  fi

  if [[ "$KEEP_DOWNLOADS" = false && "$downloaded" = true && "$existed_before" = false ]]; then
    if "$GOOSE_BIN" lm delete "$model_id" 2>&1 | tee "$delete_log"; then
      untrack_temp_model "$model_id"
      echo "Unregistered $model_id; its temporary cache will be removed at exit"
    else
      echo "Delete failed for $model_id"
      record_result "FAIL" "$model_id" "delete failed"
      OVERALL_SUCCESS=false
    fi
  elif [[ "$KEEP_DOWNLOADS" = false && "$downloaded" = true ]]; then
    echo "Keeping $model_id because it existed before this run"
  fi

  echo ""
done

echo "=== Test Summary ==="
for result in "${RESULTS[@]}"; do
  echo "$result"
done

if [[ "$OVERALL_SUCCESS" = false ]]; then
  echo ""
  echo "Some local model smoke tests failed."
  exit 1
fi

echo ""
echo "All local model smoke tests passed."
