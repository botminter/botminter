---
name: board-scanner
description: >-
  Board scanning and dispatch procedure for GitHub Projects v2.
  Scans the project board for actionable issues, handles auto-advance
  transitions, and dispatches work to specialized hats via priority tables.
  Auto-injected into coordinator prompts.
metadata:
  author: botminter
  version: 1.0.0
---

# Board Scanner

This skill defines your PLAN step when coordinating. Scan the GitHub
Projects v2 board, handle auto-advance transitions, then DELEGATE by
publishing exactly one event to the appropriate hat.

## Scan Procedure

### 1. Scratchpad

Append a new scan section to the scratchpad with the current timestamp.
Delete `tasks.jsonl` if it exists to prevent state bleed from previous
hat activations.

### 2. Sync workspace

```bash
git -C team pull --ff-only 2>/dev/null || true
```

### 3. Fetch the board

Load the `github-project` skill and use its **board-view** operation to fetch
all project items with their Status field values. The board-view operation
handles repo detection, project ID caching, and item retrieval internally.

Use the results to identify each item's issue number and current status
for dispatch.

### 4. Log to poll-log.txt

Use `$(date -u +%Y-%m-%dT%H:%M:%SZ)` for all timestamps.

```
2026-03-02T10:15:00Z — board.scan — START
2026-03-02T10:15:01Z — board.scan — 3 issues found
2026-03-02T10:15:01Z — board.scan — END
```

### 5. Auto-advance

Before dispatching, handle auto-advance statuses using the `github-project`
skill's operations:

Transitions:

- `eng:arch:sign-off` → set status to `snt:gate:merge`, comment, log. (Sentinel takes over merge gating.)

Comment format for auto-advance:

```
### 🦸 engineer — $(date -u +%Y-%m-%dT%H:%M:%SZ)

Auto-advance: eng:arch:sign-off → snt:gate:merge
```

After auto-advancing all eligible issues, continue to dispatch with the
updated board state.

### 6. Dispatch

Dispatch based on the highest-priority project status found. Process one
item at a time. Match each item's status against the tables below in order:
Epic → Story → Bug → Content. The first match wins.

The tables are organized by workflow phase, not by issue type. The scanner
does NOT need to query the issue type — it dispatches purely by status.
Hats that handle shared statuses (e.g., `human:po:plan-review`, `eng:qe:verify`)
are responsible for querying the issue type themselves.

Skip items with `snt:` prefix — those belong to the sentinel's scanner.

**Epic priority (highest first):**

| # | Status | Event |
|---|--------|-------|
| 1 | `eng:po:triage` | `po.backlog` |
| 2 | `human:po:design-review` | `po.review` |
| 3 | `human:po:plan-review` | `po.review` |
| 4 | `human:po:accept` | `po.review` |
| 5 | `eng:lead:design-review` | `lead.review` |
| 6 | `eng:lead:plan-review` | `lead.review` |
| 7 | `eng:lead:breakdown-review` | `lead.review` |
| 8 | `eng:arch:breakdown` | `arch.breakdown` |
| 9 | `eng:arch:plan` | `arch.plan` |
| 10 | `eng:arch:design` | `arch.design` |
| 11 | `eng:po:backlog` | `po.backlog` |
| 12 | `eng:po:ready` | `po.backlog` |
| 13 | `eng:arch:in-progress` | `arch.in_progress` |

**Story priority (highest first — closer to finish line wins):**

| # | Status | Event |
|---|--------|-------|
| 1 | `eng:qe:verify` | `qe.verify` |
| 2 | `eng:dev:code-review` | `dev.code_review` |
| 3 | `eng:dev:implement` | `dev.implement` |
| 4 | `eng:qe:test-design` | `qe.test_design` |
| 5 | `eng:sre:infra-setup` | `sre.setup` |

**Bug priority (highest first — closer to finish line wins):**

| # | Status | Event |
|---|--------|-------|
| 1 | `eng:qe:verify` | `qe.verify` |
| 2 | `eng:bug:in-progress` | `bug.in_progress` |
| 3 | `eng:arch:review` | `arch.review` |
| 4 | `eng:bug:investigate` | `qe.investigate` |
| 5 | `eng:arch:refine` | `arch.refine` |
| 6 | `eng:bug:breakdown` | `arch.breakdown` |

Note: `human:po:plan-review` and `eng:qe:verify` are shared with Epic/Story tables. Bugs at these statuses are dispatched to the same hats, which query the issue type to determine the correct path.

**Content priority (highest first — closer to finish line wins):**

| # | Status | Event |
|---|--------|-------|
| 1 | `eng:cw:review` | `cw.review` |
| 2 | `eng:cw:write` | `cw.write` |

No work found → emit `LOOP_COMPLETE`.

## Idempotency

Before dispatching, verify the issue is not already at the target output
status. If it is, skip it and check the next issue.

Include the issue number in the published event context so downstream hats
know which issue to work on.

## Failed Processing Escalation

Before dispatching, count comments matching `Processing failed:` on the issue.

- Count < 3 → dispatch normally.
- Count >= 3 → use the `github-project` skill's **status-transition** operation
  to set the issue's project status to `error`, skip dispatch, use the
  **add-comment** operation to post:
  `"Issue #N failed 3 times: [last error]. Status set to error. Please investigate."`
  If RObot is enabled, also send a `ralph tools interact progress` notification.

Skip items with Status `error` during dispatch.

## Error Handling

If any `github-project` skill operation fails during the scan:

1. Log the error to `errors-log.txt` with the full command and output.
2. If the failure is on a specific issue (status-transition, add-comment), skip
   that issue and continue scanning the rest.
3. If the failure is systemic (project not found, auth failure), emit
   `LOOP_COMPLETE` and log the reason.

## Comment Format

All board scanner comments use:

```
### 🦸 engineer — $(date -u +%Y-%m-%dT%H:%M:%SZ)
```

## Review Status Handling (Non-Blocking)

Issues at review statuses (`human:po:design-review`, `human:po:plan-review`, `human:po:accept`)
are dispatched to `po_reviewer` each scan cycle. The `po_reviewer` hat checks
for a human response comment, acts if found, and returns control if not. This
is non-blocking. Issues without a human response remain at their review status
and will be re-checked on the next scan cycle. NEVER skip review-status
issues — always dispatch them.

## Rules

- ALWAYS log to poll-log.txt before publishing.
- Publish exactly ONE event per scan cycle to dispatch work.
- When no work is found, emit `LOOP_COMPLETE`.
