#!/bin/bash
# Create a new issue (epic, story, or bug) with project setup
#
# Uses GitHub's native issue types and sub-issues:
#   epic  → Epic issue type
#   story → Task issue type + parentIssueId (sub-issue of epic)
#   bug   → Bug issue type
#
# Uses GitHub native issue types and sub-issues.

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
  echo "❌ ERROR: --kind is required (epic, story, or bug)"
  exit 1
fi

if [ "$KIND" != "epic" ] && [ "$KIND" != "story" ] && [ "$KIND" != "bug" ]; then
  echo "❌ ERROR: --kind must be 'epic', 'story', or 'bug'"
  exit 1
fi

# Map kind to GitHub native issue type
case "$KIND" in
  epic)  ISSUE_TYPE_NAME="Epic" ;;
  story) ISSUE_TYPE_NAME="Task" ;;
  bug)   ISSUE_TYPE_NAME="Bug" ;;
esac

OWNER_NAME=$(echo "$TEAM_REPO" | cut -d/ -f1)
REPO_NAME=$(echo "$TEAM_REPO" | cut -d/ -f2)

# Get repo ID
echo "→ Fetching repository ID..." >&2
REPO_ID=$(gh api graphql \
  -f owner="$OWNER_NAME" -f repo="$REPO_NAME" \
  -f query='query($owner: String!, $repo: String!) {
    repository(owner: $owner, name: $repo) { id }
  }' -q .data.repository.id)

if [ -z "$REPO_ID" ] || [ "$REPO_ID" = "null" ]; then
  echo "❌ ERROR: Could not fetch repository ID" >&2
  exit 1
fi

# Get issue type ID
echo "→ Resolving issue type '$ISSUE_TYPE_NAME'..." >&2
ISSUE_TYPE_ID=$(gh api graphql \
  -f owner="$OWNER_NAME" -f repo="$REPO_NAME" \
  -H "GraphQL-Features: issue_types" \
  -f query='query($owner: String!, $repo: String!) {
    repository(owner: $owner, name: $repo) {
      issueTypes(first: 20) {
        nodes { id name }
      }
    }
  }' -q ".data.repository.issueTypes.nodes[] | select(.name == \"$ISSUE_TYPE_NAME\") | .id")

if [ -z "$ISSUE_TYPE_ID" ] || [ "$ISSUE_TYPE_ID" = "null" ]; then
  echo "❌ ERROR: '$ISSUE_TYPE_NAME' issue type not found on this repository" >&2
  exit 1
fi

# For stories with a parent, link as sub-issue
if [ -n "$PARENT" ]; then
  echo "→ Fetching parent issue #$PARENT node ID..." >&2
  PARENT_ID=$(gh api graphql \
    -f owner="$OWNER_NAME" -f repo="$REPO_NAME" -F number="$PARENT" \
    -f query='query($owner: String!, $repo: String!, $number: Int!) {
      repository(owner: $owner, name: $repo) {
        issue(number: $number) { id }
      }
    }' -q .data.repository.issue.id)

  if [ -z "$PARENT_ID" ] || [ "$PARENT_ID" = "null" ]; then
    echo "❌ ERROR: Could not fetch parent issue #$PARENT node ID" >&2
    exit 1
  fi
fi

# Create issue via GraphQL with proper variable passing
echo "→ Creating $KIND (type: $ISSUE_TYPE_NAME)..." >&2
if [ -n "$PARENT" ]; then
  RESULT=$(gh api graphql \
    -H "GraphQL-Features: issue_types,sub_issues" \
    -F repositoryId="$REPO_ID" -f title="$TITLE" -f body="$BODY" \
    -F issueTypeId="$ISSUE_TYPE_ID" -F parentIssueId="$PARENT_ID" \
    -f query='
    mutation($repositoryId: ID!, $title: String!, $body: String!, $issueTypeId: ID!, $parentIssueId: ID!) {
      createIssue(input: {repositoryId: $repositoryId, title: $title, body: $body, issueTypeId: $issueTypeId, parentIssueId: $parentIssueId}) {
        issue { number url issueType { name } }
      }
    }')
else
  RESULT=$(gh api graphql \
    -H "GraphQL-Features: issue_types" \
    -F repositoryId="$REPO_ID" -f title="$TITLE" -f body="$BODY" \
    -F issueTypeId="$ISSUE_TYPE_ID" \
    -f query='
    mutation($repositoryId: ID!, $title: String!, $body: String!, $issueTypeId: ID!) {
      createIssue(input: {repositoryId: $repositoryId, title: $title, body: $body, issueTypeId: $issueTypeId}) {
        issue { number url issueType { name } }
      }
    }')
fi

ISSUE_NUM=$(echo "$RESULT" | jq -r '.data.createIssue.issue.number')
ISSUE_URL=$(echo "$RESULT" | jq -r '.data.createIssue.issue.url')

if [ -z "$ISSUE_NUM" ] || [ "$ISSUE_NUM" = "null" ]; then
  echo "❌ ERROR: Failed to create issue" >&2
  echo "$RESULT" >&2
  exit 1
fi

echo "✓ Created $KIND #$ISSUE_NUM: $ISSUE_URL"
if [ -n "$PARENT" ]; then
  echo "  Linked as sub-issue of #$PARENT"
fi

# Add milestone if specified (via REST API since GraphQL createIssue doesn't support milestone name)
if [ -n "$MILESTONE" ]; then
  gh issue edit "$ISSUE_NUM" --repo "$TEAM_REPO" --milestone "$MILESTONE" 2>&1 || \
    echo "⚠️  WARNING: Could not set milestone '$MILESTONE'"
fi

# Add assignee if specified
if [ -n "$ASSIGNEE" ]; then
  gh issue edit "$ISSUE_NUM" --repo "$TEAM_REPO" --add-assignee "$ASSIGNEE" 2>&1 || \
    echo "⚠️  WARNING: Could not assign '$ASSIGNEE'"
fi

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

# Set initial status based on kind
if [ "$KIND" = "bug" ]; then
  INITIAL_STATUS="bug:investigate"
else
  INITIAL_STATUS="po:triage"
fi

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
gh issue comment "$ISSUE_NUM" --repo "$TEAM_REPO" \
  --body "### $EMOJI $ROLE — $TIMESTAMP

Created $KIND: $TITLE"

echo "✓ Attribution comment added"
echo ""
echo "Issue #$ISSUE_NUM created successfully"
echo "URL: $ISSUE_URL"
echo "Status: $INITIAL_STATUS"
echo "Next: Board scanner will process this issue"
