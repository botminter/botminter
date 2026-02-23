# Process Conventions

This reference documents the process conventions defined by the `scrum` profile. All team members follow these formats when creating and updating issues, milestones, PRs, and comments on GitHub.

## Issue format

Issues are GitHub issues on the **team repo** (not the project repo). All issue operations use the `gh` skill.

### Fields

| Field | GitHub mapping | Description |
|-------|---------------|-------------|
| `title` | Issue title | Concise, descriptive issue title |
| `state` | Issue state | `open` or `closed` |
| `labels` | Issue labels | Kind and status labels (see below) |
| `assignee` | Issue assignee | GitHub username or unassigned |
| `milestone` | Issue milestone | Milestone name or none |
| `parent` | `parent/<number>` label + `Parent: #<number>` in body | Links stories to their parent epic |
| `body` | Issue body | Description, acceptance criteria, and context (markdown) |

## Kind labels

Every issue must have exactly one kind label:

| Label | Description |
|-------|-------------|
| `kind/epic` | A large body of work spanning multiple stories |
| `kind/story` | A single deliverable unit of work |

## Status labels

Status labels follow the naming pattern:

```
status/<role>:<phase>
```

- `<role>` — the team member role responsible (e.g., `po`, `arch`, `dev`, `qe`)
- `<phase>` — the current phase within that role's workflow

**Transition rule**: Only the role named in the status label may transition it. The PO (Product Owner) may override any status.

### Epic statuses

| Status | Role | Description |
|--------|------|-------------|
| `status/po:triage` | human-assistant | New epic, awaiting evaluation |
| `status/po:backlog` | human-assistant | Accepted, prioritized, awaiting activation |
| `status/arch:design` | architect | Architect producing design doc |
| `status/po:design-review` | human-assistant | Design doc awaiting human review |
| `status/arch:plan` | architect | Architect proposing story breakdown |
| `status/po:plan-review` | human-assistant | Story breakdown awaiting human review |
| `status/arch:breakdown` | architect | Architect creating story issues |
| `status/po:ready` | human-assistant | Stories created, epic in ready backlog |
| `status/arch:in-progress` | architect | Architect monitoring story execution |
| `status/po:accept` | human-assistant | Epic awaiting human acceptance |
| `status/done` | — | Epic complete |

### Rejection loops

At any review gate, the human can reject and send the epic back:

| From | To | Trigger |
|------|----|---------|
| `status/po:design-review` | `status/arch:design` | Human rejects design with feedback |
| `status/po:plan-review` | `status/arch:plan` | Human rejects breakdown with feedback |
| `status/po:accept` | `status/arch:in-progress` | Human rejects completed epic |

The rejecting member appends feedback as a standard comment.

### Story statuses

Currently only `status/dev:ready` is active. The remaining story statuses are planned for Milestone 4 when dev, QE (Quality Engineer), and reviewer agents are added.

| Status | Description | Available |
|--------|-------------|-----------|
| `status/dev:ready` | Story ready for development | Now |
| `status/qe:test-design` | QE designing tests | Milestone 4 |
| `status/dev:implement` | Developer implementing | Milestone 4 |
| `status/dev:code-review` | Code review | Milestone 4 |
| `status/qe:verify` | QE verifying implementation | Milestone 4 |
| `status/arch:sign-off` | Architect sign-off | Milestone 4 |
| `status/po:merge` | Merge gate | Milestone 4 |

### Error status

!!! warning "Failed processing escalation"
    If an issue fails processing 3 times, the board scanner adds `status/error` and skips it on future scans. The human must investigate and remove the label to allow retries.

| Status | Description |
|--------|-------------|
| `status/error` | Issue failed processing 3 times. Board scanner skips it. Human investigates and removes the label to retry. |

## Comment format

All comments use emoji-attributed format:

````markdown
### <emoji> <role> — <ISO-8601-UTC-timestamp>

Comment text here. May contain markdown formatting, code blocks, etc.
````

The emoji and role are read from the member's `.botminter.yml` file.

### Standard emoji mapping

| Role | Emoji | Example |
|------|-------|---------|
| po | `📝` | `### 📝 po — 2026-01-15T10:30:00Z` |
| architect | `🏗️` | `### 🏗️ architect — 2026-01-15T10:30:00Z` |
| dev | `💻` | `### 💻 dev — 2026-01-15T10:30:00Z` |
| qe | `🧪` | `### 🧪 qe — 2026-01-15T10:30:00Z` |
| sre | `🛠️` | `### 🛠️ sre — 2026-01-15T10:30:00Z` |
| cw | `✍️` | `### ✍️ cw — 2026-01-15T10:30:00Z` |
| lead | `👑` | `### 👑 lead — 2026-01-15T10:30:00Z` |

Comments are append-only. Never edit or delete existing comments.

## Milestone format

| Field | GitHub mapping | Description |
|-------|---------------|-------------|
| `title` | Milestone title | e.g., `M1: Structure + human-assistant` |
| `state` | Milestone state | `open` or `closed` |
| `description` | Milestone description | Goals and scope |
| `due_on` | Milestone due date | Optional ISO 8601 date |

Issues are assigned to milestones via `gh issue edit --milestone "<title>"`.

## Pull request format

PRs on the team repo are for **team evolution** (knowledge, invariants, process changes), not code changes. Code changes go through the project repo's own review process.

### Fields

| Field | GitHub mapping | Description |
|-------|---------------|-------------|
| `title` | PR title | Descriptive title |
| `state` | PR state | `open`, `merged`, or `closed` |
| `base` | Base branch | Target branch (usually `main`) |
| `head` | Head branch | Feature branch |
| `labels` | PR labels | e.g., `kind/process-change` |
| `body` | PR body | Summary, related issues, changes, testing |

### Reviews

Reviews use GitHub's native review system:

```bash
gh pr review <number> --approve
gh pr review <number> --request-changes
```

Review comments follow the standard comment format with an explicit status (`approved` or `changes-requested`).

### Merge criteria

- All reviewers have approved
- No unresolved `changes-requested` reviews
- PR labels are correct
- Related issues are updated with the PR reference

## Commit convention

All commits to the team repo and project repos follow conventional commits format:

```
<type>(<scope>): <subject>

<body>

Ref: #<issue-number>
```

| Element | Description |
|---------|-------------|
| Type | `feat`, `fix`, `docs`, `refactor`, `test`, `chore` |
| Scope | Optional. Area affected (e.g., `api`, `nodepool`) |
| Subject | Imperative mood, lowercase, no period |
| Body | Optional. Explains the "why" |
| Ref | Required for work-related commits |

## Communication protocols

All coordination flows through GitHub issues — there are no side channels between members.

- Team members coordinate **exclusively** through GitHub issues and the `gh` skill
- No direct member-to-member communication
- Status label transitions are the primary coordination mechanism
- The human-assistant is the only interface to the human (via Telegram in `scrum`, via GitHub comments in `scrum-compact`)

## Process evolution

The process can evolve through two paths:

| Path | When to use | Steps |
|------|------------|-------|
| Formal | Significant changes | PR on team repo → review → merge |
| Informal | Urgent corrections | Human interacts with PO → direct edit → commit |

## Related topics

- [Coordination Model](../concepts/coordination-model.md) — pull-based work discovery and handoff
- [Member Roles](member-roles.md) — role-specific hat models and event dispatch
- [CLI Reference](cli.md) — `bm init` creates labels automatically during team setup
