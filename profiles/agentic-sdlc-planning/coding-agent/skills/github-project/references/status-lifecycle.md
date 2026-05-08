# Status Lifecycle Reference

Status is tracked as a single-select field on the GitHub Project. Each value below is an option in the project's "Status" field.

## Status Convention

Statuses follow the format `<role-slug>:<persona>:<activity>`:

- **Role slug** — which member role owns the status: `eng` (engineer), `cos` (chief-of-staff), `snt` (sentinel), `human` (human gates)
- **Persona** — which hat context is active (e.g., `po`, `lead`, `dev`, `qe`)
- **Activity** — what is being done (e.g., `triage`, `plan`, `implement`)

Exception: `done` has no role owner.

## Issue Types (GitHub Native)

Classification uses GitHub's native issue types:

- **Epic** — top-level work item (epic)
- **Task** — child work item (story/subtask), linked as native sub-issue
- **Bug** — bug requiring investigation and fix

Stories are linked to epics as native sub-issues.

## Epic Lifecycle (8 statuses)

```
human:po:triage (human gate)
    |
human:po:backlog (human gate)
    |
eng:lead:plan
    |
human:po:plan-review (human gate)
    |
eng:lead:breakdown
    |
eng:lead:monitor
    |
human:po:accept (human gate)
    |
done
```

## Story Lifecycle (7 statuses)

```
eng:lead:plan
    |
human:po:plan-review (human gate)
    |
eng:dev:implement
    |
eng:qe:verify
    |
snt:gate:merge (sentinel runs merge gates)
    |
human:po:accept (human gate)
    |
done
```

## Bug Lifecycle (4 statuses)

Every confirmed bug creates a linked Story. The bug monitors the Story's progress.

```
human:po:triage (human gate)
    |
eng:qe:investigate
    |
eng:qe:monitor
    |
done
```

Simple bugs: linked Story gets `plan:auto` label (planning auto-approved).
Complex bugs: linked Story goes through full human-gated planning cycle.

## Human Gates

Human approval is required at these statuses (prefixed with `human:`):

1. **human:po:triage** — PO evaluates new epics and bugs
2. **human:po:backlog** — PO prioritizes and activates epics
3. **human:po:plan-review** — PO reviews and approves planning artifacts (epics and stories). Auto-advanced with `plan:auto` label.
4. **human:po:accept** — PO accepts completed work (epics and stories). Auto-advanced with `accept:auto` label.

All other transitions auto-advance without human-in-loop.

## Sentinel Merge Gate

The sentinel role handles PR merge gating at `snt:gate:merge`:

1. Reads merge-gate configuration from `team/projects/<project>/knowledge/merge-gate.md`
2. Runs project-specific tests (e2e, exploratory, coverage)
3. If all pass -> merges the PR, advances to `human:po:accept`
4. If any fail -> rejects, returns to `eng:dev:implement`

## Chief of Staff Lifecycle

```
cos:exec:todo
    |
cos:exec:in-progress
    |
cos:exec:done
```

The chief of staff picks up `cos:exec:todo` items and transitions them through execution to completion.

## Rejection Loops

| Gate | Reject target |
|------|---------------|
| `human:po:plan-review` | `eng:lead:plan` |
| `human:po:accept` (epic) | `eng:lead:monitor` |
| `human:po:accept` (story) | `eng:dev:implement` |
| `eng:qe:verify` | `eng:dev:implement` |
| `snt:gate:merge` (reject) | `eng:dev:implement` |
| `eng:qe:monitor` (fix failed) | `eng:qe:investigate` |
