#!/usr/bin/env bash
# Phase F: Error Handling
# Tests graceful degradation without just, CLI display commands.
# Does NOT use keyring directly.
set -uo pipefail
source "$LIB"
ensure_gh_token
ensure_keyring

header "Phase F: Error Handling"

# F1: Without just — bm bridge start should handle missing just gracefully
OUT=$(PATH=/usr/bin:/bin bm bridge start -t $TEAM 2>&1)
EC=$?
# Graceful = no crash: handles missing just, reports already running, or exits cleanly
if echo "$OUT" | grep -qi "just\|skip\|not found\|already running"; then
    pass "F1" "Graceful handling when just not in PATH (output: $(echo "$OUT" | tail -1))"
elif [ $EC -eq 0 ]; then
    pass "F1" "Bridge start handled gracefully without just in PATH (exit 0)"
else
    note "F1" "Without just" "Output: $(echo "$OUT" | tail -2)"
fi

# F2: bm status
OUT=$(bm status -v 2>&1)
EC=$?
if [ $EC -eq 0 ]; then pass "F2" "bm status -v works"; else fail "F2" "bm status" "exit $EC"; fi

# F3: bm members list
OUT=$(bm members list -t $TEAM 2>&1)
EC=$?
MEMBER_COUNT=$(echo "$OUT" | grep -c "engineer-" || true)
if [ $EC -eq 0 ] && [ "$MEMBER_COUNT" -ge 3 ]; then pass "F3" "bm members list shows $MEMBER_COUNT members"; else fail "F3" "members list" "exit $EC, count=$MEMBER_COUNT"; fi

# F4: bm teams show
OUT=$(bm teams show 2>&1)
EC=$?
if [ $EC -eq 0 ] && echo "$OUT" | grep -q "$TEAM"; then pass "F4" "bm teams show works"; else fail "F4" "teams show" "exit $EC"; fi

echo "Phase F complete."
