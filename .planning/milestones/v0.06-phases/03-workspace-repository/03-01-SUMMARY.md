---
phase: 03-workspace-repository
plan: 01
subsystem: infra
tags: [workspace-repos, git-submodules, github-repos, sync, context-assembly]

requires:
  - phase: 01-coding-agent-agnostic
    provides: CodingAgentDef for parameterized workspace assembly
  - phase: 02-profile-externalization
    provides: Disk-based profile API for reading manifests
provides:
  - Workspace repo creation with GitHub repos and git submodules
  - Context assembly (AGENTS.md, ralph.yml, PROMPT.md surfaced at workspace root)
  - Workspace sync for existing repos
  - bm start/stop adapted for workspace repo model
  - Status commands with workspace branch and submodule health
affects: [05-team-manager-chat, 06-minty]

tech-stack:
  added: []
  patterns: [submodule-based-workspaces, context-surfacing, symlink-agent-dirs, workspace-marker]

key-files:
  created: []
  modified:
    - crates/bm/src/workspace.rs
    - crates/bm/src/commands/teams.rs
    - crates/bm/src/commands/start.rs
    - crates/bm/src/commands/status.rs
    - crates/bm/src/commands/members.rs

key-decisions:
  - "GitHub-hosted workspace repos with git submodules over nested clones — clean separation, no AGENTS.md confusion"
  - "Symlinks for agent dir assembly — three scopes (team, project, member) linked into team submodule"
  - ".botminter.workspace marker file for workspace detection — simple, no config dependency"
  - "copy_if_newer_verbose for sync — only updates changed files, idempotent"
  - "Member-named branches in submodules — avoids detached HEAD, clean git log"

patterns-established:
  - "WorkspaceRepoParams struct for all workspace operations — single parameter object"
  - "Three-scope agent dir assembly via symlinks into team submodule paths"
  - ".botminter.workspace marker with member name for workspace identity"
  - "Push mode (--push) creates GitHub repos; local mode uses local paths"

one-liner: "Workspace repos with GitHub hosting, git submodules, context assembly, and adapted start/stop/status commands"

requirements-completed: [WRKS-01, WRKS-02, WRKS-03, WRKS-04, WRKS-05, WRKS-06]

completed: 2026-03-04
---

# Phase 3: Workspace Repository Completion Summary

**Workspace repo creation with GitHub-hosted repos and git submodules, context assembly surfacing AGENTS.md/ralph.yml/PROMPT.md at workspace root, sync for existing repos, and adapted start/stop/status commands**

## Performance

- **Tasks:** 3 (steps 10-12 from original plan)
- **Files modified:** ~12

## Accomplishments
- Implemented `create_workspace_repo()` (~200 lines) — push mode creates GitHub repos via `gh repo create`, local mode uses `git init`
- Built submodule model: `team/` submodule → team repo, `projects/<name>/` submodules → project forks
- Implemented `assemble_workspace_repo_context()` — copies AGENTS.md, PROMPT.md, ralph.yml from team submodule to workspace root
- Built `assemble_agent_dir_submodule()` — three-scope symlink assembly (team-level, project-level, member-level) into team submodule paths
- Implemented `sync_workspace()` — updates submodules, re-copies context files if newer, re-assembles symlinks, commits and pushes
- Added `.botminter.workspace` marker file with `member: <name>` for workspace identity detection
- Adapted `bm start/stop` to launch Ralph in workspace repos using marker detection
- Enhanced `bm status` with workspace branch display and verbose submodule health (UpToDate/Behind/Modified/Uninitialized)
- Added `hide_tracked_bm_files()` using git skip-worktree to keep BM files out of git status noise
- Sprint 3 documentation updated

## Files Created/Modified
- `crates/bm/src/workspace.rs` — WorkspaceRepoParams, create_workspace_repo(), assemble_workspace_repo_context(), assemble_agent_dir_submodule(), sync_workspace(), workspace_submodule_status()
- `crates/bm/src/commands/teams.rs` — sync() with push flag, create-vs-sync detection via marker
- `crates/bm/src/commands/start.rs` — Adapted for workspace repo model
- `crates/bm/src/commands/status.rs` — Added branch column, verbose submodule status
- `crates/bm/src/commands/members.rs` — Workspace validation via marker

## Decisions Made
- GitHub-hosted repos over nested clones — avoids AGENTS.md confusion from parent repo, enables independent git operations
- Symlinks for agent dir — keeps single source of truth in team submodule, avoids file duplication
- Member-named branches in submodules — prevents detached HEAD state, gives clean commit attribution
- `.botminter.workspace` marker — simple file-based detection, no config coupling

## Deviations from Plan

### Additions beyond plan scope

**1. Local mode (`git init` without GitHub)**
- **Issue:** Specs only describe `--push` mode with GitHub-hosted repos. Implementation added a local-only fallback using `git init`
- **Rationale:** Enables development/testing without GitHub API calls
- **Impact:** Additional code path not in design; useful for local development

**2. `hide_tracked_bm_files()` with git skip-worktree**
- **Issue:** Added `git update-index --skip-worktree` to hide BotMinter files from git status. Not in design or plan
- **Rationale:** Reduces noise in workspace repos where AGENTS.md, ralph.yml are tracked but shouldn't clutter status
- **Impact:** Quality-of-life addition; no scope creep

## Next Phase Readiness
- Workspace model complete, ready for skills extraction
- All lifecycle commands (start/stop/status/sync) working with new model
- Submodule-based architecture provides clean path for interactive sessions (chat, minty)

---
*Phase: 03-workspace-repository*
*Completed: 2026-03-04*
