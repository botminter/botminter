---
status: resolved
phase: 07-specs-foundation-bridge-contract
source: [07-01-SUMMARY.md, 07-02-SUMMARY.md, 07-03-SUMMARY.md]
started: 2026-03-08T12:20:00Z
updated: 2026-03-08T12:35:00Z
---

## Current Test

[testing complete]

## Tests

### 1. ADR Practice Files
expected: `.planning/adrs/` contains 5 files: `adr-template.md` (MADR 4.0.0 template), `README.md` (index listing 3 ADRs in a table), and 3 ADRs (`0001-adr-process.md`, `0002-bridge-abstraction.md`, `0003-ralph-robot-backend.md`). Each ADR has `status: accepted` in YAML front matter and uses MADR 4.0.0 sections (Context, Decision Drivers, Considered Options, Decision Outcome).
result: pass

### 2. Bridge Spec Readability
expected: `.planning/specs/bridge/bridge-spec.md` is a self-contained specification (300+ lines) that a developer could read without BotMinter source to understand how to build a bridge. It uses RFC 2119 keywords (MUST, SHOULD, MAY in uppercase), covers bridge.yml format, schema.json contract, lifecycle commands (start/stop/health), identity commands (onboard/rotate-credentials/remove), config exchange via `$BRIDGE_CONFIG_DIR`, and includes a Non-Goals section.
result: pass

### 3. Bridge Examples Valid
expected: `.planning/specs/bridge/examples/` contains `bridge.yml` (local type with lifecycle + identity), `bridge-external.yml` (external type with identity only, no lifecycle), and `schema.json` (valid JSON with `$schema` field). Local bridge has `spec.type: local` with lifecycle section; external has `spec.type: external` without lifecycle.
result: pass

### 4. Legacy specs/ Cleanup
expected: The `specs/` directory no longer contains `master-plan/`, `milestones/`, `prompts/`, `tasks/`, `presets/`, or `design-principles.md`. These legacy directories have been removed from the working tree (preserved in git history).
result: issue
reported: "specs/milestones/completed/architect-first-epic/.mermaid/design_1771601621_74.mmd"
severity: major

### 5. AGENTS.md References Updated
expected: `AGENTS.md` has no references to removed `specs/` paths (`specs/master-plan`, `specs/milestones`, `specs/prompts`, `specs/tasks`, `specs/presets`, `specs/design-principles`). New rows for `.planning/adrs/` and `.planning/specs/` appear in the Key Directories table.
result: pass

### 6. Conformance Tests Pass
expected: `cargo test -p bm conformance` runs 7 tests and all pass. Tests validate: stub bridge.yml required fields, lifecycle commands for local type, identity commands, schema.json structure, external bridge has no lifecycle, and stub directory has all required files.
result: pass

### 7. Full Test Suite No Regressions
expected: `cargo test -p bm` passes all tests (478+) with no failures. `cargo clippy -p bm -- -D warnings` produces no warnings.
result: pass

## Summary

total: 7
passed: 6
issues: 1
pending: 0
skipped: 0

## Gaps

- truth: "Legacy specs/ directories fully removed from working tree"
  status: resolved
  reason: "User reported: specs/milestones/completed/architect-first-epic/.mermaid/design_1771601621_74.mmd"
  severity: major
  test: 4
  root_cause: "git rm only removes tracked files; 167 untracked .mermaid generated artifacts (SVGs, .mmd, cache) in specs/milestones/completed/architect-first-epic/.mermaid/ were invisible to git rm"
  artifacts:
    - path: "specs/milestones/completed/architect-first-epic/.mermaid/"
      issue: "167 untracked generated Mermaid diagram files remain on disk"
  missing:
    - "rm -rf specs/ to remove entire directory tree including untracked files"
  debug_session: ""
