---
name: retrospective
description: >-
  Guides a structured team retrospective examining what went well, what didn't,
  and produces typed action items. Outputs a retro summary to agreements/retros/.
  Use when asked to "run a retro", "do a retrospective", "reflect on the sprint",
  "what went well", "review team performance", or when an cos:exec:todo issue requests
  a retrospective.
metadata:
  author: botminter
  version: 1.0.0
---

# Retrospective Skill

Guide a structured retrospective conversation, gather data from the project
board, and produce a retro summary with typed action items.

## When to Use

- An `cos:exec:todo` issue requests a retrospective
- The operator asks to reflect on a sprint, milestone, or time period
- You want to identify improvement opportunities from completed work

## Procedure

### Step 1: Define Scope

Determine what period the retro covers:

- **Milestone-based**: A specific milestone (e.g., "v0.5 retrospective")
- **Time-based**: A date range (e.g., "last two weeks")
- **Epic-based**: A specific epic or feature area

Ask the operator or read the issue body to clarify the scope.

### Step 2: Gather Board Data

Query the project board for completed work within the retro scope.

```bash
# Use the github-project skill's query-issues operation
# By milestone:
#   query-issues --type milestone --milestone "<milestone>"
# By status:
#   query-issues --type status --status "done"
```

Gather these data points:

1. **Completed issues**: Count and list of closed issues in scope
2. **Rejection loops**: Count comments matching `"Changes requested"` or
   `"review.changes_requested"` patterns per issue — multiple rounds indicate
   friction
3. **Error statuses**: Issues that hit `error` status (check for `status/error`
   label or project status field)
4. **Long-lived in-progress**: Issues that stayed in `*:in-progress` for more
   than 2x the average cycle time
5. **Cycle time**: Time from `*:todo` to `*:done` for each issue

### Step 3: What Went Well

Identify patterns of success from the data:

- Issues completed quickly (below-average cycle time)
- Clean reviews (0-1 review rounds)
- Good designs that led to smooth implementation
- Effective collaboration patterns visible in comments
- Knowledge or tooling improvements that paid off

Document each positive pattern with supporting evidence (issue numbers, metrics).

### Step 4: What Didn't Go Well

Identify pain points and improvement areas:

- Long cycle times (above-average or outliers)
- Repeated rejections (3+ review rounds on a single issue)
- Issues that hit `error` status
- Blocked work (long periods with no progress)
- Missing context or unclear requirements visible in comment threads
- Process friction (manual steps that could be automated)

Document each pain point with supporting evidence.

### Step 5: Generate Action Items

For each identified improvement, create a typed action item:

| Type | When to Use | Follow-Through |
|------|------------|----------------|
| `process-change` | Workflow, status lifecycle, or ceremony changes | Create an `cos:exec:todo` issue referencing the retro |
| `role-change` | Add, remove, or restructure roles | Create an `cos:exec:todo` issue referencing the retro |
| `member-tuning` | Adjust PROMPT, CLAUDE.md, hats, skills, or PROCESS for a member | Create an `cos:exec:todo` issue referencing the retro |
| `knowledge-update` | Add or update knowledge docs | Can be handled by the knowledge-manager skill |
| `norm` | Propose a new team working agreement | Write directly to `agreements/norms/` |

Each action item must include:
- **Type**: One of the five types above
- **Description**: What should change and why
- **Evidence**: Issue numbers or metrics that support the recommendation
- **Priority**: high / medium / low

### Step 6: Write the Retro Summary

Write the output to `agreements/retros/NNNN-<title>.md` using the team
agreements convention format.

Determine the next sequence number:

```bash
ls agreements/retros/ | grep -oP '^\d+' | sort -n | tail -1
# Increment by 1, zero-pad to 4 digits
```

Use this template:

````markdown
---
id: <next-id>
type: retro
status: accepted
date: <today ISO date>
participants: [operator, chief-of-staff]
refs: [<issue-numbers-examined>]
---
# Retrospective: <scope title>

## Scope
<What period/milestone this retro covers>

## Data Summary
- Issues completed: N
- Average cycle time: X days
- Rejection loops (3+ rounds): N issues
- Error statuses: N issues

## What Went Well
<Bullet points with evidence>

## What Didn't Go Well
<Bullet points with evidence>

## Action Items

### <Action title>
- **Type**: `process-change` | `role-change` | `member-tuning` | `knowledge-update` | `norm`
- **Description**: <what should change>
- **Evidence**: <supporting data>
- **Priority**: high | medium | low

<Repeat for each action item>
````

### Step 7: Execute Immediate Actions

After writing the retro summary:

1. **For `norm` actions**: Write each norm directly to `agreements/norms/`
   using the norm format from the team agreements convention. Set status to
   `active` and reference the retro ID in `refs`.

2. **For `process-change`, `role-change`, `member-tuning` actions**: Create
   `cos:exec:todo` issues on the team repo for each, referencing the retro file:

   ```bash
   gh issue create --repo "$TEAM_REPO" \
     --title "<action title>" \
     --body "Follow-up from retro agreements/retros/NNNN-<title>.md\n\n<description>" \
     --label "cos:exec:todo"
   ```

3. **For `knowledge-update` actions**: Document the recommendation in the
   retro. The knowledge-manager skill or operator can pick these up.

### Step 8: Report

Summarize what was produced:
- Path to the retro file
- Number of action items by type
- Any `cos:exec:todo` issues created
- Any norms written to `agreements/norms/`

## Comment Format

All retrospective comments on issues use:

```
### 📋 chief-of-staff — $(date -u +%Y-%m-%dT%H:%M:%SZ)
```

## Error Handling

- If `gh` commands fail during data gathering, log the error and continue
  with available data. A retro with partial data is better than no retro.
- If the `agreements/retros/` directory doesn't exist, create it.
- If writing norms fails, document the intended norm in the retro summary
  and flag it for manual follow-up.
