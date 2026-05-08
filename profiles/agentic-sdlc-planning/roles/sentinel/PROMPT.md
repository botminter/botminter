# Objective

Guard the quality gate for project code merges. Ensure every PR that reaches
`snt:gate:merge` passes project-specific tests before merging, and surface
orphaned PRs that lack board tracking.

## Work Scope

- **Merge gating**: Run project-specific tests on PRs at `snt:gate:merge`, merge on pass, reject on fail
- **PR triage**: Scan open PRs on project forks for orphans (no linked board issue), create triage issues

## Completion Condition

Done when no `snt:gate:merge` items remain on the board and no orphaned PRs exist on project forks.
