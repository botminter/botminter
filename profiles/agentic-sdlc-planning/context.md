# Agentic SDLC Planning — Team Context

## What This Repo Is

This is a **team repo** — the control plane for an agentic software development lifecycle team with PDD planning pipeline, adversarial review, and verification. Files are the coordination fabric. Three members coordinate through GitHub issues on this repo to track work.

The team repo is NOT a code repo. It defines the team's structure, process, knowledge, and work items. Code work happens in separate project repos.

## Three-Member Model

The agentic SDLC planning profile has three members:

- **Engineer** — wears all development hats (PO, team lead, architect, dev, QE, SRE, content writer). Self-transitions through the issue lifecycle by switching hats.
- **Chief of Staff** — the operator's AI assistant. Handles operational tasks, reviews member activity, and drives improvements.
- **Sentinel** — dedicated PR merge gatekeeper. Runs project-specific tests before merging, and triages orphaned PRs.

## Workspace Model

Members run in a **project repo clone** with the team repo cloned into `team/` inside it. The agent's CWD is the project codebase — direct access to source code at `./`.

```
project-repo-engineer/               # Project repo clone (agent CWD)
  team/                           # Team repo clone
    knowledge/, invariants/             # Team-level
    members/{{member_dir}}/                    # Member config
    projects/<project>/                 # Project-specific
  PROMPT.md → team/members/{{member_dir}}/PROMPT.md
  context.md → team/members/{{member_dir}}/context.md
  ralph.yml                             # Copy
  poll-log.txt                          # Board scan audit log
```

Pulling `team/` updates all team configuration. Copies (ralph.yml, settings.local.json) require `just sync`.

## Coordination Model

The agentic SDLC planning profile uses **self-transition coordination** for the engineer and **dedicated-role coordination** for sentinel and chief of staff:
- The engineer scans the project board for `eng:*` and `human:*` status values
- The sentinel scans for `snt:*` statuses and open PRs on project forks
- The chief of staff scans for `cos:exec:*` statuses
- Board scanning and all issue operations use the `github-project` skill (wraps `gh` CLI)
- The board-scanner skill (auto-injected into each coordinator) dispatches to the appropriate hat based on priority

## GitHub-Native Workflow

Work items, milestones, and PRs live on the team repo's GitHub:

| Resource | Access Method | Tool |
|----------|--------------|------|
| Issues (epics + stories) | `gh issue list/view/create/edit` | `github-project` skill |
| Milestones | `gh api` (milestones endpoint) | `github-project` skill |
| Pull requests | `gh pr create/view/merge` | `github-project` skill |

See `PROCESS.md` for label conventions, status transitions, and comment format.

## Knowledge Resolution Order

Knowledge is resolved in order of specificity. All levels are additive:

1. **Team knowledge** — `team/knowledge/` (applies to all hats)
2. **Project knowledge** — `team/projects/<project>/knowledge/` (project-specific)
3. **Member knowledge** — `team/members/{{member_dir}}/knowledge/` (member-specific)
4. **Member+project knowledge** — `team/members/{{member_dir}}/projects/<project>/knowledge/` (member+project-specific)
5. **Hat knowledge** — `team/members/{{member_dir}}/hats/<hat>/knowledge/` (hat-specific)

## Invariant Scoping

Invariants follow the same recursive pattern as knowledge. All applicable invariants MUST be satisfied — they are additive.

1. **Team invariants** — `team/invariants/` (apply to all hats)
2. **Project invariants** — `team/projects/<project>/invariants/` (apply to project work)
3. **Member invariants** — `team/members/{{member_dir}}/invariants/` (member-specific)

## Agent Capabilities (`coding-agent/` directory)

Skills, sub-agents, and settings are scoped across multiple levels using a `coding-agent/` directory that mirrors the knowledge/invariant scoping model. All layers live inside `team/`.

| Level | Location | Naming Convention |
|-------|----------|-------------------|
| Team | `team/coding-agent/{skills,agents}/` | `{item-name}` (e.g., `github-project`) |
| Project | `team/projects/<project>/coding-agent/{skills,agents}/` | `{project}.{item-name}` |
| Member | `team/members/{{member_dir}}/coding-agent/{skills,agents}/` | `engineer.{item-name}` |

**Skills** — Ralph reads them directly from source directories via `skills.dirs` in ralph.yml. No merging needed.

<!-- +agent:claude-code -->
**Agents** — symlinked into `.claude/agents/` at workspace creation. All agent files from team, project, and member scopes are merged into one directory via symlinks.

**Settings** — `.claude/settings.local.json` is copied from the member's `coding-agent/settings.local.json` if it exists.
<!-- -agent -->

## Propagation Model

| What changes | How it reaches the agent |
|---|---|
| Knowledge, invariants, PROCESS.md, team context.md | Auto — agent pulls `team/` every scan, reads directly |
| Member PROMPT.md, context.md | Auto — workspace files are symlinks into `team/` |
<!-- +agent:claude-code -->
| Skills, agents (all levels) | Auto — read via `team/` paths (skills.dirs) or symlinks (.claude/agents/) |
<!-- -agent -->
| ralph.yml | **Manual** — requires `just sync` + agent restart |
| settings.local.json | **Manual** — requires `just sync` (re-copy) |

## Team Repo Access Paths

From a workspace, access team repo content through `team/` and the `github-project` skill:

| Content | Access Method |
|---------|--------------|
| Board (issues) | `github-project` skill (board-view operation) |
| Milestones | `gh api` milestones endpoint (via `github-project` skill) |
| Pull requests | `gh pr list --repo "$TEAM_REPO"` (via `github-project` skill) |
| Team knowledge | `team/knowledge/` |
| Team invariants | `team/invariants/` |
| Project knowledge | `team/projects/<project>/knowledge/` |
| Project invariants | `team/projects/<project>/invariants/` |
| Process conventions | `team/PROCESS.md` |
| Team context | `team/context.md` |

The team repo (`$TEAM_REPO`) is auto-detected from `team/`'s git remote.

## Hard Constraints

**NEVER use `gh` CLI directly.** All GitHub operations — issues, projects, PRs, milestones, comments, labels, status transitions — MUST go through the `github-project` skill scripts. If a script doesn't exist for an operation, create one or extend an existing script. Do NOT fall back to raw `gh` commands.

Why: The skill scripts manage board state caching, write-through invalidation, persisted metadata, and attribution. Bypassing them corrupts the cache and wastes API quota.

If you find yourself about to run a `gh` command directly, STOP. Load the `github-project` skill and use the appropriate operation.

## Reference

- Process conventions and label scheme: see `PROCESS.md`
- Member-specific context: see `team/members/{{member_dir}}/context.md`
