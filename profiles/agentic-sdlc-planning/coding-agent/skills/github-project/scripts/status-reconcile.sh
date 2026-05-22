#!/bin/bash
# Reconcile null project statuses from GitHub timeline API
#
# When someone edits the Status field in the GitHub Projects UI (reorder
# options, change colors), GitHub regenerates all option IDs. Items still
# reference old IDs, so their statuses become null. This script detects
# null statuses, queries the last known status from each issue's timeline,
# and re-applies it using the current (fresh) option IDs.

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "$SCRIPT_DIR/setup.sh"

DRY_RUN=false
while [[ $# -gt 0 ]]; do
  case $1 in
    --dry-run)
      DRY_RUN=true
      shift
      ;;
    *)
      echo "❌ ERROR: Unknown argument: $1"
      echo "Usage: status-reconcile.sh [--dry-run]"
      exit 1
      ;;
  esac
done

OWNER_NAME=$(echo "$TEAM_REPO" | cut -d/ -f1)
REPO_NAME=$(echo "$TEAM_REPO" | cut -d/ -f2)

BOARD_JSON=$(project_items_json)

NULL_ITEMS=$(echo "$BOARD_JSON" | jq -c '[.items[] | select(.status == null or .status == "")]')
NULL_COUNT=$(echo "$NULL_ITEMS" | jq 'length')
TOTAL_COUNT=$(echo "$BOARD_JSON" | jq '.items | length')

if [ "$NULL_COUNT" -eq 0 ]; then
  echo "✓ No reconciliation needed: all $TOTAL_COUNT items have a status"
  exit 0
fi

echo "⚠️  Status wipe detected: $NULL_COUNT/$TOTAL_COUNT items have no status. Starting reconciliation..."
if [ "$DRY_RUN" = true ]; then
  echo "  (dry-run mode — no changes will be applied)"
fi

RESTORED=0
SKIPPED_NO_ISSUE=0
SKIPPED_NO_HISTORY=0
SKIPPED_BAD_STATUS=0
FAILED=0

for ROW in $(echo "$NULL_ITEMS" | jq -r '.[] | @base64'); do
  ITEM=$(echo "$ROW" | base64 -d)
  ISSUE_NUM=$(echo "$ITEM" | jq -r '.content.number // empty')
  ITEM_ID=$(echo "$ITEM" | jq -r '.id')
  ITEM_TYPE=$(echo "$ITEM" | jq -r '.content.type // "Issue"')
  TITLE=$(echo "$ITEM" | jq -r '.content.title // "(draft)"' | head -c 60)

  if [ -z "$ISSUE_NUM" ] || [ "$ISSUE_NUM" = "null" ]; then
    echo "  SKIP: draft item (no issue number) — $TITLE"
    SKIPPED_NO_ISSUE=$((SKIPPED_NO_ISSUE + 1))
    continue
  fi

  if [ "$ITEM_TYPE" = "PullRequest" ]; then
    QUERY_FIELD="pullRequest"
  else
    QUERY_FIELD="issue"
  fi

  TIMELINE_RAW=$(gh api graphql \
    -f owner="$OWNER_NAME" -f repo="$REPO_NAME" -F number="$ISSUE_NUM" \
    -f query='query($owner: String!, $repo: String!, $number: Int!) {
      repository(owner: $owner, name: $repo) {
        '"$QUERY_FIELD"'(number: $number) {
          timelineItems(itemTypes: [PROJECT_V2_ITEM_STATUS_CHANGED_EVENT], last: 1) {
            nodes {
              ... on ProjectV2ItemStatusChangedEvent {
                status
              }
            }
          }
        }
      }
    }' 2>&1)

  if [ $? -ne 0 ]; then
    echo "  ❌ #$ISSUE_NUM: timeline API failed — ${TIMELINE_RAW:0:100}"
    FAILED=$((FAILED + 1))
    continue
  fi

  LAST_STATUS=$(echo "$TIMELINE_RAW" | jq -r '.data.repository.'"$QUERY_FIELD"'.timelineItems.nodes[0].status // empty')

  if [ -z "$LAST_STATUS" ] || [ "$LAST_STATUS" = "null" ]; then
    echo "  SKIP #$ISSUE_NUM: no status history in timeline — $TITLE"
    SKIPPED_NO_HISTORY=$((SKIPPED_NO_HISTORY + 1))
    continue
  fi

  OPTION_ID=$(echo "$FIELD_DATA" | jq -r --arg s "$LAST_STATUS" '.fields[] | select(.name=="Status") | .options[] | select(.name==$s) | .id')
  if [ -z "$OPTION_ID" ] || [ "$OPTION_ID" = "null" ]; then
    echo "  SKIP #$ISSUE_NUM: status '$LAST_STATUS' not found in current field options — $TITLE"
    SKIPPED_BAD_STATUS=$((SKIPPED_BAD_STATUS + 1))
    continue
  fi

  if ! echo "$OPTION_ID" | grep -qE '^[0-9a-f]{8}$'; then
    echo "  SKIP #$ISSUE_NUM: option ID '$OPTION_ID' is not valid 8-char hex — $TITLE"
    SKIPPED_BAD_STATUS=$((SKIPPED_BAD_STATUS + 1))
    continue
  fi

  if [ "$DRY_RUN" = true ]; then
    echo "  DRY-RUN #$ISSUE_NUM: would restore to '$LAST_STATUS' (option: $OPTION_ID) — $TITLE"
    RESTORED=$((RESTORED + 1))
    continue
  fi

  UPDATE_OUTPUT=$(gh project item-edit \
    --project-id "$PROJECT_ID" \
    --id "$ITEM_ID" \
    --field-id "$STATUS_FIELD_ID" \
    --single-select-option-id "$OPTION_ID" 2>&1)

  if [ $? -ne 0 ]; then
    echo "  ❌ #$ISSUE_NUM: item-edit failed — $UPDATE_OUTPUT"
    FAILED=$((FAILED + 1))
    sleep 1
    continue
  fi

  sleep 2
  VERIFIED=$(gh api graphql -f query='
    query($itemId: ID!) {
      node(id: $itemId) {
        ... on ProjectV2Item {
          fieldValueByName(name: "Status") {
            ... on ProjectV2ItemFieldSingleSelectValue {
              name
            }
          }
        }
      }
    }' -F itemId="$ITEM_ID" \
    -q '.data.node.fieldValueByName.name // empty' 2>&1)

  if [ "$VERIFIED" = "$LAST_STATUS" ]; then
    echo "  ✓ #$ISSUE_NUM: restored to '$LAST_STATUS' — $TITLE"
    RESTORED=$((RESTORED + 1))
  else
    echo "  ❌ #$ISSUE_NUM: verification failed (expected '$LAST_STATUS', got '${VERIFIED:-<empty>}') — $TITLE"
    FAILED=$((FAILED + 1))
  fi

  sleep 1
done

echo ""
SKIPPED=$((SKIPPED_NO_ISSUE + SKIPPED_NO_HISTORY + SKIPPED_BAD_STATUS))
if [ "$FAILED" -gt 0 ]; then
  echo "❌ Reconciliation finished with errors: $RESTORED restored, $FAILED failed"
else
  echo "✓ Reconciliation complete: $RESTORED restored"
fi
if [ "$SKIPPED" -gt 0 ]; then
  echo "  Skipped: $SKIPPED_NO_ISSUE draft, $SKIPPED_NO_HISTORY no history, $SKIPPED_BAD_STATUS invalid status"
fi

if [ "$FAILED" -gt 0 ]; then
  exit 1
fi
