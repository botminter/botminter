---
name: gh
description: Manages GitHub Projects v2 workflows for issue tracking and project management. Use when user asks to "show the board", "view issues", "what's in [status]", "create an epic", "add a story", "move issue #N to [status]", "transition #N from [status] to [status]", "comment on #N", "create a milestone", "assign #N to [user]", "create a PR", or "review PR #N". Wraps gh CLI with validation and verification.
compatibility: "Requires gh CLI (GitHub CLI) with 'project' token scope, GitHub Projects v2, GH_TOKEN environment variable, and read/write access to the team repository. Intended for Claude Code and API usage."
license: MIT
metadata:
  author: botminter
  version: 3.0.0
  category: project-management
  tags: [github, issues, projects-v2, workflow, automation]
  requires-tools: [gh, jq]
  requires-env: [GH_TOKEN]
  requires-scope: [project]
---

# GitHub CLI Skill

Unified interface for GitHub Projects v2 workflows. Manages issues, epics, stories, statuses, milestones, and pull requests with comprehensive error handling and verification.

## Prerequisites

Before using this skill, ensure:

- `gh` CLI installed and authenticated
- `GH_TOKEN` environment variable set (shared team token)
- **`project` token scope** enabled: `gh auth refresh -s project`
- `.botminter/` directory has a GitHub remote
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
- Groups issues by status field value (po:triage, arch:design, dev:ready, etc.)
- Shows epic-to-story relationships via parent labels
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

### dev:ready
| # | Title | Kind | Parent | Assignee |
|---|-------|------|--------|----------|
| 5 | Implement OAuth | story | #3 | dev-user |

---
Summary: 5 issues (4 open, 1 closed) | 2 epics, 3 stories
```

---

### 2. Create Issue (Epic or Story)

**When to use:** User asks to create an epic, add a story, file a new issue, or add work to backlog.

**What it does:**
1. Creates issue with appropriate `kind/*` label
2. For stories: adds `parent/<number>` label and "Parent: #N" to body
3. Adds issue to project
4. Sets initial status to `po:triage`
5. Posts attribution comment

**Parameters:**
- `--title` (required) - Issue title
- `--body` (required) - Issue description (markdown)
- `--kind` (required) - `epic` or `story`
- `--parent` (optional) - Parent epic number (for stories)
- `--milestone` (optional) - Milestone name
- `--assignee` (optional) - GitHub username

**Usage:**

Claude will run:
```bash
# Epic
bash scripts/create-issue.sh \
  --title "New authentication system" \
  --body "Implement OAuth 2.0 authentication..." \
  --kind epic

# Story under epic
bash scripts/create-issue.sh \
  --title "Add Google OAuth provider" \
  --body "Implement Google OAuth..." \
  --kind story \
  --parent 15
```

**Result:** Issue created, added to project with status `po:triage`, board scanner will process it next.

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

**Parameters:**
- `--type` (required) - `label`, `status`, `milestone`, `assignee`, or `single`
- `--label` (for label query) - Label name (e.g., `kind/epic`)
- `--status` (for status query) - Status value (e.g., `arch:design`)
- `--milestone` (for milestone query) - Milestone title
- `--assignee` (for assignee query) - GitHub username
- `--issue` (for single query) - Issue number

**Usage:**

Claude will run:
```bash
# By label
bash scripts/query-issues.sh --type label --label "kind/epic"

# By status
bash scripts/query-issues.sh --type status --status "arch:design"

# By milestone
bash scripts/query-issues.sh --type milestone --milestone "Q1 2026"

# By assignee
bash scripts/query-issues.sh --type assignee --assignee "architect-bot"

# Single issue
bash scripts/query-issues.sh --type single --issue 15
```

---

## Examples

### Example 1: Create and Triage an Epic

**User says:** "Create an epic for the new authentication system"

**Actions:**
1. Claude runs `create-issue.sh` with `--kind epic`
2. Issue added to project with initial status `po:triage`
3. Attribution comment posted
4. Reports issue number and URL

**Result:** Epic created at #15, visible in `po:triage` column on board, ready for PO review.

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
2. Adds `parent/15` label and "Parent: #15" to body
3. Sets initial status to `po:triage` (same as epics - board scanner will process workflow)
4. Posts attribution comment

**Result:** Story #16 created, linked to epic #15, visible in `po:triage` column, ready for board scanner to pick up.

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

### Error: "Missing 'project' scope on GH_TOKEN"

**Solution:**
```bash
gh auth refresh -s project
gh auth status  # Verify scope is enabled
```

### Error: "Status verification failed"

**Cause:** Status didn't actually change despite gh CLI success

**Solution:**
1. Verify token has `project` scope
2. Check rate limits: `gh api rate_limit`
3. Retry the operation

See [Troubleshooting Guide](references/troubleshooting.md) for complete error reference.

---

## Notes

- **Token scope:** The `project` scope is required for all project operations. Verified at start of every script.
- **No write-locks:** GitHub handles concurrent access natively. Multiple agents can safely operate simultaneously.
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
