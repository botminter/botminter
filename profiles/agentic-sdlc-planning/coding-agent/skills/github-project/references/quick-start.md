# GitHub Project Skill Quick Start

## For Users (Claude)

Just ask naturally:

- "Show me the board"
- "Create an epic for X"
- "Move issue #15 to design"
- "Add a comment to #42"
- "Create a PR for this branch"

Claude will handle the rest.

## For Developers

### File Structure

```
github-project/
├── SKILL.md              # Read this first - high-level guide
├── scripts/              # Operations (Claude runs these)
│   ├── setup.sh          # Common setup, sourced by all
│   ├── board-view.sh
│   ├── create-issue.sh
│   ├── status-transition.sh
│   ├── add-comment.sh
│   ├── assign.sh
│   ├── milestone-ops.sh
│   ├── close-reopen.sh
│   ├── pr-ops.sh
│   ├── query-issues.sh
│   └── subtask-ops.sh
└── references/           # Deep docs (Claude loads on demand)
    ├── status-lifecycle.md
    ├── error-handling.md
    ├── graphql-queries.md
    └── troubleshooting.md
```

### Quick Test

```bash
# From skill directory (team/coding-agent/skills/github-project/)

# Test setup
bash scripts/setup.sh

# Test board view
bash scripts/board-view.sh

# Test create issue (dry run - remove --body to skip)
bash scripts/create-issue.sh --help 2>&1 | head -5
```

### Adding a New Operation

1. Create `scripts/new-operation.sh`
2. Start with:
```bash
#!/bin/bash
# Source common setup
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "$SCRIPT_DIR/setup.sh"

# Your operation here
```
3. Add section to SKILL.md under "Operations"
4. Test independently

### Updating Existing Operation

1. Edit the specific script in `scripts/`
2. Test: `bash scripts/operation.sh --test-args`
3. Update SKILL.md if behavior changed
4. Done - no other files affected

## Prerequisites

- `gh` CLI installed (auth is auto-managed via `GH_CONFIG_DIR`)
- `team/` is a git repo with GitHub remote
- `.botminter.yml` exists in workspace root

## Troubleshooting

**Error: "Cannot access GitHub Projects"**
→ Verify the GitHub App has `organization_projects: admin` permission

**Error: "Status verification failed"**
→ Check App permissions, retry operation

**Other errors?**
→ See `references/troubleshooting.md`

## Key Features (v3.0.0)

✅ Comprehensive error handling
✅ GraphQL verification (prevents silent failures)
✅ Auto-recovery (missing project items)
✅ Progressive disclosure (efficient context usage)
✅ Modular scripts (easy to maintain)
✅ Examples and troubleshooting

## Learn More

- **SKILL.md** - Full operation guide
- **references/error-handling.md** - Error patterns
- **references/graphql-queries.md** - Verification details
- **references/status-lifecycle.md** - Epic, Story, and Bug lifecycle reference
