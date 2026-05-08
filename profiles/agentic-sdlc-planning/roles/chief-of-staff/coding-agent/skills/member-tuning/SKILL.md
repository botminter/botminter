---
name: member-tuning
description: >-
  Diagnoses and tunes individual member configurations by mapping symptoms to
  the responsible artifact — PROMPT.md, CLAUDE.md, ralph.yml (hats), skills, or
  PROCESS.md. Provides a diagnostic decision tree, inspection commands, example
  edits, and propagation steps for each artifact type.
  Use when asked to "tune a member", "fix a member", "troubleshoot a member",
  "member isn't working", "adjust member behavior", "member diagnostic",
  "why is the member doing X", or when an cos:exec:todo issue requests member tuning.
metadata:
  author: botminter
  version: 1.0.0
---

# Member Tuning Skill

Diagnose and fix member behavior problems by identifying the responsible
configuration artifact and applying targeted edits. Every significant change
is recorded as a decision in `agreements/decisions/`.

## When to Use

- An `cos:exec:todo` issue requests tuning or troubleshooting a member
- A retrospective action item of type `member-tuning` needs execution
- The operator reports a member is behaving incorrectly
- A member keeps making the same kind of mistake

## Diagnostic Decision Tree

Start here. Ask the operator to describe the symptom, then map it to an artifact.

| Symptom | Likely Artifact | Section |
|---------|----------------|---------|
| Member is doing the wrong thing / wrong scope | PROMPT.md | [Tune PROMPT.md](#tune-promptmd) |
| Member doesn't understand the workspace or context | CLAUDE.md | [Tune CLAUDE.md](#tune-claudemd) |
| Member isn't switching hats correctly / wrong behavior mode | ralph.yml (hats) | [Tune Hats](#tune-hats-ralphyml) |
| Member is missing a capability or using a broken one | skills | [Tune Skills](#tune-skills) |
| Member is blocked by the process / wrong status transitions | PROCESS.md | [Tune PROCESS.md](#tune-processmd) |
| Member keeps repeating the same mistake | knowledge/ or invariants/ | Defer to knowledge-manager skill |

If the symptom is ambiguous, inspect multiple artifacts. Start with the most
likely one and work outward.

## Identifying the Member

First, determine which member needs tuning:

```bash
# List members and their roles
ls members/

# Show a member's current config
cat members/<member>/ralph.yml
cat members/<member>/PROMPT.md
cat members/<member>/CLAUDE.md
```

## Tune PROMPT.md

PROMPT.md controls **what** the member does — its objective, scope, and
completion criteria.

### Inspect

```bash
cat members/<member>/PROMPT.md
```

Look for:
- Vague or missing objective
- Scope that's too broad or too narrow
- Missing completion criteria
- Stale references to completed work

### Common Fixes

**Scope too broad** — Member tries to do too much:
```markdown
# Before
## Objective
Handle all development tasks.

# After
## Objective
Implement backend API endpoints for the user service.
Only work on issues labeled `dev:todo` in the user-service milestone.
```

**Missing completion criteria** — Member doesn't know when to stop:
```markdown
## Completion Condition
Done when all issues in milestone "v0.5" are closed and all tests pass.
```

**Wrong focus** — Member works on the wrong things:
```markdown
## Scope
- Only pick up issues from the board with label `dev:todo`
- Do NOT work on infrastructure or deployment tasks
```

### Apply

Edit `members/<member>/PROMPT.md` in the team repo with the proposed changes.

## Tune CLAUDE.md

CLAUDE.md controls **how** the coding agent understands the workspace — project
context, build commands, architectural notes.

### Inspect

```bash
cat members/<member>/CLAUDE.md
```

Look for:
- Missing project-specific context (build commands, test commands)
- Wrong workspace layout assumptions
- Stale references to renamed files or removed modules
- Missing domain knowledge that causes repeated mistakes

### Common Fixes

**Missing build context**:
```markdown
## Build Commands
- `just build` — compile the project
- `just test` — run all tests
- `just lint` — run linters
```

**Wrong workspace assumptions**:
```markdown
## Workspace Layout
The project repo is at `projects/<project>/` (submodule).
The team repo is at `team/` (submodule).
Configuration lives in `team/members/<member>/`.
```

**Missing domain knowledge**:
```markdown
## Architecture Notes
The API uses a hexagonal architecture. Handlers are in `src/handlers/`,
domain logic in `src/domain/`, and ports in `src/ports/`.
```

### Apply

Edit `members/<member>/CLAUDE.md` in the team repo.

## Tune Hats (ralph.yml)

Hats in ralph.yml control **how** the member switches behavioral modes —
triggers, instructions, and event publishing.

### Inspect

```bash
cat members/<member>/ralph.yml
```

For each hat, check:
- **Triggers**: Does the hat activate on the right events?
- **Instructions**: Are the instructions clear and specific?
- **Publishes**: Does the hat emit the correct completion events?
- **Dispatch table**: If the member has a board-scanner hat, does it route
  to the correct hats?

### Common Fixes

**Wrong trigger** — Hat doesn't activate when it should:
```yaml
# Before — only triggers on explicit event
triggers:
  - event: "design.requested"

# After — also triggers from board scanner
triggers:
  - event: "design.requested"
  - event: "board.design_needed"
```

**Vague instructions** — Hat does inconsistent work:
```yaml
# Before
instructions: "Review the code"

# After
instructions: |
  Review the code for:
  1. Correctness — does it match the acceptance criteria?
  2. Style — does it follow project conventions?
  3. Tests — are there adequate tests?
  Publish review.approved or review.changes_requested.
```

**Missing hat** — Member lacks a needed capability:
Add a new hat to the `hats` list in ralph.yml with appropriate triggers,
instructions, and publish events.

**Board-scanner dispatch mismatch** — If modifying hat triggers or adding hats,
verify the board-scanner hat's dispatch table still routes correctly:
```bash
# Check which events board-scanner dispatches
grep -A 20 "board_scanner" members/<member>/ralph.yml
```

### Apply

Edit `members/<member>/ralph.yml` in the team repo.

## Tune Skills

Skills control **what tools** the member can invoke — SKILL.md files in
the skills directory.

### Inspect

```bash
# List current skills
ls members/<member>/coding-agent/skills/ 2>/dev/null

# Check ralph.yml for skill directory configuration
grep -A 5 "skills:" members/<member>/ralph.yml
```

### Common Fixes

**Missing skill** — Member lacks a needed capability:
1. Check if the skill already exists in the role skeleton: `ls roles/<role>/coding-agent/skills/`
2. If not, create a new `SKILL.md` in the appropriate skills directory
3. Ensure `ralph.yml` `skills.dirs` includes the path to the skills directory

**Broken skill** — Skill instructions are wrong or incomplete:
1. Read the SKILL.md: `cat members/<member>/coding-agent/skills/<skill>/SKILL.md`
2. Edit to fix the instructions
3. The fix should go in `roles/<role>/coding-agent/skills/<skill>/SKILL.md`
   to benefit all members in that role

**Stale skill reference** — ralph.yml references a skill directory that
doesn't exist:
1. Check `skills.dirs` in ralph.yml
2. Remove stale entries or create the missing directory

### Apply

Edit skills in `roles/<role>/coding-agent/skills/` (role-level) or
`members/<member>/coding-agent/skills/` (member-level) in the team repo.

## Tune PROCESS.md

PROCESS.md controls the **workflow** — status transitions, conventions, and
ceremonies. Only tune this when the process itself is the root cause.

### Inspect

```bash
cat PROCESS.md
```

Look for:
- Missing status transitions that block the member
- Auto-advance rules that skip needed review steps
- Status naming that doesn't match the member's hat triggers
- Missing conventions that lead to inconsistent behavior

### Common Fixes

**Missing status transition**:
```markdown
# Add to the status lifecycle section
| From | To | Trigger |
|------|-----|---------|
| dev:code-review | done | Review approved and tests pass |
```

**Auto-advance causing problems**:
```markdown
# Clarify when auto-advance should NOT happen
Auto-advance from `dev:implement` to `dev:code-review` only when:
- All acceptance criteria are met
- Tests pass locally
```

### Apply

Edit `PROCESS.md` in the team repo root. For large-scale process changes,
use the Process Evolution skill instead.

**Important**: Process changes affect ALL members, not just the target member.
Consider the broader impact before editing.

## Recording Changes

For significant tuning changes, write a decision record:

```bash
ls agreements/decisions/ | grep -oP '^\d+' | sort -n | tail -1
# Increment by 1, zero-pad to 4 digits
```

````markdown
---
id: <next-id>
type: decision
status: accepted
date: <today ISO date>
participants: [operator, chief-of-staff]
---
# Member Tuning: <member-name>

## Context
<What symptom was observed and why the change was needed>

## Decision
Modified `<artifact>` for member `<member-name>`:
- <Summary of changes>

## Impact
- Member behavior will change after sync and restart
- <Any side effects on other members or the process>
````

## Propagation

After any change to member configuration in `team/`:

1. **Commit** the changes in the team repo
2. **Sync** workspaces to propagate: `bm teams sync`
3. **Restart** the affected member: `bm stop <member> && bm start <member>`

Changes do NOT take effect until the member is restarted with the updated
configuration.

## Error Handling

- If the member directory doesn't exist, list available members and ask
  the operator to clarify.
- If `agreements/decisions/` doesn't exist, create it.
- If the symptom maps to knowledge/ or invariants/, defer to the
  knowledge-manager skill — do not duplicate that workflow here.
- If multiple artifacts need tuning, address them one at a time and
  verify each fix before moving to the next.
