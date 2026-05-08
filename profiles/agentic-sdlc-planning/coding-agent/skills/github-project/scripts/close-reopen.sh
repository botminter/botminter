#!/bin/bash
# Close or reopen an issue

# Source common setup
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "$SCRIPT_DIR/setup.sh"

# Parse arguments
ISSUE_NUM=""
ACTION=""

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
  echo "❌ ERROR: --action is required (close or reopen)"
  exit 1
fi

# Execute action
case "$ACTION" in
  close)
    gh issue close "$ISSUE_NUM" --repo "$TEAM_REPO"
    echo "✓ Closed issue #$ISSUE_NUM"
    ;;

  reopen)
    gh issue reopen "$ISSUE_NUM" --repo "$TEAM_REPO"
    echo "✓ Reopened issue #$ISSUE_NUM"
    ;;

  *)
    echo "❌ ERROR: Invalid action '$ACTION' (must be close or reopen)"
    exit 1
    ;;
esac
