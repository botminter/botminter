---
phase: 01-coding-agent-agnostic
plan: 04
subsystem: profiles
tags: [staleness-detection, version-comparison, interactive-prompt, semver]

requires:
  - phase: 01-coding-agent-agnostic
    provides: "Profile extraction, PROFILES_FORMAT_VERSION marker, ensure_profiles_initialized"
provides:
  - "Version-based profile staleness detection using botminter.yml version field"
  - "Interactive upgrade/downgrade prompting with force override"
  - "No marker file artifacts"
affects: [profile-management, init-workflow]

tech-stack:
  added: []
  patterns: [version-field-comparison, interactive-confirmation-with-force-override]

key-files:
  created: []
  modified:
    - crates/bm/src/profile.rs
    - crates/bm/tests/integration.rs
    - docs/content/reference/cli.md
    - docs/content/concepts/profiles.md
    - docs/content/how-to/generate-team-repo.md
    - AGENTS.md

key-decisions:
  - "Used inline semver comparison (major.minor.patch numeric) instead of adding semver crate dependency"
  - "Default prompt answer is N (conservative -- don't overwrite without explicit consent)"
  - "Non-TTY mode auto re-extracts (same as force) since there is no one to prompt"

patterns-established:
  - "Version comparison via embedded botminter.yml vs on-disk botminter.yml"
  - "Interactive confirmation with force bypass for destructive operations"

requirements-completed: [CAA-03]

duration: 6min
completed: 2026-03-05
---

# Phase 01 Plan 04: Replace Marker-Based Staleness Summary

**Version-based profile staleness detection comparing embedded vs on-disk botminter.yml version fields with interactive upgrade/downgrade prompting and force override**

## Performance

- **Duration:** 6 min
- **Started:** 2026-03-05T05:54:40Z
- **Completed:** 2026-03-05T06:00:46Z
- **Tasks:** 3
- **Files modified:** 6

## Accomplishments
- Replaced redundant .profiles_version marker file with version-field comparison using existing botminter.yml version
- Added interactive prompting with upgrade/downgrade context when version mismatch detected in TTY mode
- Added force parameter and non-TTY auto re-extract behavior
- Updated all documentation for Phase 01 changes (non-interactive mode, version detection, --show-tags rename)

## Task Commits

Each task was committed atomically:

1. **Task 1: Replace marker-based staleness with version-field comparison and user prompting** - `3b87bf5` (feat)
2. **Task 2: Verify no marker file references remain across codebase** - `5cf6897` (fix)
3. **Task 3: Update documentation for all Phase 01 changes** - `4c57036` (docs)

## Files Created/Modified
- `crates/bm/src/profile.rs` - Removed marker constants, added compare_versions helper, embedded_profile_version helper, rewrote staleness check with version comparison and interactive prompting, added force parameter, rewrote 3 staleness tests + added 3 new tests
- `crates/bm/tests/integration.rs` - Replaced .profiles_version marker assertion with botminter.yml existence check
- `docs/content/reference/cli.md` - Added non-interactive mode section, updated --show-tags description
- `docs/content/concepts/profiles.md` - Added profile version detection section
- `docs/content/how-to/generate-team-repo.md` - Added non-interactive mode section
- `AGENTS.md` - Added --non-interactive to command listing

## Decisions Made
- Used inline semver comparison (parse major.minor.patch as u64 tuples) instead of adding the semver crate -- avoids a new dependency for a simple comparison
- Default prompt answer is N (conservative) -- on-disk profiles are not overwritten without explicit consent
- Non-TTY mode auto re-extracts silently since there is no user to prompt

## Deviations from Plan

None - plan executed exactly as written.

## Issues Encountered
None

## User Setup Required
None - no external service configuration required.

## Next Phase Readiness
- Profile staleness detection is clean and version-based
- All Phase 01 documentation is updated
- Plan 01-05 (real E2E init test) is the remaining gap closure item

---
*Phase: 01-coding-agent-agnostic*
*Completed: 2026-03-05*
