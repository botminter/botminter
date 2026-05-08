#!/bin/bash
# Query issues with various filters

# Source common setup
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "$SCRIPT_DIR/setup.sh"

# Parse arguments
QUERY_TYPE=""
LABEL=""
STATUS=""
MILESTONE=""
ASSIGNEE=""
ISSUE_NUM=""

while [[ $# -gt 0 ]]; do
  case $1 in
    --type)
      QUERY_TYPE="$2"
      shift 2
      ;;
    --label)
      LABEL="$2"
      shift 2
      ;;
    --status)
      STATUS="$2"
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
    --issue)
      ISSUE_NUM="$2"
      shift 2
      ;;
    *)
      echo "❌ ERROR: Unknown argument: $1"
      exit 1
      ;;
  esac
done

# Validate query type
if [ -z "$QUERY_TYPE" ]; then
  echo "❌ ERROR: --type is required (label, status, milestone, assignee, or single)"
  exit 1
fi

case "$QUERY_TYPE" in
  label)
    if [ -z "$LABEL" ]; then
      echo "❌ ERROR: --label is required for label query"
      exit 1
    fi

    gh issue list --repo "$TEAM_REPO" --label "$LABEL" \
      --json number,title,state,labels,assignees
    ;;

  status)
    if [ -z "$STATUS" ]; then
      echo "❌ ERROR: --status is required for status query"
      exit 1
    fi

    gh project item-list "$PROJECT_NUM" --owner "$OWNER" --format json \
      --jq ".items[] | select(.status == \"$STATUS\")"
    ;;

  milestone)
    if [ -z "$MILESTONE" ]; then
      echo "❌ ERROR: --milestone is required for milestone query"
      exit 1
    fi

    gh issue list --repo "$TEAM_REPO" --milestone "$MILESTONE" \
      --json number,title,state,labels,assignees
    ;;

  assignee)
    if [ -z "$ASSIGNEE" ]; then
      echo "❌ ERROR: --assignee is required for assignee query"
      exit 1
    fi

    gh issue list --repo "$TEAM_REPO" --assignee "$ASSIGNEE" \
      --json number,title,state,labels,assignees
    ;;

  single)
    if [ -z "$ISSUE_NUM" ]; then
      echo "❌ ERROR: --issue is required for single issue query"
      exit 1
    fi

    OWNER_NAME=$(echo "$TEAM_REPO" | cut -d/ -f1)
    REPO_NAME_ONLY=$(echo "$TEAM_REPO" | cut -d/ -f2)

    gh api graphql \
      -f owner="$OWNER_NAME" -f repo="$REPO_NAME_ONLY" -F number="$ISSUE_NUM" \
      -H "GraphQL-Features: issue_types,sub_issues" \
      -f query='query($owner: String!, $repo: String!, $number: Int!) {
        repository(owner: $owner, name: $repo) {
          issue(number: $number) {
            number
            title
            state
            body
            issueType { name }
            labels(first: 20) { nodes { name } }
            assignees(first: 10) { nodes { login } }
            milestone { title }
            subIssues(first: 50) {
              nodes { number title state issueType { name } }
            }
            comments(last: 20) {
              nodes { author { login } body createdAt }
            }
          }
        }
      }' -q .data.repository.issue
    ;;

  issue-type)
    # Query by native issue type (Epic, Task, Bug)
    if [ -z "$LABEL" ]; then
      echo "❌ ERROR: --label is required for issue-type query (use issue type name: Epic, Task, Bug)"
      exit 1
    fi

    OWNER_NAME=$(echo "$TEAM_REPO" | cut -d/ -f1)
    REPO_NAME_ONLY=$(echo "$TEAM_REPO" | cut -d/ -f2)

    gh api graphql \
      -f owner="$OWNER_NAME" -f repo="$REPO_NAME_ONLY" \
      -H "GraphQL-Features: issue_types,sub_issues" \
      -f query='query($owner: String!, $repo: String!) {
        repository(owner: $owner, name: $repo) {
          issues(first: 50, states: OPEN) {
            nodes {
              number
              title
              state
              issueType { name }
              subIssues(first: 20) {
                nodes { number title state }
              }
            }
          }
        }
      }' | jq --arg t "$LABEL" '[.data.repository.issues.nodes[] | select(.issueType.name == $t)]'
    ;;

  *)
    echo "❌ ERROR: Invalid query type '$QUERY_TYPE'"
    echo "Valid types: label, status, milestone, assignee, single, issue-type"
    exit 1
    ;;
esac
