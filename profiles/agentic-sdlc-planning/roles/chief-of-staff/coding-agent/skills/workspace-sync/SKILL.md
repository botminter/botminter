---
name: workspace-sync
description: >-
  Sync and diagnose BotMinter workspaces. Use when the operator says
  "sync workspaces", "apply profile changes", "propagate changes",
  "update all workspaces", "check workspace health", "diff against profile",
  "full sync", "my workspace has old files", or "why is the workspace
  out of date". Do NOT use for member behavior/config tuning (use
  member-tuning) or structured process changes (use process-evolution).
metadata:
  author: botminter
  version: 1.1.0
---

# Workspace Sync

Manages synchronization across BotMinter's three-layer model:
**profile** (template) → **team repo** (instance) → **member workspaces** (runtime).

## Three-Layer Model

```
Profile (template)              Team Repo (instance)           Workspace (runtime)
projects/botminter/             team/                          <workspace>/
  profiles/<name>/
    context.md            →       CLAUDE.md (renamed)      →     CLAUDE.md (copied)
    roles/<role>/         →       members/<member>/         →     PROMPT.md (copied)
    coding-agent/skills/  →       coding-agent/skills/           ralph.yml (copied)
    brain/system-prompt.md →      brain/system-prompt.md    →    brain-prompt.md (rendered)
    workflows/*.dot       →       workflows/*.dot                .claude/agents/ (symlinked)
                                                                 .claude/settings.json (copied)
                                                                 .botminter.workspace (marker)
```

**Profile → Team**: No `bm` command exists. The agent performs this manually using
the transformation rules below.

**Team → Workspace**: `bm teams sync --all` handles this (copies files, rebuilds
symlinks, renders brain prompt, injects RObot config).

**Skills**: Read directly from `team/` submodule via `skills.dirs` in ralph.yml —
no copy needed. Pulling the submodule makes new skills immediately available.

## Transformation Rules

When copying files from profile to team repo, apply these transformations:

### 1. Agent Tag Filtering
Files with extensions `.md`, `.yml`, `.yaml`, `.sh` contain conditional blocks:

```markdown
<!-- +agent:claude-code -->
Claude-specific content
<!-- -agent -->
```

Or in YAML/shell:
```yaml
# +agent:claude-code
backend: claude
# -agent
```

**Rule**: Include content inside `+agent:claude-code` blocks. Strip all tag lines.
Exclude content inside blocks for other agents.

### 2. context.md → CLAUDE.md Rename
Profile uses `context.md`. Team repo uses `CLAUDE.md`. When copying any file
named `context.md` from the profile, rename it to `CLAUDE.md`.

### 3. Member Placeholders
Profile role files contain `{{member_dir}}`, `{{role}}`, `{{member_name}}`.
These are rendered at `bm hire` time with actual values. In existing team repo
files, they are already rendered. When copying NEW files from profile to an
existing member directory, render these placeholders:
- `{{member_dir}}` → the member directory name (e.g., `engineer-bob`)
- `{{role}}` → the role name (e.g., `engineer`)
- `{{member_name}}` → the member's display name (e.g., `bob`)

Read these values from `team/members/<member>/botminter.yml`.

### 4. `<project>` Expansion
Profile files use `<project>` as a placeholder in paths like:
- `team/projects/<project>/knowledge/` (in hat instructions, guardrails)
- `team/projects/<project>/coding-agent/skills` (in skills.dirs)
- `team/projects/<project>/invariants/` (in guardrails)

**Rule**: In actionable contexts (hat instructions, guardrails, skills.dirs),
expand `<project>` to one entry per registered project. Read the project list
from `team/botminter.yml` under the `projects:` key.

Example: if projects are `botminter` and `hypershift`, then:
```yaml
# Profile has:
- team/projects/<project>/coding-agent/skills
# Team repo should have:
- team/projects/botminter/coding-agent/skills
- team/projects/hypershift/coding-agent/skills
```

In documentation-only contexts (workspace layout diagrams, explanatory text),
leave `<project>` as-is.

### 5. Directory Exclusion
When applying team-level profile files, skip `roles/` and `.schema/` directories.
Role content goes to `team/members/<member>/`, not `team/roles/`.

### 6. Manifest Finalization (hire-time only)
`.botminter.yml` → `botminter.yml` with `name` field added. This only happens
at `bm hire` time. When updating existing members, skip this — the manifest
already exists with the correct name.

### 7. Brain Prompt Rendering (sync-time only)
`team/brain/system-prompt.md` is rendered with these variables and written to
`brain-prompt.md` at the workspace root:
- `{{member_name}}` — member's display name
- `{{team_name}}` — team name
- `{{role}}` — role name
- `{{gh_org}}` — GitHub org
- `{{gh_repo}}` — GitHub repo name

This is handled automatically by `bm teams sync`. The skill does not need to
perform this rendering — but should know it exists when diagnosing why a
member's brain prompt has stale content (re-run sync to re-render).

## Operations

### 1. `apply-profile` — Profile → Team Repo

**Trigger**: "apply profile changes", "sync from profile", "port profile changes"

1. Read profile name from `team/botminter.yml`
2. Locate profile source at `projects/botminter/profiles/<name>/`
3. **Team-level**: Diff profile source (excluding `roles/`, `.schema/`) against `team/`:
   - Apply agent tag filtering (Rule 1) to `.md`/`.yml`/`.yaml`/`.sh` files
   - Rename `context.md` → `CLAUDE.md` (Rule 2)
   - Compare: `coding-agent/`, `knowledge/`, `invariants/`, `PROCESS.md`,
     `context.md`→`CLAUDE.md`, `workflows/`, `botminter.yml`
4. **Member-level**: For each hired member in `team/members/`:
   - Read role from `team/members/<member>/botminter.yml`
   - Diff `profiles/<name>/roles/<role>/` against `team/members/<member>/`
   - Apply Rules 1, 2, 3 (render placeholders for new files only)
   - Expand `<project>` paths (Rule 4) in ralph.yml and hat instructions
   - Skip manifest finalization (Rule 6)
5. Show diff to operator, ask which changes to apply
6. If operator approves all or a subset, apply those changes
7. Commit to team repo with `feat(team): apply profile changes`

**Requires**: BotMinter project at `projects/botminter/`

**Error handling**:
- If `team/botminter.yml` is missing or has no profile name, report and abort
- If `projects/botminter/profiles/<name>/` doesn't exist, check if the BotMinter
  project is added as a submodule (`bm projects list`)
- If a member's `botminter.yml` has no `role` field, skip that member and warn

### 2. `surface` — Team Repo → Workspaces

**Trigger**: "surface changes", "sync to workspaces", "propagate to members",
"update all workspaces"

1. Ensure central team repo is up to date:
   ```bash
   git -C ~/.botminter/workspaces/<team>/team pull origin main
   ```
2. Run `bm teams sync --all`
3. For each member workspace, verify key files were updated:
   ```bash
   diff <workspace>/CLAUDE.md team/members/<member>/CLAUDE.md
   ```
4. Report any failures and diagnose

**Error handling**:
- If central team repo is behind remote, pull it before running sync
- If `bm teams sync` fails on a workspace, check that workspace's `team/`
  submodule branch and git state. Common fixes: checkout main, pull, resolve
  merge conflicts
- If a workspace is on a feature branch with no upstream, switch to main first

### 3. `full-sync` — Profile → Team → Workspaces

**Trigger**: "full sync", "sync everything", "apply and surface"

Runs `apply-profile` then `surface` in sequence. If `apply-profile` produces
no changes, skip `surface`.

### 4. `check` — Diagnose Workspace Health

**Trigger**: "check workspace", "workspace health", "diagnose workspace",
"why is the workspace out of date"

Run these checks and report OK/WARN/FAIL:

1. **Team submodule**: `git -C team status`, check branch and ahead/behind
2. **Required files**: CLAUDE.md, PROMPT.md, ralph.yml exist at workspace root
3. **File freshness**: Compare workspace root files against `team/members/<member>/` source
4. **Symlinks**: Verify `.claude/agents/` symlinks resolve
5. **Git state**: Uncommitted changes, stale lock files
6. **Cross-workspace drift**: Compare team submodule HEADs across workspaces

### 5. `diff` — Preview Changes

**Trigger**: "what would sync change", "diff against profile", "preview sync"

Same comparison as `apply-profile` steps 3-4, but read-only. Shows which files
differ between profile source and team repo without applying anything.

### 6. `fix-member` — Fix a Specific Member's Workspace

**Trigger**: "fix `<member>`'s workspace", "sync `<member>`'s workspace",
"resync `<member>`"

1. Identify workspace at `~/.botminter/workspaces/<team>/<member>/`
2. Fix the workspace's `team/` submodule:
   ```bash
   git -C <workspace>/team checkout main
   git -C <workspace>/team pull
   ```
3. Run `bm teams sync` (without `--all` — it syncs all workspaces, which is fine,
   but the focus is verifying this specific member)
4. Verify the member's workspace has updated files

**Error handling**:
- If the workspace directory doesn't exist, check `bm members list` to verify
  the member is hired, then run `bm teams sync` to create it
- If `team/` submodule has merge conflicts, inspect the conflicting files and
  resolve. Do not force-reset without operator confirmation.
- If the submodule is on a feature branch with unpushed work, warn the operator
  before switching to main

## Examples

### Example 1: Porting a profile rename

**Operator says**: "I renamed team-manager to chief-of-staff in the profile, apply it"

**Actions**:
1. Run `diff` to see all files that differ between profile and team repo
2. The diff shows ~20 files with `team-manager`→`chief-of-staff` and `mgr:`→`cos:` changes
3. Apply changes, handling transformations:
   - Profile `context.md` → team `CLAUDE.md` (Rule 2)
   - Filter agent tags in ralph.yml (Rule 1)
   - Render `{{member_dir}}` in any new files (Rule 3)
   - Expand `<project>` in skills.dirs and guardrails (Rule 4)
4. Commit: `feat(team): apply chief-of-staff rename from profile`
5. Run `surface` to propagate to all workspaces

**Result**: All workspaces have updated files with the new role name.

### Example 2: Checking why a member has stale files

**Operator says**: "The engineer still has the old CLAUDE.md"

**Actions**:
1. Run `check` on the member's workspace
2. Find: team submodule is on `feature/bug-55` branch, behind main by 4 commits
3. Run `fix-member`: checkout main, pull, re-sync
4. Verify the workspace now has the updated file

**Result**: Member's workspace is on main with current files.

### Example 3: Full pipeline after adding a new skill to the profile

**Operator says**: "I added a new skill to the profile, get it everywhere"

**Actions**:
1. Run `full-sync`
2. `apply-profile` copies the new skill from profile to `team/coding-agent/skills/`
3. Commit to team repo
4. `surface` runs `bm teams sync --all`
5. Members pick up the skill via `skills.dirs` (reads directly from `team/` submodule)

**Result**: All members can load the new skill immediately.
