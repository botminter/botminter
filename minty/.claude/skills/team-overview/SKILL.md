---
name: team-overview
description: >-
  Shows registered BotMinter teams, their members, roles, workspaces, and
  running state. Use when the operator asks to "show teams", "list teams",
  "who is on the team", "team status", "show members", or "what teams do I have".
  Reads ~/.botminter/config.yml and workspace directories.
metadata:
  author: botminter
  version: 1.0.0
  category: team-management
  tags: [teams, members, status, overview]
---

# Team Overview

Shows registered BotMinter teams with their members, roles, workspaces, and running state.

## Data Sources

| Source | Path | Contains |
|--------|------|----------|
| Team registry | `~/.botminter/config.yml` | Team names, paths, profiles, GitHub repos, credentials |
| Team repo | `<team-path>/team/` | Member configs, knowledge, invariants |
| Member config | `<team-path>/team/members/<member>/` | ralph.yml, PROMPT.md, AGENTS.md |
| Workspace | `<team-path>/<member>/` | Workspace repo with `.botminter.workspace` marker |

## How to List Teams

Read the team registry:

```bash
cat ~/.botminter/config.yml
```

Parse the YAML to extract team entries. Each team has:
- `name` — team identifier
- `path` — local filesystem path to the team directory
- `profile` — which profile the team uses (e.g., `scrum`, `scrum-compact`)
- `github_repo` — the GitHub repo URL for coordination
- `credentials.gh_token` — GitHub token (do NOT display this)

### No Teams Configured

If `~/.botminter/config.yml` does not exist or has no teams, tell the operator:

> No teams registered yet. To create a team, run `bm init`.

## How to List Members

For each team, read the members directory:

```bash
ls <team-path>/team/members/
```

Each subdirectory is a member. To get member details, read their config:

```bash
cat <team-path>/team/members/<member>/ralph.yml
```

This shows their hat collection (roles they can perform) and Ralph Orchestrator configuration.

## How to Check Running State

Check if members have active Ralph processes by looking for lock files:

```bash
cat <team-path>/<member>/.ralph/loop.lock 2>/dev/null
```

If the lock file exists and contains a PID, the member is running. If it does not exist, the member is stopped.

Alternatively, use the CLI:

```bash
bm status -t <team-name>
```

## How to Check Workspace State

Verify a member's workspace is provisioned:

```bash
test -f <team-path>/<member>/.botminter.workspace && echo "provisioned" || echo "not provisioned"
```

If not provisioned, suggest:

```bash
bm teams sync -t <team-name>
```

## Output Format

Present the overview as a structured summary:

```
## Teams

### <team-name>
- **Profile:** <profile>
- **GitHub:** <github-repo>
- **Path:** <team-path>

| Member | Role | Workspace | Running |
|--------|------|-----------|---------|
| alice  | architect | provisioned | stopped |
| bob    | developer | provisioned | running |

---
```

## CLI Quick Reference

| Task | Command |
|------|---------|
| List teams | `bm teams list` |
| Show team details | `bm teams show <name>` |
| List members | `bm members list -t <team>` |
| Show member details | `bm members show <member> -t <team>` |
| Check status | `bm status -t <team>` |
| Sync workspaces | `bm teams sync -t <team>` |
