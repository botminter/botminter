# Workflow Variants

> Reference copy of the workflow sections Ralph Orchestrator injects based on operation mode.
> Source: `ralph-core/src/hatless_ralph.rs` workflow_section().
>
> These workflows apply during Ralph orchestration loops, NOT during interactive `bm chat` sessions.

Ralph injects one of four workflow variants depending on the configuration:

## Variant 1: Multi-Hat Coordinating (default with hats)

Used when multiple hats are configured. Ralph coordinates by planning and delegating.

```
## WORKFLOW

### 1. PLAN
You MUST update `.ralph/agent/scratchpad.md` with your understanding and plan.
You MUST create tasks with `ralph tools task add` for each work item (check `<ready-tasks>` first to avoid duplicates).

### 2. DELEGATE
You MUST publish exactly ONE event to hand off to specialized hats.
You MUST NOT do implementation work — delegation is your only job.
```

## Variant 2: Multi-Hat Fast Path

Used on a fresh run when `starting_event` is configured and no scratchpad exists yet.
Ralph immediately re-emits the starting event without planning.

```
## WORKFLOW

Publish `{starting_event}` immediately to begin the workflow.
```

## Variant 3: Solo Mode with Memories

Used in solo mode (single hat or no hats) with memory system enabled.
One task per iteration, exit when done.

```
## WORKFLOW

### 1. STUDY
Read relevant code, check memories (`ralph tools memory search`), understand the problem.

### 2. PLAN
Update `.ralph/agent/scratchpad.md` with your approach.
Create tasks with `ralph tools task add` if needed.

### 3. IMPLEMENT
Write code, following existing patterns.

### 4. VERIFY & COMMIT
Run tests and linting. Commit if passing.

### 5. EXIT
When the current task is complete, stop. The next iteration picks up remaining work.
```

## Variant 4: Solo Mode (Scratchpad Only)

Used in solo mode without the memory system. Simpler cycle.

```
## WORKFLOW

### 1. STUDY
Read relevant code and understand the problem.

### 2. PLAN
Note your approach in `.ralph/agent/scratchpad.md`.

### 3. IMPLEMENT
Write code following existing patterns.

### 4. COMMIT
Run tests. Commit if passing. Repeat from step 1 if more work remains.
```

## Selection Logic

| Condition | Variant |
|-----------|---------|
| Multi-hat + fresh run + starting_event | Fast Path |
| Multi-hat (any other case) | Coordinating |
| Solo + memories enabled | Solo with Memories |
| Solo + no memories | Solo (Scratchpad Only) |

Note: When an active hat has custom `instructions`, the `## WORKFLOW` section is
**skipped entirely** — the hat's instructions ARE the workflow.
