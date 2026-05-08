#!/bin/bash
# Update an existing issue (title, body, labels)

# Source common setup (minimal — only needs TEAM_REPO)
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
SETUP_MODE=minimal source "$SCRIPT_DIR/setup.sh"

# Parse arguments
ISSUE_NUM=""
TITLE=""
BODY=""
ADD_LABELS=""
REMOVE_LABELS=""

while [[ $# -gt 0 ]]; do
  case $1 in
    --issue)
      ISSUE_NUM="$2"
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
    --add-label)
      ADD_LABELS="$ADD_LABELS $2"
      shift 2
      ;;
    --remove-label)
      REMOVE_LABELS="$REMOVE_LABELS $2"
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

if [ -z "$TITLE" ] && [ -z "$BODY" ] && [ -z "$ADD_LABELS" ] && [ -z "$REMOVE_LABELS" ]; then
  echo "❌ ERROR: at least one of --title, --body, --add-label, or --remove-label is required"
  exit 1
fi

# Build the gh issue edit command
CMD=(gh issue edit "$ISSUE_NUM" --repo "$TEAM_REPO")

if [ -n "$TITLE" ]; then
  CMD+=(--title "$TITLE")
fi

if [ -n "$BODY" ]; then
  CMD+=(--body "$BODY")
fi

for label in $ADD_LABELS; do
  CMD+=(--add-label "$label")
done

for label in $REMOVE_LABELS; do
  CMD+=(--remove-label "$label")
done

"${CMD[@]}" 2>&1

echo "✓ Updated issue #$ISSUE_NUM"
