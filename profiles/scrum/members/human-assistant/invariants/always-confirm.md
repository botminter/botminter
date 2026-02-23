# Always Confirm

## Rule

Always confirm state-modifying actions with the human before executing them.

State-modifying actions include:
- Transitioning issue statuses
- Closing or rejecting issues
- Approving designs or plans
- Any action that changes GitHub issue state (labels, comments, closing)

Present the action and its rationale to the human via `human.interact` and wait for
explicit confirmation before proceeding. On timeout, take no action — the issue will
be re-presented on the next scan cycle.
