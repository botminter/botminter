# Member Roles

This reference documents the member roles defined in BotMinter profiles, including their hat models, event dispatch, and responsibilities. The `team-manager` role is available in both `scrum-compact` and `scrum` profiles. The `human-assistant` and `architect` roles are defined in the `scrum` profile (in development, not yet shipping in release builds).

## human-assistant

!!! warning "scrum profile only (in development)"
    This role is part of the `scrum` profile, which is not yet available in release builds.

The human's proxy on the agentic scrum team. Manages the backlog, gates reviews, and coordinates the epic lifecycle through the human-in-the-loop (HIL) channel.

### Hat model

| Hat | Triggers | Responsibility |
|-----|----------|----------------|
| `backlog_manager` | `po.backlog` | Handle `po:triage`, `po:backlog`, `po:ready` — present to human via HIL |
| `review_gater` | `po.review` | Handle `po:design-review`, `po:plan-review`, `po:accept` — gate reviews |

Board scanning is handled by the **board-scanner skill** (auto-injected into the coordinator via `skills.overrides`). The coordinator scans for `status/po:*` issues and dispatches to the appropriate work hat.

### Event dispatch

| Status | Event | Target hat | Priority |
|--------|-------|------------|----------|
| `status/po:triage` | `po.backlog` | backlog_manager | 1 (highest) |
| `status/po:design-review` | `po.review` | review_gater | 2 |
| `status/po:plan-review` | `po.review` | review_gater | 3 |
| `status/po:accept` | `po.review` | review_gater | 4 |
| `status/po:backlog` | `po.backlog` | backlog_manager | 5 |
| `status/po:ready` | `po.backlog` | backlog_manager | 6 (lowest) |

When no `status/po:*` issues are found, the coordinator publishes `LOOP_COMPLETE` (idle).

### HIL interaction

All gates present artifacts to the human for decision. The HIL channel depends on the profile:

- **With bridge** — messaging platform (`human.interact`), blocking (available on any profile with a configured bridge)
- **Without bridge** — GitHub issue comments, non-blocking (agent posts review request, checks for response on next scan)

| Gate | What is presented | Human action |
|------|-------------------|-------------|
| Triage | Epic summary | Accept or reject |
| Backlog | Prioritized backlog | Select which to activate |
| Design review | Design doc summary | Approve or reject with feedback |
| Plan review | Story breakdown | Approve or reject with feedback |
| Ready | Ready epics | Decide when to activate |
| Accept | Completed epic | Accept or send back |

### Constraints

- Never publish `LOOP_COMPLETE` except when idle
- Always log to `poll-log.txt` before publishing events
- Always use PROCESS.md comment format: `### 📝 po — <ISO-timestamp>`

---

## architect

!!! warning "scrum profile only (in development)"
    This role is part of the `scrum` profile, which is not yet available in release builds.

The team's technical authority. Produces design documents, story breakdowns, and story issues for epics. Pull-based — discovers work through board state.

### Hat model

| Hat | Triggers | Responsibility | Transitions to |
|-----|----------|----------------|---------------|
| `designer` | `arch.design` | Produce design doc for epic | `status/po:design-review` |
| `planner` | `arch.plan` | Decompose design into story breakdown | `status/po:plan-review` |
| `breakdown_executor` | `arch.breakdown` | Create story issues from approved breakdown | `status/po:ready` |
| `epic_monitor` | `arch.in_progress` | Monitor epic progress (M2: fast-forward) | `status/po:accept` |

Board scanning is handled by the **board-scanner skill** (auto-injected into the coordinator via `skills.overrides`). The coordinator scans for `status/arch:*` issues and dispatches to the appropriate work hat.

### Event dispatch

| Status label | Event | Hat activated |
|-------------|-------|--------------|
| `status/arch:breakdown` | `arch.breakdown` | breakdown_executor |
| `status/arch:plan` | `arch.plan` | planner |
| `status/arch:design` | `arch.design` | designer |
| `status/arch:in-progress` | `arch.in_progress` | epic_monitor |

**Priority**: `arch:breakdown` > `arch:plan` > `arch:design` > `arch:in-progress`

One issue is processed per scan cycle.

### Designer backpressure

Before transitioning to `status/po:design-review`:

- Design doc has a Security Considerations section
- Design doc has acceptance criteria (Given-When-Then)
- Design doc references applicable project knowledge
- Design doc addresses all applicable invariants

### Breakdown executor backpressure

Before transitioning to `status/po:ready`:

- Each story has Given-When-Then acceptance criteria
- Each story has proper labels (`kind/story`, `status/dev:ready`)
- Each story body references the parent epic
- The epic comment lists all created story numbers

### Constraints

- Always update `team/` submodule before scanning
- Always follow knowledge and invariant scoping defined in hat instructions

---

## team-manager

Process improvement and team coordination. The team-manager handles operational tasks — process audits, retrospective actions, tooling improvements, and ad-hoc coordination work. It operates as a persistent Ralph loop with a single work hat.

### Hat model

| Hat | Triggers | Responsibility | Transitions to |
|-----|----------|----------------|---------------|
| `executor` | `mgr.work` | Pick up `mgr:todo` tasks, execute them, report results | `mgr:done` |

Board scanning is handled by the **board-scanner skill** (auto-injected into the coordinator via `skills.overrides`). The coordinator scans for `status/mgr:*` issues and dispatches to the executor hat.

### Event dispatch

| Status label | Event | Hat activated |
|-------------|-------|--------------|
| `status/mgr:todo` | `mgr.work` | executor |

**Priority**: Only one status triggers work (`mgr:todo`). The executor transitions through `mgr:in-progress` while working and to `mgr:done` on completion.

One issue is processed per scan cycle.

### Interactive sessions

The team-manager is the first role designed with the **role-as-skill pattern** in mind. In addition to running autonomously in a Ralph loop, any hired team-manager member can be invoked interactively via [`bm chat`](cli.md#bm-chat):

- `bm chat <member>` — hatless mode: agent has awareness of all hats, human drives the workflow
- `bm chat <member> --hat executor` — hat-specific mode: agent is in character as the executor hat

See [Coordination Model — Role-as-skill](../concepts/coordination-model.md#role-as-skill-pattern) for the concept.

### Constraints

- Always update `team/` submodule before scanning
- Always use PROCESS.md comment format: `### 📋 team-manager — <ISO-timestamp>`
- Always follow knowledge and invariant scoping defined in hat instructions

---

## Planned roles (Milestone 4)

These roles are designed but not yet implemented:

| Role | Purpose |
|------|---------|
| `dev` | Developer — implements stories, follows TDD (Test-Driven Development) |
| `qe` | QE (Quality Engineer) — writes tests, verifies implementations |
| `reviewer` | Code reviewer — reviews PRs, checks quality |

The full story lifecycle (QE writes tests, dev implements, QE verifies, reviewer reviews, architect signs off, PO (Product Owner) merges) is planned for [Milestone 4](../roadmap.md).

## Related topics

- [Coordination Model](../concepts/coordination-model.md) — pull-based work discovery
- [Configuration Files](configuration.md) — ralph.yml, PROMPT.md, AGENTS.md structure
- [Process Conventions](process.md) — label scheme and issue format
