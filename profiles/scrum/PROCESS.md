# Scrum Process

This document defines the conventions used by the agentic scrum team. All team members follow these formats when creating and updating issues, milestones, PRs, and comments on GitHub. All GitHub operations go through the `gh` skill.

---

## Issue Format

Issues are GitHub issues on the **team repo** (not the project repo). The `gh` skill auto-detects the team repo from `.botminter/`'s git remote.

### Fields

| Field | GitHub Mapping | Description |
|-------|---------------|-------------|
| `title` | Issue title | Concise, descriptive issue title |
| `state` | Issue state | `open` or `closed` |
| `labels` | Issue labels | Kind labels (see below) |
| `assignee` | Issue assignee | GitHub username or unassigned |
| `milestone` | Issue milestone | Milestone name or none |
| `parent` | `parent/<number>` label + `Parent: #<number>` in body | Links stories to their parent epic |
| `body` | Issue body | Description, acceptance criteria, and context (markdown) |

Issues are created via `gh issue create` and managed via `gh issue edit`. See the `gh` skill for exact commands.

---

## Kind Labels

Kind labels classify the type of work:

| Label | Description |
|-------|-------------|
| `kind/epic` | A large body of work spanning multiple stories |
| `kind/story` | A single deliverable unit of work |

Every issue MUST have exactly one `kind/*` label.

---

## Project Status Convention

Status is tracked via a single-select "Status" field on the team's GitHub Project board (v2), NOT via labels. Status values follow the naming pattern:

```
<role>:<phase>
```

- `<role>` — the team member role responsible (e.g., `po`, `arch`, `dev`, `qe`)
- `<phase>` — the current phase within that role's workflow

Examples:
- `po:triage` — PO is triaging the issue
- `dev:in-progress` — developer is working on the issue
- `qe:review` — QE is reviewing the issue

Specific statuses are defined incrementally per milestone. M1 defines only the naming convention. M2 adds epic statuses, M3 adds story statuses.

**Transition rule:** Only the role named in the status may transition it. The PO may override any status.

---

## Epic Statuses (M2)

The epic lifecycle statuses, with the role responsible at each stage:

| Status | Role | Description |
|--------|------|-------------|
| `po:triage` | human-assistant | New epic, awaiting evaluation |
| `po:backlog` | human-assistant | Accepted, prioritized, awaiting activation |
| `arch:design` | architect | Architect producing design doc |
| `po:design-review` | human-assistant | Design doc awaiting human review |
| `arch:plan` | architect | Architect proposing story breakdown (plan) |
| `po:plan-review` | human-assistant | Story breakdown plan awaiting human review |
| `arch:breakdown` | architect | Architect creating story issues |
| `po:ready` | human-assistant | Stories created, epic parked in ready backlog. Human decides when to activate. |
| `arch:in-progress` | architect | Architect monitoring story execution (M2: fast-forward to `po:accept`) |
| `po:accept` | human-assistant | Epic awaiting human acceptance |
| `done` | — | Epic complete |

### Rejection Loops

At any review gate, the human can reject and send the epic back:
- `po:design-review` → `arch:design` (with feedback comment)
- `po:plan-review` → `arch:plan` (with feedback comment)
- `po:accept` → `arch:in-progress` (with feedback comment)

The feedback comment uses the standard comment format and includes the human's specific concerns.

### Story Statuses (M2 Placeholder)

| Status | Description |
|--------|-------------|
| `dev:ready` | Deliberate M3 placeholder. Stories sit idle until M3 brings dev/qe agents. |

### Error Status

| Status | Description |
|--------|-------------|
| `error` | Issue failed processing 3 times. Board scanner skips it. Human investigates and resets the status to retry. |

---

## Comment Format

Comments are GitHub issue comments, added via `gh issue comment`. Each comment uses this format:

```markdown
### <emoji> <role> — <ISO-8601-UTC-timestamp>

Comment text here. May contain markdown formatting, code blocks, etc.
```

The `<emoji>` and `<role>` are read from the member's `.botminter.yml` file at runtime by the `gh` skill. Since all agents share one `GH_TOKEN` (one GitHub user), the role attribution in the comment body is the primary way to identify which hat/role wrote it.

### Standard Emoji Mapping

| Role | Emoji | Example Header |
|------|-------|----------------|
| po | 📝 | `### 📝 po — 2026-01-15T10:30:00Z` |
| architect | 🏗️ | `### 🏗️ architect — 2026-01-15T10:30:00Z` |
| dev | 💻 | `### 💻 dev — 2026-01-15T10:30:00Z` |
| qe | 🧪 | `### 🧪 qe — 2026-01-15T10:30:00Z` |
| sre | 🛠️ | `### 🛠️ sre — 2026-01-15T10:30:00Z` |
| cw | ✍️ | `### ✍️ cw — 2026-01-15T10:30:00Z` |
| lead | 👑 | `### 👑 lead — 2026-01-15T10:30:00Z` |

Example:

```markdown
### 📝 po — 2026-01-15T10:30:00Z

Triaged. This is a high-priority story for the current milestone. Assigning to dev.
```

Comments are append-only. Never edit or delete existing comments.

---

## Milestone Format

Milestones are GitHub milestones on the team repo, managed via the `gh` skill.

**Fields:**

| Field | GitHub Mapping | Description |
|-------|---------------|-------------|
| `title` | Milestone title | Milestone name (e.g., `M1: Structure + human-assistant`) |
| `state` | Milestone state | `open` or `closed` |
| `description` | Milestone description | Goals and scope of the milestone |
| `due_on` | Milestone due date | Optional ISO 8601 date |

Issues are assigned to milestones via `gh issue edit --milestone "<title>"`. The `gh` skill provides commands for creating, listing, and managing milestones.

---

## Pull Request Format

Pull requests are real GitHub PRs on the team repo. PRs are used for team evolution (knowledge, invariants, process changes), NOT for code changes.

**Fields:**

| Field | GitHub Mapping | Description |
|-------|---------------|-------------|
| `title` | PR title | Descriptive title of the change |
| `state` | PR state | `open`, `merged`, or `closed` |
| `base` | Base branch | Target branch (usually `main`) |
| `head` | Head branch | Feature branch |
| `labels` | PR labels | e.g., `kind/process-change` |
| `body` | PR body | Description of the change (markdown) |

### Reviews

Reviews use GitHub's native review system via `gh pr review`:

- `gh pr review <number> --approve` — approve the PR
- `gh pr review <number> --request-changes` — request changes

Review comments follow the standard comment format with an explicit status:

```markdown
### <emoji> <role> — <ISO-8601-UTC-timestamp>

**Status: approved**

Review comments here.
```

Valid review statuses: `approved`, `changes-requested`.

---

## Communication Protocols

Team members coordinate through GitHub issues and PRs on the team repo using the `gh` skill:

### Status Transitions

A member transitions an issue's status by:
1. Using `gh project item-edit` to update the Status field on the project board
2. Adding an attribution comment documenting the transition

Other members detect the transition on their next board scan (querying the project board via `gh project item-list`).

### Comments

A member records work output by:
1. Adding a GitHub issue comment via `gh issue comment` using the standard comment format

### Pull Requests

PRs on the team repo are for team-level changes:
- Process changes (PROCESS.md updates)
- Knowledge additions or updates
- Invariant modifications

PRs are NOT used for code changes to the project repo. Code changes go through the project's own review process.

### Coordination Model

The team uses a pull-based coordination model:
- Each member scans the project board for issues with status values relevant to their role
- No central dispatcher — members pull work based on their role's status values
- The PO is the only role that can assign work and override statuses

---

## Process Evolution

The team process can evolve through two paths:

### Formal Path

1. Create a PR on the team repo proposing the change
2. Affected team members review and comment
3. PO approves and merges

### Informal Path

1. Human interacts with the PO via `human.interact`
2. PO edits the process file directly
3. Commit the change to the team repo

The informal path is appropriate for urgent corrections or clarifications. The formal path is preferred for significant process changes that affect multiple team members.
