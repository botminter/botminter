# Status Lifecycle Reference

Status is tracked as a single-select field on the GitHub Project, not as labels. Each value below is an option in the project's "Status" field.

## Kind Labels (GitHub Labels)

Classification labels applied directly to issues:

- `kind/epic` — top-level work item
- `kind/story` — child work item under an epic

## Parent Label (GitHub Labels)

- `parent/<number>` — links a story to its parent epic

## Epic Lifecycle

```
po:triage
    ↓
po:backlog
    ↓
arch:design
    ↓
po:design-review (human gate)
    ↓
arch:plan
    ↓
po:plan-review (human gate)
    ↓
arch:breakdown
    ↓
po:ready
    ↓
arch:in-progress
    ↓
po:accept (human gate)
    ↓
done
```

### Status Descriptions

- **po:triage** — newly created, awaiting PO triage
- **po:backlog** — triaged, in backlog
- **arch:design** — architect designing solution
- **po:design-review** — PO reviewing design (human gate)
- **arch:plan** — architect planning implementation
- **po:plan-review** — PO reviewing plan (human gate)
- **arch:breakdown** — architect breaking epic into stories
- **po:ready** — stories ready for development
- **arch:in-progress** — development in progress
- **po:accept** — PO accepting completed work (human gate)
- **done** — completed
- **error** — blocked or errored

## Story Lifecycle

```
dev:ready
    ↓
dev:in-progress
    ↓
dev:review
    ↓
qe:testing
    ↓
done
```

### Status Descriptions

- **dev:ready** — ready for development
- **dev:in-progress** — being implemented
- **dev:review** — code review
- **qe:testing** — QE verification
- **done** — completed

## Human Gates

In supervised mode, human approval is required at three epic gates:

1. **po:design-review** — PO reviews and approves design
2. **po:plan-review** — PO reviews and approves implementation plan
3. **po:accept** — PO accepts completed work

All other transitions auto-advance without human-in-loop.

## Rejection Loops

If a human gate rejects work, the issue returns to the previous status:

- **po:design-review** → reject → **arch:design**
- **po:plan-review** → reject → **arch:plan**
- **po:accept** → reject → **arch:in-progress** (or appropriate story status)

The board scanner detects rejection comments and routes back to the appropriate hat.
