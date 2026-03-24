---
phase: 01-coding-agent-agnostic
plan: 01
subsystem: infra
tags: [agent-tags, coding-agent-def, profile-restructuring, unified-files, agent-specific-extraction]

provides:
  - Agent tag filter library (agent_tags.rs) with HTML and Hash comment syntax
  - CodingAgentDef data model with profile/team override resolution
  - Profile restructuring — coding-agent/ directory, context.md as unified files with inline agent tags
  - Unified-to-agent-specific extraction — tag filtering + context.md rename during profile/member extraction
  - Workspace parameterization — config-driven file/dir names from resolved CodingAgentDef
affects: [02-profile-externalization, 03-workspace-repository, 05-team-manager-chat]

tech-stack:
  added: []
  patterns: [inline-agent-tags, config-driven-agent-resolution, filtered-extraction]

key-files:
  created:
    - crates/bm/src/agent_tags.rs
  modified:
    - crates/bm/src/profile.rs
    - crates/bm/src/workspace.rs
    - crates/bm/src/cli.rs
    - profiles/scrum/botminter.yml
    - profiles/scrum-compact/botminter.yml
    - profiles/scrum-compact-telegram/botminter.yml

key-decisions:
  - "Inline agent tags (+agent:NAME/-agent) over separate file variants — keeps files unified, avoids sync drift"
  - "Two comment syntaxes (HTML for .md, Hash for .yml/.sh) — matches native comment conventions"
  - "Unified-to-agent-specific at extraction time, not runtime — team repo always contains clean agent-specific output"
  - "context.md renamed to CodingAgentDef.context_file during extraction — unified source, agent-specific output"

patterns-established:
  - "Inline agent tags: +agent:NAME/-agent for file-section-level coding agent specificity"
  - "Config-driven agent resolution: profile default_coding_agent with team-level override"
  - "Unified-to-agent-specific extraction: text files (.md/.yml/.yaml/.sh) filtered, binaries copied verbatim"

one-liner: "Agent tag filter library, CodingAgentDef config model, unified files with inline +agent:NAME/-agent tags, and extraction transforming unified files to agent-specific output"

requirements-completed: [CAA-01, CAA-02, CAA-03, CAA-04, CAA-05, CAA-06]

completed: 2026-03-04
---

# Phase 1: Coding-Agent-Agnostic Completion Summary

**Agent tag filter library, CodingAgentDef config model, profile restructuring to unified files with inline +agent:NAME/-agent tags, and extraction transforming unified representation to agent-specific output (tag filtering + context.md rename)**

## Performance

- **Tasks:** 6 (steps 1-6 from original plan)
- **Files modified:** ~25 (across profiles, crates/bm/src/, docs/)

## Accomplishments
- Built `agent_tags.rs` (670 lines) — line-based filter with `CommentSyntax::Html` and `CommentSyntax::Hash`, balanced tag validation, agent name collection
- Added `CodingAgentDef` struct to `profile.rs` with `name`, `display_name`, `context_file`, `agent_dir`, `binary` fields
- Updated `ProfileManifest` with `coding_agents: HashMap` and `default_coding_agent` fields
- Implemented `resolve_coding_agent()` for profile-default + team-override resolution
- Restructured all 3 profiles: `agent/` → `coding-agent/`, `AGENTS.md` → `context.md` with inline agent tags
- Wired unified-to-agent-specific transformation into `extract_dir_recursive_from_disk()` (profile.rs:440-498) — filters agent tags from `.md/.yml/.yaml/.sh` files, renames `context.md` → `AGENTS.md`
- Parameterized workspace.rs — all hardcoded "claude-code" strings replaced with resolved `CodingAgentDef` config values
- Added `bm profiles describe --show-tags` via `scan_agent_tags()` function
- Sprint 1 documentation updated

## Files Created/Modified
- `crates/bm/src/agent_tags.rs` — Core tag filter library (detect_comment_syntax, filter_file, filter_agent_tags, collect_agent_names, tags_are_balanced)
- `crates/bm/src/profile.rs` — CodingAgentDef struct, resolve_coding_agent(), extract_dir_recursive_from_disk() with filtering, scan_agent_tags()
- `crates/bm/src/workspace.rs` — Parameterized with CodingAgentDef instead of hardcoded strings
- `profiles/scrum/botminter.yml` — Added coding_agents section with claude-code entry
- `profiles/scrum/context.md` — Renamed from AGENTS.md, added inline agent tags
- `profiles/scrum/coding-agent/` — Renamed from agent/, restructured skills directory
- `profiles/scrum-compact/` — Same restructuring
- `profiles/scrum-compact-telegram/` — Same restructuring

## Decisions Made
- Used inline agent tags over separate file variants — avoids maintaining parallel files that drift
- Two comment syntaxes matching native conventions — HTML for markdown, Hash for YAML/shell
- Unified-to-agent-specific transformation at extraction time rather than runtime — team repos always contain clean agent-specific output
- `coding-agent/` directory name is a BotMinter convention (not agent-specific), stays as-is in team repos

## Deviations from Plan
None — plan steps 1-6 executed as specified. Agent tag filtering correctly placed at extraction time (profile.rs), not workspace sync (workspace.rs).

## Next Phase Readiness
- Agent tag infrastructure complete, ready for profile externalization
- All profiles restructured with coding-agent/ directories and context.md files
- CodingAgentDef resolution chain working end-to-end

---
*Phase: 01-coding-agent-agnostic*
*Completed: 2026-03-04*
