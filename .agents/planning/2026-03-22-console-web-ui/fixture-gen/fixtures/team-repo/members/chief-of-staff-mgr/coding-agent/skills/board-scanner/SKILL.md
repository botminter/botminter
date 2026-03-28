---
name: board-scanner
description: >-
  Board scanning and dispatch procedure for GitHub Projects v2.
  Scans the project board for mgr:* statuses and dispatches work
  to the executor hat via priority table.
  Auto-injected into coordinator prompts.
metadata:
  author: botminter
  version: 1.0.0
---

# Board Scanner (Chief of Staff Scope)

This skill defines your PLAN step when coordinating. Scan the GitHub
Projects v2 board for `mgr:*` statuses, then DELEGATE by publishing
exactly one event to the executor hat.

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

Use the results to identify items with `mgr:*` statuses and their issue numbers
for dispatch.

### 4. Log to poll-log.txt

Use `$(date -u +%Y-%m-%dT%H:%M:%SZ)` for all timestamps.

```
2026-03-02T10:15:00Z — board.scan — START
2026-03-02T10:15:01Z — board.scan — 1 mgr issues found
2026-03-02T10:15:01Z — board.scan — END
```

### 5. Dispatch

Dispatch based on the `mgr:*` status found. Process one item at a time.

**Priority (highest first):**

| # | Status | Event |
|---|--------|-------|
| 1 | `mgr:todo` | `mgr.execute` |
| 2 | `mgr:in-progress` | `mgr.execute` |

No mgr work found -> emit `LOOP_COMPLETE`.

## Idempotency

Before dispatching, verify the issue is not already at the target output
status. If it is, skip it and check the next issue.

Include the issue number in the published event context so downstream hats
know which issue to work on.

## Failed Processing Escalation

Before dispatching, count comments matching `Processing failed:` on the issue.

- Count < 3 -> dispatch normally.
- Count >= 3 -> use the `github-project` skill's **status-transition** operation
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
### 📋 chief-of-staff — $(date -u +%Y-%m-%dT%H:%M:%SZ)
```

## Rules

- ALWAYS log to poll-log.txt before publishing.
- Publish exactly ONE event per scan cycle to dispatch work.
- When no work is found, emit `LOOP_COMPLETE`.
