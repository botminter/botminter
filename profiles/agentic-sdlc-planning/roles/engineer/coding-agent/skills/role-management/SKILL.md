---
name: role-management
description: >-
  Manages team role composition — list, add, remove, and inspect roles defined
  in botminter.yml. Includes impact analysis of statuses, hats, and knowledge
  before changes, and records every change as a team agreement decision.
  Use when asked to "add a role", "remove a role", "list roles", "inspect a role",
  "change team composition", "what roles do we have", "team structure",
  or when an cos:exec:todo issue requests a role change.
metadata:
  author: botminter
  version: 1.0.0
---

# Role Management Skill

Manage team role composition through guided, conversational workflows. Every
role change is recorded as a decision in `agreements/decisions/`.

## When to Use

- An `cos:exec:todo` issue requests adding, removing, or modifying a role
- The operator asks about the team's role structure
- You need to understand what a role does (hats, statuses, skills)
- A retrospective action item of type `role-change` needs execution

## Operations

### List Roles

Show current roles from `botminter.yml` with context.

```bash
# Read the manifest
cat botminter.yml
```

For each role, display:

| Field | Source |
|-------|--------|
| Name | `roles[].name` in botminter.yml |
| Description | `roles[].description` in botminter.yml |
| Member count | `ls members/` and match by role |
| Associated statuses | Statuses whose prefix matches the role (e.g., `dev:*` for developer) |
| Skills | `ls roles/<role>/coding-agent/skills/` |

### Add Role

Guide the operator through defining a new role. This is a conversational,
multi-step process.

#### Step 1: Understand the Role

Ask the operator:
- What does this role do? What gap does it fill?
- What existing role is it closest to? (Use as a template)

#### Step 2: Define Statuses

Ask: What statuses does this role need?

- Check for conflicts with existing status prefixes in `botminter.yml`
- Suggest a prefix based on the role name (e.g., `sec` for security-auditor)
- Minimum: `<prefix>:todo`, `<prefix>:in-progress`, `<prefix>:done`

#### Step 3: Define Hats

Ask: What hats should this role wear?

- Reference existing hats as templates (show hat names from similar roles)
- Each hat needs: name, trigger event, instructions
- Generate a `ralph.yml` with the hat collection

#### Step 4: Generate Skeleton

Create the role directory structure:

```
roles/<new-role>/
  coding-agent/
    skills/
      .gitkeep
    knowledge/
      .gitkeep
    invariants/
      .gitkeep
  PROMPT.md        # Role purpose and responsibilities
  CLAUDE.md        # Agent context and workspace model
  ralph.yml        # Hat collection
```

Generate each file with sensible defaults based on the conversation.

**PROMPT.md template:**

````markdown
# <Role Display Name>

## Purpose
<What the operator described>

## Responsibilities
- Scan for `<prefix>:todo` issues on the board
- <Role-specific responsibilities from conversation>

## Process
Follow PROCESS.md for status transitions and conventions.
````

**CLAUDE.md template:**

````markdown
# CLAUDE.md

## Role Context
You are the <role-name> for this team. <Brief description>.

## Workspace Layout
See team CLAUDE.md for the workspace model.

## Knowledge Resolution
Follow the standard knowledge resolution order documented in team CLAUDE.md.
````

#### Step 5: Update botminter.yml

Add the role to the `roles` list:

```yaml
roles:
  - name: <new-role>
    description: "<description from conversation>"
```

Add the new statuses to the `statuses` list.

Add a view entry if the role has its own status prefix:

```yaml
views:
  - name: "<Display Name>"
    prefixes: ["<prefix>"]
    also_include: ["done", "error"]
```

#### Step 6: Record Decision

Write a decision record to `agreements/decisions/`:

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
# Add Role: <role-name>

## Context
<Why the role was added — from conversation>

## Decision
Added role `<role-name>` to the team with:
- Statuses: <list>
- Hats: <list>
- Skills: <list or "none yet">

## Impact
- New statuses added to botminter.yml
- New role directory created at `roles/<role-name>/`
- No existing members affected

## Follow-Up
- Run `bm hire <role-name> --name <suggested-name>` to hire a member
- Run `bm teams sync` to provision the workspace
- Consider adding role-specific knowledge to `roles/<role-name>/coding-agent/knowledge/`
````

#### Step 7: Guide Next Steps

Tell the operator:
- `bm hire <new-role> --name <name>` to hire a member into this role
- `bm teams sync` to provision the workspace
- Consider writing knowledge docs for the new role

### Remove Role

Removing a role requires impact analysis before any changes.

#### Step 1: Impact Analysis

Before proceeding, analyze the impact:

1. **Active members**: Check `members/` for members in this role
   ```bash
   ls members/ | while read m; do
     grep -l "role: <role>" members/$m/config.yml 2>/dev/null && echo "$m"
   done
   ```

2. **Owned statuses**: Find statuses in botminter.yml with the role's prefix
   ```bash
   grep "name: \"<prefix>:" botminter.yml
   ```

3. **Active issues**: Check for issues in the role's statuses
   ```bash
   # Use the github-project skill's query-issues operation:
   #   query-issues --type status --status "<prefix>:<status>"
   ```

4. **Hat references**: Check if other roles reference this role's statuses
   ```bash
   grep -r "<prefix>:" roles/*/ralph.yml
   ```

5. **Knowledge/invariants**: Check for role-scoped knowledge
   ```bash
   ls roles/<role>/coding-agent/knowledge/ 2>/dev/null
   ls roles/<role>/coding-agent/invariants/ 2>/dev/null
   ```

Present the impact summary to the operator:

```
Impact Analysis for removing role '<role-name>':
- Active members: N (list names)
- Owned statuses: N (list them)
- Active issues in role statuses: N
- Hat references from other roles: N
- Knowledge files: N
- Invariant files: N
```

#### Step 2: Confirm with Operator

If there are active members or issues:
- **WARN**: "There are N active members in this role. They must be stopped first."
- **WARN**: "There are N issues in statuses owned by this role. They will become orphaned."
- Ask: "Should the orphaned statuses be reassigned to another role?"

Wait for explicit confirmation before proceeding.

#### Step 3: Execute Removal

1. Remove the role directory: `rm -rf roles/<role>/`
2. Remove the role from `botminter.yml` roles list
3. Remove associated statuses from `botminter.yml` (or reassign)
4. Remove the associated view from `botminter.yml`

#### Step 4: Record Decision

Write a decision record to `agreements/decisions/`:

````markdown
---
id: <next-id>
type: decision
status: accepted
date: <today ISO date>
participants: [operator, chief-of-staff]
---
# Remove Role: <role-name>

## Context
<Why the role was removed — from conversation>

## Decision
Removed role `<role-name>` from the team.

## Impact
- Statuses removed/reassigned: <list>
- Members affected: <list or "none">
- Issues affected: <count>
- Knowledge/invariants removed: <list or "none">

## Follow-Up
- Stop affected members: `bm stop` then re-start without the removed members
- Clean up member workspaces if needed
- Run `bm teams sync` to reconcile
````

### Inspect Role

Show a role's complete configuration. This is read-only.

Display:

1. **Role definition**: Name and description from botminter.yml
2. **Member skeleton**: Contents of `roles/<role>/`
3. **Hats**: List hats from the role's `ralph.yml`
4. **Skills**: List skills from `roles/<role>/coding-agent/skills/`
5. **Knowledge**: List knowledge docs from `roles/<role>/coding-agent/knowledge/`
6. **Invariants**: List invariants from `roles/<role>/coding-agent/invariants/`
7. **Associated statuses**: Statuses with matching prefix from botminter.yml
8. **Active members**: Members hired into this role

```bash
# Hats from ralph.yml
cat roles/<role>/ralph.yml | yq '.hats[].name'

# Skills
ls roles/<role>/coding-agent/skills/

# Knowledge
ls roles/<role>/coding-agent/knowledge/

# Members in this role
for m in members/*/; do
  if grep -q "role: <role>" "$m/config.yml" 2>/dev/null; then
    basename "$m"
  fi
done
```

## Comment Format

All role management comments on issues use:

```
### 🏗 chief-of-staff — $(date -u +%Y-%m-%dT%H:%M:%SZ)
```

## Error Handling

- If `botminter.yml` cannot be parsed, stop and report the error.
- If the `agreements/decisions/` directory doesn't exist, create it.
- If removing a role with active members, refuse unless the operator
  explicitly confirms after seeing the impact analysis.
- If `gh` commands fail during impact analysis, log the error and
  continue with locally available data.
