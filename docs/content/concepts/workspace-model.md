# Workspace Model

Each team member runs in an isolated workspace — a project repo clone with the team repo embedded inside it. This separation keeps runtime state (memories, scratchpad, logs) distinct from team-level configuration.

## Workspace layout

```
parent-directory/
  my-team/                              # Team repo (control plane)
    team/<member-a>/                    # Member config
    team/<member-b>/                    # Member config
  my-project-<member-a>/               # member-a workspace
    .botminter/                         # Team repo clone
    PROMPT.md -> .botminter/team/<member-a>/PROMPT.md
    CLAUDE.md -> .botminter/team/<member-a>/CLAUDE.md
    ralph.yml                           # Copy (not symlink)
    .claude/
      agents/                           # Symlinked from agent/ layers
      settings.local.json               # Copy
    poll-log.txt                        # Runtime log (workspace-local)
  my-project-<member-b>/               # member-b workspace
    .botminter/                         # Team repo clone
    PROMPT.md -> .botminter/team/<member-b>/PROMPT.md
    CLAUDE.md -> .botminter/team/<member-b>/CLAUDE.md
    ralph.yml                           # Copy
    .claude/
      agents/                           # Symlinked from agent/ layers
```

The specific member names (e.g., `human-assistant`, `architect`) depend on the [profile](profiles.md). The workspace structure is the same for all profiles.

The agent's working directory (CWD) is the project codebase. Agents have direct access to source code at `./`.

## File surfacing

`bm teams sync` surfaces member configuration files from the team repo into the workspace root. The surfacing strategy varies by file type:

| File | Method | Update mechanism |
|------|--------|-----------------|
| `PROMPT.md` | Symlink | Auto — updates when `.botminter/` is pulled |
| `CLAUDE.md` | Symlink | Auto — updates when `.botminter/` is pulled |
| `ralph.yml` | Copy | Manual — requires `bm teams sync` + agent restart |
| `settings.local.json` | Copy | Manual — requires `bm teams sync` |
| Agent files (`.claude/agents/`) | Symlink | Auto — read via symlinks |
| Skills | Direct read | Auto — Ralph reads from `.botminter/` paths via `skills.dirs` |

Symlinks update automatically when the team repo is pulled. Copies require `bm teams sync` to refresh and may require an agent restart.

## The `.botminter/` directory

`.botminter/` is a Git clone of the team repo inside the workspace. It contains all team configuration:

| Content | Path |
|---------|------|
| Team knowledge | `.botminter/knowledge/` |
| Team invariants | `.botminter/invariants/` |
| Project knowledge | `.botminter/projects/<project>/knowledge/` |
| Project invariants | `.botminter/projects/<project>/invariants/` |
| Process conventions | `.botminter/PROCESS.md` |
| Team context | `.botminter/CLAUDE.md` |
| Member configs | `.botminter/team/<member>/` |

Agents pull `.botminter/` at the start of every board scan cycle to stay current with team configuration changes.

## The `.member` marker

`bm teams sync` writes a `.botminter/.member` file containing the member name. The sync process reads this marker to identify which member the workspace belongs to and which files to surface.

## Git exclusions

!!! note "Dual exclusion mechanism"
    Workspace files use both `.git/info/exclude` (local, not committed) and `.gitignore` (project-level) to prevent accidental commits. `bm teams sync` verifies and repairs `.git/info/exclude` if patterns are missing.

Excluded files: `.botminter/`, `PROMPT.md`, `CLAUDE.md`, `ralph.yml`, `.claude/`, `.ralph/`, `poll-log.txt`.

## Syncing a workspace

Run `bm teams sync` to:

1. Pull the team repo (`.botminter/`)
2. Pull the project repo
3. Re-copy `ralph.yml` if the source is newer
4. Re-copy `settings.local.json` if the source is newer
5. Re-assemble `.claude/agents/` symlinks
6. Verify `PROMPT.md` and `CLAUDE.md` symlinks
7. Verify `.git/info/exclude` patterns

## Related topics

- [Architecture](architecture.md) — three-layer generator model
- [Launch Members](../how-to/launch-members.md) — creating workspaces and launching agents
- [CLI Reference](../reference/cli.md) — `bm teams sync`, `bm start` commands
