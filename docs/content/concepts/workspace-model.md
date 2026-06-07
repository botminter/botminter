# Workspace Model

Each team member runs in an isolated **session workspace** — an ephemeral workspace created on demand from shared repository clones. Context files are assembled at the workspace root as tracked, first-class citizens.

## Workspace layout

```
workzone/
  my-team/                                        # Team directory
    team/                                         # Team repo (control plane, git repo)
    .clones/                                      # Shared bare clones
      <hash>/                                     # Bare clone (team repo or project repo)
    sessions/
      <session-id>/                               # Ephemeral session workspace
        .gitmodules
        team/                                       # Git worktree from team clone
        projects/
          my-project/                               # Git worktree from project clone
        CLAUDE.md                                   # Assembled from team/members/<member>/CLAUDE.md
        PROMPT.md                                   # Assembled from team/members/<member>/PROMPT.md
        ralph.yml                                   # Assembled from team/members/<member>/ralph.yml
        .claude/
          agents/                                   # Symlinks into team/ paths
          settings.json                             # Team-level (from coding-agent/settings.json)
          settings.local.json                       # Member-level (from members/<member>/coding-agent/)
        .botminter.workspace                        # Marker file (includes session ID)
        .ralph/                                     # Ralph runtime state (session-scoped)
```

The specific member names (e.g., `engineer-01`, `architect-01`) depend on the [profile](profiles.md). The workspace structure is the same for all profiles.

The agent's working directory (CWD) is the session workspace root. Projects are accessible as git worktrees under `projects/`. The team repo is accessible at `team/`.

## Session lifecycle

Sessions are created by `bm start`, `bm chat`, or `bm meetings` and follow a defined lifecycle:

1. **Creating** — workspace is being hydrated from shared clones
2. **Active** — agent is running in the workspace
3. **Finalizing** — agent has exited, uncommitted work is being committed and pushed
4. **Completed** — finalization succeeded, workspace enters retention
5. **Retained** — workspace is kept for diagnostic inspection
6. **Removed** — workspace is cleaned up by the retention engine

See the [design document](https://github.com/devguyio-bot-squad/may-team-team/blob/main/specs/botminter/85-ephemeral-workspaces/design.md) for the full state machine.

## Context files

Context files (CLAUDE.md, PROMPT.md, ralph.yml) are **assembled** from the team repo into the workspace root during session creation. They are present as regular files in the session workspace.

| File | Method | Update mechanism |
|------|--------|-----------------|
| `CLAUDE.md` | Assembled | Created fresh each session from `team/members/<member>/CLAUDE.md` |
| `PROMPT.md` | Assembled | Created fresh each session from `team/members/<member>/PROMPT.md` |
| `ralph.yml` | Assembled | Created fresh each session from `team/members/<member>/ralph.yml` |
| `settings.json` | Assembled | Created fresh each session from `team/coding-agent/settings.json` |
| `settings.local.json` | Assembled | Created fresh each session from `team/members/<member>/coding-agent/` |
| Agent files (`.claude/agents/`) | Symlink | Assembled each session with symlinks into `team/` paths |
| Skills | Direct read | Ralph reads from `team/` paths via `skills.dirs` |

Changes to team repo files take effect on the next session start — no manual synchronization needed.

## Shared clones

Shared clones are permanent bare git repositories stored in the team's `.clones/` directory. Each clone is named by a SHA-256 hash of the repository URL.

Sessions create git worktrees from these clones, sharing the object store for disk efficiency. Before creating a worktree, the system fetches from remote if the clone hasn't been fetched within the freshness threshold.

### Team repo (`team/`)

The `team/` directory in each session is a git worktree from the team repo's shared clone. It contains all team configuration:

| Content | Path |
|---------|------|
| Team knowledge | `team/knowledge/` |
| Team invariants | `team/invariants/` |
| Project knowledge | `team/projects/<project>/knowledge/` |
| Project invariants | `team/projects/<project>/invariants/` |
| Process conventions | `team/PROCESS.md` |
| Team context | `team/CLAUDE.md` |
| Member configs | `team/members/<member>/` |

Agents update the team content at the start of every board scan cycle (`git pull --ff-only` in `team/`) to stay current with team configuration changes.

### Multi-project agents

An agent assigned multiple projects has multiple git worktrees under `projects/`:

```
projects/
  project-a/                            # Worktree from project-a clone
  project-b/                            # Worktree from project-b clone
```

Work routing is handled by issue labels in the team repo (label per project). The agent reads the label and `cd`s to the right project.

## The `.botminter.workspace` marker

The workspace marker file identifies the directory as a valid BotMinter session workspace. It contains the member name, team repo reference, project number, and session ID.

## Finalization

When a session ends (agent exits), the finalization subagent inspects all repos for uncommitted or unpushed work:

| Action | Scope | Examples |
|--------|-------|---------|
| Commit and push | Project repos: uncommitted changes on non-default branches | Code on feature branches |
| Commit and push | Team repo: uncommitted files under `specs/`, `knowledge/` | Spec docs, design files |
| Push only | Any repo: committed-but-unpushed branches | Committed code not yet pushed |
| Never commit | Credential files, auth tokens | `.config/gh/hosts.yml` |
| Leave in place | Runtime state | Logs, locks, diagnostics, event files |

## Migration from permanent workspaces

Existing permanent workspaces can be migrated to the shared clone model using `bm minty`. The migration skill:
1. Discovers permanent workspaces via `.botminter.workspace` markers
2. Identifies project repos and their remote URLs
3. Creates bare clones in the team's `.clones/` directory
4. Sets clone remotes to upstream URLs

After migration, `bm start` creates ephemeral sessions from the shared clones. Permanent workspace directories remain on disk at their original paths with contents unchanged.

## Related topics

- [Architecture](architecture.md) — two-layer runtime model
- [Launch Members](../how-to/launch-members.md) — creating sessions and launching agents
- [CLI Reference](../reference/cli.md) — `bm start`, `bm stop`, `bm status` commands
