#!/bin/bash
# Update an existing issue (title, body, labels)

# Source common setup (minimal — only needs TEAM_REPO)
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
SETUP_MODE=minimal source "$SCRIPT_DIR/setup.sh"

# Parse arguments
ISSUE_NUM=""
TITLE=""
BODY=""
BODY_FILE=""
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
    --body-file)
      BODY_FILE="$2"
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

if [ -n "$BODY" ] && [ -n "$BODY_FILE" ]; then
  echo "❌ ERROR: --body and --body-file are mutually exclusive"
  exit 1
fi

if [ -n "$BODY_FILE" ] && [ ! -f "$BODY_FILE" ]; then
  echo "❌ ERROR: body file not found: $BODY_FILE"
  exit 1
fi

if [ -z "$TITLE" ] && [ -z "$BODY" ] && [ -z "$BODY_FILE" ] && [ -z "$ADD_LABELS" ] && [ -z "$REMOVE_LABELS" ]; then
  echo "❌ ERROR: at least one of --title, --body, --body-file, --add-label, or --remove-label is required"
  exit 1
fi

# Build the gh issue edit command
CMD=(gh issue edit "$ISSUE_NUM" --repo "$TEAM_REPO")

if [ -n "$TITLE" ]; then
  CMD+=(--title "$TITLE")
fi

if [ -n "$BODY_FILE" ]; then
  CMD+=(--body-file "$BODY_FILE")
elif [ -n "$BODY" ]; then
  CMD+=(--body "$BODY")
fi

for label in $ADD_LABELS; do
  CMD+=(--add-label "$label")
done

for label in $REMOVE_LABELS; do
  CMD+=(--remove-label "$label")
done

if ! "${CMD[@]}" 2>&1; then
  echo "❌ ERROR: Failed to update issue #$ISSUE_NUM"
  exit 1
fi

echo "✓ Updated issue #$ISSUE_NUM"
