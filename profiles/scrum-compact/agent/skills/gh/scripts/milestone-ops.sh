#!/bin/bash
# Milestone management operations

# Source common setup
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "$SCRIPT_DIR/setup.sh"

# Parse arguments
ACTION=""
TITLE=""
DESCRIPTION=""
DUE_DATE=""
ISSUE_NUM=""

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
    --description)
      DESCRIPTION="$2"
      shift 2
      ;;
    --due-date)
      DUE_DATE="$2"
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

# Validate action
if [ -z "$ACTION" ]; then
  echo "❌ ERROR: --action is required (list, create, or assign)"
  exit 1
fi

case "$ACTION" in
  list)
    gh api "repos/$TEAM_REPO/milestones" --jq '.[] | {number, title, state, due_on}'
    ;;

  create)
    if [ -z "$TITLE" ]; then
      echo "❌ ERROR: --title is required for create action"
      exit 1
    fi

    CMD=(gh api "repos/$TEAM_REPO/milestones" -f title="$TITLE")

    if [ -n "$DESCRIPTION" ]; then
      CMD+=(-f description="$DESCRIPTION")
    fi

    if [ -n "$DUE_DATE" ]; then
      CMD+=(-f due_on="$DUE_DATE")
    fi

    "${CMD[@]}"
    echo "✓ Milestone '$TITLE' created"
    ;;

  assign)
    if [ -z "$ISSUE_NUM" ]; then
      echo "❌ ERROR: --issue is required for assign action"
      exit 1
    fi

    if [ -z "$TITLE" ]; then
      echo "❌ ERROR: --title is required for assign action"
      exit 1
    fi

    gh issue edit "$ISSUE_NUM" --repo "$TEAM_REPO" --milestone "$TITLE"
    echo "✓ Assigned issue #$ISSUE_NUM to milestone '$TITLE'"
    ;;

  *)
    echo "❌ ERROR: Invalid action '$ACTION' (must be list, create, or assign)"
    exit 1
    ;;
esac
