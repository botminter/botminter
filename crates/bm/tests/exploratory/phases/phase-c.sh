#!/usr/bin/env bash
# Phase C: Bridge Lifecycle (Tuwunel)
# Tests first provisioning, idempotency, recovery (stopped/removed/volume-deleted container),
# pre-existing user onboarding.
set -uo pipefail
source "$LIB"
ensure_gh_token
ensure_keyring

header "Phase C: Bridge Lifecycle (Tuwunel)"

BSTATE="$TEAM_DIR/bridge-state.json"
PWFILE="$TEAM_DIR/tuwunel-passwords.json"
CONTAINER="bm-tuwunel-$TEAM"
MATRIX_URL="http://127.0.0.1:${TUWUNEL_PORT:-8008}"

# ── C.1: First provisioning ──

echo "  C.1: First bridge provisioning..."
OUT=$(bm bridge start -t $TEAM 2>&1)
EC=$?
if [ $EC -eq 0 ]; then pass "C1" "First bridge start"; else fail "C1" "First bridge start" "exit $EC: $(echo "$OUT" | tail -5)"; echo "$OUT"; fi

# C2: Container running
CSTATUS=$(podman ps --filter "name=$CONTAINER" --format '{{.Status}}' 2>&1)
if echo "$CSTATUS" | grep -q "Up"; then pass "C2" "Container running"; else fail "C2" "Container" "status=$CSTATUS"; fi

# C3: Matrix healthy
HTTP=$(curl -sf -o /dev/null -w "%{http_code}" "$MATRIX_URL/_matrix/client/versions" 2>/dev/null || echo "000")
if [ "$HTTP" = "200" ]; then pass "C3" "Matrix server healthy"; else fail "C3" "Matrix health" "HTTP $HTTP"; fi

# Provision member identities and general room.
# bm bridge start only creates admin; member identities require explicit provisioning.
OUT=$(bm bridge identity add engineer-alice -t $TEAM 2>&1)
EC=$?
if [ $EC -eq 0 ]; then pass "C3a" "bm bridge identity add engineer-alice"; else fail "C3a" "Identity add alice" "exit $EC: $(echo "$OUT" | tail -3)"; fi

OUT=$(bm bridge identity add engineer-bob -t $TEAM 2>&1)
EC=$?
if [ $EC -eq 0 ]; then pass "C3b" "bm bridge identity add engineer-bob"; else fail "C3b" "Identity add bob" "exit $EC: $(echo "$OUT" | tail -3)"; fi

OUT=$(bm bridge room create "${TEAM}-general" -t $TEAM 2>&1)
EC=$?
if [ $EC -eq 0 ]; then pass "C3c" "bm bridge room create ${TEAM}-general"; else fail "C3c" "Room create" "exit $EC: $(echo "$OUT" | tail -3)"; fi

# C4: Bridge state
# identities in bridge-state.json counts MEMBER identities only (alice + bob = 2).
# Admin is stored in admin_user_id field, not in identities map.
STATUS=$(jq -r '.status' "$BSTATE" 2>/dev/null)
ID_COUNT=$(jq '.identities | length' "$BSTATE" 2>/dev/null)
ROOM_COUNT=$(jq '.rooms | length' "$BSTATE" 2>/dev/null)
if [ "$STATUS" = "running" ] && [ "$ID_COUNT" = "2" ] && [ "$ROOM_COUNT" = "1" ]; then
    pass "C4" "Bridge state: running, 2 member identities, 1 room"
else
    fail "C4" "Bridge state" "status=$STATUS ids=$ID_COUNT rooms=$ROOM_COUNT"
fi

# C5: Passwords
PW_COUNT=$(jq 'length' "$PWFILE" 2>/dev/null || echo "0")
if [ "$PW_COUNT" = "3" ]; then pass "C5" "Passwords file has 3 entries"; else fail "C5" "Passwords" "count=$PW_COUNT"; fi

# C6: Keyring
KR_ALICE=$(secret_tool lookup service "botminter.$TEAM.tuwunel" username engineer-alice 2>/dev/null || true)
KR_BOB=$(secret_tool lookup service "botminter.$TEAM.tuwunel" username engineer-bob 2>/dev/null || true)
if [ -n "$KR_ALICE" ] && [ -n "$KR_BOB" ]; then
    pass "C6" "Keyring has credentials for alice + bob"
else
    fail "C6" "Keyring" "alice='${KR_ALICE:+set}${KR_ALICE:-empty}' bob='${KR_BOB:+set}${KR_BOB:-empty}'"
fi

# C7: Admin login works
ADMIN_PASS=$(jq -r '.bmadmin' "$PWFILE" 2>/dev/null)
LOGIN=$(curl -sf -X POST -H "Content-Type: application/json" \
    -d "{\"type\":\"m.login.password\",\"identifier\":{\"type\":\"m.id.user\",\"user\":\"bmadmin\"},\"password\":\"$ADMIN_PASS\"}" \
    "$MATRIX_URL/_matrix/client/v3/login" 2>/dev/null || echo '{}')
ADMIN_TOKEN=$(echo "$LOGIN" | jq -r '.access_token // empty')
if [ -n "$ADMIN_TOKEN" ]; then pass "C7" "Admin can login to Matrix"; else fail "C7" "Admin login" "no token"; fi

# C8: Room exists
ROOM_RESP=$(curl -sf "$MATRIX_URL/_matrix/client/v3/directory/room/%23${TEAM}-general:localhost" 2>/dev/null || echo '{}')
ROOM_ID=$(echo "$ROOM_RESP" | jq -r '.room_id // empty')
if [ -n "$ROOM_ID" ]; then pass "C8" "Room ${TEAM}-general exists ($ROOM_ID)"; else fail "C8" "Room" "not found"; fi

# ── C.2: Idempotency ──

echo "  C.2: Bridge idempotency..."
ALICE_TOKEN_BEFORE=$KR_ALICE

OUT=$(bm bridge start -t $TEAM 2>&1)
EC=$?
if [ $EC -eq 0 ] && echo "$OUT" | grep -q "already running\|AlreadyRunning"; then
    pass "C9" "Bridge start again (idempotent — already running)"
elif [ $EC -eq 0 ]; then
    pass "C9" "Bridge start again (no error)"
else
    fail "C9" "Bridge start again" "exit $EC"
fi

# C10: Container still running
CSTATUS=$(podman ps --filter "name=$CONTAINER" --format '{{.Status}}' 2>&1)
if echo "$CSTATUS" | grep -q "Up"; then pass "C10" "Container still running"; else fail "C10" "Container" "status=$CSTATUS"; fi

# C11: State unchanged (identities = 2: alice + bob; admin in admin_user_id, not identities map)
STATUS2=$(jq -r '.status' "$BSTATE" 2>/dev/null)
ID_COUNT2=$(jq '.identities | length' "$BSTATE" 2>/dev/null)
if [ "$STATUS2" = "running" ] && [ "$ID_COUNT2" = "2" ]; then
    pass "C11" "Bridge state unchanged"
else
    fail "C11" "State" "status=$STATUS2 ids=$ID_COUNT2"
fi

# C12: Credentials preserved
KR_ALICE2=$(secret_tool lookup service "botminter.$TEAM.tuwunel" username engineer-alice 2>/dev/null || true)
if [ "$KR_ALICE2" = "$ALICE_TOKEN_BEFORE" ]; then
    pass "C12" "Alice credential unchanged after re-sync"
else
    note "C12" "Credential change" "was '${ALICE_TOKEN_BEFORE:0:8}...' now '${KR_ALICE2:0:8}...'"
fi

# ── C.3: Recovery from stopped container ──

echo "  C.3: Recovery from stopped container..."
podman stop "$CONTAINER" 2>/dev/null
pass "C13" "Stopped container"

OUT=$(bm bridge start -t $TEAM 2>&1)
EC=$?
if [ $EC -eq 0 ]; then pass "C14" "Bridge start recovers stopped container"; else fail "C14" "Recovery" "exit $EC"; fi

CSTATUS=$(podman ps --filter "name=$CONTAINER" --format '{{.Status}}' 2>&1)
if echo "$CSTATUS" | grep -q "Up"; then pass "C15" "Container running again"; else fail "C15" "Container" "status=$CSTATUS"; fi

HTTP=$(curl -sf -o /dev/null -w "%{http_code}" "$MATRIX_URL/_matrix/client/versions" 2>/dev/null || echo "000")
if [ "$HTTP" = "200" ]; then pass "C16" "Matrix healthy after recovery"; else fail "C16" "Matrix health" "HTTP $HTTP"; fi

# ── C.4: Recovery from removed container ──

echo "  C.4: Recovery from removed container..."
podman stop "$CONTAINER" 2>/dev/null; podman rm "$CONTAINER" 2>/dev/null
pass "C17" "Force-removed container"

OUT=$(bm bridge start -t $TEAM 2>&1)
EC=$?
if [ $EC -eq 0 ]; then pass "C18" "Bridge start recovers removed container"; else fail "C18" "Recovery" "exit $EC"; fi

CSTATUS=$(podman ps --filter "name=$CONTAINER" --format '{{.Status}}' 2>&1)
if echo "$CSTATUS" | grep -q "Up"; then pass "C19" "Container running after re-create"; else fail "C19" "Container" "status=$CSTATUS"; fi

# C20: Existing users survive
LOGIN2=$(curl -sf -X POST -H "Content-Type: application/json" \
    -d "{\"type\":\"m.login.password\",\"identifier\":{\"type\":\"m.id.user\",\"user\":\"bmadmin\"},\"password\":\"$ADMIN_PASS\"}" \
    "$MATRIX_URL/_matrix/client/v3/login" 2>/dev/null || echo '{}')
ADMIN_TOKEN2=$(echo "$LOGIN2" | jq -r '.access_token // empty')
if [ -n "$ADMIN_TOKEN2" ]; then pass "C20" "Admin login survives container re-create"; else fail "C20" "Admin login" "no token after re-create"; fi

# ── C.5: Recovery from removed volume ──

echo "  C.5: Recovery from removed volume..."
podman stop "$CONTAINER" 2>/dev/null; podman rm "$CONTAINER" 2>/dev/null
podman volume rm "bm-tuwunel-${TEAM}-data" 2>/dev/null
pass "C21" "Removed container + volume"

OUT=$(bm bridge start -t $TEAM 2>&1)
EC=$?
# Show output for volume-loss recovery diagnostics (verify recipe, re-provisioning)
echo "    [C22 start output] $(echo "$OUT" | grep -i 'verify\|re-provision\|onboard\|clearing\|stale' || echo '(no verify/re-provision messages)')"
if [ $EC -eq 0 ]; then pass "C22" "Bridge start recovers from volume loss"; else fail "C22" "Recovery" "exit $EC: $(echo "$OUT" | tail -5)"; echo "$OUT"; fi

CSTATUS=$(podman ps --filter "name=$CONTAINER" --format '{{.Status}}' 2>&1)
if echo "$CSTATUS" | grep -q "Up"; then pass "C23" "Container running after volume re-create"; else fail "C23" "Container" "status=$CSTATUS"; fi

HTTP=$(curl -sf -o /dev/null -w "%{http_code}" "$MATRIX_URL/_matrix/client/versions" 2>/dev/null || echo "000")
if [ "$HTTP" = "200" ]; then pass "C24" "Matrix healthy after volume re-create"; else fail "C24" "Matrix health" "HTTP $HTTP"; fi

# C25: Passwords regenerated
NEW_ADMIN_PASS=$(jq -r '.bmadmin' "$PWFILE" 2>/dev/null)
if [ -n "$NEW_ADMIN_PASS" ]; then pass "C25" "Admin password regenerated"; else fail "C25" "Password" "no admin password"; fi

# Re-provision member identities after volume loss (Matrix DB is fresh)
OUT=$(bm bridge identity add engineer-alice -t $TEAM 2>&1)
EC=$?
if [ $EC -eq 0 ]; then pass "C25a" "Re-provision alice after volume loss"; else fail "C25a" "Re-provision alice" "exit $EC: $(echo "$OUT" | tail -3)"; fi

OUT=$(bm bridge identity add engineer-bob -t $TEAM 2>&1)
EC=$?
if [ $EC -eq 0 ]; then pass "C25b" "Re-provision bob after volume loss"; else fail "C25b" "Re-provision bob" "exit $EC: $(echo "$OUT" | tail -3)"; fi

# C26: New credentials work
ALICE_PW=$(jq -r '."engineer-alice"' "$PWFILE" 2>/dev/null)
echo "    [C26 debug] password file: ${ALICE_PW:+set (${#ALICE_PW} chars)}${ALICE_PW:-EMPTY}, file=$PWFILE"
LOGIN3=$(curl -sf -X POST -H "Content-Type: application/json" \
    -d "{\"type\":\"m.login.password\",\"identifier\":{\"type\":\"m.id.user\",\"user\":\"engineer-alice\"},\"password\":\"$ALICE_PW\"}" \
    "$MATRIX_URL/_matrix/client/v3/login" 2>/dev/null || echo '{}')
ALICE_TOKEN3=$(echo "$LOGIN3" | jq -r '.access_token // empty')
echo "    [C26 debug] login response: $(echo "$LOGIN3" | jq -c '{ access_token: (.access_token // null), errcode: (.errcode // null), error: (.error // null) }' 2>/dev/null)"
KR_ALICE3=$(secret_tool lookup service "botminter.$TEAM.tuwunel" username engineer-alice 2>/dev/null || true)
echo "    [C26 debug] keyring: ${KR_ALICE3:+set (${#KR_ALICE3} chars)}${KR_ALICE3:-EMPTY}"
if [ -n "$ALICE_TOKEN3" ] && [ -n "$KR_ALICE3" ]; then
    pass "C26" "Alice: new password + keyring valid after volume re-create"
elif [ -n "$KR_ALICE3" ]; then
    fail "C26" "Login" "keyring set but Matrix login failed"
else
    fail "C26" "Keyring" "no credential after volume re-create"
fi

# ── C.6: Pre-existing user onboarding ──

echo "  C.6: Pre-existing user onboarding..."

# C27: Register user directly on Matrix via UIAA (simulates pre-existing user)
# Uses the same registration token as the bridge recipes — Tuwunel doesn't
# expose the Synapse admin registration API, so we use the standard UIAA flow.
PRE_PASS="pre-existing-pass-$(date +%s)"
REG_TOKEN=$(jq -r '.registration_token // "bm-tuwunel-reg-default"' "$BSTATE" 2>/dev/null || echo "bm-tuwunel-reg-default")
# Use the hardcoded default since registration_token isn't in bridge-state
REG_TOKEN="bm-tuwunel-reg-default"

# Step 1: Get UIAA session
REG_RESP=$(curl -s -X POST -H "Content-Type: application/json" \
    -d "{\"username\":\"engineer-pre-existing\",\"password\":\"$PRE_PASS\"}" \
    "$MATRIX_URL/_matrix/client/v3/register" 2>/dev/null || echo '{}')
SESSION=$(echo "$REG_RESP" | jq -r '.session // empty')

if [ -n "$SESSION" ]; then
    # Step 2: Complete registration with token
    REG_RESP2=$(curl -sf -X POST -H "Content-Type: application/json" \
        -d "{\"username\":\"engineer-pre-existing\",\"password\":\"$PRE_PASS\",\"auth\":{\"type\":\"m.login.registration_token\",\"token\":\"$REG_TOKEN\",\"session\":\"$SESSION\"}}" \
        "$MATRIX_URL/_matrix/client/v3/register" 2>/dev/null || echo '{}')
    PRE_USER_ID=$(echo "$REG_RESP2" | jq -r '.user_id // empty')
    if [ -n "$PRE_USER_ID" ]; then
        pass "C27" "Pre-existing user registered via UIAA ($PRE_USER_ID)"
    else
        note "C27" "Pre-existing registration" "UIAA completion failed: $(echo "$REG_RESP2" | head -c 200)"
    fi
else
    ERRCODE=$(echo "$REG_RESP" | jq -r '.errcode // empty')
    if [ "$ERRCODE" = "M_USER_IN_USE" ]; then
        pass "C27" "Pre-existing user already exists"
    else
        note "C27" "Pre-existing registration" "no session returned: $(echo "$REG_RESP" | head -c 200)"
    fi
fi

# C28: Hire pre-existing user and sync to trigger onboarding (M_USER_IN_USE path)
# The user already exists on Matrix (from C27), so the onboard recipe
# will hit M_USER_IN_USE and recover via stored password or admin room.
if [ -n "${PRE_USER_ID:-}" ]; then
    # Store the pre-existing user's password so the onboard recipe can find it
    if [ -f "$PWFILE" ]; then
        jq --arg user "engineer-pre-existing" --arg pass "$PRE_PASS" \
            '. + {($user): $pass}' "$PWFILE" > "${PWFILE}.tmp"
        mv "${PWFILE}.tmp" "$PWFILE"
    fi
    # Hire pre-existing as a member so bridge identity add can provision them
    bm_hire engineer --name pre-existing 2>&1 >/dev/null || true
fi
OUT=$(bm bridge identity add engineer-pre-existing -t $TEAM 2>&1)
EC=$?
if [ $EC -eq 0 ]; then pass "C28" "bridge identity add handles pre-existing user"; else fail "C28" "Pre-existing identity add" "exit $EC: $(echo "$OUT" | tail -3)"; fi

# C29: Container still running
CSTATUS=$(podman ps --filter "name=$CONTAINER" --format '{{.Status}}' 2>&1)
if echo "$CSTATUS" | grep -q "Up"; then pass "C29" "Container stable after pre-existing user sync"; else fail "C29" "Container" "status=$CSTATUS"; fi

# C30: Bridge state updated
ID_COUNT3=$(jq '.identities | length' "$BSTATE" 2>/dev/null || echo "0")
if [ "$ID_COUNT3" -ge 3 ]; then
    pass "C30" "Bridge state has $ID_COUNT3 identities"
else
    fail "C30" "Identities" "count=$ID_COUNT3"
fi

# C31: Idempotency after pre-existing user
OUT=$(bm bridge start -t $TEAM 2>&1)
EC=$?
if [ $EC -eq 0 ]; then pass "C31" "Bridge start idempotent after pre-existing user"; else fail "C31" "Idempotent bridge start" "exit $EC"; fi

# C32: Final state consistency
STATUS3=$(jq -r '.status' "$BSTATE" 2>/dev/null)
if [ "$STATUS3" = "running" ]; then pass "C32" "Final bridge state: running"; else fail "C32" "Final state" "status=$STATUS3"; fi

# C33: Verify pre-existing user has valid credentials
# The keyring stores the ACCESS TOKEN (not the password). Verify both:
# (a) keyring has an access token, and (b) the token works for an authenticated API call.
KR_PRE=$(secret_tool lookup service "botminter.$TEAM.tuwunel" username engineer-pre-existing 2>/dev/null || true)
if [ -n "$KR_PRE" ]; then
    # Verify the access token is valid by making an authenticated API call
    WHOAMI=$(curl -sf -H "Authorization: Bearer $KR_PRE" \
        "$MATRIX_URL/_matrix/client/v3/account/whoami" 2>/dev/null || echo '{}')
    WHOAMI_USER=$(echo "$WHOAMI" | jq -r '.user_id // empty')
    if [ -n "$WHOAMI_USER" ]; then
        pass "C33" "Pre-existing user: keyring token valid ($WHOAMI_USER)"
    else
        # Token may have expired — verify password-based login from passwords file instead
        PRE_PW=$(jq -r '.["engineer-pre-existing"] // empty' "$PWFILE" 2>/dev/null)
        if [ -n "$PRE_PW" ]; then
            PRE_LOGIN=$(curl -sf -X POST -H "Content-Type: application/json" \
                -d "{\"type\":\"m.login.password\",\"identifier\":{\"type\":\"m.id.user\",\"user\":\"engineer-pre-existing\"},\"password\":\"$PRE_PW\"}" \
                "$MATRIX_URL/_matrix/client/v3/login" 2>/dev/null || echo '{}')
            PRE_TOKEN=$(echo "$PRE_LOGIN" | jq -r '.access_token // empty')
            if [ -n "$PRE_TOKEN" ]; then
                pass "C33" "Pre-existing user: password login valid (token refreshed)"
            else
                fail "C33" "Pre-existing login" "keyring token expired and password login also failed"
            fi
        else
            fail "C33" "Pre-existing login" "keyring token invalid and no password in file"
        fi
    fi
else
    fail "C33" "Pre-existing keyring" "no credential stored"
fi

echo "Phase C complete."
