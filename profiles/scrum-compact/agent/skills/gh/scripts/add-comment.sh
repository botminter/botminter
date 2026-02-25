#!/bin/bash
# Add a role-attributed comment to an issue

# Source common setup
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "$SCRIPT_DIR/setup.sh"

# Parse arguments
ISSUE_NUM=""
BODY=""

while [[ $# -gt 0 ]]; do
  case $1 in
    --issue)
      ISSUE_NUM="$2"
      shift 2
      ;;
    --body)
      BODY="$2"
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

if [ -z "$BODY" ]; then
  echo "❌ ERROR: --body is required"
  exit 1
fi

# Add comment with attribution header
TIMESTAMP=$(date -u +%Y-%m-%dT%H:%M:%SZ)
gh issue comment "$ISSUE_NUM" --repo "$TEAM_REPO" \
  --body "### $EMOJI $ROLE — $TIMESTAMP

$BODY"

echo "✓ Comment added to issue #$ISSUE_NUM"
