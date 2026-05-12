# PR Standards

## Rule

All pull requests follow a standard format and review process.

## Team Repo PRs

PRs on the team repo are for team-level changes only:
- Process document updates
- Knowledge file additions or modifications
- Invariant changes

## Project Code PRs

Project code PRs follow the full PR lifecycle with sentinel merge gating.

### Branch Naming

```
feature/<type>-<issue-number>-<description>
```

Examples:
- `feature/feat-42-user-auth`
- `feature/fix-87-null-pointer-crash`
- `feature/refactor-103-extract-service`

### PR Title Format

```
[#<issue-number>] <description>
```

Examples:
- `[#42] Add user authentication flow`
- `[#87] Fix null pointer in login handler`

### PR Lifecycle

1. **PR created** during `eng:dev:implement` — implementation with TDD (red/green/refactor)
2. **Internal code review** during `eng:dev:implement` — `dev_implement-review` hat uses `gh pr review` (approve/request-changes)
3. **QE verification** at `eng:qe:verify` — validates against acceptance criteria
4. **Merge gating** at `snt:gate:merge` — sentinel runs project-specific tests, merges or rejects

### Merge Gate Configuration

Per-project merge gate configuration lives at:
```
team/projects/<project>/knowledge/merge-gate.md
```

This file defines:
- Test commands to run (e2e, exploratory, coverage)
- Pass/fail thresholds
- Required checks before merge

### Code Review

Code review uses `gh pr review` with approve or request-changes, not issue comments:

```bash
# Approve
gh pr review <PR-NUMBER> --approve --body "LGTM — <summary>"

# Request changes
gh pr review <PR-NUMBER> --request-changes --body "<feedback>"
```

## Description Template

Every PR body should include:

1. **Summary** — What this PR changes and why
2. **Related Issues** — Issue numbers this PR addresses (e.g., `Closes #42`)
3. **Changes** — Bulleted list of specific changes
4. **Testing** — How the changes were validated

## Review Expectations

- Code review occurs during `eng:dev:implement` via the `dev_implement-review` hat using `gh pr review`
- Reviewer uses the review format defined in `PROCESS.md`
- Review status must be `approved` before proceeding to QE verification

## Merge Criteria

- Code review approved via `gh pr review` (during `eng:dev:implement`)
- QE verification passed at `eng:qe:verify`
- Sentinel merge gate passed at `snt:gate:merge`
- PR labels are correct
- Related issues are updated with the PR reference
