# Communication Protocols

## Rule

The compact profile uses a single-member self-transition model. The agent coordinates through GitHub issues via the `gh` skill, self-transitioning between roles by switching hats.

## Project Status Transitions

The primary coordination mechanism. The agent signals work state by updating an issue's project status:

1. Use the `gh` skill to read the current issue's project status
2. Update status via `gh project item-edit` with the cached project and field IDs

The board scanner detects the change on the next scan cycle and dispatches the appropriate hat.

## Issue Comments

The agent records work output, decisions, and questions as comments on issues:

1. Add a comment via `gh issue comment` using the format in `PROCESS.md`

Comments use the emoji + role header of the active hat (e.g., `🏗️ architect`, `💻 dev`, `🧪 qe`) to preserve audit trail clarity, even though it is a single agent.

## Escalation Paths

When the agent encounters a blocker or needs guidance:

1. **Within workflow:** Record the issue in a comment, continue processing
2. **To human:** Add a review request comment on the issue (see Human-in-the-Loop below)

## Human-in-the-Loop (GitHub Comments)

The agent uses supervised mode — human gates only at major decision points:
- `po:design-review` — design approval
- `po:plan-review` — plan approval
- `po:accept` — final acceptance

### How it works

1. The agent adds a **review request comment** on the issue summarizing the artifact
2. The agent **returns control** — the issue stays at its review status
3. The agent **moves on to other work** — no blocking, no timeout

The **human** reads the comment on GitHub, then responds via a comment:
- `Approved` (or `LGTM`) → agent advances status on next scan
- `Rejected: <feedback>` → agent reverts status and appends feedback

### Detection rules

The `po_reviewer` hat scans issue comments for the human's response:
- Look for comments NOT authored by the bot user (i.e., from a human)
- Scan the **most recent** human comment after the agent's review request comment
- If the comment contains `approved` or `lgtm` (case-insensitive) → approval
- If the comment contains `rejected` or `changes requested` (case-insensitive) → rejection, with the rest of the comment as feedback
- If no human comment found after the review request → no action, return control

### Telegram notifications (optional)

If RObot is enabled (`RObot.enabled: true` in ralph.yml and a Telegram token is configured), the agent sends non-blocking `progress` notifications via `ralph tools interact progress` to alert the human that a review is waiting. These are FYI only — they do not block the loop.

---
*GitHub comments are the primary HIL channel. Telegram is optional and used for notifications only.*
