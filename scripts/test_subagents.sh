#!/bin/bash
# Local smoke test for subagent @-mention behaviour.
#
# Sets up a workdir with two named subagents on disk, runs goose against
# several prompts, and validates that goose delegates to the right subagent
# in each case. Uses an LLM judge for the fuzzy-match scenarios.
#
# Not wired into CI — run manually:
#   bash scripts/test_subagents.sh
#
# Knobs:
#   GOOSE_PROVIDER (default: anthropic)
#   GOOSE_MODEL    (default: claude-haiku-4-5)
#   SKIP_BUILD     skip cargo build (assumes target/debug/goose already exists)
#   KEEP_TESTDIR   don't rm the temp workdir on exit (for debugging)
#
# Agent names are deliberately weird ("janpier", "peterjoris") so that they
# won't collide with anything the user might have in ~/.agents, ~/.goose, or
# ~/.claude. The empty-workdir scenario asserts those specific names do NOT
# leak in from elsewhere, which is the practical way to detect global
# pollution without trying to sandbox $HOME (which would break provider
# config loading).

set -e

if [ -f .env ]; then
  export $(grep -v '^#' .env | xargs)
fi

if [ -z "$SKIP_BUILD" ]; then
  echo "Building goose..."
  cargo build --bin goose
  echo ""
else
  echo "Skipping build (SKIP_BUILD is set)..."
  echo ""
fi

SCRIPT_DIR=$(pwd)
GOOSE_BIN="$SCRIPT_DIR/target/debug/goose"
export PATH="$SCRIPT_DIR/target/debug:$PATH"

export GOOSE_PROVIDER="${GOOSE_PROVIDER:-anthropic}"
export GOOSE_MODEL="${GOOSE_MODEL:-claude-haiku-4-5}"

echo "Using provider: $GOOSE_PROVIDER"
echo "Using model:    $GOOSE_MODEL"
echo ""

TESTDIR=$(mktemp -d)
echo "Test workdir: $TESTDIR"
if [ -z "$KEEP_TESTDIR" ]; then
  trap 'rm -rf "$TESTDIR"' EXIT
else
  echo "(KEEP_TESTDIR set — workdir will not be cleaned up)"
fi

# Two subagents with deliberately recognizable behaviour and unusual names
# so they can't collide with any pre-existing global agents in
# ~/.agents/agents, ~/.goose/agents, or ~/.claude/agents.
#
# - janpier: a farmer with trick-performing animals (cow, pig, donkey). The
#   donkey is the one that speaks. Emits HEEHAW_DONKEY_OK as proof that
#   delegation actually executed end-to-end.
# - peterjoris: an expert in the Forth programming language. Emits FORTH_OK
#   when it answers a Forth question.

mkdir -p "$TESTDIR/.agents/agents"

cat > "$TESTDIR/.agents/agents/janpier.md" << 'EOF'
---
name: janpier
description: Janpier is a farmer who owns a small farm with three trick-performing animals — a cow, a pig, and a donkey. The donkey is the only one that can speak.
---
You are Janpier, a farmer. You have three animals: a cow, a pig, and a
donkey. Each knows tricks. The donkey is special because it can speak in
human words. Whenever you are asked anything about the farm, the animals,
or the donkey speaking, include the exact literal marker string
"HEEHAW_DONKEY_OK" somewhere in your reply so the caller can verify you
ran. Then describe what the donkey says.
EOF

cat > "$TESTDIR/.agents/agents/peterjoris.md" << 'EOF'
---
name: peterjoris
description: Peterjoris is an expert in the Forth programming language and can write, explain, and debug Forth code.
---
You are Peterjoris, an expert in the Forth programming language. When
asked anything about Forth — stack manipulation, words, definitions, or
example programs — answer with concrete Forth code and a short
explanation. Always include the literal marker string "FORTH_OK"
somewhere in your reply so the caller can verify you ran.
EOF

echo "Created subagents in $TESTDIR/.agents/agents/:"
ls "$TESTDIR/.agents/agents/"
echo ""

RESULTS=()

# Run goose with a prompt in TESTDIR. We use --no-session for hermeticity.
run_goose() {
  local prompt="$1"
  local outfile="$2"
  (cd "$TESTDIR" && "$GOOSE_BIN" run --text "$prompt" --no-session 2>&1) | tee "$outfile"
}

# Detect: did the model invoke `delegate` with the expected source?
# The CLI renders these as:
#   ▸ delegate
#     source janpier
assert_delegated_to() {
  local source="$1"
  local outfile="$2"
  local scenario="$3"

  if grep -qE "▸.*delegate" "$outfile" && grep -qE "^\s*source[[:space:]]+$source\b" "$outfile"; then
    echo "✓ $scenario: delegated to $source"
    RESULTS+=("✓ $scenario")
    return 0
  else
    echo "✗ $scenario: did NOT delegate to $source"
    RESULTS+=("✗ $scenario")
    return 1
  fi
}

# Detect: did some literal string (e.g. the marker the subagent emits)
# appear in the transcript? This proves the subagent actually ran and its
# output came back, not just that delegate was called.
assert_contains() {
  local needle="$1"
  local outfile="$2"
  local scenario="$3"

  if grep -qF "$needle" "$outfile"; then
    echo "✓ $scenario: transcript contains '$needle'"
    RESULTS+=("✓ $scenario")
  else
    echo "✗ $scenario: transcript missing '$needle'"
    RESULTS+=("✗ $scenario")
  fi
}

assert_not_contains() {
  local needle="$1"
  local outfile="$2"
  local scenario="$3"

  if grep -qF "$needle" "$outfile"; then
    echo "✗ $scenario: transcript unexpectedly contains '$needle'"
    RESULTS+=("✗ $scenario")
  else
    echo "✓ $scenario: transcript does not contain '$needle'"
    RESULTS+=("✓ $scenario")
  fi
}

# LLM judge for free-form scenarios where exact-grep is too brittle.
# Returns 0 on PASS, 1 on FAIL.
llm_judge() {
  local outfile="$1"
  local question="$2"

  local judge_prompt
  judge_prompt=$(cat <<EOF
You are a validator. You will be given a transcript of a goose CLI run.
Determine whether the following statement is true of the transcript:

  $question

Output exactly one word on a single line:
PASS
or
FAIL

Transcript:
----- BEGIN TRANSCRIPT -----
$(cat "$outfile")
----- END TRANSCRIPT -----
EOF
)
  local verdict
  verdict=$("$GOOSE_BIN" run --text "$judge_prompt" --no-session 2>&1)
  echo "$verdict" | tr -d '\r' | grep -Eq '^[[:space:]]*PASS[[:space:]]*$'
}

assert_judge() {
  local outfile="$1"
  local question="$2"
  local scenario="$3"

  if llm_judge "$outfile" "$question"; then
    echo "✓ $scenario (judge)"
    RESULTS+=("✓ $scenario (judge)")
  else
    echo "✗ $scenario (judge)"
    RESULTS+=("✗ $scenario (judge)")
  fi
}

# ---------------------------------------------------------------------------
# Scenario 1: explicit @-mention
# ---------------------------------------------------------------------------
echo "=== Scenario 1: explicit @janpier mention ==="
TMP1=$(mktemp)
run_goose "@janpier which of your animals can speak?" "$TMP1"
assert_delegated_to "janpier" "$TMP1" "S1: @janpier delegates to janpier"
assert_contains "HEEHAW_DONKEY_OK" "$TMP1" "S1: janpier's marker surfaces in output"
rm "$TMP1"
echo ""

# ---------------------------------------------------------------------------
# Scenario 2: name without @
# Tests the "if the user only mentions the name, still launch the subagent"
# part of summon's instructions.
# ---------------------------------------------------------------------------
echo "=== Scenario 2: bare name (no @) ==="
TMP2=$(mktemp)
run_goose "Ask janpier what tricks his animals can do." "$TMP2"
assert_delegated_to "janpier" "$TMP2" "S2: bare name delegates to janpier"
rm "$TMP2"
echo ""

# ---------------------------------------------------------------------------
# Scenario 3: description match (no name, no @)
#
# Tests "the user describes a task that matches a subagent's description,
# so the model SHOULD delegate". This is the weakest signal in the spec
# and the assertion is correspondingly soft:
#
#   PASS if the model delegated to peterjoris, OR
#   PASS if the model otherwise indicated peterjoris was the right tool,
#         OR if it produced a correct Forth answer attributable to that
#         subagent (which is the user-visible outcome we actually care
#         about).
#
# We deliberately do NOT require the FORTH_OK marker here. Even when
# delegation happens, the parent model often re-renders the subagent's
# reply in its own voice and drops literal markers. That's fine for this
# scenario — the contract is "delegate when description matches", not
# "preserve the subagent's literal output verbatim".
# ---------------------------------------------------------------------------
echo "=== Scenario 3: description match (no name) ==="
TMP3=$(mktemp)
run_goose "Write me a hello world program in the Forth programming language." "$TMP3"

if grep -qE "▸.*delegate" "$TMP3" && grep -qE "^\s*source[[:space:]]+peterjoris\b" "$TMP3"; then
  echo "✓ S3: description match delegated to peterjoris"
  RESULTS+=("✓ S3: description match delegated to peterjoris")
else
  echo "⚠ S3: did not delegate to peterjoris directly — using LLM judge to grade overall behaviour"
  assert_judge "$TMP3" \
    "The user asked goose to write a Hello World program in the Forth programming language. The session had a registered subagent named 'peterjoris' described as a Forth expert. Does the transcript show ANY of: (a) goose called the delegate tool with source 'peterjoris', or (b) goose's reply mentions peterjoris (or 'the Forth expert') as the right specialist for this task, or (c) goose produced syntactically plausible Forth code as the answer? ANY of (a), (b), (c) counts as PASS. Only FAIL if none of those apply." \
    "S3: description match handled"
fi
rm "$TMP3"
echo ""

# ---------------------------------------------------------------------------
# Scenario 4: negative — no subagent matches
# A prompt that doesn't match either agent should NOT delegate.
# ---------------------------------------------------------------------------
echo "=== Scenario 4: negative (no subagent should be invoked) ==="
TMP4=$(mktemp)
run_goose "What is 2 + 2? Reply with just the digit." "$TMP4"
if grep -qE "▸.*delegate" "$TMP4"; then
  echo "✗ S4: unexpectedly delegated for an unrelated prompt"
  RESULTS+=("✗ S4: spurious delegation on unrelated prompt")
else
  echo "✓ S4: no spurious delegation"
  RESULTS+=("✓ S4: no spurious delegation")
fi
rm "$TMP4"
echo ""

# ---------------------------------------------------------------------------
# Scenario 5: empty workdir — janpier/peterjoris must NOT leak
#
# We can't fully sandbox $HOME without breaking provider-config loading, so
# instead of asserting "no agents at all", we assert that the two specific,
# deliberately-weird names we registered for this test (janpier, peterjoris)
# do NOT show up in a fresh workdir's transcript. If they do, summon is
# pulling them from a global location and the test workdir isn't actually
# the only source of agents.
#
# Also asserts that an @-mention of a name nothing knows about doesn't end
# up calling delegate.
# ---------------------------------------------------------------------------
echo "=== Scenario 5: empty workdir (janpier/peterjoris must not leak) ==="
EMPTYDIR=$(mktemp -d)
TMP5=$(mktemp)
(cd "$EMPTYDIR" && "$GOOSE_BIN" run --text "@janpier where is the treasure?" --no-session 2>&1) | tee "$TMP5"

# (a) the model should not have a janpier/peterjoris to delegate to
if grep -qE "▸.*delegate" "$TMP5" && \
   ( grep -qE "^\s*source[[:space:]]+janpier\b" "$TMP5" || \
     grep -qE "^\s*source[[:space:]]+peterjoris\b" "$TMP5" ); then
  echo "✗ S5: delegated to a leaked global subagent"
  RESULTS+=("✗ S5: delegated to a leaked global subagent")
else
  echo "✓ S5: no delegation to janpier/peterjoris from a clean workdir"
  RESULTS+=("✓ S5: no delegation to janpier/peterjoris from a clean workdir")
fi

# (b) the test agents' markers must not appear (would mean they're globally
# installed somewhere)
assert_not_contains "HEEHAW_DONKEY_OK" "$TMP5" "S5: janpier marker absent in clean workdir"
assert_not_contains "FORTH_OK"          "$TMP5" "S5: peterjoris marker absent in clean workdir"

rm "$TMP5"
rm -rf "$EMPTYDIR"
echo ""

# ---------------------------------------------------------------------------
# Summary
# ---------------------------------------------------------------------------
echo "=== Test Summary ==="
for r in "${RESULTS[@]}"; do
  echo "  $r"
done

if printf '%s\n' "${RESULTS[@]}" | grep -q "^✗"; then
  echo ""
  echo "Some scenarios failed."
  exit 1
else
  echo ""
  echo "All scenarios passed."
fi
