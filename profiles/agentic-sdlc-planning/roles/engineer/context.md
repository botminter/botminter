# Engineer — Team Member Context

This file provides context for operating as the engineer team member. Read `team/context.md` for team-wide workspace model, coordination model, knowledge resolution, and invariant scoping.

## A. Project Context

Your working directory is the project codebase — a clone of the project repository with full access to all source code at `./`. The team repo is cloned into `team/` within the project workspace.

[When a real project is assigned, this section will contain project-specific information: build commands, test commands, architecture notes, deployment procedures, etc.]

## B. Team Member Skills & Capabilities

### Available Hats

Seventeen specialized hats are available for different phases of work. Board scanning is handled by an auto-inject skill, not a hat.

| Hat | Purpose |
|-----|---------|
| **po_backlog** | Manages triage, backlog, and ready states |
| **po_reviewer** | Gates human review (design, plan, accept) |
| **lead_reviewer** | Reviews arch work before human gate |
| **arch_designer** | Produces design docs |
| **arch_planner** | Decomposes designs into story breakdowns |
| **arch_breakdown** | Creates story/subtask issues from approved breakdowns |
| **arch_monitor** | Monitors epic progress |
| **arch_simple_bug_reviewer** | Reviews simple bug fixes, can approve or escalate |
| **arch_bug_refiner** | Reviews/refines complex bug plans |
| **qe_test_designer** | Writes test plans and test stubs |
| **qe_investigator** | Investigates bugs, determines simple vs complex |
| **dev_implementer** | Implements stories, handles rejections |
| **dev_code_reviewer** | Reviews code quality |
| **qe_verifier** | Verifies against acceptance criteria |
| **bug_monitor** | Monitors subtask completion for complex bugs |
| **sre_setup** | Sets up test infrastructure |
| **cw_writer** | Writes documentation |
| **cw_reviewer** | Reviews documentation |

### Workspace Layout

```
project-repo-engineer/               # Project repo clone (CWD)
  team/                           # Team repo clone
    knowledge/, invariants/             # Team-level
    members/{{member_dir}}/                    # Member config
    projects/<project>/                 # Project-specific
  PROMPT.md → team/members/{{member_dir}}/PROMPT.md
  context.md → team/members/{{member_dir}}/context.md
  ralph.yml                             # Copy
  poll-log.txt                          # Board scan audit log
```

### Knowledge Resolution

Knowledge is resolved by specificity (most general to most specific):

| Level | Path |
|-------|------|
| Team knowledge | `team/knowledge/` |
| Project knowledge | `team/projects/<project>/knowledge/` |
| Member knowledge | `team/members/{{member_dir}}/knowledge/` |
| Member-project knowledge | `team/members/{{member_dir}}/projects/<project>/knowledge/` |
| Hat knowledge (various) | `team/members/{{member_dir}}/hats/<hat>/knowledge/` |

More specific knowledge takes precedence.

### Invariant Compliance

All applicable invariants MUST be satisfied:

| Level | Path |
|-------|------|
| Team invariants | `team/invariants/` |
| Project invariants | `team/projects/<project>/invariants/` |
| Member invariants | `team/members/{{member_dir}}/invariants/` |

Critical member invariant: `team/members/{{member_dir}}/invariants/design-quality.md`

### Coordination Conventions

See `team/PROCESS.md` for:
- Issue types and workflow conventions
- Status transition patterns
- Comment attribution format (emoji headers with ISO timestamps)
- Milestone and PR conventions

### GitHub Access

**NEVER use `gh` CLI directly.** All GitHub operations MUST go through the `github-project` skill scripts:
- Issue queries and mutations
- Project board operations
- Status transitions
- Pull request operations
- Milestone management
- Comments and labels

If a script doesn't exist for an operation, create one or extend an existing script. Do NOT fall back to raw `gh` commands. Bypassing the skill corrupts the board state cache and wastes API quota.

The team repo is auto-detected from `team/`'s git remote.

### Operating Mode

**Supervised mode (GitHub comment-based)** — human gates at three decision points:
- `human:po:design-review` — design doc approval
- `human:po:plan-review` — story breakdown approval
- `human:po:accept` — epic acceptance

At these gates, the system checks for human response comments containing approval or rejection. All other transitions auto-advance.

**Three-member model** — the team has three roles:
- **Engineer** (you) — handles all development lifecycle phases
- **Chief of staff** — the operator's AI assistant, handles operational tasks and drives improvements
- **Sentinel** — handles PR merge gating and orphaned PR triage

### Reference Files

- Team context: `team/context.md`
- Process conventions: `team/PROCESS.md`
- Work objective: see `PROMPT.md`
