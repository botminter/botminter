#!/usr/bin/env bash
#
# subtask-ops.sh - GitHub native sub-issue operations
#
# Uses GitHub's native issue types and sub-issues (CreateIssueInput with
# issueTypeId + parentIssueId).
#
# Operations:
# - create: Create a sub-issue with native parent relationship and issue type
# - list: List all sub-issues for a parent issue
# - status: Check completion status of all sub-issues

set -euo pipefail

# Source setup in minimal mode (only need TEAM_REPO, not project IDs)
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
SETUP_MODE=minimal source "$SCRIPT_DIR/setup.sh"

# Parse arguments
ACTION=""
PARENT=""
TITLE=""
BODY=""
TYPE="Task"  # Default issue type

while [[ $# -gt 0 ]]; do
  case $1 in
    --action)
      ACTION="$2"
      shift 2
      ;;
    --parent)
      PARENT="$2"
      shift 2
      ;;
    --title)
      TITLE="$2"
      shift 2
      ;;
    --body)
      BODY="$2"
      shift 2
      ;;
    --type)
      TYPE="$2"
      shift 2
      ;;
    *)
      echo "Unknown option: $1" >&2
      exit 1
      ;;
  esac
done

# Validate required arguments
if [ -z "$ACTION" ]; then
  echo "Error: --action is required" >&2
  exit 1
fi

OWNER_NAME=$(echo "$TEAM_REPO" | cut -d/ -f1)
REPO_NAME=$(echo "$TEAM_REPO" | cut -d/ -f2)

case "$ACTION" in
  create)
    if [ -z "$PARENT" ] || [ -z "$TITLE" ]; then
      echo "Error: --parent and --title are required for create action" >&2
      exit 1
    fi

    # Get repo ID
    echo "Fetching repository ID..." >&2
    REPO_ID=$(gh api graphql \
      -f owner="$OWNER_NAME" -f repo="$REPO_NAME" \
      -f query='query($owner: String!, $repo: String!) {
        repository(owner: $owner, name: $repo) { id }
      }' -q .data.repository.id)

    if [ -z "$REPO_ID" ] || [ "$REPO_ID" = "null" ]; then
      echo "Error: Could not fetch repository ID" >&2
      exit 1
    fi

    # Get parent issue node ID
    echo "Fetching parent issue #$PARENT node ID..." >&2
    PARENT_ID=$(gh api graphql \
      -f owner="$OWNER_NAME" -f repo="$REPO_NAME" -F number="$PARENT" \
      -f query='query($owner: String!, $repo: String!, $number: Int!) {
        repository(owner: $owner, name: $repo) {
          issue(number: $number) { id }
        }
      }' -q .data.repository.issue.id)

    if [ -z "$PARENT_ID" ] || [ "$PARENT_ID" = "null" ]; then
      echo "Error: Could not fetch parent issue #$PARENT node ID" >&2
      exit 1
    fi

    # Get issue type ID by name
    echo "Resolving issue type '$TYPE'..." >&2
    TYPE_ID=$(gh api graphql \
      -f owner="$OWNER_NAME" -f repo="$REPO_NAME" \
      -H "GraphQL-Features: issue_types" \
      -f query='query($owner: String!, $repo: String!) {
        repository(owner: $owner, name: $repo) {
          issueTypes(first: 20) {
            nodes { id name }
          }
        }
      }' -q ".data.repository.issueTypes.nodes[] | select(.name == \"$TYPE\") | .id")

    if [ -z "$TYPE_ID" ] || [ "$TYPE_ID" = "null" ]; then
      echo "Error: Issue type '$TYPE' not found. Available types:" >&2
      gh api graphql \
        -f owner="$OWNER_NAME" -f repo="$REPO_NAME" \
        -H "GraphQL-Features: issue_types" \
        -f query='query($owner: String!, $repo: String!) {
          repository(owner: $owner, name: $repo) {
            issueTypes(first: 20) { nodes { name } }
          }
        }' -q '.data.repository.issueTypes.nodes[].name' >&2
      exit 1
    fi

    # Create sub-issue in a single mutation: type + parent + body
    echo "Creating sub-issue under #$PARENT (type: $TYPE)..." >&2
    RESULT=$(gh api graphql \
      -H "GraphQL-Features: sub_issues,issue_types" \
      -F repositoryId="$REPO_ID" \
      -f title="$TITLE" \
      -f body="${BODY:-}" \
      -F issueTypeId="$TYPE_ID" \
      -F parentIssueId="$PARENT_ID" \
      -f query='
      mutation($repositoryId: ID!, $title: String!, $body: String!, $issueTypeId: ID!, $parentIssueId: ID!) {
        createIssue(input: {repositoryId: $repositoryId, title: $title, body: $body, issueTypeId: $issueTypeId, parentIssueId: $parentIssueId}) {
          issue {
            number
            title
            issueType { name }
          }
        }
      }')

    SUBTASK_NUM=$(echo "$RESULT" | jq -r '.data.createIssue.issue.number')
    SUBTASK_TYPE=$(echo "$RESULT" | jq -r '.data.createIssue.issue.issueType.name')

    if [ -z "$SUBTASK_NUM" ] || [ "$SUBTASK_NUM" = "null" ]; then
      echo "Error: Failed to create sub-issue" >&2
      echo "$RESULT" >&2
      exit 1
    fi

    echo "Sub-issue #$SUBTASK_NUM created (type: $SUBTASK_TYPE, parent: #$PARENT)" >&2

    # Output as JSON
    echo "{\"number\": $SUBTASK_NUM, \"type\": \"$SUBTASK_TYPE\", \"parent\": $PARENT}"
    ;;

  list)
    if [ -z "$PARENT" ]; then
      echo "Error: --parent is required for list action" >&2
      exit 1
    fi

    echo "Fetching sub-issues for issue #$PARENT..." >&2
    gh api graphql \
      -f owner="$OWNER_NAME" -f repo="$REPO_NAME" -F number="$PARENT" \
      -H "GraphQL-Features: sub_issues,issue_types" \
      -f query='query($owner: String!, $repo: String!, $number: Int!) {
        repository(owner: $owner, name: $repo) {
          issue(number: $number) {
            subIssues(first: 50) {
              nodes {
                number
                title
                state
                issueType { name }
              }
            }
          }
        }
      }' -q .data.repository.issue.subIssues.nodes
    ;;

  status)
    if [ -z "$PARENT" ]; then
      echo "Error: --parent is required for status action" >&2
      exit 1
    fi

    echo "Checking sub-issue completion status for issue #$PARENT..." >&2
    SUBTASKS=$(gh api graphql \
      -f owner="$OWNER_NAME" -f repo="$REPO_NAME" -F number="$PARENT" \
      -H "GraphQL-Features: sub_issues,issue_types" \
      -f query='query($owner: String!, $repo: String!, $number: Int!) {
        repository(owner: $owner, name: $repo) {
          issue(number: $number) {
            subIssues(first: 50) {
              nodes {
                number
                state
              }
            }
          }
        }
      }' -q .data.repository.issue.subIssues.nodes)

    # Count total and closed sub-issues
    TOTAL=$(echo "$SUBTASKS" | jq 'length')
    CLOSED=$(echo "$SUBTASKS" | jq '[.[] | select(.state == "CLOSED")] | length')

    echo "{\"total\": $TOTAL, \"closed\": $CLOSED, \"all_complete\": $([ "$TOTAL" -gt 0 ] && [ "$TOTAL" -eq "$CLOSED" ] && echo true || echo false)}"
    ;;

  *)
    echo "Error: Unknown action '$ACTION'" >&2
    echo "Valid actions: create, list, status" >&2
    exit 1
    ;;
esac
