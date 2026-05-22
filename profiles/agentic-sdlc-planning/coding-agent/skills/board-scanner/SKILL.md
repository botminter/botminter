---
name: board-scanner
description: >-
  Board scanning and dispatch procedure for GitHub Projects v2.
  Scans the project board for actionable issues, dispatches human gates
  to po_gate, and delegates to specialized hats via priority tables.
  Auto-injected into coordinator prompts.
metadata:
  author: botminter
  version: 2.0.0
---

# Board Scanner

This skill defines your PLAN step when coordinating. Scan the GitHub
Projects v2 board, then DELEGATE by publishing exactly one event to
the appropriate hat.

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

### 4. Reconcile (if needed)

If ANY items from the board fetch have null or empty status, run the
`github-project` skill's **status-reconcile** operation before proceeding:

```bash
bash ${CLAUDE_SKILL_DIR}/../github-project/scripts/status-reconcile.sh
```

Then re-fetch the board (repeat step 3) to get the corrected statuses.

**Why this happens:** The Status field is a `ProjectV2SingleSelectField` where
each option has an internal ID. Editing the field in the GitHub Projects UI
(reorder, color change, add/remove option) regenerates all option IDs. Items
referencing old IDs lose their status. The reconciliation script recovers each
item's last status from GitHub's timeline API and re-applies it with fresh IDs.

Log the event to poll-log.txt:
```
2026-05-17T10:15:01Z — board.scan — RECONCILE — N items had null status, reconciliation applied
```

### 5. Log to poll-log.txt

Use `$(date -u +%Y-%m-%dT%H:%M:%SZ)` for all timestamps.

```
2026-03-02T10:15:00Z — board.scan — START
2026-03-02T10:15:01Z — board.scan — 3 issues found
2026-03-02T10:15:01Z — board.scan — END
```

### 6. Dispatch

Dispatch based on the highest-priority project status found. Process one
item at a time. Match each item's status against the tables below. The
first match wins.

The tables are organized by workflow area. The scanner dispatches purely
by status — it does NOT need to query the issue type. Hats that handle
shared statuses (e.g., `human:po:plan-review`, `human:po:accept`) are
responsible for querying the issue type themselves.

Skip items with `snt:` prefix — those belong to the sentinel's scanner.
Skip items with `cos:` prefix — those belong to the chief-of-staff's scanner.

Principle: **finish in-progress work before starting new work.** `eng:*`
statuses closer to `done` have higher priority. Human gates (`human:*`)
are lowest priority — the PO advances issues directly on GitHub. The
`po_gate` hat detects those actions but does not need eager polling.

All `human:*` statuses dispatch to `po.gate`. The `po_gate` hat determines
per-status behavior including auto-advance label support (`plan:auto` at
`human:po:plan-review`, `accept:auto` at `human:po:accept`).

| # | Status | Event |
|---|--------|-------|
| 1 | `eng:qe:verify` | `qe.verify` |
| 2 | `eng:cw:review` | `cw.review` |
| 3 | `eng:qe:monitor` | `qe.monitor` |
| 4 | `eng:dev:implement` | `dev.implement` |
| 5 | `eng:cw:write` | `cw.write` |
| 6 | `eng:qe:investigate` | `qe.investigate` |
| 7 | `eng:sre:setup` | `sre.setup` |
| 8 | `eng:lead:breakdown` | `lead.breakdown` |
| 9 | `eng:lead:monitor` | `lead.monitor` |
| 10 | `eng:lead:plan` | `lead.plan` |
| 11 | `human:po:accept` | `po.gate` |
| 12 | `human:po:plan-review` | `po.gate` |
| 13 | `human:po:triage` | `po.gate` |
| 14 | `human:po:backlog` | `po.gate` |

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

## Rules

- ALWAYS log to poll-log.txt before publishing.
- Publish exactly ONE event per scan cycle to dispatch work.
- When no work is found, emit `LOOP_COMPLETE`.
