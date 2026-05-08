#!/bin/bash
# Pull request operations

# Source common setup
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "$SCRIPT_DIR/setup.sh"

# Parse arguments
ACTION=""
TITLE=""
BODY=""
BRANCH=""
BASE="main"
PR_NUM=""

while [[ $# -gt 0 ]]; do
  case $1 in
    --action)
      ACTION="$2"
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
    --branch)
      BRANCH="$2"
      shift 2
      ;;
    --base)
      BASE="$2"
      shift 2
      ;;
    --pr)
      PR_NUM="$2"
      shift 2
      ;;
    *)
      echo "❌ ERROR: Unknown argument: $1"
      exit 1
      ;;
  esac
done

# Validate action
if [ -z "$ACTION" ]; then
  echo "❌ ERROR: --action is required (create, approve, request-changes, comment, or list)"
  exit 1
fi

case "$ACTION" in
  create)
    if [ -z "$TITLE" ]; then
      echo "❌ ERROR: --title is required for create action"
      exit 1
    fi

    if [ -z "$BODY" ]; then
      echo "❌ ERROR: --body is required for create action"
      exit 1
    fi

    if [ -z "$BRANCH" ]; then
      echo "❌ ERROR: --branch is required for create action"
      exit 1
    fi

    gh pr create --repo "$TEAM_REPO" \
      --title "$TITLE" \
      --body "$BODY" \
      --base "$BASE" \
      --head "$BRANCH"
    echo "✓ Pull request created"
    ;;

  approve)
    if [ -z "$PR_NUM" ]; then
      echo "❌ ERROR: --pr is required for approve action"
      exit 1
    fi

    if [ -z "$BODY" ]; then
      echo "❌ ERROR: --body is required for approve action"
      exit 1
    fi

    gh pr review "$PR_NUM" --repo "$TEAM_REPO" --approve --body "$BODY"
    echo "✓ Pull request #$PR_NUM approved"
    ;;

  request-changes)
    if [ -z "$PR_NUM" ]; then
      echo "❌ ERROR: --pr is required for request-changes action"
      exit 1
    fi

    if [ -z "$BODY" ]; then
      echo "❌ ERROR: --body is required for request-changes action"
      exit 1
    fi

    gh pr review "$PR_NUM" --repo "$TEAM_REPO" --request-changes --body "$BODY"
    echo "✓ Changes requested on pull request #$PR_NUM"
    ;;

  comment)
    if [ -z "$PR_NUM" ]; then
      echo "❌ ERROR: --pr is required for comment action"
      exit 1
    fi

    if [ -z "$BODY" ]; then
      echo "❌ ERROR: --body is required for comment action"
      exit 1
    fi

    TIMESTAMP=$(date -u +%Y-%m-%dT%H:%M:%SZ)
    gh pr comment "$PR_NUM" --repo "$TEAM_REPO" \
      --body "### $EMOJI $ROLE — $TIMESTAMP

$BODY"
    echo "✓ Comment added to pull request #$PR_NUM"
    ;;

  list)
    gh pr list --repo "$TEAM_REPO" --state all \
      --json number,title,state,labels,author,reviewDecision
    ;;

  *)
    echo "❌ ERROR: Invalid action '$ACTION'"
    echo "Valid actions: create, approve, request-changes, comment, list"
    exit 1
    ;;
esac
