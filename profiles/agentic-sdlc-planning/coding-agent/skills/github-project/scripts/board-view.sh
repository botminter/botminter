#!/bin/bash
# Display all issues grouped by project status with epic-to-story relationships

# Source common setup
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "$SCRIPT_DIR/setup.sh"

# Fetch all project items
gh project item-list "$PROJECT_NUM" --owner "$OWNER" --format json
