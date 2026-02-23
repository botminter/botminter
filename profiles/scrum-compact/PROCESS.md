# Compact Process

This document defines the conventions used by the compact single-member team. All hats follow these formats when creating and updating issues, milestones, PRs, and comments on GitHub. All GitHub operations go through the `gh` skill.

The compact profile has a single member ("superman") — the agent self-transitions through the full issue lifecycle wearing different hats.

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
| `kind/docs` | A documentation story, routed to content writer hats |

Every issue MUST have exactly one `kind/*` label.

---

## Project Status Convention

Status is tracked via a single-select "Status" field on the team's GitHub Project board (v2), NOT via labels. Status values follow the naming pattern:

```
<role>:<phase>
```

- `<role>` — the role responsible (e.g., `po`, `arch`, `dev`, `qe`, `lead`, `sre`, `cw`)
- `<phase>` — the current phase within that role's workflow

Examples:
- `po:triage` — PO is triaging the issue
- `dev:implement` — developer is implementing the story
- `qe:verify` — QE is verifying the implementation

In the compact profile, the same agent self-transitions through all statuses by switching hats. Comment headers still use the role of the active hat (e.g., architect, dev, qe) for audit trail clarity.

---

## Epic Statuses

The epic lifecycle statuses, with the role responsible at each stage:

| Status | Role | Description |
|--------|------|-------------|
| `po:triage` | PO | New epic, awaiting evaluation |
| `po:backlog` | PO | Accepted, prioritized, awaiting activation |
| `arch:design` | architect | Producing design doc |
| `lead:design-review` | team lead | Design doc awaiting lead review |
| `po:design-review` | PO | Design doc awaiting human review |
| `arch:plan` | architect | Proposing story breakdown (plan) |
| `lead:plan-review` | team lead | Story breakdown awaiting lead review |
| `po:plan-review` | PO | Story breakdown awaiting human review |
| `arch:breakdown` | architect | Creating story issues |
| `lead:breakdown-review` | team lead | Story issues awaiting lead review |
| `po:ready` | PO | Stories created, epic parked in ready backlog. Human decides when to activate. |
| `arch:in-progress` | architect | Monitoring story execution (fast-forwards to `po:accept`) |
| `po:accept` | PO | Epic awaiting human acceptance |
| `done` | — | Epic complete |

### Rejection Loops

At human review gates, the human can reject and send the epic back:
- `po:design-review` → `arch:design` (with feedback comment)
- `po:plan-review` → `arch:plan` (with feedback comment)
- `po:accept` → `arch:in-progress` (with feedback comment)

At team lead review gates, the lead can reject and send back to the work hat:
- `lead:design-review` → `arch:design` (with feedback comment)
- `lead:plan-review` → `arch:plan` (with feedback comment)
- `lead:breakdown-review` → `arch:breakdown` (with feedback comment)

The feedback comment uses the standard comment format and includes specific concerns.

---

## Story Statuses

The story lifecycle follows a TDD flow:

| Status | Role | Description |
|--------|------|-------------|
| `qe:test-design` | QE | QE designing tests and writing test stubs |
| `dev:implement` | dev | Developer implementing the story |
| `dev:code-review` | dev | Code review of implementation |
| `qe:verify` | QE | QE verifying implementation against acceptance criteria |
| `arch:sign-off` | architect | Auto-advance (see below) |
| `po:merge` | PO | Auto-advance (see below) |
| `done` | — | Story complete |

### Story Rejection Loops

- `dev:code-review` → `dev:implement` (code reviewer rejects with feedback)
- `qe:verify` → `dev:implement` (QE rejects with feedback)

---

## SRE Statuses

| Status | Role | Description |
|--------|------|-------------|
| `sre:infra-setup` | SRE | Setting up test infrastructure |

SRE is a service role — after completing infrastructure work, the issue returns to its previous status.

---

## Content Writer Statuses

For documentation stories (`kind/docs`):

| Status | Role | Description |
|--------|------|-------------|
| `cw:write` | content writer | Writing documentation |
| `cw:review` | content writer | Reviewing documentation |

Content stories follow the same terminal path as regular stories: on review approval, transition to `po:merge` → auto-advance to `done`.

---

## Auto-Advance Statuses

Some statuses are handled automatically by the board scanner without dispatching a hat:

- `arch:sign-off` → auto-advances to `po:merge`. In the compact profile, the same agent that designed the epic signs off — no separate gate needed.
- `po:merge` → auto-advances to `done` for stories. Code review already approved the implementation — no separate merge gate needed.

---

## Supervised Mode

The compact profile uses supervised mode by default. Only these transitions require human approval via GitHub issue comments:

| Gate | Status | What's Presented |
|------|--------|-----------------|
| Design approval | `po:design-review` | Design doc summary |
| Plan approval | `po:plan-review` | Story breakdown |
| Final acceptance | `po:accept` | Completed epic summary |

All other transitions auto-advance without human interaction.

### How approval works

1. The agent adds a **review request comment** on the issue summarizing the artifact
2. The agent **returns control** and moves on to other work
3. The **human** reviews the artifact on GitHub and responds via an issue comment:
   - `Approved` (or `LGTM`) → agent advances the status on the next scan cycle
   - `Rejected: <feedback>` → agent reverts the status and appends the feedback
4. If no human comment is found, the issue stays at its review status — **the agent NEVER auto-approves**

### Idempotency

The agent adds only ONE review request comment per review gate. On subsequent scan cycles, it checks for a human response but does NOT re-comment if a review request is already present.

---

## Error Status

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
| superman | 🦸 | `### 🦸 superman — 2026-01-15T10:30:00Z` |

In the compact profile, the `<role>` in the comment header reflects which hat is acting (e.g., architect, dev, qe, lead, sre, cw) even though it is the same agent. This preserves audit trail clarity and compatibility with multi-member profiles.

Example:

```markdown
### 🏗️ architect — 2026-01-15T10:30:00Z

Design document produced. See `projects/my-project/knowledge/designs/epic-1.md`.
```

Comments are append-only. Never edit or delete existing comments.

---

## Milestone Format

Milestones are GitHub milestones on the team repo, managed via the `gh` skill.

**Fields:**

| Field | GitHub Mapping | Description |
|-------|---------------|-------------|
| `title` | Milestone title | Milestone name (e.g., `M1: Initial setup`) |
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

The compact profile uses a single-member self-transition model. All operations use the `gh` skill:

### Status Transitions

The agent transitions an issue's status by:
1. Using `gh project item-edit` to update the Status field on the project board
2. Adding an attribution comment documenting the transition

The same agent detects the new status on the next board scan cycle (querying the project board via `gh project item-list`) and dispatches the appropriate hat.

### Comments

The agent records work output by:
1. Adding a GitHub issue comment via `gh issue comment` using the standard comment format

### Pull Requests

PRs on the team repo are for team-level changes:
- Process document updates
- Knowledge file additions or modifications
- Invariant changes

PRs are NOT used for code changes to the project repo. Code changes go through the project's own review process.

---

## Process Evolution

The team process can evolve through two paths:

### Formal Path

1. Create a PR on the team repo proposing the change
2. Review the PR (self-review via lead hat)
3. Approve and merge

### Informal Path

1. Human comments on an issue or the team repo with the change request
2. Agent edits the process file directly
3. Commit the change to the team repo

The informal path is appropriate for urgent corrections or clarifications. The formal path is preferred for significant process changes.
