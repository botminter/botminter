# Team Agreements Convention

Team agreements are structured records that capture decisions, retrospective outcomes, and working norms. They provide traceability for why team processes, roles, and workflows changed over time.

## Directory Structure

```
agreements/
  decisions/   # Formal team decisions (role changes, process changes, tool adoption)
  retros/      # Retrospective summaries (output of the retrospective skill)
  norms/       # Living team norms and working agreements
```

## File Naming

Files use sequential numbering with kebab-case titles:

```
NNNN-<kebab-case-title>.md
```

Examples: `0001-adopt-trunk-based-development.md`, `0002-sprint-3-retro.md`

## File Format

### Decisions and Retros

```yaml
---
id: 1
type: decision | retro
status: proposed | accepted | superseded
date: 2026-03-21
participants: [operator, chief-of-staff]
supersedes: null  # id of previous agreement if replacing one
refs: []          # related issue numbers, retro ids, etc.
---
# Title

## Context
Why this decision was needed.

## Decision
What was decided.

## Alternatives Considered
What else was considered and why it was rejected.

## Consequences
Expected outcomes, tradeoffs, follow-up actions.
```

### Norms

Norms use a simpler format for living working agreements:

```yaml
---
id: 1
type: norm
status: active | retired
date: 2026-03-21
refs: []
---
# Norm Title

**Agreement:** One-line statement of the norm.

**Rationale:** Why this norm exists.

**Adopted:** Date and context (e.g., "retro #2, 2026-03-15").
```

## Frontmatter Fields

| Field | Required | Values | Description |
|-------|----------|--------|-------------|
| `id` | Yes | Integer | Sequential identifier |
| `type` | Yes | `decision`, `retro`, `norm` | Category of agreement |
| `status` | Yes | See lifecycle below | Current state |
| `date` | Yes | ISO date | Creation date |
| `participants` | Decisions/retros | List of strings | Who participated |
| `supersedes` | No | Integer or null | ID of replaced agreement |
| `refs` | No | List | Related issues, retro IDs, etc. |

## Lifecycle

### Decisions
- `proposed` — Under discussion, not yet agreed
- `accepted` — Agreed and in effect
- `superseded` — Replaced by a newer decision (set `supersedes` on the new one)

### Retros
- Always `accepted` — they are records of what happened, not proposals

### Norms
- `active` — Currently in effect
- `retired` — No longer relevant or replaced

## Integration Points

- The **retrospective skill** writes summaries to `agreements/retros/`
- The **process evolution skill** writes decisions to `agreements/decisions/` before modifying PROCESS.md
- The **role management skill** writes decisions to `agreements/decisions/` before adding or removing roles
- The **member tuning skill** can reference agreements as justification for changes
