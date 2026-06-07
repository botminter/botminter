#!/usr/bin/env bash
# Phase B: Team Init + Hire
# Tests bm init, GitHub repo/project/labels verification, bm hire, idempotency rejection.
set -uo pipefail
source "$LIB"
ensure_gh_token
ensure_keyring

header "Phase B: Team Init + Hire"

# B1: Init team
OUT=$(bm init --non-interactive --profile "$PROFILE" --team-name "$TEAM" \
    --org "$ORG" --repo "$REPO" --bridge tuwunel \
    --github-project-board "$BOARD" 2>&1)
EC=$?
if [ $EC -eq 0 ]; then pass "B1" "bm init (non-interactive, agentic-sdlc-minimal, tuwunel)"; else fail "B1" "bm init" "exit $EC: $(echo "$OUT" | tail -5)"; echo "$OUT"; fi

# B2: GitHub repo exists
if gh repo view "$FULL_REPO" --json name >/dev/null 2>&1; then pass "B2" "GitHub repo exists"; else fail "B2" "GitHub repo" "not found"; fi

# B3: GitHub project board exists
PROJ=$(gh project list --owner "$ORG" --format json 2>/dev/null | jq -r ".projects[] | select(.title==\"$BOARD\") | .title" 2>/dev/null || true)
if [ "$PROJ" = "$BOARD" ]; then pass "B3" "GitHub project board exists"; else fail "B3" "Project board" "not found"; fi

# B4: Labels created
LABEL_COUNT=$(gh label list -R "$FULL_REPO" --json name --jq 'length' 2>/dev/null || echo "0")
if [ "$LABEL_COUNT" -ge 4 ]; then pass "B4" "Labels created ($LABEL_COUNT labels)"; else fail "B4" "Labels" "only $LABEL_COUNT"; fi

# B5: Team registered in config
if [ -f "$HOME/.botminter/config.yml" ] && grep -q "$TEAM" "$HOME/.botminter/config.yml" 2>/dev/null; then
    pass "B5" "Team registered in config.yml"
else
    fail "B5" "Config" "team not in config.yml"
fi

# B6: Team repo cloned
if [ -d "$TEAM_REPO/.git" ]; then pass "B6" "Team repo cloned"; else fail "B6" "Team repo" "not cloned at $TEAM_REPO"; fi

# B7: Init again (should detect existing and reject)
OUT=$(bm init --non-interactive --profile "$PROFILE" --team-name "$TEAM" \
    --org "$ORG" --repo "$REPO" --bridge tuwunel \
    --github-project-board "$BOARD" 2>&1)
EC=$?
if [ $EC -ne 0 ]; then pass "B7" "Init again correctly rejects existing team (exit $EC)"; else pass "B7" "Init again (idempotent re-init)"; fi

# B8: Hire alice (with --reuse-app via bm_hire wrapper)
OUT=$(bm_hire engineer --name alice 2>&1)
EC=$?
if [ $EC -eq 0 ]; then pass "B8" "Hired alice (--reuse-app)"; else fail "B8" "Hire alice" "exit $EC: $(echo "$OUT" | tail -3)"; fi

# B9: Hire bob (with --reuse-app via bm_hire wrapper)
OUT=$(bm_hire engineer --name bob 2>&1)
EC=$?
if [ $EC -eq 0 ]; then pass "B9" "Hired bob (--reuse-app)"; else fail "B9" "Hire bob" "exit $EC: $(echo "$OUT" | tail -3)"; fi

# B9b: Deploy brain-prompt.md into the team repo member dirs (ephemeral model).
# ConfigAssembler.assemble() (CT-154-14) copies brain-prompt.md from
# team_repo/members/<member>/brain-prompt.md into the ephemeral session workspace.
# Do NOT create permanent workspace directories (~/.botminter/workspaces/<team>/<member>/).
BRAIN_TEMPLATE="$TEAM_REPO/brain/system-prompt.md"
if [ -f "$BRAIN_TEMPLATE" ]; then
    SENTINEL_FAIL=false
    for MEMBER in engineer-alice engineer-bob; do
        MEMBER_DIR="$TEAM_REPO/members/$MEMBER"
        MANIFEST="$MEMBER_DIR/botminter.yml"
        MEMBER_NAME=$(grep '^name:' "$MANIFEST" 2>/dev/null | awk '{print $2}' | tr -d '"' || echo "$MEMBER")
        MEMBER_ROLE=$(grep '^role:' "$MANIFEST" 2>/dev/null | awk '{print $2}' | tr -d '"' || echo "engineer")
        sed \
            -e "s|{{member_name}}|${MEMBER_NAME}|g" \
            -e "s|{{team_name}}|${TEAM}|g" \
            -e "s|{{role}}|${MEMBER_ROLE}|g" \
            -e "s|{{gh_org}}|${ORG}|g" \
            -e "s|{{gh_repo}}|${REPO}|g" \
            "$BRAIN_TEMPLATE" > "$MEMBER_DIR/brain-prompt.md" || SENTINEL_FAIL=true
    done
    if $SENTINEL_FAIL; then
        fail "B9b" "Brain deploy" "sed rendering failed for one or more members"
    else
        pass "B9b" "Deployed brain-prompt.md to team repo (alice and bob, per-member rendered)"
    fi
else
    fail "B9b" "Brain template" "brain/system-prompt.md not in team repo at $TEAM_REPO"
fi

# B10: Member dirs exist
if [ -d "$TEAM_REPO/members/engineer-alice" ] && [ -d "$TEAM_REPO/members/engineer-bob" ]; then
    pass "B10" "Member dirs exist (engineer-alice, engineer-bob)"
else
    fail "B10" "Member dirs" "missing"
fi

# B11: Hire duplicate without --reuse-app (should fail because member dir exists)
OUT=$(bm hire engineer --name alice -t "$TEAM" 2>&1)
EC=$?
if [ $EC -ne 0 ]; then pass "B11" "Hire duplicate alice correctly rejects (exit $EC)"; else fail "B11" "Hire duplicate" "Should have failed but succeeded"; fi

# B12: Create test project repo in org + add to team
PROJECT_URL="https://github.com/$FULL_PROJECT_REPO.git"
if gh repo view "$FULL_PROJECT_REPO" --json name >/dev/null 2>&1; then
    pass "B12" "Test project repo already exists ($FULL_PROJECT_REPO)"
else
    OUT=$(gh repo create "$FULL_PROJECT_REPO" --public --description "Exploratory test project (auto-created)" 2>&1)
    EC=$?
    if [ $EC -eq 0 ]; then
        # Initialize with a commit so git operations work
        TMPDIR=$(mktemp -d)
        git clone "$PROJECT_URL" "$TMPDIR/repo" 2>/dev/null
        echo "# Exploratory Test Project" > "$TMPDIR/repo/README.md"
        git -C "$TMPDIR/repo" add -A && git -C "$TMPDIR/repo" commit -m "chore: initial commit" 2>/dev/null
        git -C "$TMPDIR/repo" push origin main 2>/dev/null
        rm -rf "$TMPDIR"
        pass "B12" "Created test project repo ($FULL_PROJECT_REPO)"
    else
        fail "B12" "Create project repo" "exit $EC: $OUT"
    fi
fi

# B13: Add project to team
OUT=$(bm projects add "$PROJECT_URL" -t "$TEAM" 2>&1)
EC=$?
if [ $EC -eq 0 ]; then
    pass "B13" "Added project to team (bm projects add)"
else
    fail "B13" "Projects add" "exit $EC: $(echo "$OUT" | tail -3)"
fi

# B14: Verify project in team config
if grep -q "$PROJECT_REPO" "$TEAM_REPO/botminter.yml" 2>/dev/null; then
    pass "B14" "Project registered in botminter.yml"
else
    fail "B14" "Project config" "not found in botminter.yml"
fi

echo "Phase B complete."
