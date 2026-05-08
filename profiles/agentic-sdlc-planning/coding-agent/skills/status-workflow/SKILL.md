---
name: status-workflow
description: >-
  Performs GitHub Projects v2 status transitions for issue workflow.
  Use when you need to "move issue to status", "transition status",
  "update project status", "set status field", or "change issue state".
  Handles status field mutations, GraphQL verification, and comment attribution.
  Works with cached board-scanner environment variables.
metadata:
  author: botminter
  version: 1.0.0
  category: workflow
  tags: [github, projects-v2, status, workflow, transitions]
  requires-tools: [gh, jq]
---

# Status Workflow

Performs status transitions on GitHub Projects v2 items. This skill covers the mutation side of status management — updating the Status field on project items, verifying the update via GraphQL, and posting attribution comments.

## Prerequisites

Status transitions require these environment variables, typically set by the board-scanner skill during board scanning:

| Variable | Source | Contains |
|----------|--------|----------|
| `OWNER` | Board scanner | GitHub org/user owning the project |
| `PROJECT_NUM` | Board scanner | Project number |
| `PROJECT_ID` | Board scanner | Project node ID (for GraphQL) |
| `STATUS_FIELD_ID` | Board scanner | Status field node ID |
| `FIELD_DATA` | Board scanner | Cached field list JSON |
| `TEAM_REPO` | Board scanner | `owner/repo` for the team repo |

If these variables are not available, resolve them manually:

```bash
TEAM_REPO=$(cd team && git remote get-url origin | sed 's|.*github.com[:/]||;s|\.git$||')
OWNER=$(echo "$TEAM_REPO" | cut -d/ -f1)
PROJECT_NUM=$(gh project list --owner "$OWNER" --format json | jq -r '.projects[0].number')
PROJECT_ID=$(gh project view "$PROJECT_NUM" --owner "$OWNER" --format json | jq -r '.id')
FIELD_DATA=$(gh project field-list "$PROJECT_NUM" --owner "$OWNER" --format json)
STATUS_FIELD_ID=$(echo "$FIELD_DATA" | jq -r '.fields[] | select(.name=="Status") | .id')
```

## Performing a Status Transition

### Step 1: Resolve IDs

```bash
# Get the project item ID for the issue
ITEM_ID=$(gh project item-list "$PROJECT_NUM" --owner "$OWNER" --format json \
  --jq ".items[] | select(.content.number == $ISSUE_NUM) | .id")

# Get the option ID for the target status
OPTION_ID=$(echo "$FIELD_DATA" | jq -r \
  '.fields[] | select(.name=="Status") | .options[] | select(.name=="'"$TARGET_STATUS"'") | .id')
```

### Step 2: Validate

```bash
# Ensure item exists in project
if [ -z "$ITEM_ID" ] || [ "$ITEM_ID" = "null" ]; then
  # Auto-add issue to project
  gh project item-add "$PROJECT_NUM" --owner "$OWNER" \
    --url "https://github.com/$TEAM_REPO/issues/$ISSUE_NUM"
  sleep 2
  ITEM_ID=$(gh project item-list "$PROJECT_NUM" --owner "$OWNER" --format json \
    --jq ".items[] | select(.content.number == $ISSUE_NUM) | .id")
fi

# Ensure target status is valid
if [ -z "$OPTION_ID" ] || [ "$OPTION_ID" = "null" ]; then
  echo "Status '$TARGET_STATUS' not found. Available:"
  echo "$FIELD_DATA" | jq -r '.fields[] | select(.name=="Status") | .options[] | .name'
fi
```

### Step 3: Update Status

```bash
gh project item-edit \
  --project-id "$PROJECT_ID" \
  --id "$ITEM_ID" \
  --field-id "$STATUS_FIELD_ID" \
  --single-select-option-id "$OPTION_ID"
```

### Step 4: Verify (Recommended)

Use the GraphQL verification query to confirm the status actually changed. The `gh project item-edit` command may return success even when the status did not change (permissions issues, transient API errors).

```bash
sleep 1
CURRENT_STATUS=$(gh api graphql -f query='
query($itemId: ID!) {
  node(id: $itemId) {
    ... on ProjectV2Item {
      fieldValueByName(name: "Status") {
        ... on ProjectV2ItemFieldSingleSelectValue {
          name
        }
      }
    }
  }
}' -F itemId="$ITEM_ID" \
  | jq -r '.data.node.fieldValueByName.name')

if [ "$CURRENT_STATUS" != "$TARGET_STATUS" ]; then
  echo "Verification failed: expected '$TARGET_STATUS', got '$CURRENT_STATUS'"
fi
```

**Critical:** Use `-F` (uppercase) for GraphQL ID type variables, not `-f` (lowercase). This query looks up the specific project item by node ID — no pagination needed.

### Step 5: Post Attribution Comment

```bash
TIMESTAMP=$(date -u +%Y-%m-%dT%H:%M:%SZ)
gh issue comment "$ISSUE_NUM" --repo "$TEAM_REPO" \
  --body "### $EMOJI $ROLE — $TIMESTAMP

Status: $FROM_STATUS -> $TARGET_STATUS"
```

Where `ROLE` and `EMOJI` come from `.botminter.yml` in the workspace root.

## Quick Reference

Minimal transition (when board-scanner variables are available):

```bash
ITEM_ID=$(gh project item-list "$PROJECT_NUM" --owner "$OWNER" --format json \
  --jq ".items[] | select(.content.number == $ISSUE_NUM) | .id")
OPTION_ID=$(echo "$FIELD_DATA" | jq -r \
  '.fields[] | select(.name=="Status") | .options[] | select(.name=="'"$TARGET_STATUS"'") | .id')
gh project item-edit --project-id "$PROJECT_ID" --id "$ITEM_ID" \
  --field-id "$STATUS_FIELD_ID" --single-select-option-id "$OPTION_ID"
```

## Label Operations

Some workflow transitions also require label changes:

### Adding a Label

```bash
gh issue edit "$ISSUE_NUM" --repo "$TEAM_REPO" --add-label "label-name"
```

### Removing a Label

```bash
gh issue edit "$ISSUE_NUM" --repo "$TEAM_REPO" --remove-label "label-name"
```

### Common Label Patterns

| Operation | Labels |
|-----------|--------|
| Classify issue | Use native issue types (Epic, Task, Bug) |
| Link story to parent | Use native sub-issues |
| Assign to role | `role/<role-name>` |

## References

- **[GraphQL Mutations](references/graphql-mutations.md)** — Verification queries, variable type handling, mutation templates
