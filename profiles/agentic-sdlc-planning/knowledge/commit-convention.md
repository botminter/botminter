# Commit Convention

## Rule

All commits to the team repo and project repos follow the conventional commits format with an issue reference.

## Format

```
<type>(<scope>): <subject>

<body>

Ref: #<issue-number>
```

**Types:** `feat`, `fix`, `docs`, `refactor`, `test`, `chore`

**Scope:** Optional. The area of the codebase affected (e.g., `api`, `nodepool`, `upgrade`).

**Subject:** Imperative mood, lowercase, no period at end.

**Body:** Optional. Explains the "why" behind the change.

**Ref:** GitHub issue number. Required for all work-related commits.

## Examples

```
feat(nodepool): add reconciliation retry logic

Handles transient failures during NodePool scaling by retrying
with exponential backoff up to 3 attempts.

Ref: #42
```

```
docs: update upgrade-flow knowledge with v4.16 changes

Ref: #15
```

## One Logical Change Per Commit

Each commit should represent a single logical change. Do not mix unrelated changes in a single commit.

---
*Placeholder — to be populated with project-specific commit conventions before the team goes live.*
