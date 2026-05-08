---
name: team-design
description: >-
  Entry point for all day-2 team design operations. Routes operator intent to
  the appropriate sub-skill — retrospective, role-management, member-tuning, or
  process-evolution — and provides a unified dashboard of team design state.
  Use when asked to "design the team", "show me the team", "team overview",
  "what's our team setup", "let's evolve the team", "team health check",
  or when the operator's intent spans multiple team design areas.
metadata:
  author: botminter
  version: 1.0.0
---

# Team Design Hub

Conversational front door for all day-2 team design operations. Detect what
the operator needs, load the right sub-skill, and pass along relevant context
(recent agreements, pending action items).

## When to Use

- The operator wants to change or inspect the team's design
- A request could map to multiple sub-skills and needs triage
- The operator asks for a team overview or dashboard
- A retro has produced action items that span multiple areas

## Intent Routing

Classify the operator's intent and load the matching skill:

| Operator says... | Sub-skill | Load command |
|-----------------|-----------|-------------|
| "Let's do a retro" / "what went well" / "how did the sprint go" / "reflect on the milestone" | retrospective | `ralph tools skill load retrospective` |
| "Add a role" / "remove a role" / "we need a new role" / "list roles" / "team structure" | role-management | `ralph tools skill load role-management` |
| "X member isn't working right" / "fix the architect's hats" / "tune the prompt" / "member diagnostic" | member-tuning | `ralph tools skill load member-tuning` |
| "Change the process" / "add a review step" / "modify the workflow" / "add a status" / "evolve the process" | process-evolution | `ralph tools skill load process-evolution` |
| "Show me the team" / "what's our current setup" / "team overview" / "team dashboard" | Dashboard (below) | — |

If the intent is ambiguous, ask one clarifying question before routing.

## Routing Procedure

1. **Classify**: Match the operator's request to one of the five intents above
2. **Gather context**: Read recent agreements and pending action items (see below)
3. **Brief the operator**: Summarize relevant context before handing off
4. **Load the skill**: Use `ralph tools skill load <skill-name>` to hand off
5. **After completion**: Ask if there are follow-up actions for other sub-skills

### Gathering Context Before Handoff

Before loading a sub-skill, check for relevant context to pass along:

```bash
# Recent decisions (last 5)
ls -t agreements/decisions/ 2>/dev/null | head -5

# Recent norms
ls -t agreements/norms/ 2>/dev/null | head -5

# Pending retro action items
for f in $(ls -t agreements/retros/ 2>/dev/null | head -3); do
  grep -A3 "Priority.*high\|Priority.*medium" "agreements/retros/$f" 2>/dev/null
done
```

Summarize any relevant findings for the operator before loading the sub-skill.

## Dashboard View

When the operator asks for a team overview, present this information:

### Roles and Members

```bash
# List roles defined in botminter.yml
grep -A 20 "roles:" botminter.yml 2>/dev/null

# Count members per role
for role_dir in roles/*/; do
  role=$(basename "$role_dir")
  members=$(ls "$role_dir"members/ 2>/dev/null | wc -l)
  echo "$role: $members member(s)"
done
```

### Recent Agreements

```bash
# Last 5 decisions
echo "=== Recent Decisions ==="
for f in $(ls -t agreements/decisions/ 2>/dev/null | head -5); do
  head -10 "agreements/decisions/$f"
  echo "---"
done

# Active norms
echo "=== Active Norms ==="
for f in agreements/norms/*.md; do
  grep -l "status: active" "$f" 2>/dev/null
done
```

### Pending Action Items

```bash
# Scan retros for unresolved action items
echo "=== Pending Retro Action Items ==="
for f in $(ls -t agreements/retros/ 2>/dev/null | head -3); do
  echo "--- From $f ---"
  grep -B1 -A4 "Type.*process-change\|Type.*role-change\|Type.*member-tuning" \
    "agreements/retros/$f" 2>/dev/null
done

# Check for open cos:exec:todo issues
# Use the github-project skill's query-issues operation:
#   query-issues --type label --label "cos:exec:todo"
```

### Process Summary

```bash
# Key statuses and gates
grep -A 5 "Review Gates\|Supervised" PROCESS.md 2>/dev/null
grep -A 15 "Epic Lifecycle\|Story Lifecycle" PROCESS.md 2>/dev/null | head -20
```

Present the dashboard in a readable format. Highlight items that need attention
(pending action items, open cos:exec:todo issues).

## Retro-First Flow

When the operator says "let's design the team" or gives a broad request:

1. **Suggest retro-first**: "Want to start with a retro to identify what needs
   changing? This gives us data-driven action items to work from."
2. **If accepted**: Load the retrospective skill:
   ```bash
   ralph tools skill load retrospective
   ```
3. **After retro completes**: Review the action items produced
4. **Route each action item** to the appropriate sub-skill:
   - `process-change` items → `ralph tools skill load process-evolution`
   - `role-change` items → `ralph tools skill load role-management`
   - `member-tuning` items → `ralph tools skill load member-tuning`
   - `norm` items → Already handled by the retro (written to agreements/norms/)
   - `knowledge-update` items → Flag for knowledge-manager or manual follow-up
5. **Track progress**: After each sub-skill completes, summarize what was done
   and move to the next action item

## Agreement Context Passing

When routing to any sub-skill, include relevant context:

- **For retrospective**: No prior context needed — the retro gathers its own data
- **For role-management**: Recent role-related decisions from `agreements/decisions/`
- **For member-tuning**: Recent member-tuning decisions and the member's current
  configuration summary
- **For process-evolution**: Recent process decisions and current status lifecycle
  summary

This prevents the operator from repeating decisions already made.

## Error Handling

- If `agreements/` directories don't exist, note that the team agreements
  convention hasn't been set up and proceed without agreement context
- If no sub-skills are installed, list what's expected and suggest running
  `ralph tools skill list` to check
- If the operator's intent doesn't match any sub-skill, ask for clarification
  rather than guessing
