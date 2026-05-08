# Event Writing

> Reference copy of the event writing instructions Ralph Orchestrator injects in every iteration.
> Source: `ralph-core/src/hatless_ralph.rs` event_writing_section().
>
> These instructions apply during Ralph orchestration loops, NOT during interactive `bm chat` sessions.

## Injected Prompt

```
## EVENT WRITING

Events are routing signals, not data transport. You SHOULD keep payloads brief.

You MUST use `ralph emit` to write events (handles JSON escaping correctly):
\```bash
ralph emit "build.done" "tests: pass, lint: pass, typecheck: pass, audit: pass, coverage: pass"
ralph emit "review.done" --json '{"status": "approved", "issues": 0}'
\```

You MUST NOT use echo/cat to write events because shell escaping breaks JSON.

You SHOULD write detailed output to `.ralph/agent/scratchpad.md` and emit only a brief event.

**Constraints:**
- You MUST stop working after publishing an event because a new iteration will start with fresh context
- You MUST NOT continue with additional work after publishing because the next iteration handles it with the appropriate hat persona
```

## Key Behaviors

- **Always present**: This section appears in every iteration, both coordinating and active hat modes.
- **ralph emit CLI**: Events MUST be written via the `ralph emit` command, not via echo/cat, to avoid JSON escaping issues.
- **One event per iteration**: After publishing, the agent must stop. The next iteration starts fresh with the event routed to the appropriate hat.
- **Brief payloads**: Events are routing signals. Detailed work output goes in the scratchpad.
