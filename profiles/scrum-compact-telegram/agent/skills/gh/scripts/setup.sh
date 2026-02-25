#!/bin/bash
# Common setup for all gh skill operations
# Source this file at the start of each script

set -euo pipefail

# Verify project scope by testing access
if ! gh project list --owner "$(gh api user -q .login)" --limit 1 --format json &>/dev/null; then
  echo "❌ ERROR: Missing 'project' scope on GH_TOKEN"
  echo "Run: gh auth refresh -s project -h github.com"
  exit 1
fi

# Detect team repo
TEAM_REPO=$(cd .botminter && gh repo view --json nameWithOwner -q .nameWithOwner 2>/dev/null)

# Fallback: extract owner/repo from git remote URL
if [ -z "$TEAM_REPO" ]; then
  TEAM_REPO=$(cd .botminter && git remote get-url origin | sed 's|.*github.com[:/]||;s|\.git$||')
fi

if [ -z "$TEAM_REPO" ]; then
  echo "❌ ERROR: Could not detect team repository"
  exit 1
fi

# Resolve project IDs (cache for session)
OWNER=$(echo "$TEAM_REPO" | cut -d/ -f1)

# Get project number with error checking
PROJECT_NUM=$(gh project list --owner "$OWNER" --format json 2>&1 | jq -r '.projects[0].number')
if [ -z "$PROJECT_NUM" ] || [ "$PROJECT_NUM" = "null" ]; then
  echo "❌ ERROR: No GitHub Project found for organization: $OWNER"
  exit 1
fi

# Get project ID with error checking
PROJECT_ID=$(gh project view "$PROJECT_NUM" --owner "$OWNER" --format json 2>&1 | jq -r '.id')
if [ -z "$PROJECT_ID" ] || [ "$PROJECT_ID" = "null" ]; then
  echo "❌ ERROR: Could not get project ID for project #$PROJECT_NUM"
  exit 1
fi

# Get field data with error checking
FIELD_DATA=$(gh project field-list "$PROJECT_NUM" --owner "$OWNER" --format json 2>&1)
if [ $? -ne 0 ] || [ -z "$FIELD_DATA" ]; then
  echo "❌ ERROR: Could not fetch project field list"
  echo "$FIELD_DATA"
  exit 1
fi

# Extract Status field ID with validation
STATUS_FIELD_ID=$(echo "$FIELD_DATA" | jq -r '.fields[] | select(.name=="Status") | .id')
if [ -z "$STATUS_FIELD_ID" ] || [ "$STATUS_FIELD_ID" = "null" ]; then
  echo "❌ ERROR: No 'Status' field found in project #$PROJECT_NUM"
  echo "Available fields:"
  echo "$FIELD_DATA" | jq -r '.fields[] | .name'
  exit 1
fi

# Get member identity from .botminter.yml (optional)
if [ -f .botminter.yml ]; then
  ROLE=$(grep '^role:' .botminter.yml | awk '{print $2}')
  EMOJI=$(grep '^comment_emoji:' .botminter.yml | sed 's/comment_emoji: *"//' | sed 's/"$//')
else
  ROLE="superman"
  EMOJI="🦸"
fi

echo "✓ Setup complete: $TEAM_REPO, project #$PROJECT_NUM"

# Export variables for use in calling scripts
export TEAM_REPO OWNER PROJECT_NUM PROJECT_ID FIELD_DATA STATUS_FIELD_ID ROLE EMOJI
