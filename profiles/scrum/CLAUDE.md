# Scrum Team Context

## What This Repo Is

This is a **team repo** — the control plane for an agentic scrum team. Files are the coordination fabric. Every team member reads from and writes to this repo to coordinate work.

The team repo is NOT a code repo. It defines the team's structure, process, knowledge, and work items. Code work happens in separate project repos.

## Workspace Model (M2)

Each team member runs in a **project repo clone** with the team repo cloned into `.botminter/` inside it. The agent's CWD is the project codebase — agents have direct access to source code at `./`.

```
parent-directory/
  my-team/                    # Team repo (human operates here)
  my-project-ha/                      # human-assistant workspace (project repo clone)
    .botminter/                       # Team repo clone
    PROMPT.md → .botminter/team/human-assistant/PROMPT.md
    CLAUDE.md → .botminter/team/human-assistant/CLAUDE.md
    .claude/                          # Assembled from agent/ layers
    ralph.yml                         # Copy
  my-project-arch/                    # architect workspace (project repo clone)
    .botminter/                       # Team repo clone
    PROMPT.md → .botminter/team/architect/PROMPT.md
    CLAUDE.md → .botminter/team/architect/CLAUDE.md
    .claude/                          # Assembled from agent/ layers
    ralph.yml                         # Copy
```

Pulling `.botminter/` updates all team configuration. Copies (ralph.yml, settings.local.json) require `just sync`.

## Coordination Model

The team uses **pull-based coordination**:
- Each member scans the board (GitHub issues on the team repo) for issues with status labels matching their role
- Board scanning and all issue operations use the `gh` skill (wraps `gh` CLI)
- No central dispatcher — coordination is emergent from shared process conventions
- The human-assistant is the human's interface to the team and the only role that can assign work

## GitHub-Native Workflow

Work items, milestones, and PRs live on the team repo's GitHub:

| Resource | Access Method | Tool |
|----------|--------------|------|
| Issues (epics + stories) | `gh issue list/view/create/edit` | `gh` skill |
| Milestones | `gh api` (milestones endpoint) | `gh` skill |
| Pull requests | `gh pr create/view/merge` | `gh` skill |

See `PROCESS.md` for label conventions, status transitions, and comment format.

## Knowledge Resolution Order

Knowledge is resolved in order of specificity. All levels are additive:

1. **Team knowledge** — `.botminter/knowledge/` (applies to all members)
2. **Project knowledge** — `.botminter/projects/<project>/knowledge/` (project-specific)
3. **Member knowledge** — `.botminter/team/<member>/knowledge/` (role-specific)
4. **Member+project knowledge** — `.botminter/team/<member>/projects/<project>/knowledge/` (role+project-specific)
5. **Hat knowledge** — `.botminter/team/<member>/hats/<hat>/knowledge/` (hat-specific)

## Invariant Scoping

Invariants follow the same recursive pattern as knowledge. All applicable invariants MUST be satisfied — they are additive.

1. **Team invariants** — `.botminter/invariants/` (apply to all members)
2. **Project invariants** — `.botminter/projects/<project>/invariants/` (apply to project work)
3. **Member invariants** — `.botminter/team/<member>/invariants/` (role-specific)

## Agent Capabilities (`agent/` directory)

Skills, sub-agents, and settings are scoped across multiple levels using an `agent/` directory that mirrors the knowledge/invariant scoping model. All layers live inside `.botminter/`.

| Level | Location | Naming Convention |
|-------|----------|-------------------|
| Team | `.botminter/agent/{skills,agents}/` | `{item-name}` (e.g., `gh`) |
| Project | `.botminter/projects/<project>/agent/{skills,agents}/` | `{project}.{item-name}` (e.g., `my-project.codebase-search`) |
| Member | `.botminter/team/<member>/agent/{skills,agents}/` | `{member}.{item-name}` (e.g., `architect.design-template`) |

**Skills** — Ralph reads them directly from source directories via `skills.dirs` in ralph.yml. No merging needed.

**Agents** — symlinked into `.claude/agents/` at workspace creation. All agent files from team, project, and member scopes are merged into one directory via symlinks.

**Settings** — `.claude/settings.local.json` is copied from the member's `agent/settings.local.json` if it exists.

## Propagation Model

| What changes | How it reaches agents |
|---|---|
| Knowledge, invariants, PROCESS.md, team CLAUDE.md | Auto — agents pull `.botminter/` every scan, read directly |
| Member PROMPT.md, CLAUDE.md | Auto — workspace files are symlinks into `.botminter/` |
| Skills, agents (all levels) | Auto — read via `.botminter/` paths (skills.dirs) or symlinks (.claude/agents/) |
| ralph.yml | **Manual** — requires `just sync` + agent restart |
| settings.local.json | **Manual** — requires `just sync` (re-copy) |

## Team Repo Access Paths

From a workspace, access team repo content through `.botminter/` and the `gh` skill:

| Content | Access Method |
|---------|--------------|
| Board (issues) | `gh issue list --repo "$TEAM_REPO"` (via `gh` skill) |
| Milestones | `gh api` milestones endpoint (via `gh` skill) |
| Pull requests | `gh pr list --repo "$TEAM_REPO"` (via `gh` skill) |
| Team knowledge | `.botminter/knowledge/` |
| Team invariants | `.botminter/invariants/` |
| Project knowledge | `.botminter/projects/<project>/knowledge/` |
| Project invariants | `.botminter/projects/<project>/invariants/` |
| Process conventions | `.botminter/PROCESS.md` |
| Team context | `.botminter/CLAUDE.md` |

The team repo (`$TEAM_REPO`) is auto-detected from `.botminter/`'s git remote. No write-locks are needed — GitHub handles concurrent access natively.

## Reference

- Process conventions and label scheme: see `PROCESS.md`
- Member-specific context: see each member's `CLAUDE.md` in their workspace
