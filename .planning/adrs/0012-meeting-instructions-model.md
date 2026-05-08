---
status: accepted
date: 2026-05-12
decision-makers: ahmed (PO)
---

# Decouple meetings from chat meta-prompt pipeline

## Problem

Meetings shared the full `prepare_chat_session()` pipeline with `bm chat`, building a meta-prompt with role identity, capabilities, skills, guardrails, and PROMPT.md content. Meetings have a fundamentally different goal — they are syntactic sugar for launching an agent with specific instructions and an initial prompt, not a full role-based chat session. The `hat` and `args` fields coupled meetings to the chat system prompt pipeline unnecessarily.

## Constraints

* Meetings must still launch a coding agent session via `launch_session()`
* Profile authors must be able to define meeting behavior without knowing about hats or ralph.yml
* The `bm chat` command must continue to use the full meta-prompt pipeline unchanged
* Existing teams initialized with the old format silently lose `bm meetings` (Alpha policy — no migration paths)

## Decision

Replace `hat` and `args` fields on the `Meeting` struct with a single `instructions: String` field. The `instructions` field IS the system prompt — it gets written to a temp file and passed via `--append-system-prompt-file`. Meetings bypass `prepare_chat_session()` entirely and construct `AgentSession` directly via a new `prepare_meeting_session()` function.

The `prompt` field semantics shift from skill invocation prefix (e.g., `/pdd`) to initial message (e.g., `start`). Meeting subcommands accept free-form trailing input instead of custom named/positional args.

## Rejected Alternatives

### Keep shared pipeline with a "meeting mode" flag

Rejected because: adds complexity to `prepare_chat_session()` for a fundamentally different use case. Meetings don't need role identity, hat capabilities, guardrails, or skills — they define their own context entirely.

### Separate meeting prompt builder reusing some chat components

Rejected because: partial reuse creates coupling without benefit. The only shared component worth reusing is `launch_session()`, which is already factored out.

## Consequences

* Profile authors write raw system prompts in meeting `instructions`. This gives full control but also full responsibility.
* No template variable substitution exists — instructions must use generic role-level language ("You are an engineer on this team") rather than literal team/member names. This is a known gap for future work.
* Meetings no longer benefit from hat/skill/guardrail changes automatically — each meeting's instructions are independently maintained.
* `MeetingArg` struct and `extract_user_args()` function are deleted. Meeting subcommands use trailing var args for free-form input.
* `ChatSession` renamed to `AgentSession` to reflect general usage by both chat and meetings.

## Anti-patterns

* **Do NOT** add meeting-specific logic to `prepare_chat_session()` — meetings have their own `prepare_meeting_session()` function. The two pipelines serve different purposes and should remain independent.
* **Do NOT** use template variables (e.g., `{{team_name}}`) in meeting instructions — the extraction pipeline does not perform variable substitution. Use generic role-level language instead.
