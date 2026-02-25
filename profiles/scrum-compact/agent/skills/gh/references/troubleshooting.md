# Troubleshooting Guide

## Error: "Missing 'project' scope on GH_TOKEN"

**When:** At skill invocation (setup.sh verification)

**Cause:** The GitHub token doesn't have the `project` scope enabled

**Solution:**

```bash
# Refresh token with project scope
gh auth refresh -s project

# Or for specific host:
gh auth refresh -h github.com -s project

# Verify scope is enabled:
gh auth status
# Look for: ✓ Token scopes: ..., 'project'
```

**Why this happens:** GitHub Projects v2 operations require the `project` scope. This is separate from `repo` scope.

---

## Error: "Issue #N not found in project"

**When:** During status transition or query operations

**Cause:** Issue exists in the repository but wasn't added to the GitHub Project

**Solution:** The skill auto-recovers by:

1. Adding the issue to the project automatically
2. Retrying the item ID lookup
3. Proceeding with the operation

**Manual fix (if auto-recovery fails):**

```bash
# Add issue to project manually
gh project item-add "$PROJECT_NUM" --owner "$OWNER" \
  --url "https://github.com/org/repo/issues/N"
```

---

## Error: "Status verification failed"

**When:** After status transition (status-transition.sh)

**Full error:**

```
❌ ERROR: Status verification failed!
Expected: arch:design
Actual: po:triage
The gh project item-edit command appeared to succeed but the status did not change.
This may indicate a permissions issue or an API error.
```

**Root causes:**

1. **Token lacks `project` scope** (most common)
2. GraphQL API rate limit or transient error
3. Status option ID was invalid
4. Permissions issue (read vs write access)

**Solution:**

1. **First, verify token scope:**

```bash
gh auth status
# Look for: ✓ Token scopes: ..., 'project'
```

If missing:

```bash
gh auth refresh -s project
```

2. **Check rate limits:**

```bash
gh api rate_limit
# Look at: resources.graphql.remaining
```

3. **Retry the operation** - transient errors usually resolve

4. **Verify project permissions:**
   - Check you have write access to the project
   - Verify the project ID is correct

---

## Error: "Could not fetch project field list"

**When:** During setup (setup.sh project configuration)

**Cause:** Project doesn't have required fields or token lacks permissions

**Solution:**

1. **Verify the GitHub Project exists:**

```bash
gh project list --owner ORG_NAME
```

2. **Check the project has a "Status" field:**
   - Open the project in GitHub UI
   - Verify there's a single-select field named exactly "Status"

3. **Check token has `project` scope:**

```bash
gh auth status
```

4. **Verify you have project access:**
   - Check you're a member of the organization
   - Verify you have read access to the project

---

## Error: "GraphQL query failed" or "Variable type mismatch"

**When:** During status verification (rare in v3.0.0)

**Cause:** GraphQL variable passed with wrong type

**Note:** v3.0.0 fixed this by using `-F` (uppercase) for ID type variables instead of `-f` (lowercase).

**If this error appears:**

1. Check that all GraphQL variables use `-F` for ID types:

```bash
# Correct:
gh api graphql -f query='...' -F projectId="$ID" -F itemId="$ID"

# Wrong:
gh api graphql -f query='...' -f projectId="$ID" -f itemId="$ID"
```

2. Verify the query syntax is valid GraphQL

3. Check logs for the actual GraphQL error message

**See:** [GraphQL Queries Reference](graphql-queries.md) for details

---

## Error: "No GitHub Project found for organization"

**When:** During setup (setup.sh project detection)

**Cause:** Organization has no GitHub Projects or wrong organization detected

**Solution:**

1. **Verify the team repo is correct:**

```bash
cd .botminter
gh repo view
```

2. **Check organization has projects:**

```bash
gh project list --owner ORG_NAME
```

3. **If no projects exist, create one:**
   - Go to GitHub organization page
   - Create a new Project (Projects v2)
   - Add a "Status" single-select field

---

## Error: "Status 'X' not found in project"

**When:** During status transition

**Cause:** The requested status doesn't exist in the project's Status field options

**Solution:**

1. **List available statuses:**

```bash
gh project field-list PROJECT_NUM --owner OWNER --format json \
  | jq -r '.fields[] | select(.name=="Status") | .options[] | .name'
```

2. **Add missing status to project:**
   - Open project in GitHub UI
   - Edit the Status field
   - Add the missing status option

**Common status values:** See [Status Lifecycle Reference](status-lifecycle.md)

---

## Error: "Could not detect team repository"

**When:** During setup (setup.sh repo detection)

**Cause:** `.botminter/` directory doesn't have a GitHub remote

**Solution:**

1. **Check .botminter/ has a git remote:**

```bash
cd .botminter
git remote -v
```

2. **If no remote, add one:**

```bash
cd .botminter
git remote add origin https://github.com/org/team-repo.git
```

3. **Verify gh CLI can access it:**

```bash
cd .botminter
gh repo view
```

---

## Warning: "Status updated but comment failed"

**When:** After successful status transition

**Cause:** Status was updated successfully but the attribution comment couldn't be posted

**Impact:** Minor - status change succeeded, only the audit comment is missing

**Solution:** Usually transient. The operation succeeded. If this happens repeatedly:

1. Check token has `repo` scope for issue comments:

```bash
gh auth status
```

2. Verify you have write access to issues

---

## Performance: Slow operations

**Symptom:** Scripts take several seconds to complete

**Causes:**

1. **GraphQL verification delay** - intentional 1-second sleep for API consistency
2. **Project item indexing** - 2-second wait after adding items
3. **Large projects** - fetching 100+ items takes time

**Solutions:**

- **For verification:** The 1-second sleep is required for API consistency
- **For bulk operations:** Add delays between calls to avoid rate limits
- **For large projects:** No current optimization (gh CLI limitation)

**Not a bug:** These delays prevent race conditions and ensure consistency

---

## Common User Errors

### Using issue number instead of PR number

**Error:** `gh pr review 123` fails

**Solution:** Use PR number, not issue number (they're different)

### Wrong milestone name

**Error:** Milestone assignment fails

**Solution:** Use exact milestone title (case-sensitive):

```bash
# List milestones to get exact names:
gh api "repos/$TEAM_REPO/milestones" --jq '.[].title'
```

### Wrong assignee username

**Error:** Assignment fails with "user not found"

**Solution:** Use exact GitHub username (case-sensitive)

---

## Debug Mode

To debug script execution, run with bash trace:

```bash
bash -x scripts/status-transition.sh --issue 123 --to "arch:design"
```

This shows every command executed and variable expansion.

---

## Getting Help

If you encounter errors not covered here:

1. Check the error message for specific details
2. Verify token scope with `gh auth status`
3. Review [Error Handling Patterns](error-handling.md)
4. Check [GraphQL Queries Reference](graphql-queries.md) for API details
5. Run with `bash -x` for detailed trace
