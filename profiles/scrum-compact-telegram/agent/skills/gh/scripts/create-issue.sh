#!/bin/bash
# Create a new issue (epic or story) with project setup

# Source common setup
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "$SCRIPT_DIR/setup.sh"

# Parse arguments
TITLE=""
BODY=""
KIND=""
PARENT=""
MILESTONE=""
ASSIGNEE=""

while [[ $# -gt 0 ]]; do
  case $1 in
    --title)
      TITLE="$2"
      shift 2
      ;;
    --body)
      BODY="$2"
      shift 2
      ;;
    --kind)
      KIND="$2"
      shift 2
      ;;
    --parent)
      PARENT="$2"
      shift 2
      ;;
    --milestone)
      MILESTONE="$2"
      shift 2
      ;;
    --assignee)
      ASSIGNEE="$2"
      shift 2
      ;;
    *)
      echo "❌ ERROR: Unknown argument: $1"
      exit 1
      ;;
  esac
done

# Validate required parameters
if [ -z "$TITLE" ]; then
  echo "❌ ERROR: --title is required"
  exit 1
fi

if [ -z "$BODY" ]; then
  echo "❌ ERROR: --body is required"
  exit 1
fi

if [ -z "$KIND" ]; then
  echo "❌ ERROR: --kind is required (epic or story)"
  exit 1
fi

if [ "$KIND" != "epic" ] && [ "$KIND" != "story" ]; then
  echo "❌ ERROR: --kind must be 'epic' or 'story'"
  exit 1
fi

# Build gh issue create command
CMD=(gh issue create --repo "$TEAM_REPO" --title "$TITLE" --label "kind/$KIND")

# Add body (with parent reference for stories)
if [ "$KIND" = "story" ] && [ -n "$PARENT" ]; then
  CMD+=(--body "Parent: #$PARENT

$BODY")
  CMD+=(--label "parent/$PARENT")
else
  CMD+=(--body "$BODY")
fi

# Add optional parameters
if [ -n "$MILESTONE" ]; then
  CMD+=(--milestone "$MILESTONE")
fi

if [ -n "$ASSIGNEE" ]; then
  CMD+=(--assignee "$ASSIGNEE")
fi

# Create issue
ISSUE_URL=$("${CMD[@]}")
if [ $? -ne 0 ]; then
  echo "❌ ERROR: Failed to create issue"
  exit 1
fi

ISSUE_NUM=$(echo "$ISSUE_URL" | grep -o '[0-9]*$')
echo "✓ Created issue #$ISSUE_NUM: $ISSUE_URL"

# Add issue to project with error checking
ADD_OUTPUT=$(gh project item-add "$PROJECT_NUM" --owner "$OWNER" --url "$ISSUE_URL" 2>&1)
if [ $? -ne 0 ]; then
  echo "❌ ERROR: Failed to add issue to project"
  echo "Output: $ADD_OUTPUT"
  exit 1
fi

# Wait briefly for the item to be indexed
sleep 2

# Get the item ID for the newly added issue with validation
ITEM_ID=$(gh project item-list "$PROJECT_NUM" --owner "$OWNER" --format json 2>&1 \
  | jq -r ".items[] | select(.content.number == $ISSUE_NUM) | .id")

if [ -z "$ITEM_ID" ] || [ "$ITEM_ID" = "null" ]; then
  echo "❌ ERROR: Could not retrieve item ID for newly created issue #$ISSUE_NUM"
  exit 1
fi

# Resolve the option ID for the initial status with validation
INITIAL_STATUS="po:triage"
OPTION_ID=$(echo "$FIELD_DATA" | jq -r '.fields[] | select(.name=="Status") | .options[] | select(.name=="'"$INITIAL_STATUS"'") | .id')
if [ -z "$OPTION_ID" ] || [ "$OPTION_ID" = "null" ]; then
  echo "❌ ERROR: '$INITIAL_STATUS' status option not found in project"
  exit 1
fi

# Set initial status with error checking
STATUS_OUTPUT=$(gh project item-edit --project-id "$PROJECT_ID" --id "$ITEM_ID" \
  --field-id "$STATUS_FIELD_ID" --single-select-option-id "$OPTION_ID" 2>&1)

if [ $? -ne 0 ]; then
  echo "❌ ERROR: Failed to set initial status"
  echo "Output: $STATUS_OUTPUT"
  exit 1
fi

echo "✓ Issue #$ISSUE_NUM added to project with status '$INITIAL_STATUS'"

# Add attribution comment
TIMESTAMP=$(date -u +%Y-%m-%dT%H:%M:%SZ)
KIND_LABEL=$([ "$KIND" = "epic" ] && echo "epic" || echo "story")
gh issue comment "$ISSUE_NUM" --repo "$TEAM_REPO" \
  --body "### $EMOJI $ROLE — $TIMESTAMP

Created $KIND_LABEL: $TITLE"

echo "✓ Attribution comment added"
echo ""
echo "Issue #$ISSUE_NUM created successfully"
echo "URL: $ISSUE_URL"
echo "Status: $INITIAL_STATUS"
echo "Next: Board scanner will process this issue"
