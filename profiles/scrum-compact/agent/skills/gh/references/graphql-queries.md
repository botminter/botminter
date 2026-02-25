# GraphQL Queries Reference

## The v3.0.0 Fix: Variable Type Handling

The critical fix in v3.0.0 was changing how GraphQL variables are passed to the `gh api` command.

### The Problem (v2.x and earlier)

Variables were embedded in the query string, causing type parsing errors:

```bash
# WRONG - Variables embedded as strings
gh api graphql -f query='
query {
  node(id: "'$PROJECT_ID'") {
    ... on ProjectV2 {
      items(first: 100) {
        nodes {
          id
        }
      }
    }
  }
}'
```

**Issue:** GraphQL expects `ID` type but receives a string literal. Causes parsing errors and silent failures.

### The Solution (v3.0.0)

Use `-F` (uppercase) for GraphQL ID types:

```bash
# CORRECT - Variables passed with proper types
gh api graphql -f query='
query($projectId: ID!, $itemId: ID!) {
  node(id: $projectId) {
    ... on ProjectV2 {
      items(first: 100) {
        nodes {
          id
          fieldValueByName(name: "Status") {
            ... on ProjectV2ItemFieldSingleSelectValue {
              name
            }
          }
        }
      }
    }
  }
}' -F projectId="$PROJECT_ID" -F itemId="$ITEM_ID"
```

**Key differences:**

1. Query uses **variables** (`$projectId`, `$itemId`) with type declarations
2. Variables passed with **`-F` flag** (uppercase) for proper type handling
3. GraphQL receives properly typed ID values instead of strings

### Flag Differences

| Flag | Type | Use Case |
|------|------|----------|
| `-f` (lowercase) | String | For string values, query text |
| `-F` (uppercase) | Typed (ID, Int, etc.) | For GraphQL typed variables |

## Status Verification Query

Used in `status-transition.sh` to verify status changes:

```bash
CURRENT_STATUS=$(gh api graphql -f query='
query($projectId: ID!, $itemId: ID!) {
  node(id: $projectId) {
    ... on ProjectV2 {
      items(first: 100) {
        nodes {
          id
          fieldValueByName(name: "Status") {
            ... on ProjectV2ItemFieldSingleSelectValue {
              name
            }
          }
        }
      }
    }
  }
}' -F projectId="$PROJECT_ID" -F itemId="$ITEM_ID" \
  | jq -r ".data.node.items.nodes[] | select(.id == \"$ITEM_ID\") | .fieldValueByName.name")
```

**What it does:**

1. Queries the project for all items
2. Filters to the specific item by ID
3. Extracts the current Status field value
4. Returns the status name as a string

**Why it's needed:**

The `gh project item-edit` command may succeed (exit code 0) but the status might not actually change due to:

- Permissions issues
- GraphQL API rate limits
- Invalid option IDs
- Transient API errors

Verification catches these silent failures.

## Project Structure Query

Used to understand GitHub Projects v2 structure:

```graphql
query($owner: String!, $number: Int!) {
  organization(login: $owner) {
    projectV2(number: $number) {
      id
      title
      fields(first: 20) {
        nodes {
          ... on ProjectV2SingleSelectField {
            id
            name
            options {
              id
              name
            }
          }
        }
      }
      items(first: 100) {
        nodes {
          id
          content {
            ... on Issue {
              number
              title
              state
            }
          }
          fieldValues(first: 20) {
            nodes {
              ... on ProjectV2ItemFieldSingleSelectValue {
                name
                field {
                  ... on ProjectV2SingleSelectField {
                    name
                  }
                }
              }
            }
          }
        }
      }
    }
  }
}
```

This query is not used in the current scripts but is useful for debugging.

## Why GraphQL Instead of gh CLI?

For some operations, the `gh` CLI doesn't provide sufficient detail:

| Need | gh CLI | GraphQL API |
|------|--------|-------------|
| Update status | ✅ `gh project item-edit` | ✅ Direct mutation |
| Verify status | ❌ No verification command | ✅ Query with field values |
| Get field options | ✅ `gh project field-list` | ✅ Query fields |
| Add item to project | ✅ `gh project item-add` | ✅ Mutation |

The verification step requires GraphQL because there's no `gh project item-get` command to check field values after update.

## Rate Limiting

GraphQL API has separate rate limits from REST:

- **REST API:** 5,000 requests/hour
- **GraphQL API:** 5,000 points/hour (complex queries cost more)

Our verification query costs ~1 point (simple node lookup).

## Common GraphQL Errors

### Error: "Could not resolve to a node with the global id"

**Cause:** Invalid ID format or ID doesn't exist

**Fix:** Verify IDs are from the correct project/organization

### Error: "Field 'fieldValueByName' doesn't exist"

**Cause:** Typo in field name or field doesn't exist on project

**Fix:** Check field name exactly matches project configuration

### Error: "Variable $projectId of type ID! was provided invalid value"

**Cause:** Using `-f` instead of `-F` for ID type variables

**Fix:** Use `-F` (uppercase) for all ID type variables

## Reference: GitHub Projects v2 Types

Common GraphQL types in Projects v2:

- `ProjectV2` - The project itself
- `ProjectV2Item` - An item in the project (issue, PR, draft)
- `ProjectV2Field` - A field definition (Status, Assignee, etc.)
- `ProjectV2SingleSelectField` - Single-select field (like Status)
- `ProjectV2ItemFieldSingleSelectValue` - A field value on an item
- `ProjectV2FieldOption` - An option in a single-select field

All these use opaque IDs like `PVT_...`, `PVTI_...`, `PVTSSF_...`.
