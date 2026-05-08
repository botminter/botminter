#!/bin/bash
# Common setup for all github-project skill operations
# Source this file at the start of each script

set -euo pipefail

# Detect team repo
TEAM_REPO=$(cd team && gh repo view --json nameWithOwner -q .nameWithOwner 2>/dev/null)

# Fallback: extract owner/repo from git remote URL
if [ -z "$TEAM_REPO" ]; then
  TEAM_REPO=$(cd team && git remote get-url origin | sed 's|.*github.com[:/]||;s|\.git$||')
fi

if [ -z "$TEAM_REPO" ]; then
  echo "❌ ERROR: Could not detect team repository"
  exit 1
fi

# Resolve project IDs (cache for session)
OWNER=$(echo "$TEAM_REPO" | cut -d/ -f1)

# Minimal mode: only detect team repo and owner (for scripts that don't need project IDs)
if [ "${SETUP_MODE:-}" = "minimal" ]; then
  export TEAM_REPO OWNER
  return 0 2>/dev/null || exit 0
fi

# Get project number with error checking
PROJECT_NUM=$(gh project list --owner "$OWNER" --format json 2>&1 | jq -r '.projects[0].number')
if [ -z "$PROJECT_NUM" ] || [ "$PROJECT_NUM" = "null" ]; then
  echo "❌ ERROR: Cannot access GitHub Projects for organization: $OWNER"
  echo "Verify the GitHub App has 'organization_projects: admin' permission and is installed on the org"
  exit 1
fi

# Get project ID with error checking
PROJECT_ID=$(gh project view "$PROJECT_NUM" --owner "$OWNER" --format json 2>&1 | jq -r '.id')
if [ -z "$PROJECT_ID" ] || [ "$PROJECT_ID" = "null" ]; then
  echo "❌ ERROR: Could not get project ID for project #$PROJECT_NUM"
  exit 1
fi

# Get field data with error checking
if ! FIELD_DATA=$(gh project field-list "$PROJECT_NUM" --owner "$OWNER" --format json 2>&1); then
  echo "❌ ERROR: Could not fetch project field list"
  echo "$FIELD_DATA"
  exit 1
fi

if [ -z "$FIELD_DATA" ]; then
  echo "❌ ERROR: Empty response from project field list"
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
