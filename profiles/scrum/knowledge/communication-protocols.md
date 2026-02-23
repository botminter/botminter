# Communication Protocols

## Rule

Team members coordinate exclusively through GitHub issues and the `gh` skill. There is no direct member-to-member communication channel.

## Project Status Transitions

The primary coordination mechanism. A member signals work state by updating an issue's project status:

1. Use the `gh` skill to read the current issue's project status
2. Update status via `gh project item-edit` with the cached project and field IDs

Other members detect the change on their next board scan cycle.

## Issue Comments

Members record work output, decisions, and questions as comments on issues:

1. Add a comment via `gh issue comment` using the format in `PROCESS.md`

Comments are the audit trail for all work on an issue.

## Escalation Paths

When a member encounters a blocker or needs guidance:

1. **Within role:** Record the issue in a comment, update status to reflect the block
2. **Cross-role:** Add a comment tagging the relevant role, update status to hand off
3. **To human:** The PO uses `human.interact` to escalate to the human operator

## Human-in-the-Loop

The human-assistant is the human's interface to the team:
- PO sends status updates and questions via RObot (Telegram)
- Human responds via Telegram
- PO incorporates human guidance into team decisions

No other team member communicates directly with the human.

---
*Placeholder — to be populated with detailed coordination protocols before the team goes live.*
