#!/bin/sh
# Model-checks ToolDiscovery.tla with TLC, one rule at a time, and compares
# each result with EXPECTED. Exits 0 when every rule matches.
#
#   check.sh                 check the model in this folder
#   check.sh MUTANT.tla      check another copy of the model (its MODULE line
#                            must still say ToolDiscovery)
#   check.sh --install       download TLC v1.7.4 into the cache and verify it
#
# TLA2TOOLS overrides the path to tla2tools.jar, JAVA the java binary
# (Java 11 or later). Each run's TLC output is kept in $OUT, a temp folder
# unless set.
set -eu

here=$(cd "$(dirname "$0")" && pwd)
version=1.7.4
sha=936a262061c914694dfd669a543be24573c45d5aa0ff20a8b96b23d01e050e88
url=https://github.com/tlaplus/tlaplus/releases/download/v$version/tla2tools.jar
cached=${XDG_CACHE_HOME:-$HOME/.cache}/goose-verify/tla2tools-$version.jar

# A rule that breaks here is a known finding, not a regression. Change a line
# only after the code changed on purpose and the skill says why.
EXPECTED="
EveryRequestFits breaks
NoLoadPushesPastLimit holds
LoadsOnlyAppend holds
LoadedToolsAreSent holds
UnsentToolsNeverRun holds
LoadsAreDistinct holds
"

if [ "${1:-}" = "--install" ]; then
    mkdir -p "$(dirname "$cached")"
    curl -fsSL -o "$cached.part" "$url"
    echo "$sha  $cached.part" | shasum -a 256 -c - >/dev/null
    mv "$cached.part" "$cached"
    echo "TLC $version installed at $cached"
    exit 0
fi

jar=${TLA2TOOLS:-$cached}
if [ ! -f "$jar" ]; then
    echo "TLC not found at $jar. Run '$0 --install', or set TLA2TOOLS." >&2
    exit 2
fi
if ! echo "$sha  $jar" | shasum -a 256 -c - >/dev/null 2>&1; then
    echo "$jar is not tla2tools.jar v$version (sha256 $sha)." >&2
    exit 2
fi
java=${JAVA:-java}

spec=${1:-$here/ToolDiscovery.tla}
out=${OUT:-$(mktemp -d)}
mkdir -p "$out"
cp "$spec" "$out/ToolDiscovery.tla"

status=0
echo "$EXPECTED" | while read -r rule expected; do
    [ -n "$rule" ] || continue
    sed '/^INVARIANTS/,$d' "$here/ToolDiscovery.cfg" > "$out/$rule.cfg"
    printf 'INVARIANT %s\n' "$rule" >> "$out/$rule.cfg"
    (cd "$out" && "$java" -XX:+UseParallelGC -jar "$jar" -workers auto \
        -cleanup -config "$rule.cfg" ToolDiscovery.tla > "$rule.out" 2>&1) || true
    if grep -q "Invariant $rule is violated" "$out/$rule.out"; then
        steps=$(grep -c '^State [0-9]*: <' "$out/$rule.out")
        actual=breaks
        detail="shortest path: $steps states, $out/$rule.out"
    elif grep -q "No error has been found" "$out/$rule.out"; then
        actual=holds
        detail=$(grep 'distinct states found' "$out/$rule.out" | sed 's/.*generated, //')
    else
        actual=error
        detail="TLC failed, see $out/$rule.out"
    fi
    mark=ok
    [ "$actual" = "$expected" ] || mark=UNEXPECTED
    printf '%-10s %-22s %-6s (expected %s) %s\n' "$mark" "$rule" "$actual" "$expected" "$detail"
    [ "$mark" = ok ] || echo fail >> "$out/.failed"
done
[ -f "$out/.failed" ] && status=1
exit $status
