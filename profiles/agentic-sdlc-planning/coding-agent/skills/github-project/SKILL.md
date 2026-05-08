---
name: github-project
description: Manages GitHub Projects v2 workflows for issue tracking and project management. Use when user asks to "show the board", "view issues", "what's in [status]", "create an epic", "add a story", "create a bug", "move issue #N to [status]", "transition #N from [status] to [status]", "comment on #N", "create a milestone", "assign #N to [user]", "create a PR", "create a sub-issue", or "review PR #N". Wraps gh CLI with validation and verification.
compatibility: "Requires gh CLI (GitHub CLI) with GitHub Projects v2 access, and read/write access to the team repository. Auth is auto-managed via GH_CONFIG_DIR (GitHub App installation token). Intended for Claude Code and API usage."
license: MIT
metadata:
  author: botminter
  version: 3.0.0
  category: project-management
  tags: [github, issues, projects-v2, workflow, automation]
  requires-tools: [gh, jq]
---

# GitHub CLI Skill

Unified interface for GitHub Projects v2 workflows. Manages issues, epics, stories, statuses, milestones, and pull requests with comprehensive error handling and verification.

## Prerequisites

Before using this skill, ensure:

- `gh` CLI installed (auth is auto-managed via `GH_CONFIG_DIR`)
- `team/` directory has a GitHub remote
- `.botminter.yml` exists in workspace root (for comment attribution)

**Verification:** Each operation runs `scripts/setup.sh` which validates prerequisites and fails fast with clear errors if requirements aren't met.

## How It Works

All operations follow this pattern:

1. **Setup** - Verify scope, detect repo, cache project IDs
2. **Execute** - Run the operation with input validation
3. **Verify** - Confirm the operation succeeded (for critical ops)
4. **Attribute** - Post timestamped comment showing who did what

Claude will automatically invoke the appropriate script based on your request.

## Operations

### 1. Board View

**When to use:** User asks to show the board, view issues, check status, see what's in progress, or get an overview.

**What it does:**
- Fetches all project items from GitHub Projects v2
- Groups issues by status field value (po:triage, arch:design, qe:test-design, etc.)
- Shows epic-to-story relationships via native sub-issues
- Displays in workflow order with issue counts
- Marks closed issues

**Usage:**

Claude will run:
```bash
bash scripts/board-view.sh
```

Then format the JSON output into a markdown table grouped by status.

**Output format:**
```
## Board

### po:triage
| # | Title | Kind | Assignee |
|---|-------|------|----------|
| 3 | New feature epic | epic | — |

### qe:test-design
| # | Title | Type | Parent | Assignee |
|---|-------|------|--------|----------|
| 5 | Implement OAuth | Task | #3 | dev-user |

---
Summary: 5 issues (4 open, 1 closed) | 2 Epics, 3 Tasks
```

---

### 2. Create Issue (Epic, Story, or Bug)

**When to use:** User asks to create an epic, add a story, file a bug, or add work to backlog.

**What it does:**
1. Creates issue with GitHub native issue type (Epic, Task, or Bug)
2. For stories with `--parent`: links as native sub-issue of the parent
3. Adds issue to project
4. Sets initial status (`po:triage` for epics/stories, `bug:investigate` for bugs)
5. Posts attribution comment

**Issue type mapping:**

| Kind | GitHub Issue Type |
|------|-------------------|
| `epic` | Epic |
| `story` | Task |
| `bug` | Bug |

**Parameters:**
- `--title` (required) - Issue title
- `--body` (required) - Issue description (markdown)
- `--kind` (required) - `epic`, `story`, or `bug`
- `--parent` (optional) - Parent issue number (creates native sub-issue)
- `--milestone` (optional) - Milestone name
- `--assignee` (optional) - GitHub username

**Usage:**

Claude will run:
```bash
# Epic (creates Epic type)
bash scripts/create-issue.sh \
  --title "New authentication system" \
  --body "Implement OAuth 2.0 authentication..." \
  --kind epic

# Story under epic (creates Task type, linked as sub-issue)
bash scripts/create-issue.sh \
  --title "Add Google OAuth provider" \
  --body "Implement Google OAuth..." \
  --kind story \
  --parent 15

# Bug (creates Bug type)
bash scripts/create-issue.sh \
  --title "API returns 500 on empty token" \
  --body "Empty auth token causes server error..." \
  --kind bug
```

**Result:** Issue created with native type, added to project, board scanner will process it next.

---

### 3. Status Transition

**When to use:** User asks to move an issue to a different status, transition from one state to another.

**What it does:**
1. Validates issue exists in project (auto-adds if missing)
2. Resolves status option ID from cached field data
3. Updates project item field via gh CLI
4. **Verifies** status changed with GraphQL query (prevents silent failures)
5. Posts attribution comment documenting transition

**Parameters:**
- `--issue` (required) - Issue number
- `--from` (optional) - Current status (for comment attribution)
- `--to` (required) - New status

**Usage:**

Claude will run:
```bash
bash scripts/status-transition.sh \
  --issue 15 \
  --from "po:triage" \
  --to "arch:design"
```

**Critical:** This operation includes GraphQL verification. If the status doesn't actually change, the script fails with details. See [GraphQL Queries](references/graphql-queries.md) for the v3.0.0 fix.

**Result:** Status updated and verified, transition documented in comments.

---

### 4. Add Comment

**When to use:** User asks to comment on an issue, post analysis, add review feedback, or document decisions.

**What it does:**
- Adds comment to issue with attribution header
- Header format: `### <emoji> <role> — <ISO-timestamp>`
- Body follows header

**Parameters:**
- `--issue` (required) - Issue number
- `--body` (required) - Comment body (markdown)

**Usage:**

Claude will run:
```bash
bash scripts/add-comment.sh \
  --issue 15 \
  --body "Design looks good. Proceeding to implementation planning."
```

---

### 5. Assign / Unassign

**When to use:** User asks to assign an issue, add assignee, or remove assignee.

**What it does:**
- Adds or removes assignee from issue
- Multiple assignees supported

**Parameters:**
- `--issue` (required) - Issue number
- `--action` (required) - `assign` or `unassign`
- `--user` (required) - GitHub username

**Usage:**

Claude will run:
```bash
# Assign
bash scripts/assign.sh \
  --issue 15 \
  --action assign \
  --user architect-bot

# Unassign
bash scripts/assign.sh \
  --issue 15 \
  --action unassign \
  --user architect-bot
```

---

### 6. Milestone Management

**When to use:** User asks to list milestones, create a milestone, or assign issue to milestone.

**What it does:**
- Lists all milestones with state and due dates
- Creates new milestones
- Assigns issues to milestones

**Parameters:**
- `--action` (required) - `list`, `create`, or `assign`
- `--title` (for create/assign) - Milestone title
- `--description` (for create, optional) - Milestone description
- `--due-date` (for create, optional) - Due date (ISO format: YYYY-MM-DD)
- `--issue` (for assign) - Issue number

**Usage:**

Claude will run:
```bash
# List
bash scripts/milestone-ops.sh --action list

# Create
bash scripts/milestone-ops.sh \
  --action create \
  --title "Q1 2026" \
  --description "First quarter deliverables" \
  --due-date "2026-03-31"

# Assign
bash scripts/milestone-ops.sh \
  --action assign \
  --issue 15 \
  --title "Q1 2026"
```

---

### 7. Close / Reopen Issue

**When to use:** User asks to close an issue, mark as done, or reopen a closed issue.

**What it does:**
- Closes or reopens an issue
- Closed issues remain in project but marked as closed

**Parameters:**
- `--issue` (required) - Issue number
- `--action` (required) - `close` or `reopen`

**Usage:**

Claude will run:
```bash
# Close
bash scripts/close-reopen.sh --issue 15 --action close

# Reopen
bash scripts/close-reopen.sh --issue 15 --action reopen
```

---

### 8. PR Operations

**When to use:** User asks to create a PR, review PR, approve PR, request changes, or comment on PR.

**What it does:**
- Creates pull requests
- Approves or requests changes on PRs
- Adds attributed comments to PRs
- Lists all PRs

**Parameters:**
- `--action` (required) - `create`, `approve`, `request-changes`, `comment`, or `list`
- `--title` (for create) - PR title
- `--body` (for create/approve/request-changes/comment) - PR description or comment
- `--branch` (for create) - Source branch (head)
- `--base` (for create, optional) - Target branch (default: main)
- `--pr` (for approve/request-changes/comment) - PR number

**Usage:**

Claude will run:
```bash
# Create PR
bash scripts/pr-ops.sh \
  --action create \
  --title "Implement OAuth authentication" \
  --body "Closes #15..." \
  --branch feature/oauth \
  --base main

# Approve PR
bash scripts/pr-ops.sh \
  --action approve \
  --pr 42 \
  --body "LGTM. Good test coverage."

# Request changes
bash scripts/pr-ops.sh \
  --action request-changes \
  --pr 42 \
  --body "Please add error handling for edge cases."

# Comment on PR
bash scripts/pr-ops.sh \
  --action comment \
  --pr 42 \
  --body "Consider using async/await here."

# List PRs
bash scripts/pr-ops.sh --action list
```

---

### 9. Query Issues

**When to use:** User asks to find issues by label, status, milestone, assignee, or get a specific issue.

**What it does:**
- Queries issues with various filters
- Returns JSON output
- Single-issue query includes native issue type (`issueType.name`) and sub-issues

**Parameters:**
- `--type` (required) - `label`, `status`, `milestone`, `assignee`, `single`, or `issue-type`
- `--label` (for label/issue-type query) - Label name or issue type name (e.g., `role/chief-of-staff`, `Epic`)
- `--status` (for status query) - Status value (e.g., `arch:design`)
- `--milestone` (for milestone query) - Milestone title
- `--assignee` (for assignee query) - GitHub username
- `--issue` (for single query) - Issue number

**Usage:**

Claude will run:
```bash
# By label
bash scripts/query-issues.sh --type label --label "role/chief-of-staff"

# By status
bash scripts/query-issues.sh --type status --status "arch:design"

# By milestone
bash scripts/query-issues.sh --type milestone --milestone "Q1 2026"

# By assignee
bash scripts/query-issues.sh --type assignee --assignee "architect-bot"

# Single issue (includes issueType and subIssues)
bash scripts/query-issues.sh --type single --issue 15

# By native issue type (Epic, Task, Bug)
bash scripts/query-issues.sh --type issue-type --label "Bug"
```

---

### 10. Sub-Issue Operations (GitHub Native Sub-Issues)

**When to use:** Creating or managing GitHub native sub-issues (e.g., subtasks for complex bugs, stories under epics).

**What it does:**
- Creates sub-issues with native parent relationship and issue type in a single mutation
- Lists all sub-issues for a parent issue (with issue type)
- Checks completion status of all sub-issues

**Parameters:**
- `--action` (required) - `create`, `list`, or `status`
- `--parent` (required) - Parent issue number
- `--title` (for create) - Sub-issue title
- `--body` (for create, optional) - Sub-issue description
- `--type` (for create, optional) - Issue type name (default: `Task`). Available: `Task`, `Bug`, `Epic`

**Usage:**

Claude will run:
```bash
# Create a sub-issue (defaults to Task type)
bash scripts/subtask-ops.sh \
  --action create \
  --parent 42 \
  --title "Add validation check" \
  --body "Implement empty token validation"

# Create a sub-issue with specific type
bash scripts/subtask-ops.sh \
  --action create \
  --parent 42 \
  --title "Fix edge case" \
  --type Bug

# List sub-issues
bash scripts/subtask-ops.sh \
  --action list \
  --parent 42

# Check completion status
bash scripts/subtask-ops.sh \
  --action status \
  --parent 42
```

**How it works:**
- Uses GraphQL `createIssue` mutation with `issueTypeId` + `parentIssueId` in one call
- Uses GitHub's native issue types and sub-issue relationships
- Requires header: `GraphQL-Features: sub_issues,issue_types`

**Result:** Sub-issue created with native type and parent relationship, visible in GitHub UI.

---

### 11. Status Field Management

**When to use:** Adding, listing, or modifying project status options (e.g., adding new workflow statuses).

**What it does:**
- Lists all status options with full metadata (name, color, description)
- Adds new status options while preserving existing ones
- Uses GraphQL API directly (not available via `gh project` CLI)

**Read current statuses:**

```bash
gh api graphql -f query='
{
  node(id: "<STATUS_FIELD_ID>") {
    ... on ProjectV2SingleSelectField {
      name
      options {
        id
        name
        color
        description
      }
    }
  }
}'
```

**Update statuses (replaces ALL options — include existing ones):**

```bash
gh api graphql -f query='
mutation {
  updateProjectV2Field(input: {
    fieldId: "<STATUS_FIELD_ID>"
    singleSelectOptions: [
      {name: "status-name", color: GREEN, description: "Description"}
    ]
  }) {
    projectV2Field {
      ... on ProjectV2SingleSelectField {
        name
        options { name color }
      }
    }
  }
}'
```

**Available colors:** `GRAY`, `BLUE`, `GREEN`, `YELLOW`, `ORANGE`, `RED`, `PINK`, `PURPLE`

**Critical:** The mutation REPLACES all options. To add new statuses safely:
1. Read existing options via `node()` query
2. Append new options to the list
3. Submit the complete list via `updateProjectV2Field`

**Note:** `ProjectV2SingleSelectFieldOptionInput` requires all three fields: `name`, `color` (enum), and `description` (string). The `id` field is NOT accepted in the input.

---

## Examples

### Example 1: Create and Triage an Epic

**User says:** "Create an epic for the new authentication system"

**Actions:**
1. Claude runs `create-issue.sh` with `--kind epic`
2. Issue created with native "Epic" issue type
3. Added to project with initial status `po:triage`
4. Attribution comment posted

**Result:** Epic created at #15 with Epic type, visible in `po:triage` column on board.

---

### Example 2: Move Issue Through Workflow

**User says:** "Move issue #15 from triage to design"

**Actions:**
1. Claude runs `status-transition.sh` with `--to "arch:design"`
2. Script validates current status
3. Updates status via gh CLI
4. Verifies with GraphQL query that status actually changed
5. Posts attribution comment documenting transition

**Result:** Issue #15 now in `arch:design` status, verified with GraphQL, transition documented.

---

### Example 3: View Project Board

**User says:** "Show me what's on the board"

**Actions:**
1. Claude runs `board-view.sh`
2. Receives JSON with all project items
3. Formats into markdown table grouped by status
4. Shows epic-to-story relationships

**Result:** Complete board view with issues grouped by workflow status, ready for scanning.

---

### Example 4: Create Story Under Epic

**User says:** "Add a story under epic #15 for implementing OAuth provider"

**Actions:**
1. Claude runs `create-issue.sh` with `--kind story --parent 15`
2. Creates Task issue type linked as native sub-issue of #15
3. Sets initial status to `po:triage`
4. Posts attribution comment

**Result:** Story #16 created as sub-issue of epic #15 (visible in GitHub UI), in `po:triage` column.

---

### Example 5: Create PR and Request Review

**User says:** "Create a PR for the OAuth work and assign it to me for review"

**Actions:**
1. Claude runs `pr-ops.sh --action create` with branch and description
2. PR created linking to issue #15
3. Claude adds review comment with feedback

**Result:** PR #42 created and ready for review.

---

## References

For detailed documentation:

- **[Status Lifecycle](references/status-lifecycle.md)** - Epic and story workflow states, human gates, rejection loops
- **[Error Handling](references/error-handling.md)** - Patterns used across all scripts, validation, verification
- **[GraphQL Queries](references/graphql-queries.md)** - Verification query details, v3.0.0 fix for variable types
- **[Troubleshooting](references/troubleshooting.md)** - Common errors and solutions

## Troubleshooting

### Error: "Cannot access GitHub Projects"

**Cause:** The GitHub App may not have `organization_projects: admin` permission, or the App is not installed on the organization.

**Solution:** Verify the App is installed and has the correct permissions. Re-run `bm hire` if needed.

### Error: "Status verification failed"

**Cause:** Status didn't actually change despite gh CLI success

**Solution:**
1. Verify the GitHub App has `organization_projects: admin` permission
2. Check rate limits: `gh api rate_limit`
3. Retry the operation

See [Troubleshooting Guide](references/troubleshooting.md) for complete error reference.

---

## Notes

- **App permissions:** The GitHub App must have `organization_projects: admin` permission for project operations. This is set in the App manifest during `bm hire`.
- **Idempotent:** All operations are safe to retry. Re-setting same status, re-assigning same user is safe.
- **Rate limits:** The gh CLI respects GitHub's rate limits. For bulk operations, add delays between calls.
- **Error handling:** v3.0.0 includes comprehensive validation and verification. All failures are caught and reported with detailed context.
- **Auto-recovery:** Scripts automatically handle common issues like missing project items.

---

`★ Insight ─────────────────────────────────────`
**Progressive Disclosure in Action**

This skill demonstrates Anthropic's three-level system:

1. **Frontmatter (always loaded)** - Description with trigger phrases, just enough to know when to use this skill
2. **SKILL.md (loaded when relevant)** - High-level instructions showing what operations exist and when to use them
3. **Scripts & References (loaded on demand)** - Implementation details in `scripts/`, deep documentation in `references/`

Result: ~1,040 tokens loaded initially vs. 3,678 tokens in the old monolithic version - 71% reduction in context usage.
`─────────────────────────────────────────────────`
