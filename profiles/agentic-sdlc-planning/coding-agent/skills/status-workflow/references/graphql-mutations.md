# GraphQL Mutations Reference

## Status Verification Query

After updating a project item's status via `gh project item-edit`, verify the change took effect using this GraphQL query:

```bash
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
```

This queries the specific project item by node ID — no pagination, works regardless of project size.

**Why verify?** The `gh project item-edit` command may exit 0 without the status actually changing — permissions issues, rate limits, or invalid option IDs cause silent failures.

## Variable Type Handling

GitHub's GraphQL API requires proper variable typing. The `gh api graphql` command uses different flags for different types:

| Flag | Type | Use For |
|------|------|---------|
| `-f` (lowercase) | String | Query text, string values |
| `-F` (uppercase) | Typed (ID, Int) | Node IDs (`PVT_...`, `PVTI_...`), integers |

**Always use `-F` for ID type variables.** Using `-f` for IDs causes `Variable type mismatch` errors.

```bash
# CORRECT
gh api graphql -f query='...' -F projectId="$PROJECT_ID" -F itemId="$ITEM_ID"

# WRONG — treats IDs as strings
gh api graphql -f query='...' -f projectId="$PROJECT_ID" -f itemId="$ITEM_ID"
```

## Status Field Update (item-edit)

The `gh project item-edit` command updates a project item's field value:

```bash
gh project item-edit \
  --project-id "$PROJECT_ID" \
  --id "$ITEM_ID" \
  --field-id "$STATUS_FIELD_ID" \
  --single-select-option-id "$OPTION_ID"
```

| Parameter | Source | Description |
|-----------|--------|-------------|
| `--project-id` | `gh project view` | Project node ID (`PVT_...`) |
| `--id` | `gh project item-list` | Item node ID (`PVTI_...`) |
| `--field-id` | `gh project field-list` | Status field ID (`PVTSSF_...`) |
| `--single-select-option-id` | Field data JSON | Status option ID |

## Resolving Option IDs

Status options are single-select field values. Resolve the option ID from cached field data:

```bash
OPTION_ID=$(echo "$FIELD_DATA" | jq -r \
  '.fields[] | select(.name=="Status") | .options[] | select(.name=="'"$TARGET_STATUS"'") | .id')
```

To list all available status options:

```bash
echo "$FIELD_DATA" | jq -r '.fields[] | select(.name=="Status") | .options[] | .name'
```

## Resolving Item IDs

Get the project item ID for a repository issue:

```bash
ITEM_ID=$(gh project item-list "$PROJECT_NUM" --owner "$OWNER" --format json \
  --jq ".items[] | select(.content.number == $ISSUE_NUM) | .id")
```

If the issue is not in the project, add it first:

```bash
gh project item-add "$PROJECT_NUM" --owner "$OWNER" \
  --url "https://github.com/$TEAM_REPO/issues/$ISSUE_NUM"
```

## Common GraphQL Errors

### "Could not resolve to a node with the global id"

Invalid or non-existent node ID. Verify IDs come from the correct project.

### "Variable $projectId of type ID! was provided invalid value"

Using `-f` instead of `-F` for ID type variables. Switch to uppercase `-F`.

### "Field 'fieldValueByName' doesn't exist"

Field name mismatch. Verify the project has a field named exactly "Status" (case-sensitive).

## GitHub Projects v2 ID Types

| Type | Prefix | Example |
|------|--------|---------|
| Project | `PVT_` | `PVT_kwDOAbcdef` |
| Item | `PVTI_` | `PVTI_lADOAbcdef` |
| Single-Select Field | `PVTSSF_` | `PVTSSF_lADOAbcdef` |
| Field Option | (varies) | Opaque ID from field data |
