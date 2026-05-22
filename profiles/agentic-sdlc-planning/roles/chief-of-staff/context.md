# Chief of Staff — Team Member Context

<!-- +agent:claude-code -->
This file provides context for operating as the chief of staff. Read @team/CLAUDE.md for team-wide workspace model, coordination model, knowledge resolution, and invariant scoping.
<!-- -agent -->

## A. Project Context

Your working directory is your workspace — not a project repo. Projects are checked out as git submodules under `projects/` as well as the team repo at `team/`.

### Operating in the `team/` Submodule

Your workspace has the team repo cloned into `team/`. All your work happens inside this submodule:

- **Knowledge and invariants**: Read from `team/knowledge/` and `team/invariants/`
- **Process conventions**: Follow `team/PROCESS.md`
- **Member config**: Your config lives in `team/members/{{member_dir}}/`
- **Committing changes**: Commit and push within `team/` — this is a submodule, not the workspace root

## B. Team Member Skills & Capabilities

### Available Hats

| Hat | Purpose |
|-----|---------|
| **executor** | Picks up and executes chief of staff tasks |

Board scanning is handled by an auto-inject skill, not a hat.

### Workspace Layout

<!-- BM:WORKSPACE_LAYOUT -->
<!-- /BM:WORKSPACE_LAYOUT -->

### Knowledge Resolution

| Level | Path |
|-------|------|
| Team knowledge | `team/knowledge/` |
| Member knowledge | `team/members/{{member_dir}}/knowledge/` |
| Hat knowledge (executor) | `team/members/{{member_dir}}/hats/executor/knowledge/` |

### Invariant Compliance

| Level | Path |
|-------|------|
| Team invariants | `team/invariants/` |
| Member invariants | `team/members/{{member_dir}}/invariants/` |

### Coordination Conventions

See `team/PROCESS.md` for issue format, status transitions, comment attribution, and milestone conventions.

### GitHub Access

**NEVER use `gh` CLI directly.** All GitHub operations — issues, projects, PRs, milestones, comments, labels, status transitions — MUST go through the `github-project` skill scripts. If a script doesn't exist for an operation, create one or extend an existing script. Do NOT fall back to raw `gh` commands. Bypassing the skill corrupts the board state cache and wastes API quota.

The team repo is auto-detected from `team/`'s git remote.

### Reference Files

<!-- +agent:claude-code -->
- Team context: @team/CLAUDE.md
<!-- -agent -->
- Process conventions: `team/PROCESS.md`
- Work objective: see `PROMPT.md`
