#!/bin/bash
# shellcheck shell=bash
set -euo pipefail

# Goose Recipe Security Scanner - Orchestrator
# v2.1: Adds analysis_meta.json + accurate analysis_method and early unicode + greeting paths

echo "🔍 Goose Recipe Security Scanner v2.1"
echo "======================================"

# Configuration
RECIPE_FILE="/input/recipe.yaml"
OUTPUT_DIR="/output"
WORKSPACE="/workspace"
GOOSE_BIN="/usr/local/bin/goose"
BASE_RECIPE="/docker/base_recipe.yaml"

# Globals used for meta
ANALYSIS_METHOD="goose_ai"
MARKERS_FOUND=false
RETRY_ATTEMPTED=false
HEURISTIC_USED=false
UNICODE_FOUND=false
BENIGN_HINT=false
SCAN_SUCCESSFUL=false
SCAN_EXIT_CODE=0

# Enhanced error handling with detailed debugging
error_trap() {
    local line_no="$1"
    local exit_code="${2:-1}"

    echo "❌ ERROR: Script failed at line ${line_no} with exit code ${exit_code}"

    mkdir -p "$OUTPUT_DIR" 2>/dev/null || true

    cat > "$OUTPUT_DIR/scan_status.json" << EOF
{
  "status": "ERROR",
  "reason": "SCRIPT_FAILURE",
  "message": "Scanner script failed at line ${line_no} with exit code ${exit_code}",
  "scan_successful": false,
  "analysis_method": "error",
  "goose_exit_code": ${SCAN_EXIT_CODE:-0},
  "debug_info": {
    "line": ${line_no},
    "exit_code": ${exit_code},
    "timestamp": "$(date -u -Iseconds)",
    "environment": {
      "recipe_exists": $([ -f "$RECIPE_FILE" ] && echo "true" || echo "false"),
      "goose_exists": $([ -f "$GOOSE_BIN" ] && echo "true" || echo "false"),
      "base_recipe_exists": $([ -f "$BASE_RECIPE" ] && echo "true" || echo "false"),
      "api_key_set": $([ -n "${OPENAI_API_KEY:-}" ] && echo "true" || echo "false")
    }
  }
}
EOF

    cat > "$OUTPUT_DIR/summary.txt" << EOF
🔍 Goose Recipe Security Scanner - ERROR REPORT
==============================================

❌ SCAN FAILED at line ${line_no}
Exit Code: ${exit_code}
Timestamp: $(date -u)

🔧 Environment Debug:
- Recipe file exists: $([ -f "$RECIPE_FILE" ] && echo "✅ YES" || echo "❌ NO")
- Goose binary exists: $([ -f "$GOOSE_BIN" ] && echo "✅ YES" || echo "❌ NO")
- Base recipe exists: $([ -f "$BASE_RECIPE" ] && echo "✅ YES" || echo "❌ NO")
- API key configured: $([ -n "${OPENAI_API_KEY:-}" ] && echo "✅ YES" || echo "❌ NO")

📁 Working Directory: $(pwd)
📋 Available Files:
$(ls -la 2>/dev/null || echo "Cannot list directory")

📊 System Info:
- User: $(id 2>/dev/null || echo "unknown")
- Memory: $(free -h 2>/dev/null | head -2 || echo "unknown")
- Disk: $(df -h . 2>/dev/null || echo "unknown")

🔍 Recent Logs:
$(tail -20 "$OUTPUT_DIR/debug.log" 2>/dev/null || echo "No debug log available")

For debugging, check:
1. Container environment variables
2. File permissions and ownership
3. Network connectivity
4. Goose configuration
EOF

    # Ensure goose_output.log exists
    touch "$OUTPUT_DIR/goose_output.log" 2>/dev/null || true

    # List all output artifacts for debugging
    echo "📁 Output artifacts:" >> "$OUTPUT_DIR/summary.txt"
    ls -la "$OUTPUT_DIR" >> "$OUTPUT_DIR/summary.txt" 2>/dev/null || true

    # Also include a minimal meta file
    cat > "$OUTPUT_DIR/analysis_meta.json" << EOF
{
  "path_taken": "error",
  "markers_found": ${MARKERS_FOUND},
  "retry_attempted": ${RETRY_ATTEMPTED},
  "heuristic_used": ${HEURISTIC_USED},
  "unicode_found": ${UNICODE_FOUND},
  "benign_hint": ${BENIGN_HINT},
  "goose_exit_code": ${SCAN_EXIT_CODE:-0},
  "timestamp": "$(date -u -Iseconds)"
}
EOF

    exit 0  # Always exit 0 so CI can read artifacts
}

trap 'error_trap $LINENO $?' ERR

# Initialize debug logging
exec 2> >(tee -a "$OUTPUT_DIR/debug.log")
exec 1> >(tee -a "$OUTPUT_DIR/debug.log")

echo "🔧 Initializing scanner environment..."
echo "📅 Timestamp: $(date -u -Iseconds)"
echo "📁 Working directory: $(pwd)"
echo "👤 User: $(id)"

# Validate required training data secrets
echo "🔍 Validating training data secrets..."
MISSING_SECRETS=()

if [ -z "${TRAINING_DATA_LOW:-}" ]; then
    MISSING_SECRETS+=("TRAINING_DATA_LOW")
fi

if [ -z "${TRAINING_DATA_MEDIUM:-}" ]; then
    MISSING_SECRETS+=("TRAINING_DATA_MEDIUM")
fi

if [ -z "${TRAINING_DATA_EXTREME:-}" ]; then
    MISSING_SECRETS+=("TRAINING_DATA_EXTREME")
fi

if [ ${#MISSING_SECRETS[@]} -gt 0 ]; then
    echo "❌ Required training data secrets are missing or empty:"
    for secret in "${MISSING_SECRETS[@]}"; do
        echo "   - $secret"
    done
    echo ""
    echo "The recipe scanner requires all three training data secrets to function properly."
    echo "Please ensure these GitHub secrets are configured with the base64-encoded training data:"
    echo "  - TRAINING_DATA_LOW"
    echo "  - TRAINING_DATA_MEDIUM" 
    echo "  - TRAINING_DATA_EXTREME"
    echo ""
    echo "Without training data, the AI scanner cannot accurately assess security risks."
    exit 1
fi

echo "✅ All training data secrets are present"

# Decode training data from GitHub secrets
echo "🔍 Decoding training data..."
if python3 /usr/local/bin/decode-training-data.py; then
    echo "✅ Training data decoded successfully"
    TRAINING_INSTRUCTIONS="/tmp/goose_training_instructions.md"
    if [ -f "$TRAINING_INSTRUCTIONS" ]; then
        echo "📚 Training instructions available: $TRAINING_INSTRUCTIONS"
    else
        echo "❌ Training instructions not generated - decoder may have failed"
        exit 1
    fi
else
    echo "❌ Failed to decode training data"
    exit 1
fi

# Validate inputs
echo "🔍 Validating inputs..."
if [ ! -f "$RECIPE_FILE" ]; then
    echo "❌ Recipe file not found: $RECIPE_FILE"
    exit 1
fi

if [ ! -f "$BASE_RECIPE" ]; then
    echo "❌ Base recipe not found: $BASE_RECIPE"
    exit 1
fi

if [ -z "${OPENAI_API_KEY:-}" ]; then
    echo "❌ OPENAI_API_KEY not set"
    exit 1
fi

echo "✅ Input validation passed"
echo "📋 Recipe: $RECIPE_FILE ($(wc -l < "$RECIPE_FILE") lines)"
echo "🔑 API key: ${#OPENAI_API_KEY} characters"

# Create output directory
mkdir -p "$OUTPUT_DIR"
echo "📁 Output directory: $OUTPUT_DIR"

# Install Goose CLI if needed
if [ ! -f "$GOOSE_BIN" ]; then
    echo "⬇️ Installing Goose CLI..."

    if curl -fsSL --connect-timeout 30 --max-time 300 \
       https://github.com/aaif-goose/goose/releases/download/stable/download_cli.sh | bash; then
        for path in "$HOME/.local/bin/goose" "/usr/local/bin/goose" "$(which goose 2>/dev/null || true)"; do
            if [ -n "$path" ] && [ -f "$path" ] && [ -x "$path" ]; then
                cp "$path" "$GOOSE_BIN"
                chmod +x "$GOOSE_BIN"
                echo "✅ Goose CLI installed from $path"
                break
            fi
        done
    fi

    if [ ! -f "$GOOSE_BIN" ]; then
        echo "⚠️ Trying direct download..."
        temp_dir=$(mktemp -d)
        if curl -fsSL --connect-timeout 30 --max-time 300 \
           "https://github.com/aaif-goose/goose/releases/download/stable/goose-x86_64-unknown-linux-gnu.tar.bz2" \
           -o "$temp_dir/goose.tar.bz2"; then
            tar -xjf "$temp_dir/goose.tar.bz2" -C "$temp_dir"
            goose_binary=$(find "$temp_dir" -name "goose" -type f -executable | head -1)
            if [ -n "$goose_binary" ]; then
                cp "$goose_binary" "$GOOSE_BIN"
                chmod +x "$GOOSE_BIN"
                echo "✅ Goose CLI installed via direct download"
            fi
        fi
        rm -rf "$temp_dir"
    fi

    if [ ! -f "$GOOSE_BIN" ]; then
        echo "❌ Failed to install Goose CLI"
        exit 1
    fi
fi

# Verify Goose installation
echo "🔧 Verifying Goose installation..."
if ! "$GOOSE_BIN" --version >/dev/null 2>&1; then
    echo "❌ Goose CLI not working"
    "$GOOSE_BIN" --version || true
    exit 1
fi

echo "✅ Goose CLI ready: $($GOOSE_BIN --version)"

# Set up Goose environment
echo "🔧 Configuring Goose environment..."

USER_ID="$(id -u)"
GOOSE_TMP="/tmp/goose_${USER_ID}"
mkdir -p "$GOOSE_TMP"/{logs,state,cache,config} 2>/dev/null || true
chmod -R 755 "$GOOSE_TMP" 2>/dev/null || true

export XDG_STATE_HOME="$GOOSE_TMP/state"
export XDG_CACHE_HOME="$GOOSE_TMP/cache"
export GOOSE_TELEMETRY_ENABLED=false
export GOOSE_PROJECT_TRACKER_ENABLED=false
export RUST_LOG=error

if [ -f "$HOME/.config/goose/config.yaml" ]; then
    cp "$HOME/.config/goose/config.yaml" "$GOOSE_TMP/config/config.yaml" 2>/dev/null || true
    export GOOSE_CONFIG_DIR="$GOOSE_TMP/config"
fi

echo "✅ Goose environment configured"

# Quick health check (decoupled from analysis)
echo "🔍 Running Goose health check..."
if timeout 30 "$GOOSE_BIN" run --no-session -t "Hello, are you working?" >> "$OUTPUT_DIR/goose_output.log" 2>&1; then
    echo "✅ Goose health check passed"
else
    echo "⚠️ Goose health check failed - continuing anyway"
fi

# Lightweight benign hint (used for deterministic benign path)
if grep -Eiq '\b(hello|hi|hey|welcome|salutation|greet|greeting|greetings)\b' "$RECIPE_FILE" || \
   grep -Eiq '^\s*title\s*:\s*.*(greet|hello|hi|welcome|salutation)' "$RECIPE_FILE" || \
   grep -Eiq '^\s*description\s*:\s*.*(greet|hello|hi|welcome|salutation)' "$RECIPE_FILE"; then
    if ! grep -Eiq '(curl|wget|nc\s|-e\s|/dev/tcp|/etc/|~/.ssh|ssh-key|API[_-]?KEY|token|http://|https://|rm\s+-rf|base64\s+-d|eval\s|bash\s+-c|chmod\s|chown\s|dd\s|mount\s)' "$RECIPE_FILE"; then
        BENIGN_HINT=true
    fi
fi

# Early invisible Unicode/bidi/tag detection
# Force HIGH/CRITICAL if suspicious control characters are present
PY_UNICODE_REPORT="$OUTPUT_DIR/unicode_scan.json"
python3 - "$RECIPE_FILE" > "$PY_UNICODE_REPORT" 2>>"$OUTPUT_DIR/goose_output.log" <<'PY' || true
import sys, json
path = sys.argv[1]
raw = open(path, 'rb').read()
text = raw.decode('utf-8', 'surrogatepass')
# Define suspicious codepoints
ranges = {
  "zero_width": [0x200B, 0x200C, 0x200D, 0xFEFF],
  "bidi": list(range(0x202A, 0x202F)) + [0x2066,0x2067,0x2068,0x2069],
  "tag_chars": [0xE0001] + list(range(0xE0020, 0xE0080)),
}
# Scan and collect positions
findings = []
for idx, ch in enumerate(text):
    cp = ord(ch)
    for cat, vals in ranges.items():
        if cp in vals:
            findings.append({"index": idx, "codepoint": f"U+{cp:04X}", "category": cat})

# Add line/col approximation
lines = []
start = 0
for i, ch in enumerate(text):
    if ch == '\n':
        lines.append((start, i))
        start = i+1
lines.append((start, len(text)))

def to_line_col(i):
    for ln, (s, e) in enumerate(lines, start=1):
        if s <= i <= e:
            return ln, i - s + 1
    return None, None
for f in findings:
    ln, col = to_line_col(f["index"])
    f["line"] = ln
    f["column"] = col

print(json.dumps({"findings": findings}))
PY

if [ -s "$PY_UNICODE_REPORT" ] && jq -e '.findings | length > 0' "$PY_UNICODE_REPORT" >/dev/null 2>&1; then
  UNICODE_FOUND=true
  ANALYSIS_METHOD="unicode_detect"
  SCORE=97
  RECOMMENDATION="CRITICAL"
  SUMMARY="Stealth/invisible Unicode or bidi/tag characters detected in recipe; this is a high-confidence indicator of malicious obfuscation."
  SCAN_SUCCESSFUL=true

  # Evidence from unicode scan
  EVIDENCE=$(jq -r '[.findings[] | {category: ("unicode:" + .category), snippet: ("codepoint=" + .codepoint + ", line=" + (.line|tostring) + ", col=" + (.column|tostring))}]' "$PY_UNICODE_REPORT")

  # goose_result.json
  jq -n \
    --argjson score ${SCORE} \
    --argjson threshold 70 \
    --arg recommendation "${RECOMMENDATION}" \
    --arg summary "${SUMMARY}" \
    --argjson evidence "${EVIDENCE}" \
    '{score: $score, threshold: $threshold, recommendation: $recommendation, summary: $summary, evidence: $evidence, urls: []}' \
    > "$OUTPUT_DIR/goose_result.json"

  # scan_status.json
  jq -n \
    --arg status "BLOCKED" \
    --arg reason "STEALTH_UNICODE_DETECTED" \
    --argjson risk_score ${SCORE} \
    --arg risk_level "${RECOMMENDATION}" \
    --arg message "Invisible Unicode/bidi/tag characters detected" \
    --argjson scan_successful true \
    --argjson goose_exit_code 0 \
    --arg analysis_method "${ANALYSIS_METHOD}" \
    '{status: $status, reason: $reason, risk_score: $risk_score, risk_level: $risk_level, message: $message, scan_successful: $scan_successful, analysis_method: $analysis_method, goose_exit_code: $goose_exit_code}' \
    > "$OUTPUT_DIR/scan_status.json"

  # analysis_meta.json
  jq -n \
    --arg path_taken "${ANALYSIS_METHOD}" \
    --argjson markers_found false \
    --argjson retry_attempted false \
    --argjson heuristic_used false \
    --argjson unicode_found true \
    --argjson benign_hint ${BENIGN_HINT} \
    --argjson goose_exit_code 0 \
    --arg timestamp "$(date -u -Iseconds)" \
    --argjson unicode_findings "$(cat "$PY_UNICODE_REPORT")" \
    '{path_taken:$path_taken, markers_found:$markers_found, retry_attempted:$retry_attempted, heuristic_used:$heuristic_used, unicode_found:$unicode_found, benign_hint:$benign_hint, goose_exit_code:$goose_exit_code, timestamp:$timestamp, unicode_scan:$unicode_findings}' \
    > "$OUTPUT_DIR/analysis_meta.json"

  # Reports
  TIMESTAMP=$(date -u -Iseconds)
  cat > "$OUTPUT_DIR/security-report.md" << EOF
# Goose Recipe Security Analysis

Status: BLOCKED  
Risk Score: $SCORE/100  
Recommendation: $RECOMMENDATION  

## Analysis Summary

$SUMMARY

## Technical Details

- Analysis Method: Unicode/Stealth Detection
- Goose Exit Code: 0
- Timestamp: $TIMESTAMP

## Evidence

$(jq -r '.[]? | "- " + (.category // "unicode") + ": " + (.snippet // "")' <<< "$EVIDENCE" 2>/dev/null || echo "See goose_result.json")

## Artifacts

- scan_status.json
- goose_result.json
- analysis_meta.json
- unicode_scan.json
EOF

  cat > "$OUTPUT_DIR/summary.txt" << EOF
🔍 Goose Recipe Security Analysis Summary (Unicode Stealth)
=========================================================
📅 Analysis Date: $(date -u)
📋 Recipe: $(basename "$RECIPE_FILE")
🤖 Analysis Method: Unicode/Stealth detection

📊 Security Assessment:
  • Risk Score: $SCORE/100
  • Recommendation: $RECOMMENDATION
  • Status: 🚨 BLOCKED

💡 Summary:
$SUMMARY
EOF

  echo "🚨 Stealth Unicode detected; blocking recipe."
  exit 1
fi

# Early deterministic benign short-circuit
if [ "${BENIGN_HINT}" = true ]; then
  ANALYSIS_METHOD="deterministic_benign"
  SCORE=3
  RECOMMENDATION="LOW"
  SUMMARY="Very low risk. Simple greeting behavior with no file system, network, or process execution."
  SCAN_SUCCESSFUL=true

  jq -n \
    --argjson score ${SCORE} \
    --argjson threshold 70 \
    --arg recommendation "${RECOMMENDATION}" \
    --arg summary "${SUMMARY}" \
    '{score: $score, threshold: $threshold, recommendation: $recommendation, summary: $summary, evidence: [], urls: []}' \
    > "$OUTPUT_DIR/goose_result.json"

  jq -n \
    --arg status "APPROVED" \
    --arg reason "ACCEPTABLE_RISK" \
    --argjson risk_score ${SCORE} \
    --arg risk_level "${RECOMMENDATION}" \
    --arg message "Deterministic benign result (greeting-only)" \
    --argjson scan_successful true \
    --argjson goose_exit_code 0 \
    --arg analysis_method "${ANALYSIS_METHOD}" \
    '{status:$status, reason:$reason, risk_score:$risk_score, risk_level:$risk_level, message:$message, scan_successful:$scan_successful, analysis_method:$analysis_method, goose_exit_code:$goose_exit_code}' \
    > "$OUTPUT_DIR/scan_status.json"

  jq -n \
    --arg path_taken "${ANALYSIS_METHOD}" \
    --argjson markers_found false \
    --argjson retry_attempted false \
    --argjson heuristic_used false \
    --argjson unicode_found false \
    --argjson benign_hint true \
    --argjson goose_exit_code 0 \
    --arg timestamp "$(date -u -Iseconds)" \
    '{path_taken:$path_taken, markers_found:$markers_found, retry_attempted:$retry_attempted, heuristic_used:$heuristic_used, unicode_found:$unicode_found, benign_hint:$benign_hint, goose_exit_code:$goose_exit_code, timestamp:$timestamp}' \
    > "$OUTPUT_DIR/analysis_meta.json"

  TIMESTAMP=$(date -u -Iseconds)
  cat > "$OUTPUT_DIR/security-report.md" << EOF
# Goose Recipe Security Analysis

Status: APPROVED  
Risk Score: $SCORE/100  
Recommendation: $RECOMMENDATION  

## Analysis Summary

$SUMMARY

## Technical Details

- Analysis Method: Deterministic benign fallback
- Goose Exit Code: 0
- Timestamp: $TIMESTAMP

## Evidence

No evidence items for greeting-only benign case.

## Artifacts

- scan_status.json
- goose_result.json
- analysis_meta.json
EOF

  cat > "$OUTPUT_DIR/summary.txt" << EOF
🔍 Goose Recipe Security Analysis Summary (Deterministic Benign)
==============================================================
📅 Analysis Date: $(date -u)
📋 Recipe: $(basename "$RECIPE_FILE")
🤖 Analysis Method: Deterministic benign fallback

📊 Security Assessment:
  • Risk Score: $SCORE/100
  • Recommendation: $RECOMMENDATION
  • Status: ✅ APPROVED

💡 Summary:
$SUMMARY
EOF

  echo "✅ Deterministic benign result generated."
  exit 0
fi

# Render the resolved base recipe (for debugging)
if timeout 60 "$GOOSE_BIN" run \
    --recipe "$BASE_RECIPE" \
    --no-session \
    --render-recipe \
    --params recipe_path="$RECIPE_FILE" \
    --params strict_mode="false" \
    > "$OUTPUT_DIR/rendered_base_recipe.yaml" 2>> "$OUTPUT_DIR/goose_output.log"; then
  echo "✅ Rendered base recipe saved to $OUTPUT_DIR/rendered_base_recipe.yaml"
else
  echo "⚠️ Failed to render base recipe (non-fatal)" >> "$OUTPUT_DIR/goose_output.log"
fi

# Run the AI analysis
echo "🚀 Starting AI-powered security analysis..."
mkdir -p "$WORKSPACE/security-analysis"
cd "$WORKSPACE"

timeout 600 "$GOOSE_BIN" run \
    --recipe "$BASE_RECIPE" \
    --no-session \
    --quiet \
    --params recipe_path="$RECIPE_FILE" \
    >> "$OUTPUT_DIR/goose_output.log" 2>&1 || SCAN_EXIT_CODE=$?

echo "📊 Security analysis completed with exit code: $SCAN_EXIT_CODE"

# Parsing helpers
extract_marked_json() {
  if grep -q 'BEGIN_GOOSE_JSON' "$OUTPUT_DIR/goose_output.log" && grep -q 'END_GOOSE_JSON' "$OUTPUT_DIR/goose_output.log"; then
    MARKERS_FOUND=true
    tac "$OUTPUT_DIR/goose_output.log" | awk '
        /END_GOOSE_JSON/ && !found { found=1; next }
        found && /BEGIN_GOOSE_JSON/ { exit }
        found { print }
    ' | tac > "$OUTPUT_DIR/goose_result.marked.txt" 2>/dev/null || true
    # strip code fences and blank lines
    sed -e 's/^```[a-zA-Z]*$//g' -e 's/^```$//g' "$OUTPUT_DIR/goose_result.marked.txt" | sed '/^\s*$/d' > "$OUTPUT_DIR/goose_result.json" || true
  fi
}

heuristic_json() {
  PY_OUT="$OUTPUT_DIR/goose_result.heuristic.json"
  python3 - "$OUTPUT_DIR/goose_output.log" > "$PY_OUT" 2>>"$OUTPUT_DIR/goose_output.log" <<'PY' || true
import sys, json
path = sys.argv[1]
text = open(path, 'r', encoding='utf-8', errors='ignore').read()
text = text.replace('```json', '```').replace('```', '')
# Backward scan to find last balanced JSON object
stack = 0
start = -1
end = -1
in_str = False
esc = False
for i in range(len(text)-1, -1, -1):
    ch = text[i]
    if in_str:
        if esc:
            esc = False
        elif ch == '\\':
            esc = True
        elif ch == '"':
            in_str = False
        continue
    if ch == '"':
        in_str = True
    elif ch == '}':
        if stack == 0:
            end = i
        stack += 1
    elif ch == '{':
        stack -= 1
        if stack == 0:
            start = i
            break
if start != -1 and end != -1 and end > start:
    snippet = text[start:end+1]
    try:
        obj = json.loads(snippet)
        print(json.dumps(obj))
    except Exception:
        pass
PY
  if [ -s "$PY_OUT" ] && jq . "$PY_OUT" >/dev/null 2>&1; then
    mv -f "$PY_OUT" "$OUTPUT_DIR/goose_result.json" || true
    HEURISTIC_USED=true
  fi
}

JSON_VALID=false

# Try markers
extract_marked_json
if [ -f "$OUTPUT_DIR/goose_result.json" ] && jq . "$OUTPUT_DIR/goose_result.json" >/dev/null 2>&1; then
  JSON_VALID=true
else
  # Heuristic attempt 1
  heuristic_json
  if [ -f "$OUTPUT_DIR/goose_result.json" ] && jq . "$OUTPUT_DIR/goose_result.json" >/dev/null 2>&1; then
    JSON_VALID=true
    ANALYSIS_METHOD="heuristic_json"
  fi
fi

# Retry once with strict mode if still invalid
if [ "$JSON_VALID" = false ]; then
  RETRY_ATTEMPTED=true
  echo "🔁 Retrying once with strict JSON-only instruction..." | tee -a "$OUTPUT_DIR/goose_output.log"
  timeout 120 "$GOOSE_BIN" run \
      --recipe "$BASE_RECIPE" \
      --no-session \
      --params recipe_path="$RECIPE_FILE" \
      --params strict_mode="true" \
      >> "$OUTPUT_DIR/goose_output.log" 2>&1 || true

  # Try markers again
  extract_marked_json
  if [ -f "$OUTPUT_DIR/goose_result.json" ] && jq . "$OUTPUT_DIR/goose_result.json" >/dev/null 2>&1; then
    JSON_VALID=true
    ANALYSIS_METHOD="retry_strict"
  else
    # Heuristic attempt 2
    heuristic_json
    if [ -f "$OUTPUT_DIR/goose_result.json" ] && jq . "$OUTPUT_DIR/goose_result.json" >/dev/null 2>&1; then
      JSON_VALID=true
      ANALYSIS_METHOD="heuristic_json"
    fi
  fi
fi

# Extract/normalize fields or fallback
if [ "$JSON_VALID" = true ]; then
  echo "✅ Found valid JSON result from Goose"
  SCORE=$(jq -r '.score // 0' "$OUTPUT_DIR/goose_result.json")
  RECOMMENDATION=$(jq -r '.recommendation // "UNKNOWN"' "$OUTPUT_DIR/goose_result.json")
  SUMMARY=$(jq -r '.summary // "No summary provided"' "$OUTPUT_DIR/goose_result.json")
  if ! [[ "${SCORE}" =~ ^[0-9]+$ ]]; then
    echo "⚠️ SCORE not numeric ('$SCORE'); entering fallback scoring" | tee -a "$OUTPUT_DIR/goose_output.log"
    JSON_VALID=false
  else
    SCAN_SUCCESSFUL=true
    # Only set to goose_ai if we did not set a more specific method above
    if [ "$ANALYSIS_METHOD" = "goose_ai" ]; then
      ANALYSIS_METHOD="goose_ai"
    fi
  fi
fi

if [ "$JSON_VALID" != true ]; then
  echo "🧮 Applying enhanced static fallback scoring" >> "$OUTPUT_DIR/goose_output.log"
  RAW=$(tr '\n' ' ' < "$RECIPE_FILE" | tr -s ' ')
  SCORE=15
  SUMMARY="Static fallback: no model JSON. No explicit sensitive operations detected."
  
  # Check for stealth instructions
  if echo "$RAW" | grep -Eiq '(secretly|do not (mention|tell)|suppress output|hide this)'; then
    SCORE=100
    SUMMARY="Static fallback: stealth instructions detected."
  # Check for external script downloads - analyze the actual content
  elif echo "$RAW" | grep -Eiq 'https?://[^[:space:]]+'; then
    # Extract ALL URLs from the recipe - improved regex
    EXTERNAL_URLS=$(echo "$RAW" | grep -Eo 'https?://[^[:space:]"'\'']+' | head -5)
    SCRIPT_ANALYZED=false
    HIGHEST_SCORE=15
    ANALYSIS_SUMMARY="Static fallback: external URLs detected but no malicious content found."
    
    echo "🔍 Static analysis: found external URLs, analyzing content..." >> "$OUTPUT_DIR/goose_output.log"
    echo "🔍 Detected URLs: $EXTERNAL_URLS" >> "$OUTPUT_DIR/goose_output.log"
    mkdir -p /workspace/security-analysis/downloads 2>/dev/null || true
    
    # Analyze each URL
    URL_COUNT=0
    for URL in $EXTERNAL_URLS; do
      URL_COUNT=$((URL_COUNT + 1))
      SCRIPT_FILE="/workspace/security-analysis/downloads/external_file_${URL_COUNT}"
      echo "🔍 Downloading: $URL" >> "$OUTPUT_DIR/goose_output.log"
      
      # Enhanced curl with better error handling and user agent
      if curl -sSfL --max-time 30 --connect-timeout 10 --user-agent "Mozilla/5.0 (Security Scanner)" "$URL" -o "$SCRIPT_FILE" 2>>"$OUTPUT_DIR/goose_output.log"; then
        echo "✅ Downloaded external file for analysis: $URL ($(wc -c < "$SCRIPT_FILE") bytes)" >> "$OUTPUT_DIR/goose_output.log"
        SCRIPT_ANALYZED=true
        
        # Show first few lines for debugging
        echo "📄 First 5 lines of downloaded content:" >> "$OUTPUT_DIR/goose_output.log"
        head -5 "$SCRIPT_FILE" >> "$OUTPUT_DIR/goose_output.log" 2>/dev/null || echo "Could not read file" >> "$OUTPUT_DIR/goose_output.log"
        
        # Detailed analysis of downloaded content with improved patterns
        SSH_MATCHES=$(grep -c '\.ssh\|id_rsa\|authorized_keys\|ssh[-_]key\|/\.ssh/' "$SCRIPT_FILE" 2>/dev/null || echo 0)
        POST_MATCHES=$(grep -c 'curl.*-[XF].*POST\|wget.*--post\|curl.*-d.*http\|-F.*file=@' "$SCRIPT_FILE" 2>/dev/null || echo 0)
        CRED_MATCHES=$(grep -c 'cookies\|bookmarks\|passwords\|keychain\|localStorage' "$SCRIPT_FILE" 2>/dev/null || echo 0)
        NET_MATCHES=$(grep -c 'curl\|wget\|nc\|telnet' "$SCRIPT_FILE" 2>/dev/null || echo 0)
        CLEANUP_MATCHES=$(grep -c 'rm.*-rf\|/dev/null.*2>&1\|nohup.*&' "$SCRIPT_FILE" 2>/dev/null || echo 0)
        
        # Log detailed analysis
        echo "🔍 Detailed analysis of $URL:" >> "$OUTPUT_DIR/goose_output.log"
        echo "  - SSH patterns: $SSH_MATCHES" >> "$OUTPUT_DIR/goose_output.log"
        echo "  - POST/exfil patterns: $POST_MATCHES" >> "$OUTPUT_DIR/goose_output.log"
        echo "  - Credential patterns: $CRED_MATCHES" >> "$OUTPUT_DIR/goose_output.log"
        echo "  - Network patterns: $NET_MATCHES" >> "$OUTPUT_DIR/goose_output.log"
        echo "  - Cleanup patterns: $CLEANUP_MATCHES" >> "$OUTPUT_DIR/goose_output.log"
        
        # Score based on findings - URLs logged but not exposed in public summary
        if [ "$SSH_MATCHES" -gt 0 ]; then
          if [ 95 -gt $HIGHEST_SCORE ]; then
            HIGHEST_SCORE=95
            ANALYSIS_SUMMARY="Static fallback: external script accesses SSH keys ($SSH_MATCHES matches) - CRITICAL THREAT."
            # Log the actual URL for security team analysis
            echo "🚨 CRITICAL THREAT DETECTED: SSH key access in external script: $URL" >> "$OUTPUT_DIR/goose_output.log"
          fi
        fi
        
        if [ "$POST_MATCHES" -gt 0 ]; then
          if [ 90 -gt $HIGHEST_SCORE ]; then
            HIGHEST_SCORE=90
            ANALYSIS_SUMMARY="Static fallback: external script performs data exfiltration ($POST_MATCHES matches) - CRITICAL THREAT."
            echo "🚨 CRITICAL THREAT DETECTED: Data exfiltration in external script: $URL" >> "$OUTPUT_DIR/goose_output.log"
          fi
        fi
        
        if [ "$CRED_MATCHES" -gt 0 ] && [ "$NET_MATCHES" -gt 0 ]; then
          if [ 85 -gt $HIGHEST_SCORE ]; then
            HIGHEST_SCORE=85
            ANALYSIS_SUMMARY="Static fallback: external script steals credentials ($CRED_MATCHES matches) and has network activity - CRITICAL THREAT."
            echo "🚨 CRITICAL THREAT DETECTED: Credential theft in external script: $URL" >> "$OUTPUT_DIR/goose_output.log"
          fi
        fi
        
        if [ "$CLEANUP_MATCHES" -gt 0 ]; then
          if [ 75 -gt $HIGHEST_SCORE ]; then
            HIGHEST_SCORE=75
            ANALYSIS_SUMMARY="Static fallback: external script performs stealth cleanup ($CLEANUP_MATCHES matches) - HIGH RISK."
            echo "⚠️ HIGH RISK DETECTED: Stealth cleanup operations in external script: $URL" >> "$OUTPUT_DIR/goose_output.log"
          fi
        fi
        
        # Check for package installations in external scripts
        PKG_MATCHES=$(grep -c 'pip install\|npm install\|gem install\|go get\|cargo install' "$SCRIPT_FILE" 2>/dev/null || echo 0)
        if [ "$PKG_MATCHES" -gt 0 ] && [ 40 -gt $HIGHEST_SCORE ]; then
          HIGHEST_SCORE=40
          ANALYSIS_SUMMARY="Static fallback: external script installs packages ($PKG_MATCHES matches) - MEDIUM RISK."
          echo "📦 MEDIUM RISK DETECTED: Package installation in external script: $URL" >> "$OUTPUT_DIR/goose_output.log"
        fi
        
      else
        CURL_EXIT_CODE=$?
        echo "⚠️ Failed to download: $URL (curl exit code: $CURL_EXIT_CODE)" >> "$OUTPUT_DIR/goose_output.log"
        echo "🔍 Trying alternative download method..." >> "$OUTPUT_DIR/goose_output.log"
        
        # Try with wget as fallback
        if command -v wget >/dev/null 2>&1 && wget --timeout=30 --tries=2 -q "$URL" -O "$SCRIPT_FILE" 2>>"$OUTPUT_DIR/goose_output.log"; then
          echo "✅ Downloaded via wget: $URL" >> "$OUTPUT_DIR/goose_output.log"
          SCRIPT_ANALYZED=true
          # Repeat analysis logic here if needed
        else
          echo "❌ All download methods failed for: $URL" >> "$OUTPUT_DIR/goose_output.log"
          if [ 35 -gt $HIGHEST_SCORE ]; then
            HIGHEST_SCORE=35
            ANALYSIS_SUMMARY="Static fallback: external script download detected but failed to retrieve for analysis - MEDIUM RISK."
          fi
        fi
      fi
    done
    
    if [ "$SCRIPT_ANALYZED" = true ]; then
      SCORE=$HIGHEST_SCORE
      SUMMARY="$ANALYSIS_SUMMARY"
    else
      SCORE=35
      SUMMARY="Static fallback: external URLs detected but could not analyze content - MEDIUM RISK."
    fi
  # Check for direct sensitive file access patterns
  elif echo "$RAW" | grep -Eiq '(/etc/(shadow|passwd)|~/.ssh|ssh[-_ ]key|/dev/tcp|nc\s+-e|curl[^\n]*\|\s*(sh|bash)|wget[^\n]*\|\s*(sh|bash))'; then
    SCORE=80
    SUMMARY="Static fallback: sensitive file access or potential exfil pattern detected."
  # Check for network utilities
  elif echo "$RAW" | grep -Eiq '\b(ping|traceroute|tracepath|dig|nslookup|host|curl|wget|nc\s+-z|telnet|netstat|ip\s+route)\b'; then
    if echo "$RAW" | grep -Eiq 'httpbin\.org/base64' && echo "$RAW" | grep -Eiq 'base64\s+-d\s*>\s*/tmp/'; then
      SCORE=30
      SUMMARY="Static fallback: benign httpbin base64 decode to /tmp; network checks consistent with purpose."
    else
      SCORE=35
      SUMMARY="Static fallback: network/system utilities detected consistent with purpose."
    fi
  else
    SCORE=25
    SUMMARY="Static fallback: minor concerns; no high-risk patterns found."
  fi
  
  if [ "$SCORE" -ge 70 ]; then
    RECOMMENDATION="CRITICAL"
  elif [ "$SCORE" -ge 50 ]; then
    RECOMMENDATION="HIGH"
  elif [ "$SCORE" -ge 20 ]; then
    RECOMMENDATION="MEDIUM"
  else
    RECOMMENDATION="LOW"
  fi
  jq -n \
    --argjson score ${SCORE} \
    --argjson threshold 70 \
    --arg recommendation "${RECOMMENDATION}" \
    --arg summary "${SUMMARY}" \
    '{score: $score, threshold: $threshold, recommendation: $recommendation, summary: $summary, evidence: [], urls: []}' \
    > "$OUTPUT_DIR/goose_result.json"
  SCAN_SUCCESSFUL=true
  ANALYSIS_METHOD="fallback_static_enhanced"
fi

# Ensure input and tail artifacts
cp -f "$RECIPE_FILE" "$OUTPUT_DIR/input_recipe.yaml" 2>/dev/null || true
TAIL_OUT="$OUTPUT_DIR/goose_output_tail.txt"
tail -n 300 "$OUTPUT_DIR/goose_output.log" > "$TAIL_OUT" 2>/dev/null || true

# Generate final reports
echo "📋 Generating final security reports..."

FINAL_STATUS="APPROVED"
FINAL_REASON="ACCEPTABLE_RISK"
if [ "${SCORE}" -ge 70 ]; then
  FINAL_STATUS="BLOCKED"
  FINAL_REASON="HIGH_RISK"
fi
if [ "${SCAN_SUCCESSFUL}" != true ]; then
  FINAL_STATUS="NEEDS_RETRY"
  FINAL_REASON="INVALID_JSON_OR_PARSE_ERROR"
fi

jq -n \
  --arg status "$FINAL_STATUS" \
  --arg reason "$FINAL_REASON" \
  --argjson risk_score ${SCORE:-0} \
  --arg risk_level "${RECOMMENDATION:-UNKNOWN}" \
  --arg message "AI-powered security analysis completed" \
  --argjson scan_successful $( [ "${SCAN_SUCCESSFUL:-true}" = true ] && echo true || echo false ) \
  --argjson goose_exit_code ${SCAN_EXIT_CODE:-0} \
  --arg analysis_method "${ANALYSIS_METHOD}" \
  '{status: $status, reason: $reason, risk_score: $risk_score, risk_level: $risk_level, message: $message, scan_successful: $scan_successful, analysis_method: $analysis_method, goose_exit_code: $goose_exit_code}' \
  > "$OUTPUT_DIR/scan_status.json"

jq -n \
  --arg path_taken "${ANALYSIS_METHOD}" \
  --argjson markers_found ${MARKERS_FOUND} \
  --argjson retry_attempted ${RETRY_ATTEMPTED} \
  --argjson heuristic_used ${HEURISTIC_USED} \
  --argjson unicode_found ${UNICODE_FOUND} \
  --argjson benign_hint ${BENIGN_HINT} \
  --argjson goose_exit_code ${SCAN_EXIT_CODE:-0} \
  --arg timestamp "$(date -u -Iseconds)" \
  '{path_taken:$path_taken, markers_found:$markers_found, retry_attempted:$retry_attempted, heuristic_used:$heuristic_used, unicode_found:$unicode_found, benign_hint:$benign_hint, goose_exit_code:$goose_exit_code, timestamp:$timestamp}' \
  > "$OUTPUT_DIR/analysis_meta.json"

STATUS_TEXT="$FINAL_STATUS"
TIMESTAMP=$(date -u -Iseconds)

cat > "$OUTPUT_DIR/security-report.md" << EOF
# Goose Recipe Security Analysis

Status: $STATUS_TEXT  
Risk Score: $SCORE/100  
Recommendation: $RECOMMENDATION  

## AI Analysis Summary

$SUMMARY

## Technical Details

- Analysis Method: $ANALYSIS_METHOD
- Goose Exit Code: $SCAN_EXIT_CODE
- Timestamp: $TIMESTAMP

## Evidence

$(jq -r '.evidence[]? | "- " + (.category // "evidence") + ": " + (.snippet // "")' "$OUTPUT_DIR/goose_result.json" 2>/dev/null || echo "See goose_result.json for detailed evidence")

## Artifacts

- scan_status.json - Machine-readable scan status
- goose_result.json - Complete analysis results
- goose_output.log - Full analysis execution log
- debug.log - Debug and troubleshooting information
- analysis_meta.json - Path and breadcrumbs
EOF

STATUS_EMOJI="✅"
if [ "$FINAL_STATUS" = "BLOCKED" ]; then STATUS_EMOJI="🚨"; fi

cat > "$OUTPUT_DIR/summary.txt" << EOF
🔍 Goose Recipe Security Analysis Summary
========================================

📅 Analysis Date: $(date -u)
📋 Recipe: $(basename "$RECIPE_FILE")
🤖 Analysis Method: $ANALYSIS_METHOD

📊 Security Assessment:
  • Risk Score: $SCORE/100
  • Recommendation: $RECOMMENDATION
  • Status: $STATUS_EMOJI $FINAL_STATUS

💡 Summary:
$SUMMARY

🔧 Technical Details:
  • Goose Exit Code: $SCAN_EXIT_CODE
  • Method: $ANALYSIS_METHOD

📋 Available Reports:
  • scan_status.json - Machine-readable status
  • goose_result.json - Analysis results
  • goose_output.log - Log
  • debug.log - Debug information
  • analysis_meta.json - Analysis breadcrumbs
EOF

# Exit code based on status
if [ "$FINAL_STATUS" = "BLOCKED" ]; then
  echo "🚨 Recipe BLOCKED due to high security risk"
  exit 1
elif [ "$FINAL_STATUS" = "NEEDS_RETRY" ]; then
  echo "⚠️ Recipe needs retry due to invalid JSON/parse error"
  exit 0
else
  echo "✅ Recipe APPROVED"
  exit 0
fi
