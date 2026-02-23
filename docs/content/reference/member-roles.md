# Member Roles

This reference documents the available member roles in the `scrum` profile, including their hat models, event dispatch, and responsibilities.

## human-assistant

The human's proxy on the agentic scrum team. Manages the backlog, gates reviews, and coordinates the epic lifecycle through the human-in-the-loop (HIL) channel.

### Hat model

| Hat | Triggers | Responsibility |
|-----|----------|----------------|
| `board_scanner` | `board.scan`, `board.rescan` | Scan for `status/po:*` issues, dispatch to appropriate hat |
| `backlog_manager` | `po.backlog` | Handle `po:triage`, `po:backlog`, `po:ready` — present to human via HIL |
| `review_gater` | `po.review` | Handle `po:design-review`, `po:plan-review`, `po:accept` — gate reviews |

### Event dispatch

| Status | Event | Target hat | Priority |
|--------|-------|------------|----------|
| `status/po:triage` | `po.backlog` | backlog_manager | 1 (highest) |
| `status/po:design-review` | `po.review` | review_gater | 2 |
| `status/po:plan-review` | `po.review` | review_gater | 3 |
| `status/po:accept` | `po.review` | review_gater | 4 |
| `status/po:backlog` | `po.backlog` | backlog_manager | 5 |
| `status/po:ready` | `po.backlog` | backlog_manager | 6 (lowest) |

When no `status/po:*` issues are found, the board scanner publishes `LOOP_COMPLETE` (idle).

### HIL interaction

All gates present artifacts to the human for decision. The HIL channel depends on the profile:

- **`scrum`** and **`scrum-compact-telegram`** — Telegram (`human.interact`), blocking
- **`scrum-compact`** — GitHub issue comments, non-blocking (agent posts review request, checks for response on next scan)

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

The team's technical authority. Produces design documents, story breakdowns, and story issues for epics. Pull-based — discovers work through board state.

### Hat model

| Hat | Triggers | Responsibility | Transitions to |
|-----|----------|----------------|---------------|
| `board_scanner` | `board.scan`, `board.rescan` | Scan for `status/arch:*` issues, dispatch | (publishes `arch.*` events) |
| `designer` | `arch.design` | Produce design doc for epic | `status/po:design-review` |
| `planner` | `arch.plan` | Decompose design into story breakdown | `status/po:plan-review` |
| `breakdown_executor` | `arch.breakdown` | Create story issues from approved breakdown | `status/po:ready` |
| `epic_monitor` | `arch.in_progress` | Monitor epic progress (M2: fast-forward) | `status/po:accept` |

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

- Always sync `.botminter/` before scanning
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
- [Configuration Files](configuration.md) — ralph.yml, PROMPT.md, CLAUDE.md structure
- [Process Conventions](process.md) — label scheme and issue format
