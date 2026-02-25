#!/bin/bash
# Update issue status with GraphQL verification (v3.0.0 fix)

# Source common setup
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "$SCRIPT_DIR/setup.sh"

# Parse arguments
ISSUE_NUM=""
FROM_STATUS=""
TO_STATUS=""

while [[ $# -gt 0 ]]; do
  case $1 in
    --issue)
      ISSUE_NUM="$2"
      shift 2
      ;;
    --from)
      FROM_STATUS="$2"
      shift 2
      ;;
    --to)
      TO_STATUS="$2"
      shift 2
      ;;
    *)
      echo "❌ ERROR: Unknown argument: $1"
      exit 1
      ;;
  esac
done

# Validate inputs
if [ -z "$ISSUE_NUM" ]; then
  echo "❌ ERROR: --issue is required"
  exit 1
fi

if [ -z "$TO_STATUS" ]; then
  echo "❌ ERROR: --to is required"
  exit 1
fi

# FROM_STATUS is optional (used for comment only)
if [ -z "$FROM_STATUS" ]; then
  FROM_STATUS="(previous)"
fi

# Resolve option ID for target status with validation
OPTION_ID=$(echo "$FIELD_DATA" | jq -r '.fields[] | select(.name=="Status") | .options[] | select(.name=="'"$TO_STATUS"'") | .id')
if [ -z "$OPTION_ID" ] || [ "$OPTION_ID" = "null" ]; then
  echo "❌ ERROR: Status '$TO_STATUS' not found in project"
  echo "Available statuses:"
  echo "$FIELD_DATA" | jq -r '.fields[] | select(.name=="Status") | .options[] | .name'
  exit 1
fi

# Get item ID for the issue with validation
ITEM_ID=$(gh project item-list "$PROJECT_NUM" --owner "$OWNER" --format json 2>&1 \
  | jq -r ".items[] | select(.content.number == $ISSUE_NUM) | .id")

if [ -z "$ITEM_ID" ] || [ "$ITEM_ID" = "null" ]; then
  echo "❌ ERROR: Issue #$ISSUE_NUM not found in project #$PROJECT_NUM"
  echo "Issue may not be added to the project. Adding it now..."

  # Try to add the issue to the project
  ISSUE_URL="https://github.com/$TEAM_REPO/issues/$ISSUE_NUM"
  gh project item-add "$PROJECT_NUM" --owner "$OWNER" --url "$ISSUE_URL" 2>&1

  # Retry getting the item ID
  sleep 2
  ITEM_ID=$(gh project item-list "$PROJECT_NUM" --owner "$OWNER" --format json 2>&1 \
    | jq -r ".items[] | select(.content.number == $ISSUE_NUM) | .id")

  if [ -z "$ITEM_ID" ] || [ "$ITEM_ID" = "null" ]; then
    echo "❌ ERROR: Could not add issue to project or retrieve item ID"
    exit 1
  fi
  echo "✓ Issue added to project, item ID: $ITEM_ID"
fi

echo "→ Updating status for issue #$ISSUE_NUM (item: $ITEM_ID) to '$TO_STATUS' (option: $OPTION_ID)"

# Update status with error checking
UPDATE_OUTPUT=$(gh project item-edit \
  --project-id "$PROJECT_ID" \
  --id "$ITEM_ID" \
  --field-id "$STATUS_FIELD_ID" \
  --single-select-option-id "$OPTION_ID" 2>&1)
UPDATE_EXIT=$?

if [ $UPDATE_EXIT -ne 0 ]; then
  echo "❌ ERROR: gh project item-edit failed with exit code $UPDATE_EXIT"
  echo "Output: $UPDATE_OUTPUT"
  exit 1
fi

# Verify the update succeeded by querying the current status
# CRITICAL: Use -F (uppercase) for GraphQL ID types, not -f (lowercase)
echo "→ Verifying status update..."
sleep 1
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

if [ "$CURRENT_STATUS" != "$TO_STATUS" ]; then
  echo "❌ ERROR: Status verification failed!"
  echo "Expected: $TO_STATUS"
  echo "Actual: $CURRENT_STATUS"
  echo "The gh project item-edit command appeared to succeed but the status did not change."
  echo "This may indicate a permissions issue or an API error."
  exit 1
fi

echo "✓ Status verified: issue #$ISSUE_NUM is now '$CURRENT_STATUS'"

# Add attribution comment documenting the transition
TIMESTAMP=$(date -u +%Y-%m-%dT%H:%M:%SZ)
COMMENT_OUTPUT=$(gh issue comment "$ISSUE_NUM" --repo "$TEAM_REPO" \
  --body "### $EMOJI $ROLE — $TIMESTAMP

Status: $FROM_STATUS → $TO_STATUS" 2>&1)

if [ $? -ne 0 ]; then
  echo "⚠️  WARNING: Status updated but comment failed"
  echo "Output: $COMMENT_OUTPUT"
fi

echo "✓ Status transition complete: #$ISSUE_NUM: $FROM_STATUS → $TO_STATUS"
