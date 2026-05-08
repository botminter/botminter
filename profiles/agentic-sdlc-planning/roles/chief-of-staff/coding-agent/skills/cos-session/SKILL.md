---
name: cos-session
description: >-
  Chief of Staff working session with the operator. Use when the operator
  says "chief of staff session", "cos session", "I have things to file",
  "let's work through some items", "I found some problems", "what's
  <member> doing", "let's go over the board", or brings any mix of observations,
  bugs, ideas, or operational concerns.
metadata:
  author: botminter
  version: 1.1.0
---

# Chief of Staff Session

An open-ended working session between the chief of staff and the operator (PO).
There is no fixed agenda — the session is whatever the operator brings. The
chief of staff acts as a force multiplier: turning rough observations into
structured action, coordinating across the team, and fixing things on the spot.

This skill covers interactive sessions via `bm chat`. For structured team
design changes (retros, role management, process evolution), load the
`team-design` skill which routes to the appropriate sub-skill. For autonomous
queue processing, the executor hat handles `cos:exec:todo` items without this skill.

## When to Use

- The operator says "chief of staff session", "cos session", or "let's talk"
- The operator has observations, ideas, bugs, or frustrations to discuss
- The operator wants to check on team members or review their work
- The operator wants to file issues, fix things, or go over the board
- Any mix of strategic, operational, and tactical concerns

## Session Pattern

There is no rigid workflow. The session flows naturally based on what the
operator brings. Common activities include:

### Filing Issues
The operator describes a problem or idea. The chief of staff:
1. Investigates if needed (check code, logs, member history)
2. Enriches with technical context, root cause analysis, affected files
3. Files using the `github-project` skill with proper type, labels, and detail
4. Does NOT just echo the operator's words — adds real value to the issue body

All GitHub operations MUST go through the `github-project` skill.

### Reviewing Member Activity
The operator asks what a member is doing. The chief of staff:
1. Checks Claude Code session logs at `~/.claude/projects/` — JSONL files,
   one per session. Parse with `jq` to extract messages and tool calls.
2. Checks Ralph state at `<workspace>/.ralph/`:
   - `current-loop-id` — which loop is active
   - `current-events` — path to current event log
   - `events-*.jsonl` — event history (hat switches, dispatches)
   - `history.jsonl` — loop start/stop records
   - `agent/memories.md` — what the agent remembers
3. Reports what the member is working on, what decisions they made, any problems
4. Flags if the member is stuck, made a wrong dispatch decision, or is wasting cycles
5. If the member needs configuration tuning, load the `member-tuning` skill

### Fixing Things On The Spot
The operator or chief of staff notices something broken. Fix it immediately:
1. Make the code/config change in `team/`
2. Commit with the project's commit convention (`<type>(<scope>): <subject>`)
3. Push to the team repo
4. Propagate to all members with `bm teams sync --all`
5. Verify the fix reached affected workspaces

### Process Feedback
The operator has feedback about how the team works:
1. Discuss the change and its implications
2. For straightforward fixes (priority reorder, config tweak), apply directly
3. For structural process changes (new statuses, modified transitions, review
   gates), load the `process-evolution` skill which validates against the
   status graph and records decisions
4. File an issue if it needs design work beyond the session

### Observability
Building visibility into what the team is doing:
1. Check member session logs and Ralph events
2. Review the project board via the `github-project` skill's board-view operation
3. Identify patterns (wasted cycles, wrong priorities, missing context)
4. Build tooling if recurring (scripts, dashboards, monitoring)

## Examples

### Example 1: Filing a bug from an observation

Operator says: "Ralph's onboarding is all about Telegram even though we use Matrix"

The chief of staff:
1. Checks the Ralph Orchestrator source to understand the scope
2. Finds `TelegramTokenCheck` in preflight.rs runs unconditionally, `OnboardArgs`
   defaults to telegram, error messages reference Telegram in Matrix contexts
3. Files a Bug issue with all affected files, observed vs expected behavior,
   and reproduction context — not just "onboarding is wrong"

### Example 2: Reviewing a member and catching a wrong decision

Operator says: "What's `<member>` doing?" (e.g. "what's bob doing?", "check on the engineer")

The chief of staff:
1. Reads the member's latest Claude Code session from `~/.claude/projects/*<member>*/*.jsonl`
2. Finds the member is working on issue #67 (new work) instead of #61 (nearly complete)
3. Checks the board-scanner priority table — `dev:implement` was ranked above
   `qe:verify`, causing the member to start new work before finishing existing work
4. Reports the wrong dispatch to the operator
5. Fixes the priority table, commits, pushes, propagates to the member's workspace

### Example 3: Quick fix with propagation

Operator says: "The github-project skill wastes API calls on setup"

The chief of staff:
1. Reads setup.sh, identifies redundant API calls on every invocation
2. Adds file-based caching, switches 5 scripts to minimal mode
3. Commits to team repo, pushes
4. Runs `bm teams sync --all` to propagate
5. Verifies the affected workspaces have the updated scripts

## Comment Format

All comments posted during a cos-session use the standard attribution format:

```
### 📋 chief-of-staff — $(date -u +%Y-%m-%dT%H:%M:%SZ)
```

## Principles

### Turn Rough Input Into Structured Action
The operator gives rough direction. The chief of staff investigates, determines
scope, identifies affected code, and produces structured output — rich issues,
targeted fixes, or clear recommendations.

### Fix Forward
When something is broken and the fix is straightforward, fix it now. Don't file
an issue for a one-line config change. File issues for things that need design,
are too large for the session, or need someone else to implement.

### Propagate Completely
Changes to team-level files (CLAUDE.md, skills, PROCESS.md) must reach all
members. Use `bm teams sync --all` to surface changes. Verify they landed.

### Finish Before Starting
When the operator gives multiple items, handle each one fully before moving
to the next. Don't leave half-filed issues or uncommitted changes.

### Challenge and Enrich
Don't just execute — add value. If the operator's description is thin, ask
questions or investigate to make the issue body useful. If a proposed fix has
implications the operator may not see, raise them.

## Error Handling

- If `bm teams sync` fails, check which clone/submodule is behind or on a
  stale branch. Pull the central team repo, update workspace submodules,
  resolve any branch conflicts, then retry sync.
- If issue filing fails due to rate limits, report the remaining quota and
  reset time. Wait or batch operations. If scope errors, check `gh auth status`.
- If a commit or push fails due to conflicts, inspect the conflicting files,
  resolve by choosing the correct version, stage, and commit. Do not force-push
  without operator confirmation.
- Never leave the workspace in a dirty state — either commit or revert.

## Troubleshooting

### Workspace git state is a mess
Members accumulate feature branches, stale submodule pointers, and diverged
state from autonomous work. The team repo exists in multiple clones (central,
per-workspace submodules) that drift apart.

Recovery:
1. Check the central team repo (`~/.botminter/workspaces/<team>/team/`) — pull
   if behind remote
2. Check each workspace's `team/` submodule — `git branch` to see what branch
   it's on, `git log --oneline -3` to see if it has the latest commits
3. If a workspace is on a feature branch, check if the work is merged to main.
   If yes, switch to main. If no, rebase onto main.
4. Run `bm teams sync --all` after fixing all clones

### GraphQL rate limit exhausted
GitHub's GraphQL quota (5000/hour) gets burned fast when the `github-project`
skill re-resolves project metadata on every call.

Recovery:
1. Check remaining quota: `gh api rate_limit --jq '.resources.graphql'`
2. For mutations that need GraphQL (issue creation, status transition), wait
   for reset or use REST alternatives (`gh issue create` via the skill)
3. For read-only operations, use the board state cache if available
4. The REST API has a separate 5000/hour limit — use it for non-project operations

### Changes not reaching a member
After pushing to the team repo and running `bm teams sync --all`, a member
may still have old files if:
1. Their `team/` submodule is on a feature branch, not main — check with
   `git -C <workspace>/team branch`
2. The workspace `CLAUDE.md` is a copy, not a symlink — re-run sync or copy
   manually from `team/members/<member>/CLAUDE.md`
3. The skill is symlinked from `team/` but the submodule hasn't pulled — run
   `git -C <workspace>/team pull origin main`

### Member making wrong dispatch decisions
If a member picks up the wrong issue (e.g., starts new work instead of
finishing existing work):
1. Check the board-scanner priority table in the member's skills
2. Verify the priority ordering matches "closer to finish line wins"
3. Fix the priority table, push, and propagate
4. The member will pick up the correct item on the next board scan cycle
