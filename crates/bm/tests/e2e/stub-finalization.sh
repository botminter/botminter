#!/bin/bash
# Stub finalization agent for E2E testing.
# Called as: claude --dangerously-skip-permissions --agent finalization -p "..."
# CWD is set to the session workspace by the daemon.
#
# Commits and pushes uncommitted files in team/ (if any), then exits 0.
set -euo pipefail

for repo_dir in team; do
    [ -d "$repo_dir" ] || continue
    if [ -n "$(git -C "$repo_dir" status --porcelain 2>/dev/null)" ]; then
        git -C "$repo_dir" config user.email "e2e-finalization@botminter.test" 2>/dev/null
        git -C "$repo_dir" config user.name "E2E Finalization" 2>/dev/null
        git -C "$repo_dir" config commit.gpgsign false 2>/dev/null
        git -C "$repo_dir" add -A 2>/dev/null
        git -C "$repo_dir" commit -m "finalization: e2e stub commit" 2>/dev/null
        git -C "$repo_dir" push 2>/dev/null
    fi
done

exit 0
