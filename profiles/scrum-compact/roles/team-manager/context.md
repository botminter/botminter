# Team Manager — Team Member Context

This file provides context for operating as the team manager. Read `team/context.md` for team-wide workspace model, coordination model, knowledge resolution, and invariant scoping.

## A. Project Context

Your working directory is the team repository itself — you operate on the team repo as your default project, coordinating process improvements and team-level tasks.

### Operating in the `team/` Submodule

Your workspace has the team repo cloned into `team/`. All your work happens inside this submodule:

- **Knowledge and invariants**: Read from `team/knowledge/` and `team/invariants/`
- **Process conventions**: Follow `team/PROCESS.md`
- **Member config**: Your config lives in `team/members/team-manager-*/`
- **Committing changes**: Commit and push within `team/` — this is a submodule, not the workspace root

## B. Team Member Skills & Capabilities

### Available Hats

| Hat | Purpose |
|-----|---------|
| **executor** | Picks up and executes team management tasks |

Board scanning is handled by an auto-inject skill, not a hat.

### Workspace Layout

```
team-manager-workspace/              # Workspace (CWD)
  team/                              # Team repo clone (submodule)
    knowledge/, invariants/          # Team-level
    members/team-manager-*/          # Member config
    projects/<project>/              # Project-specific
  PROMPT.md
<!-- +agent:claude-code -->
  AGENTS.md
<!-- -agent -->
  ralph.yml
```

### Knowledge Resolution

| Level | Path |
|-------|------|
| Team knowledge | `team/knowledge/` |
| Member knowledge | `team/members/team-manager-*/knowledge/` |
| Hat knowledge (executor) | `team/members/team-manager-*/hats/executor/knowledge/` |

### Invariant Compliance

| Level | Path |
|-------|------|
| Team invariants | `team/invariants/` |
| Member invariants | `team/members/team-manager-*/invariants/` |

### Coordination Conventions

See `team/PROCESS.md` for issue format, status transitions, comment attribution, and milestone conventions.

### GitHub Access

All GitHub operations use the `gh` skill. The team repo is auto-detected from `team/`'s git remote.

### Reference Files

- Team context: `team/context.md`
- Process conventions: `team/PROCESS.md`
- Work objective: see `PROMPT.md`
