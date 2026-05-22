#!/bin/bash
# Display all issues grouped by project status with epic-to-story relationships

# Source common setup
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "$SCRIPT_DIR/setup.sh"

# Fetch all project items
BOARD_JSON=$(project_items_json)
echo "$BOARD_JSON"

# Detect null statuses — a board should NEVER have items without a status
NULL_COUNT=$(echo "$BOARD_JSON" | jq '[.items[] | select(.status == null or .status == "")] | length')
if [ "$NULL_COUNT" -gt 0 ]; then
  TOTAL=$(echo "$BOARD_JSON" | jq '.items | length')
  echo "⚠️  STATUS WIPE DETECTED: $NULL_COUNT/$TOTAL items have no status. Run: bash scripts/status-reconcile.sh" >&2
fi
