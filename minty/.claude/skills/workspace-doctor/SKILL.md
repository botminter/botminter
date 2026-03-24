---
name: workspace-doctor
description: >-
  Diagnoses common BotMinter workspace issues — stale submodules, broken
  symlinks, missing files, outdated context, and sync problems. Use when the
  operator asks to "check my workspace", "diagnose issues", "why isn't X working",
  "fix my setup", "workspace health", or "something seems wrong".
metadata:
  author: botminter
  version: 1.0.0
  category: diagnostics
  tags: [workspace, diagnostics, troubleshooting, health-check]
---

# Workspace Doctor

Diagnoses common BotMinter workspace issues. Runs checks against team workspaces and reports findings with suggested fixes.

## Data Sources

| Source | Path | Contains |
|--------|------|----------|
| Team registry | `~/.botminter/config.yml` | Team paths |
| Workspace marker | `<workspace>/.botminter.workspace` | Workspace identity |
| Team submodule | `<workspace>/team/` | Team repo as submodule |
| Ralph config | `<workspace>/ralph.yml` | Member's Ralph Orchestrator config |
| Context file | `<workspace>/AGENTS.md` | Member's coding agent context |
| Prompt file | `<workspace>/PROMPT.md` | Member's work objective |

## Running Diagnostics

When the operator asks for a workspace check, run these checks in order. Report each finding with a status indicator:
- `OK` — check passed
- `WARN` — non-critical issue found
- `FAIL` — critical issue that needs fixing

### Check 1: Workspace Marker

```bash
test -f <workspace>/.botminter.workspace
```

- `OK` if marker exists
- `FAIL` if missing — workspace not provisioned. Fix: `bm teams sync -t <team>`

### Check 2: Team Submodule

```bash
cd <workspace>/team && git status
```

- `OK` if submodule is clean and on a branch
- `WARN` if submodule has uncommitted changes
- `WARN` if submodule is in detached HEAD state
- `FAIL` if `team/` directory is missing. Fix: `bm teams sync -t <team>`

Check for stale submodule (behind remote):

```bash
cd <workspace>/team && git fetch --dry-run 2>&1
```

If fetch reports new commits, the submodule may be outdated:

> Team submodule is behind remote. Run `cd <workspace>/team && git pull` or `bm teams sync -t <team>`.

### Check 3: Required Files

Verify these files exist at the workspace root:

```bash
test -f <workspace>/ralph.yml
test -f <workspace>/AGENTS.md
test -f <workspace>/PROMPT.md
```

- `OK` if all present
- `FAIL` for each missing file. Fix: `bm teams sync -t <team>` (re-surfaces files from team repo)

### Check 4: Symlinks

Check that `.claude/agents/` symlinks point to valid targets:

```bash
find <workspace>/.claude/agents/ -type l 2>/dev/null | while read link; do
  if [ ! -e "$link" ]; then
    echo "BROKEN: $link -> $(readlink "$link")"
  fi
done
```

- `OK` if no broken symlinks
- `WARN` for each broken symlink. Fix: `bm teams sync -t <team>` (recreates symlinks)

### Check 5: Ralph Lock State

```bash
cat <workspace>/.ralph/loop.lock 2>/dev/null
```

If a lock file exists but the process is not running:

```bash
PID=$(cat <workspace>/.ralph/loop.lock 2>/dev/null | grep -o '"pid":[0-9]*' | grep -o '[0-9]*')
if [ -n "$PID" ] && ! kill -0 "$PID" 2>/dev/null; then
  echo "WARN: Stale lock file — process $PID is not running"
fi
```

- `OK` if no lock or process is alive
- `WARN` if stale lock. Fix: `rm <workspace>/.ralph/loop.lock`

### Check 6: Git Status

```bash
cd <workspace> && git status --short
```

- `OK` if clean
- `WARN` if there are uncommitted changes (may indicate interrupted work)

## Scope Selection

### Single Member

If the operator specifies a member:

```bash
# Derive workspace path
WORKSPACE="<team-path>/<member-name>"
```

### All Members in a Team

If the operator asks to check the whole team:

```bash
# List all members
ls <team-path>/team/members/
```

Run checks for each member's workspace.

### No Teams Configured

If `~/.botminter/config.yml` does not exist:

> No teams registered. There are no workspaces to diagnose. To get started, run `bm init`.

## Output Format

```
## Workspace Health: <member> (<team>)

| Check | Status | Details |
|-------|--------|---------|
| Workspace marker | OK | .botminter.workspace present |
| Team submodule | WARN | Submodule is 3 commits behind remote |
| Required files | OK | ralph.yml, AGENTS.md, PROMPT.md present |
| Symlinks | FAIL | 2 broken symlinks in .claude/agents/ |
| Ralph lock | OK | No stale locks |
| Git status | WARN | 4 uncommitted files |

### Suggested Fixes

1. **Team submodule behind:** Run `cd <workspace>/team && git pull`
2. **Broken symlinks:** Run `bm teams sync -t <team>`
```

## CLI Quick Reference

| Task | Command |
|------|---------|
| Sync workspaces | `bm teams sync -t <team>` |
| Check team status | `bm status -t <team>` |
| Show member details | `bm members show <member> -t <team>` |
| Start member | `bm start -t <team>` |
| Stop member | `bm stop -t <team>` |
