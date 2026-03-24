# Workspace Model

Each team member runs in an isolated **workspace repo** — a dedicated GitHub-hosted git repository containing submodules for the team repo and project forks. Context files live at the workspace root as tracked, first-class citizens.

## Workspace layout

```
workzone/
  my-team/                                        # Team directory
    team/                                         # Team repo (control plane, git repo)
    superman-01/                                  # Workspace repo for member
      .gitmodules
      team/                                       # Submodule → org/my-team (team repo)
      projects/
        my-project/                               # Submodule → org/my-project (fork)
      AGENTS.md                                   # Copied from team/members/<member>/AGENTS.md
      PROMPT.md                                   # Copied from team/members/<member>/PROMPT.md
      ralph.yml                                   # Copied from team/members/<member>/ralph.yml
      .claude/
        agents/                                   # Symlinks into team/ submodule paths
        settings.local.json                       # Copy
      .botminter.workspace                        # Marker file
      .ralph/                                     # Ralph runtime state (gitignored)
```

The specific member names (e.g., `superman-01`, `architect-01`) depend on the [profile](profiles.md). The workspace structure is the same for all profiles.

The agent's working directory (CWD) is the workspace repo root. Projects are accessible as submodules under `projects/`. The team repo is accessible at `team/`.

## Context files

Context files (AGENTS.md, PROMPT.md, ralph.yml) are **copied** from the team submodule to the workspace root during `bm teams sync`. They are **tracked** in the workspace repo — committed, versioned, and directly visible to the agent.

| File | Method | Update mechanism |
|------|--------|-----------------|
| `AGENTS.md` | Copy | `bm teams sync` re-copies if team submodule version is newer |
| `PROMPT.md` | Copy | `bm teams sync` re-copies if team submodule version is newer |
| `ralph.yml` | Copy | `bm teams sync` re-copies if team submodule version is newer |
| `settings.local.json` | Copy | `bm teams sync` re-copies if present |
| Agent files (`.claude/agents/`) | Symlink | `bm teams sync` re-assembles symlinks into `team/` submodule paths |
| Skills | Direct read | Ralph reads from `team/` submodule paths via `skills.dirs` |

## Submodules

The workspace repo uses git submodules to reference shared repos:

| Submodule | Path | Points to |
|-----------|------|-----------|
| Team repo | `team/` | The team's GitHub repo (control plane) |
| Project fork | `projects/<project>/` | A project fork on GitHub |

Each submodule checks out a **member branch** (e.g., `superman-01`). This gives each agent its own branch to work on without conflicting with other members.

### Team repo submodule (`team/`)

The `team/` submodule contains all team configuration:

| Content | Path |
|---------|------|
| Team knowledge | `team/knowledge/` |
| Team invariants | `team/invariants/` |
| Project knowledge | `team/projects/<project>/knowledge/` |
| Project invariants | `team/projects/<project>/invariants/` |
| Process conventions | `team/PROCESS.md` |
| Team context | `team/AGENTS.md` |
| Member configs | `team/members/<member>/` |

Agents update the team submodule at the start of every board scan cycle (`git submodule update --remote team`) to stay current with team configuration changes.

### Multi-project agents

An agent assigned multiple projects has multiple submodules under `projects/`:

```
projects/
  project-a/                            # Submodule → fork A
  project-b/                            # Submodule → fork B
```

Work routing is handled by issue labels in the team repo (label per project). The agent reads the label and `cd`s to the right submodule.

## The `.botminter.workspace` marker

`bm teams sync` writes a `.botminter.workspace` marker file at the workspace root. This marker identifies the directory as a valid BotMinter workspace. `bm start` discovers workspaces by scanning for this marker file.

The marker is BotMinter-specific — using `.gitmodules` alone would false-positive on any repo with submodules.

## Git exclusions

Runtime state (`.ralph/`) is gitignored in the workspace repo. `bm teams sync` writes a `.gitignore` file excluding runtime-only paths.

## Syncing a workspace

Run `bm teams sync` to create or update workspaces:

**New workspace** (with `--repos`):

1. Create a GitHub repo: `org/<team>-<member>`
2. Clone locally
3. Add team repo as `team/` submodule
4. Checkout member branch in team submodule
5. For each assigned project: add as `projects/<project>/` submodule
6. Checkout member branch in each project submodule
7. Copy context files from `team/members/<member>/` to workspace root
8. Assemble `.claude/agents/` with symlinks into `team/` submodule paths
9. Write `.gitignore` and `.botminter.workspace` marker
10. Commit and push

**Existing workspace:**

1. Update submodules to latest (`git submodule update --remote`)
2. Checkout member branch in each submodule
3. Re-copy context files if team submodule versions are newer
4. Re-assemble `.claude/agents/` symlinks
5. Commit changes (if any) and push

After syncing, restart agents for `ralph.yml` changes to take effect:

```bash
bm stop && bm start
```

## Related topics

- [Architecture](architecture.md) — two-layer runtime model
- [Launch Members](../how-to/launch-members.md) — creating workspaces and launching agents
- [CLI Reference](../reference/cli.md) — `bm teams sync`, `bm start` commands
