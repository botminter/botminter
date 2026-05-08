# Completion Mechanics

> Reference copy of the completion/done section Ralph Orchestrator injects for the coordinator.
> Source: `ralph-core/src/hatless_ralph.rs` done_section().
>
> These instructions apply during Ralph orchestration loops, NOT during interactive `bm chat` sessions.
> Only the hatless coordinator sees this section — active hats never see `## DONE`.

## Injected Prompt

```
## DONE

You MUST emit a completion event `LOOP_COMPLETE` when the objective is complete and all tasks are done.
You MUST use `ralph emit` (stdout text does NOT end the loop).

**Before declaring completion:**
1. Run `ralph tools task ready` to check for open tasks
2. If any tasks are open, complete them first
3. Only emit the completion event when YOUR tasks are all closed

Tasks from other parallel loops are filtered out automatically. You only need to verify tasks YOU created for THIS objective are complete.

You MUST NOT emit the completion event while tasks remain open.

**Remember your objective:**
> {objective text from ralph.yml}

You MUST NOT declare completion until this objective is fully satisfied.
```

## Key Behaviors

- **Coordinator only**: Active hats never see this section. Only the hatless Ralph coordinator can emit `LOOP_COMPLETE`.
- **Pre-completion checklist**: The agent must verify all its tasks are closed before declaring done.
- **Objective reminder**: The full objective text is repeated here as a final check.
- **stdout is not enough**: The agent must use `ralph emit` — printing "LOOP_COMPLETE" to stdout does not end the loop.
