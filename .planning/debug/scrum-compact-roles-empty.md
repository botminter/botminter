---
status: diagnosed
trigger: "bm init with scrum-compact profile fails — Role 'superman' not available, available roles: empty"
created: 2026-03-05T00:00:00Z
updated: 2026-03-05T03:30:00Z
---

## Current Focus

hypothesis: CONFIRMED — stale disk profiles from pre-6499c62 binary have members/ not roles/
test: Simulated stale profiles by renaming/emptying roles/ dir on disk
expecting: Empty roles list or "no roles/ directory" error
next_action: Report diagnosis

## Symptoms

expected: bm init with scrum-compact profile lists "superman" as available role
actual: Error says role 'superman' not available, available roles list is empty
errors: "Error: Role 'superman' not available in profile 'scrum-compact'. Available roles: (empty list)"
reproduction: bm init with profile scrum-compact when disk profiles are stale
started: After Phase 01 restructuring (members/ -> roles/ rename in commit 6499c62)

## Eliminated

- hypothesis: Bug in list_roles_from logic
  evidence: Test all_profiles_list_roles_matches_manifest passes; function correctly scans roles/ subdirectory
  timestamp: 2026-03-05T03:10:00Z

- hypothesis: Bug in embedded profile structure
  evidence: profiles/scrum-compact/roles/superman/ exists on disk in source tree with all required files
  timestamp: 2026-03-05T03:12:00Z

- hypothesis: include_dir fails to embed .gitkeep-only directories
  evidence: Roles dir has real files (PROMPT.md, ralph.yml, context.md etc), and extraction test passes
  timestamp: 2026-03-05T03:15:00Z

- hypothesis: Bug in extract_dir_recursive_from_disk or context.md rename
  evidence: Code correctly renames context.md -> AGENTS.md and handles coding-agent/ dir; no path confusion
  timestamp: 2026-03-05T03:20:00Z

## Evidence

- timestamp: 2026-03-05T03:05:00Z
  checked: profiles/scrum-compact/ directory structure
  found: roles/superman/ exists with full content (.botminter.yml, context.md, PROMPT.md, ralph.yml, hats/, etc.)
  implication: Profile source is correct

- timestamp: 2026-03-05T03:07:00Z
  checked: profiles/scrum-compact/botminter.yml
  found: roles array lists superman and team-manager with descriptions; coding_agents map has claude-code
  implication: Manifest is correct

- timestamp: 2026-03-05T03:10:00Z
  checked: profile.rs list_roles_from (line 327-339)
  found: Scans base.join(name).join("roles") for subdirectories
  implication: Code expects "roles/" directory name

- timestamp: 2026-03-05T03:12:00Z
  checked: git history — commit 6499c62
  found: Renamed profiles/*/members/ -> profiles/*/roles/ AND updated code from members_dir to roles_dir
  implication: Directory rename was part of Phase 01 restructuring

- timestamp: 2026-03-05T03:14:00Z
  checked: ensure_profiles_initialized_with (line 517-559)
  found: Returns Ok(()) if profiles_path has ANY subdirectory — NO version/staleness check
  implication: Stale profiles from older binary are never auto-updated

- timestamp: 2026-03-05T03:16:00Z
  checked: Pre-6499c62 code (git show 6499c62^:crates/bm/src/profile.rs)
  found: Code used "members/" directory name; profile had members/superman/ not roles/superman/
  implication: Binary built from pre-6499c62 would extract profiles with members/ dir

- timestamp: 2026-03-05T03:20:00Z
  checked: Simulated stale profiles (renamed roles/ to members/ on disk)
  found: "Error: Profile 'scrum-compact' has no roles/ directory" — matches hypothesis
  implication: Stale disk profiles cause the error

- timestamp: 2026-03-05T03:22:00Z
  checked: Simulated empty roles/ directory
  found: list_roles returns empty Vec; "Available Roles:" shows nothing — matches user's "empty list" report
  implication: If roles/ exists but is empty (partial update), exact user error is reproduced

- timestamp: 2026-03-05T03:25:00Z
  checked: profiles_init command (profiles_init.rs)
  found: Per-profile skip/overwrite prompt; skipping preserves stale content; force flag overwrites
  implication: User who ran bm profiles init and skipped would keep stale profiles

## Resolution

root_cause: Stale disk profiles at ~/.config/botminter/profiles/. The profiles/*/members/ directory was renamed to profiles/*/roles/ in commit 6499c62, but ensure_profiles_initialized() has no version/staleness detection — it returns early if ANY profile subdirectory exists. Users who extracted profiles from a pre-6499c62 binary still have the old members/ layout on disk, while the new code looks for roles/.
fix:
verification:
files_changed: []
