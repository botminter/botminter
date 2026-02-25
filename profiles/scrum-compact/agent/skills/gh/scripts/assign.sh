#!/bin/bash
# Assign or unassign an issue

# Source common setup
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "$SCRIPT_DIR/setup.sh"

# Parse arguments
ISSUE_NUM=""
ACTION=""
USERNAME=""

while [[ $# -gt 0 ]]; do
  case $1 in
    --issue)
      ISSUE_NUM="$2"
      shift 2
      ;;
    --action)
      ACTION="$2"
      shift 2
      ;;
    --user)
      USERNAME="$2"
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

if [ -z "$ACTION" ]; then
  echo "❌ ERROR: --action is required (assign or unassign)"
  exit 1
fi

if [ "$ACTION" != "assign" ] && [ "$ACTION" != "unassign" ]; then
  echo "❌ ERROR: --action must be 'assign' or 'unassign'"
  exit 1
fi

if [ -z "$USERNAME" ]; then
  echo "❌ ERROR: --user is required"
  exit 1
fi

# Execute assignment operation
if [ "$ACTION" = "assign" ]; then
  gh issue edit "$ISSUE_NUM" --repo "$TEAM_REPO" --add-assignee "$USERNAME"
  echo "✓ Assigned issue #$ISSUE_NUM to $USERNAME"
else
  gh issue edit "$ISSUE_NUM" --repo "$TEAM_REPO" --remove-assignee "$USERNAME"
  echo "✓ Unassigned $USERNAME from issue #$ISSUE_NUM"
fi
