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
2. **To human:** Use `human.interact` to escalate to the human operator at supervised mode gates

## Human-in-the-Loop

The agent uses supervised mode — human gates only at major decision points:
- `po:design-review` — design approval
- `po:plan-review` — plan approval
- `po:accept` — final acceptance

The agent communicates with the human via RObot (Telegram) at these gates. All other transitions auto-advance without human interaction.

---
*Placeholder — to be populated with detailed coordination protocols before the team goes live.*
