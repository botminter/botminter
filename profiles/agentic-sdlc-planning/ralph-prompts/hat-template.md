# Hat Instruction Template

> Reference copy of how Ralph Orchestrator presents hat instructions to the coding agent.
> Sources: `ralph-core/src/hatless_ralph.rs` hats_section(),
> `ralph-core/src/instructions.rs` custom hat instruction template.

Ralph operates in two modes depending on whether a hat is active:

## Mode A: Coordinating (No Active Hat)

When no hat is triggered, Ralph shows a topology table of all hats with their
triggers and publishes, plus a Mermaid event flow diagram:

```
## HATS

Delegate via events.

**After coordination, publish `{starting_event}` to start the workflow.**

| Hat | Triggers On | Publishes | Description |
|-----|-------------|----------|-------------|
| {hat_name} | {triggers} | {publishes} | {description} |
| ... | ... | ... | ... |

{Mermaid flowchart of event routing}

**CONSTRAINT:** You MUST only publish events from this list: {valid_events}
Publishing other events will have no effect - no hat will receive them.
```

## Mode B: Active Hat

When a hat is triggered by a pending event, Ralph shows only that hat's instructions
under `## ACTIVE HAT`, followed by an Event Publishing Guide:

```
## ACTIVE HAT

### {Hat Display Name} Instructions

{Hat's custom instructions from ralph.yml `instructions` field}

### Event Publishing Guide

Your hat can publish these events:

| Event | Received By |
|-------|------------|
| `{event}` | **{receiving_hat}** — {description} |
| ... | ... |

Publish exactly ONE event per iteration. The next iteration handles it with the appropriate hat.
```

## Auto-Generated Instructions (from instructions.rs)

For hats WITHOUT a custom `instructions` field in ralph.yml, Ralph auto-generates
instructions from the hat's pub/sub contract:

```
You are {hat_name}. You have fresh context each iteration.

## Orientation
Review the scratchpad and ready-tasks for context.

## Execute
{Auto-generated guidance based on triggers and publishes}

## Verify
Run backpressure checks if applicable.

## Report
Publish your completion event.
```

## How `bm chat` should use this

In interactive mode:
- **Hatless mode** (`bm chat <member>`): Show all hats as capabilities the agent has,
  but don't require event-based delegation. The human drives the workflow.
- **Hat-specific mode** (`bm chat <member> --hat <hat>`): Show only that hat's
  instructions as the agent's active capabilities.
- The Event Publishing Guide and event constraints do NOT apply in interactive mode.
