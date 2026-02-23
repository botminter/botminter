---
name: gh
description: Use this skill for all GitHub interaction — viewing the board, creating issues (epics/stories), updating project statuses, adding comments, managing milestones, and PR operations. Wraps the `gh` CLI. Replaces the old `board` and `create-epic` skills.
version: 2.0.0
---

# GitHub CLI Skill

Single interaction point for all GitHub operations. Uses the `gh` CLI with a shared team token (`GH_TOKEN` env var). All operations target the team repo, auto-detected from `.botminter/`'s git remote. Issue status is tracked via GitHub Projects v2 (a single-select field on project items), not labels.

## Prerequisites

- `gh` CLI installed
- `GH_TOKEN` env var set (shared team token, passed via `just launch`)
- `project` token scope required: `gh auth refresh -s project`
- `.botminter/` has a GitHub remote
- `.botminter.yml` exists in the member's directory (for comment attribution)

## Repo Auto-Detection

All `gh` commands target the team repo. Detect it once per session:

```bash
TEAM_REPO=$(cd .botminter && gh repo view --json nameWithOwner -q .nameWithOwner 2>/dev/null)

# Fallback: extract owner/repo from git remote URL
if [ -z "$TEAM_REPO" ]; then
  TEAM_REPO=$(cd .botminter && git remote get-url origin | sed 's|.*github.com[:/]||;s|\.git$||')
fi
```

Then use `--repo "$TEAM_REPO"` on every `gh` command.

## Project Setup (ID Caching)

GitHub Projects v2 uses opaque IDs (`PVT_...`, `PVTSSF_...`, etc.) for projects, fields, and field options. Resolve and cache these once per session:

```bash
# Resolve project IDs (cache once per session)
OWNER=$(echo "$TEAM_REPO" | cut -d/ -f1)
PROJECT_NUM=$(gh project list --owner "$OWNER" --format json --jq '.[0].number')
PROJECT_ID=$(gh project view "$PROJECT_NUM" --owner "$OWNER" --format json --jq '.id')
FIELD_DATA=$(gh project field-list "$PROJECT_NUM" --owner "$OWNER" --format json)
STATUS_FIELD_ID=$(echo "$FIELD_DATA" | jq -r '.fields[] | select(.name=="Status") | .id')
```

These five variables (`OWNER`, `PROJECT_NUM`, `PROJECT_ID`, `FIELD_DATA`, `STATUS_FIELD_ID`) are used throughout all project operations. Cache them at the start of each session and reuse.

## Member Identity

Read the member's role and emoji from `.botminter.yml` (located in the workspace root, surfaced from the member's skeleton):

```bash
ROLE=$(grep '^role:' .botminter.yml | awk '{print $2}')
EMOJI=$(grep '^comment_emoji:' .botminter.yml | sed 's/comment_emoji: *"//' | sed 's/"$//')
```

Used for comment attribution: `### <emoji> <role> — <ISO-timestamp>`.

---

## Operations

### 1. Board View

Displays all issues grouped by project status with epic-to-story relationships. Read-only.

**When to use:** When asked to show the board, view issues, check issue status, see what's in progress, or get a board overview.

**Command:**

```bash
gh project item-list "$PROJECT_NUM" --owner "$OWNER" --format json
```

**Behavior:**

1. Run the command above to fetch all project items with their status field values
2. For each item, extract the `kind/*` label from the issue's labels (use `gh issue view` if needed for label data)
3. Build epic-to-story relationships. Stories reference their parent epic via a `Parent: #<number>` line in the issue body, or via a `parent/<number>` label
4. Group issues by their project status field value in this display order:
   - `po:triage`
   - `po:backlog`
   - `arch:design`
   - `po:design-review`
   - `arch:plan`
   - `po:plan-review`
   - `arch:breakdown`
   - `po:ready`
   - `arch:in-progress`
   - `po:accept`
   - `done`
   - `error`
   - Any other statuses (e.g., `dev:ready` for stories)

**Output format:**

```
## Board

### po:triage
| # | Title | Kind | Assignee |
|---|-------|------|----------|
| 3 | New feature epic | epic | — |

### arch:design
| # | Title | Kind | Assignee |
|---|-------|------|----------|
| 1 | Infrastructure epic | epic | architect |
|   └── stories: #4, #5

### dev:ready
| # | Title | Kind | Parent | Assignee |
|---|-------|------|--------|----------|
| 4 | Setup CI pipeline | story | #1 | — |
| 5 | Add monitoring | story | #1 | — |

### done
| # | Title | Kind | Assignee |
|---|-------|------|----------|
| 2 | Initial setup | epic | — |
  (closed)

---
Summary: 5 issues (4 open, 1 closed) | 2 epics, 3 stories
```

- Show the `kind/*` label as the Kind column (epic or story)
- For stories, include a Parent column showing `#<parent-number>`
- Mark closed issues with `(closed)`
- Include a summary line with total counts
- Omit status groups that have no issues

### 2. Create Issue (Epic or Story)

Creates a new issue with appropriate labels, adds it to the project, and sets the initial status.

**When to use:** When asked to create an epic, add a story, file a new issue, or add work items to the backlog.

**Parameters:**

| Parameter | Required | Description |
|-----------|----------|-------------|
| `title` | Yes | Issue title (concise, descriptive) |
| `body` | Yes | Issue description (markdown — goals, scope, context) |
| `kind` | Yes | `epic` or `story` |
| `parent` | No | Parent epic number (for stories — adds `parent/<number>` label) |
| `milestone` | No | Milestone name to assign |
| `assignee` | No | GitHub username to assign |

**Command (epic):**

```bash
ISSUE_URL=$(gh issue create --repo "$TEAM_REPO" \
  --title "<title>" \
  --body "<body>" \
  --label "kind/epic")
ISSUE_NUM=$(echo "$ISSUE_URL" | grep -o '[0-9]*$')
```

**Command (story):**

```bash
ISSUE_URL=$(gh issue create --repo "$TEAM_REPO" \
  --title "<title>" \
  --body "Parent: #<parent>\n\n<body>" \
  --label "kind/story" \
  --label "parent/<parent>")
ISSUE_NUM=$(echo "$ISSUE_URL" | grep -o '[0-9]*$')
```

Optional flags: `--milestone "<name>"`, `--assignee "<username>"`.

**Add to project and set initial status:**

```bash
# Add issue to project
gh project item-add "$PROJECT_NUM" --owner "$OWNER" --url "$ISSUE_URL"

# Get the item ID for the newly added issue
ITEM_ID=$(gh project item-list "$PROJECT_NUM" --owner "$OWNER" --format json \
  --jq ".items[] | select(.content.number == $ISSUE_NUM) | .id")

# Resolve the option ID for the initial status
OPTION_ID=$(echo "$FIELD_DATA" | jq -r '.fields[] | select(.name=="Status") | .options[] | select(.name=="po:triage") | .id')

# Set initial status
gh project item-edit --project-id "$PROJECT_ID" --id "$ITEM_ID" \
  --field-id "$STATUS_FIELD_ID" --single-select-option-id "$OPTION_ID"
```

**After creation:**

Add an attribution comment to the new issue:

```bash
TIMESTAMP=$(date -u +%Y-%m-%dT%H:%M:%SZ)
gh issue comment "$ISSUE_NUM" --repo "$TEAM_REPO" \
  --body "### $EMOJI $ROLE — $TIMESTAMP

Created $([ "$kind" = "epic" ] && echo "epic" || echo "story"): <title>"
```

**Output:** Report issue number, URL, initial status (`po:triage`), and next step (board scanner will pick it up).

### 3. Status Transition (Update Project Status)

Transitions an issue from one status to another by updating the project field. This is a single atomic operation (no remove+add race condition).

**When to use:** When moving an issue through the workflow (e.g., triage -> design, design -> plan, ready -> in-progress).

**Parameters:**

| Parameter | Required | Description |
|-----------|----------|-------------|
| `issue` | Yes | Issue number |
| `from` | Yes | Current status (e.g., `po:triage`) — for comment attribution only |
| `to` | Yes | New status (e.g., `arch:design`) |

**Command:**

```bash
# Resolve option ID for target status
OPTION_ID=$(echo "$FIELD_DATA" | jq -r '.fields[] | select(.name=="Status") | .options[] | select(.name=="'"$TO_STATUS"'") | .id')

# Get item ID for the issue
ITEM_ID=$(gh project item-list "$PROJECT_NUM" --owner "$OWNER" --format json \
  --jq ".items[] | select(.content.number == $ISSUE_NUM) | .id")

# Update status (single operation, no remove+add needed)
gh project item-edit --project-id "$PROJECT_ID" --id "$ITEM_ID" \
  --field-id "$STATUS_FIELD_ID" --single-select-option-id "$OPTION_ID"
```

**After transition:** Add an attribution comment documenting the transition:

```bash
TIMESTAMP=$(date -u +%Y-%m-%dT%H:%M:%SZ)
gh issue comment "$ISSUE_NUM" --repo "$TEAM_REPO" \
  --body "### $EMOJI $ROLE — $TIMESTAMP

Status: $FROM_STATUS → $TO_STATUS"
```

### 4. Add Comment

Adds a role-attributed comment to an issue.

**When to use:** When posting analysis, design decisions, review feedback, or any discussion on an issue.

**Parameters:**

| Parameter | Required | Description |
|-----------|----------|-------------|
| `issue` | Yes | Issue number |
| `body` | Yes | Comment body (markdown) |

**Command:**

```bash
TIMESTAMP=$(date -u +%Y-%m-%dT%H:%M:%SZ)
gh issue comment "$ISSUE_NUM" --repo "$TEAM_REPO" \
  --body "### $EMOJI $ROLE — $TIMESTAMP

$BODY"
```

The comment header (`### <emoji> <role> — <timestamp>`) is always prepended automatically. The `body` parameter is the content below the header.

### 5. Assign / Unassign

Assigns or removes an assignee from an issue.

**Commands:**

```bash
# Assign
gh issue edit "$ISSUE_NUM" --repo "$TEAM_REPO" --add-assignee "<username>"

# Unassign
gh issue edit "$ISSUE_NUM" --repo "$TEAM_REPO" --remove-assignee "<username>"
```

### 6. Milestone Management

Creates, lists, and assigns milestones.

**List milestones:**

```bash
gh api "repos/$TEAM_REPO/milestones" --jq '.[] | {number, title, state, due_on}'
```

**Create milestone:**

```bash
gh api "repos/$TEAM_REPO/milestones" -f title="<title>" -f description="<desc>" -f due_on="<ISO-date>"
```

**Assign issue to milestone:**

```bash
gh issue edit "$ISSUE_NUM" --repo "$TEAM_REPO" --milestone "<milestone-title>"
```

### 7. Close / Reopen Issue

**Commands:**

```bash
# Close
gh issue close "$ISSUE_NUM" --repo "$TEAM_REPO"

# Reopen
gh issue reopen "$ISSUE_NUM" --repo "$TEAM_REPO"
```

### 8. PR Operations

Create, review, and comment on pull requests.

**Create PR:**

```bash
gh pr create --repo "$TEAM_REPO" \
  --title "<title>" \
  --body "<body>" \
  --base main \
  --head "<branch>"
```

**Review PR:**

```bash
# Approve
gh pr review "$PR_NUM" --repo "$TEAM_REPO" --approve --body "<comment>"

# Request changes
gh pr review "$PR_NUM" --repo "$TEAM_REPO" --request-changes --body "<comment>"
```

**Comment on PR:**

```bash
TIMESTAMP=$(date -u +%Y-%m-%dT%H:%M:%SZ)
gh pr comment "$PR_NUM" --repo "$TEAM_REPO" \
  --body "### $EMOJI $ROLE — $TIMESTAMP

$BODY"
```

**List PRs:**

```bash
gh pr list --repo "$TEAM_REPO" --state all \
  --json number,title,state,labels,author,reviewDecision
```

### 9. Query Issues (Filtered)

Fetch issues matching specific criteria for targeted lookups.

**By label (kind, parent, etc.):**

```bash
gh issue list --repo "$TEAM_REPO" --label "<label>" \
  --json number,title,state,labels,assignees
```

**By project status:**

```bash
gh project item-list "$PROJECT_NUM" --owner "$OWNER" --format json \
  --jq ".items[] | select(.status == \"$TARGET_STATUS\")"
```

**By milestone:**

```bash
gh issue list --repo "$TEAM_REPO" --milestone "<milestone>" \
  --json number,title,state,labels,assignees
```

**By assignee:**

```bash
gh issue list --repo "$TEAM_REPO" --assignee "<username>" \
  --json number,title,state,labels,assignees
```

**Single issue detail:**

```bash
gh issue view "$ISSUE_NUM" --repo "$TEAM_REPO" --json number,title,state,labels,assignees,milestone,body,comments
```

---

## Status & Label Scheme Reference

### Kind Labels (GitHub Labels)

Classification labels applied directly to issues:

- `kind/epic` — top-level work item
- `kind/story` — child work item under an epic

### Parent Label (GitHub Labels)

- `parent/<number>` — links a story to its parent epic

### Project Statuses (GitHub Projects v2 Field)

Status is tracked as a single-select field on the GitHub Project, not as labels. Each value below is an option in the project's "Status" field.

**Epic lifecycle:**
- `po:triage` — newly created, awaiting PO triage
- `po:backlog` — triaged, in backlog
- `arch:design` — architect designing solution
- `po:design-review` — PO reviewing design
- `arch:plan` — architect planning implementation
- `po:plan-review` — PO reviewing plan
- `arch:breakdown` — architect breaking epic into stories
- `po:ready` — stories ready for development
- `arch:in-progress` — development in progress
- `po:accept` — PO accepting completed work
- `done` — completed
- `error` — blocked or errored

**Story lifecycle:**
- `dev:ready` — ready for development
- `dev:in-progress` — being implemented
- `dev:review` — code review
- `qe:testing` — QE verification
- `done` — completed

## Notes

- **Token scope.** The `project` scope is required for all project operations. Run `gh auth refresh -s project` to add it.
- **No write-locks.** GitHub handles concurrent access natively. Multiple agents can safely read and write issues simultaneously.
- **All operations are idempotent.** Re-adding an existing label, re-assigning the same user, or setting the same project status is safe.
- **Rate limits.** The `gh` CLI respects GitHub's rate limits. For bulk operations, add a brief delay between calls.
- **Error handling.** If a `gh` command fails, check: (1) `GH_TOKEN` is set, (2) the `project` scope is enabled, (3) the repo exists and is accessible, (4) the issue/PR number is valid.
