---
name: process-evolution
description: >-
  Guides deliberate, team-wide process changes — adding or removing statuses,
  modifying transitions, updating review gates, and evolving the workflow
  lifecycle. Validates changes against the status graph before applying and
  records every decision as a team agreement.
  Use when asked to "change the process", "add a status", "remove a status",
  "modify the workflow", "update transitions", "add a review gate",
  "evolve the process", "change auto-advance rules", or when an cos:exec:todo
  issue requests a process change.
metadata:
  author: botminter
  version: 1.0.0
---

# Process Evolution Skill

Evolve the team's workflow by modifying statuses, transitions, review gates,
and process conventions. Every change is validated against the status graph
and recorded in `agreements/decisions/`.

## When to Use

- An `cos:exec:todo` issue requests a process or workflow change
- A retrospective action item of type `process-change` needs execution
- The operator wants to add, remove, or modify statuses in the lifecycle
- The team needs a new review gate or wants to remove one
- Auto-advance rules need updating
- Comment conventions or label schemes need changing

## Understanding the Process Files

Process changes touch multiple files that must stay in sync:

| File | What It Controls |
|------|-----------------|
| `PROCESS.md` | Human-readable workflow documentation, status lifecycles, conventions |
| `botminter.yml` | Machine-readable statuses, labels, views, project configuration |
| `roles/*/ralph.yml` | Hat triggers — which hats activate on which statuses |
| `coding-agent/skills/board-scanner/SKILL.md` | Dispatch tables — how the board scanner routes work |
| `coding-agent/skills/status-workflow/` | GraphQL mutations for status transitions |

**Rule**: Never update one file without checking all others for consistency.

## Show Current Process

Before making changes, render the current status graph:

```bash
# Epic lifecycle statuses
grep -A 30 "Epic Lifecycle" PROCESS.md

# Story lifecycle statuses
grep -A 50 "Story Lifecycle" PROCESS.md

# Machine-readable statuses
grep -A 20 "statuses:" botminter.yml

# Hat triggers per role
for role_dir in roles/*/; do
  echo "=== $(basename "$role_dir") ==="
  grep -B1 -A3 "triggers:" "$role_dir"ralph.yml 2>/dev/null || echo "  (no ralph.yml)"
done
```

Summarize the lifecycle as a table:

| Status | Role | Next (happy) | Next (reject) | Auto-advance? | Review gate? |
|--------|------|--------------|---------------|---------------|-------------|
| ... | ... | ... | ... | ... | ... |

## Adding a Status

### Conversational Flow

1. **Ask**: What role owns this status? (Must be an existing role — if not, defer to the role-management skill first)
2. **Ask**: Where in the lifecycle? (After which status? Before which status?)
3. **Ask**: Is it a review gate? (If yes, add to supervised mode config)
4. **Ask**: Does it auto-advance? (If yes, add to auto-advance config)
5. **Validate**: Run status graph validation (see below)
6. **Apply**: Update all affected files together
7. **Record**: Write an agreement decision

### Apply Checklist

- [ ] Add status to `botminter.yml` statuses list
- [ ] Add status to `PROCESS.md` lifecycle table
- [ ] Add hat trigger for the owning role in `roles/<role>/ralph.yml`
- [ ] Update board-scanner dispatch table if applicable
- [ ] Update status-workflow skill if it has hardcoded status lists
- [ ] Add label if the status needs one (check label conventions)

## Removing a Status

### Impact Analysis (before removal)

```bash
# Issues currently in this status
gh project item-list <project-number> --format json | jq '.[] | select(.status == "<status>")'

# Hats triggered by this status
grep -rn "<status>" roles/*/ralph.yml

# References in PROCESS.md
grep -n "<status>" PROCESS.md

# Board scanner references
grep -n "<status>" coding-agent/skills/board-scanner/SKILL.md
```

### Conversational Flow

1. **Check**: Are there issues currently in this status? If yes, propose reassignment
2. **Check**: Which hats trigger on this status? List them for the operator
3. **Check**: Is this status in any rejection loop? Identify the loop
4. **Propose**: Show the updated lifecycle without this status
5. **Validate**: Run status graph validation on the proposed state
6. **Apply**: Update all affected files
7. **Record**: Write an agreement decision

## Modifying Transitions

When changing what status follows another:

1. **Show** the current transition being modified
2. **Propose** the new transition
3. **Validate** the resulting graph (no orphans, no dead ends, no infinite loops)
4. **Apply** to PROCESS.md and any hat triggers that reference the transition
5. **Record** the decision

## Adding or Removing Review Gates

Review gates are supervised-mode checkpoints where human approval is required.

**Adding a gate**:
- Add the status to the supervised mode configuration
- Document the gate in PROCESS.md with the approval/rejection criteria
- Ensure a hat exists that handles the gate status
- The gate status should NOT auto-advance

**Removing a gate**:
- Remove from supervised mode configuration
- Update PROCESS.md to reflect the new flow
- Consider whether the status should now auto-advance
- Verify no other process depends on the human review at this point

## Modifying Auto-Advance Rules

Auto-advance moves issues forward without human intervention. Changes here
affect workflow speed and oversight.

1. **Show** current auto-advance statuses
2. **Propose** the change (add or remove auto-advance)
3. **Validate**: Auto-advance statuses must NOT be review gates
4. **Apply** to relevant hat instructions and PROCESS.md
5. **Record** the decision

## Status Graph Validation

Before applying ANY process change, validate the resulting status graph:

### Orphan Status Check

Every status must be reachable from an entry point (the first status in the
lifecycle). Walk the graph forward from entry points — any status not reached
is an orphan.

```
Entry → status_a → status_b → status_c (terminal)
                 ↘ status_d → status_c
If status_e exists but is not reachable → ORPHAN — reject the change
```

### Dead-End Check

Every non-terminal status must have at least one outgoing transition. A status
with no outgoing transition that is not explicitly marked as terminal is a
dead end.

```
status_a → status_b → ??? (no outgoing, not terminal) → DEAD END
```

### Infinite Loop Check

Rejection loops must always have a forward path. If a cycle exists with no
exit to a forward status, issues will loop forever.

```
status_a → status_b → status_a (rejection loop)
status_b → status_c (forward path exists) → OK
status_b has NO forward path → INFINITE LOOP — reject
```

### Role Coverage Check

Every status must have a role that can handle it — meaning a hat with
matching triggers exists in some role's ralph.yml.

### Board Scanner Consistency Check

The board scanner's dispatch table must include every status that requires
dispatching. Compare the dispatch table against `botminter.yml` statuses.

## Updating Comment Conventions

Comment format changes affect all members:

1. **Show** current emoji mappings and attribution format from PROCESS.md
2. **Propose** the change
3. **Apply** to PROCESS.md comment convention section
4. **Verify** hat instructions reference the updated format
5. **Record** the decision

## Adding or Removing Labels

Label changes affect issue classification:

1. **Show** current labels from `botminter.yml`
2. **Propose** the change (add/remove/rename)
3. **Apply** to `botminter.yml` labels section
4. **Update** PROCESS.md label documentation
5. **Record** the decision

## Retro-Driven Process Changes

The skill can start from retrospective outputs:

```bash
# Find retro action items tagged for process changes
ls agreements/retros/
# Look for action items with type: process-change
grep -l "process-change" agreements/retros/*.md
```

Present these as conversation starting points, then follow the appropriate
flow above. Cross-reference the final decision back to the retro.

## Recording Decisions

Every process change MUST be recorded in `agreements/decisions/`:

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
# Process Change: <summary>

## Context
<What prompted this change — retro action item, operator request, observed problem>

## Before
<Current status graph / configuration snippet>

## After
<New status graph / configuration snippet>

## Affected Files
- PROCESS.md: <what changed>
- botminter.yml: <what changed>
- roles/*/ralph.yml: <what changed>
- board-scanner: <what changed>

## Validation
- Orphan check: PASS
- Dead-end check: PASS
- Loop check: PASS
- Role coverage: PASS
- Board scanner consistency: PASS
````

## Propagation

After any process change:

1. **Commit** the changes in the team repo
2. **Sync** workspaces to propagate: `bm teams sync`
3. **Restart** all members: `bm stop && bm start`

Process changes affect ALL members — every workspace must be synced and
every member restarted.

## Error Handling

- If `PROCESS.md` doesn't exist, check that you're in the team repo root
- If `botminter.yml` doesn't exist, the profile may not use it — check the
  profile documentation
- If `agreements/decisions/` doesn't exist, create it
- If a role referenced in a status doesn't exist, defer to the role-management
  skill before proceeding
- If validation fails, explain the specific failure and propose alternatives —
  do not apply the change
