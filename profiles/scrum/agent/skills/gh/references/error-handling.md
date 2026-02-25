# Error Handling Patterns

All gh skill scripts follow comprehensive error handling patterns introduced in v3.0.0.

## Standard Error Handling Pattern

Every script operation follows this pattern:

```bash
# 1. Validate input
if [ -z "$VAR" ]; then
  echo "❌ ERROR: VAR is required"
  exit 1
fi

# 2. Execute with output capture
OUTPUT=$(command_here 2>&1)
EXIT_CODE=$?

# 3. Check exit code
if [ $EXIT_CODE -ne 0 ]; then
  echo "❌ ERROR: command failed with exit code $EXIT_CODE"
  echo "Output: $OUTPUT"
  exit 1
fi

# 4. Verify result (for critical ops)
VERIFY=$(gh api ...)
if [ "$VERIFY" != "$EXPECTED" ]; then
  echo "❌ ERROR: Verification failed"
  exit 1
fi

echo "✓ Success"
```

## Five Error Handling Principles

### 1. Input Validation

Check all variables before use:

```bash
if [ -z "$ISSUE_NUM" ]; then
  echo "❌ ERROR: --issue is required"
  exit 1
fi

if [ "$KIND" != "epic" ] && [ "$KIND" != "story" ]; then
  echo "❌ ERROR: --kind must be 'epic' or 'story'"
  exit 1
fi
```

### 2. Exit Code Checking

Capture exit codes and fail fast:

```bash
UPDATE_OUTPUT=$(gh project item-edit ... 2>&1)
UPDATE_EXIT=$?

if [ $UPDATE_EXIT -ne 0 ]; then
  echo "❌ ERROR: gh project item-edit failed with exit code $UPDATE_EXIT"
  echo "Output: $UPDATE_OUTPUT"
  exit 1
fi
```

### 3. Output Capture

Capture stderr/stdout for debugging:

```bash
# Captures both stdout and stderr
OUTPUT=$(command 2>&1)

# Use the output for error reporting
if [ $? -ne 0 ]; then
  echo "Output: $OUTPUT"
fi
```

### 4. Post-Operation Verification

For critical operations, verify the result with a follow-up query:

```bash
# Update status
gh project item-edit ...

# Verify it actually changed
CURRENT_STATUS=$(gh api graphql ... | jq ...)
if [ "$CURRENT_STATUS" != "$TO_STATUS" ]; then
  echo "❌ ERROR: Status verification failed!"
  echo "Expected: $TO_STATUS"
  echo "Actual: $CURRENT_STATUS"
  exit 1
fi
```

See [GraphQL Queries](graphql-queries.md) for verification query details.

### 5. Descriptive Errors

Always provide context in error messages:

```bash
# Bad
echo "ERROR: Failed"

# Good
echo "❌ ERROR: Status '$TO_STATUS' not found in project"
echo "Available statuses:"
echo "$FIELD_DATA" | jq -r '.fields[] | select(.name=="Status") | .options[] | .name'
```

## Auto-Recovery Patterns

### Missing Project Item

If an issue isn't in the project, automatically add it:

```bash
ITEM_ID=$(gh project item-list ... | jq ...)

if [ -z "$ITEM_ID" ] || [ "$ITEM_ID" = "null" ]; then
  echo "Issue may not be added to the project. Adding it now..."

  ISSUE_URL="https://github.com/$TEAM_REPO/issues/$ISSUE_NUM"
  gh project item-add "$PROJECT_NUM" --owner "$OWNER" --url "$ISSUE_URL"

  sleep 2
  ITEM_ID=$(gh project item-list ... | jq ...)

  if [ -z "$ITEM_ID" ] || [ "$ITEM_ID" = "null" ]; then
    echo "❌ ERROR: Could not add issue to project"
    exit 1
  fi
fi
```

## Common Error Scenarios

### Scope Errors

**Error:** `Missing 'project' scope on GH_TOKEN`

**Detection:** Checked at start of `setup.sh`

**Solution:** Automatic failure with clear instructions

### API Rate Limits

**Error:** `API rate limit exceeded`

**Detection:** Exit code from gh CLI

**Solution:** Script fails with error message (caller should retry with delay)

### GraphQL Variable Type Errors

**Error:** `Variable type mismatch`

**Prevention:** Always use `-F` (uppercase) for ID types in GraphQL queries

**Example:**
```bash
# Correct - uses -F for ID types
gh api graphql -f query='...' -F projectId="$PROJECT_ID" -F itemId="$ITEM_ID"

# Wrong - uses -f for ID types (treats as string)
gh api graphql -f query='...' -f projectId="$PROJECT_ID" -f itemId="$ITEM_ID"
```

See [GraphQL Queries](graphql-queries.md) for details on the v3.0.0 fix.

## Idempotent Operations

All operations are idempotent - safe to retry:

- Re-adding an existing label: Safe, no-op
- Re-assigning the same user: Safe, no-op
- Setting the same status: Safe, no-op
- Creating duplicate issues: Creates new issue (not idempotent, but safe)

## Exit Codes

All scripts use standard exit codes:

- **0** - Success
- **1** - Error (with descriptive message to stderr)

No custom exit codes - rely on error messages for details.
