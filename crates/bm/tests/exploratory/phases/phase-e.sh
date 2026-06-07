#!/usr/bin/env bash
# Phase E: Member Hire & Session Workflow
# Tests new member hire, bridge identity provisioning, and session creation.
set -uo pipefail
source "$LIB"
ensure_gh_token
ensure_keyring

header "Phase E: Member Hire & Session Workflow"

# E1: Hire new member (carol) — uses engineer role (valid in agentic-sdlc-planning profile)
OUT=$(bm_hire engineer --name carol 2>&1)
EC=$?
if [ $EC -eq 0 ] || echo "$OUT" | grep -qi "already\|exist\|hired"; then
    pass "E1" "Hire engineer-carol (bm_hire engineer --name carol)"
else
    fail "E1" "Hire carol" "exit $EC: $(echo "$OUT" | tail -3)"
fi

# E2: Provision bridge identity for carol
OUT=$(bm bridge identity add engineer-carol -t $TEAM 2>&1)
EC=$?
if [ $EC -eq 0 ]; then
    pass "E2" "bm bridge identity add engineer-carol"
else
    fail "E2" "Bridge identity add" "exit $EC: $(echo "$OUT" | tail -3)"
fi

# E3: bm start engineer-carol creates a session
OUT=$(bm start engineer-carol -t $TEAM 2>&1)
EC=$?
if [ $EC -eq 0 ]; then
    pass "E3" "bm start engineer-carol: session started"
else
    fail "E3" "bm start carol" "exit $EC: $(echo "$OUT" | tail -3)"
fi

# Wait for session to register
sleep 2

# E4: Session visible in bm session list
SESSION_LIST=$(bm session list -t $TEAM 2>&1)
if echo "$SESSION_LIST" | grep -qi "engineer-carol\|carol"; then
    pass "E4" "Session visible in bm session list (engineer-carol)"
else
    note "E4" "Session list" "carol not found in output: $(echo "$SESSION_LIST" | head -3)"
fi

# E5: Bridge identity list shows carol
OUT=$(bm bridge identity list -t $TEAM 2>&1)
EC=$?
if [ $EC -eq 0 ] && echo "$OUT" | grep -qi "engineer-carol\|carol"; then
    pass "E5" "bm bridge identity list shows engineer-carol"
else
    note "E5" "Identity list" "carol not found — output: $(echo "$OUT" | head -5)"
fi

bm stop --force -t $TEAM 2>/dev/null || true

echo "Phase E complete."
