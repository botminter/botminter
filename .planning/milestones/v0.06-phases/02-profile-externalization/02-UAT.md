---
status: complete
phase: 02-profile-externalization
source: 02-01-SUMMARY.md
started: 2026-03-07T00:00:00Z
updated: 2026-03-07T00:00:00Z
---

## Current Test

[testing complete]

## Tests

### 1. Build & Tests Pass
expected: Run `just build` and `just test` from the repo root. Both complete without errors. No compilation warnings related to profiles_init, ensure_profiles_initialized, or disk-based profile API.
result: pass

### 2. Profiles Init Extracts to Disk
expected: Run `bm profiles init`. Profiles are extracted to `~/.config/botminter/profiles/`. The directory contains at least the `scrum` and `scrum-compact` profile directories with their full contents (botminter.yml, PROCESS.md, knowledge/, etc.) — verbatim copies of the embedded source, including raw agent tags (not filtered).
result: pass

### 3. Profiles Init Force Flag
expected: Run `bm profiles init --force`. Existing profiles on disk are overwritten without any interactive prompting. Command completes successfully with output indicating profiles were extracted.
result: pass
agent-notes: Without --force, command prompted per profile and defaulted to N (non-TTY). With --force, completed immediately with "(overwritten)" per profile and "0 new, 3 overwritten, 0 skipped" summary.

### 4. Auto-Prompt on First Use
expected: Delete `~/.config/botminter/profiles/` directory. Then run `bm profiles list`. Instead of failing with "profiles not found", the command detects missing profiles and either prompts to initialize (if TTY) or auto-initializes (if non-TTY), then proceeds to list available profiles.
result: pass
agent-notes: After deleting profiles dir, `bm profiles list` auto-initialized with "Initialized 3 profiles in ~/.config/botminter/profiles" then showed profiles table. Seamless first-run experience.

### 5. Disk-Based Profile Commands
expected: After profiles are initialized on disk, run `bm profiles list` and `bm profiles describe scrum`. Both commands read from `~/.config/botminter/profiles/` (not embedded data). Output shows available profiles with their names and descriptions.
result: pass
agent-notes: `profiles list` showed well-formatted table with Profile, Version, Schema, Description columns. `profiles describe scrum` showed name, version, roles, labels, coding agents. Clear and informative.

### 6. Verbatim vs Filtered Extraction
expected: Compare the disk profile at `~/.config/botminter/profiles/scrum/` with what `bm init` generates in a team repo. Disk profiles contain raw agent tags (+agent:claude-code / -agent markers visible in source files like context.md). Team repo extraction filters these tags and renames context.md to AGENTS.md — producing agent-specific output.
result: pass
agent-notes: Extracted files contain raw `<!-- +agent:claude-code -->` and `<!-- -agent -->` tags plus `# +agent:claude-code` in YAML. context.md shows full template source with agent-conditional blocks unprocessed.

## Summary

total: 6
passed: 6
issues: 0
pending: 0
skipped: 0

## Gaps

[none yet]
