# Compact Team Context

## What This Repo Is

This is a **team repo** — the control plane for a compact single-member agentic team. Files are the coordination fabric. The single agent ("superman") reads from and writes to this repo to track work.

The team repo is NOT a code repo. It defines the team's structure, process, knowledge, and work items. Code work happens in separate project repos.

## Single-Member Model

The compact profile has one member — `superman` — who wears all hats (PO, team lead, architect, dev, QE, SRE, content writer). The agent self-transitions through the entire issue lifecycle by switching hats. No write-locks are needed since there is no concurrent access.

## Workspace Model

The member runs in a **project repo clone** with the team repo cloned into `.botminter/` inside it. The agent's CWD is the project codebase — direct access to source code at `./`.

```
project-repo-superman/               # Project repo clone (agent CWD)
  .botminter/                           # Team repo clone
    knowledge/, invariants/             # Team-level
    team/superman/                      # Member config
    projects/<project>/                 # Project-specific
  PROMPT.md → .botminter/team/superman/PROMPT.md
  CLAUDE.md → .botminter/team/superman/CLAUDE.md
  ralph.yml                             # Copy
  poll-log.txt                          # Board scanner audit log
```

Pulling `.botminter/` updates all team configuration. Copies (ralph.yml, settings.local.json) require `just sync`.

## Coordination Model

The compact profile uses **self-transition coordination**:
- The single member scans the project board for all status values
- Board scanning and all issue operations use the `gh` skill (wraps `gh` CLI)
- A unified board scanner dispatches to the appropriate hat based on priority
- No concurrent agents, no coordination overhead

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

1. **Team knowledge** — `.botminter/knowledge/` (applies to all hats)
2. **Project knowledge** — `.botminter/projects/<project>/knowledge/` (project-specific)
3. **Member knowledge** — `.botminter/team/superman/knowledge/` (member-specific)
4. **Member+project knowledge** — `.botminter/team/superman/projects/<project>/knowledge/` (member+project-specific)
5. **Hat knowledge** — `.botminter/team/superman/hats/<hat>/knowledge/` (hat-specific)

## Invariant Scoping

Invariants follow the same recursive pattern as knowledge. All applicable invariants MUST be satisfied — they are additive.

1. **Team invariants** — `.botminter/invariants/` (apply to all hats)
2. **Project invariants** — `.botminter/projects/<project>/invariants/` (apply to project work)
3. **Member invariants** — `.botminter/team/superman/invariants/` (member-specific)

## Agent Capabilities (`agent/` directory)

Skills, sub-agents, and settings are scoped across multiple levels using an `agent/` directory that mirrors the knowledge/invariant scoping model. All layers live inside `.botminter/`.

| Level | Location | Naming Convention |
|-------|----------|-------------------|
| Team | `.botminter/agent/{skills,agents}/` | `{item-name}` (e.g., `gh`) |
| Project | `.botminter/projects/<project>/agent/{skills,agents}/` | `{project}.{item-name}` |
| Member | `.botminter/team/superman/agent/{skills,agents}/` | `superman.{item-name}` |

**Skills** — Ralph reads them directly from source directories via `skills.dirs` in ralph.yml. No merging needed.

**Agents** — symlinked into `.claude/agents/` at workspace creation. All agent files from team, project, and member scopes are merged into one directory via symlinks.

**Settings** — `.claude/settings.local.json` is copied from the member's `agent/settings.local.json` if it exists.

## Propagation Model

| What changes | How it reaches the agent |
|---|---|
| Knowledge, invariants, PROCESS.md, team CLAUDE.md | Auto — agent pulls `.botminter/` every scan, reads directly |
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

The team repo (`$TEAM_REPO`) is auto-detected from `.botminter/`'s git remote.

## Reference

- Process conventions and label scheme: see `PROCESS.md`
- Member-specific context: see `team/superman/CLAUDE.md`
