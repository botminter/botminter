# Guardrails Framing

> Reference copy of how Ralph Orchestrator frames guardrails from ralph.yml config.
> Source: `ralph-core/src/hatless_ralph.rs` core_prompt() guardrails section.

Ralph reads the `guardrails` list from ralph.yml and injects them as a numbered section
starting at 999. This framing gives guardrails high visual weight in the prompt without
conflicting with other numbered sections.

## Template

```
### GUARDRAILS
999. {guardrail 1 from ralph.yml}
1000. {guardrail 2 from ralph.yml}
1001. {guardrail 3 from ralph.yml}
...
```

## How `bm chat` should use this

Read the `guardrails` array from the member's `ralph.yml` and format each entry as a
numbered item starting at 999, under a `### GUARDRAILS` heading. Inject the result
into the meta-prompt's `## Guardrails` section.

Guardrails always apply — in both orchestration mode and interactive mode.
