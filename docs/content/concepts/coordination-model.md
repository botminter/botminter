# Coordination Model

botminter teams use a **pull-based coordination model**. There is no central dispatcher. Each member independently scans the board for work matching its role, processes it, and hands off to the next member by updating the issue's status label.

## Pull-based work discovery

Each member scans GitHub issues on the team repo for status labels matching its role. When a member finds work, it processes the highest-priority issue, updates the status label to hand off to the next role, and rescans.

The specific roles, labels, and priorities are defined by the [profile](profiles.md). The generator provides the mechanism; the profile defines the vocabulary.

## Status label convention

Status labels follow the pattern `status/<role>:<phase>`:

- `<role>` — the team member role responsible
- `<phase>` — the current phase within that role's workflow

**Transition rule**: Only the role named in the status label may transition it. Profiles may designate a role with override authority.

???+ example "Example: scrum status labels"
    In the `scrum` profile, roles include `po`, `arch`, `dev`, and `qe`. A label like `status/arch:design` means the architect is responsible for producing a design. When finished, the architect transitions the label to the next role (e.g., `status/po:design-review`).

    See [Process Conventions](../reference/process.md) for the full scrum label scheme.

## Board scanning

Each member has a **board scanner** hat that runs at the start of every loop cycle:

1. Sync the workspace (pull `.botminter/` and project repos)
2. Clear the scratchpad and tasks (prevent context pollution between issues)
3. Query GitHub issues for status labels matching the member's role
4. Dispatch to the appropriate work hat based on the highest-priority match
5. If no work is found, publish `LOOP_COMPLETE` (idle)

Board scanning uses the `gh` skill, which wraps the `gh` CLI. The team repo is auto-detected from `.botminter/`'s Git remote.

## Handoff mechanism

Work transitions between members through status label updates:

1. The current member removes the old status label and adds the new one via `gh issue edit`
2. The current member adds an attribution comment documenting the transition
3. The next member detects the change on its next board scan cycle

No direct communication occurs between members. The board (GitHub issues) is the sole coordination mechanism.

???+ example "Example: scrum handoff sequence"
    ```mermaid
    sequenceDiagram
        participant HA as human-assistant
        participant Board as GitHub Issues
        participant Arch as architect

        HA->>Board: Scan for status/po:*
        Board-->>HA: #42 at status/po:triage
        HA->>Board: Triage → status/po:backlog
        HA->>Board: Activate → status/arch:design

        Arch->>Board: Scan for status/arch:*
        Board-->>Arch: #42 at status/arch:design
        Arch->>Board: Produce design doc
        Arch->>Board: → status/po:design-review

        HA->>Board: Scan for status/po:*
        Board-->>HA: #42 at status/po:design-review
        HA->>Board: Human approves → status/arch:plan
    ```

## Priority dispatch

When multiple issues match a member's role, the board scanner processes them by priority. Priority orderings are defined per role in the profile's member skeletons.

One issue is processed per scan cycle. After processing, the board rescans.

???+ example "Example: scrum priority dispatch"
    **Architect priority** (highest to lowest):
    `arch:breakdown` > `arch:plan` > `arch:design` > `arch:in-progress`

    **human-assistant priority** (highest to lowest):
    `po:triage` > `po:design-review` > `po:plan-review` > `po:accept` > `po:backlog` > `po:ready`

## Rejection loops

At review gates, a reviewer can reject work and send it back to the producing role. The rejection mechanism is profile-defined — profiles specify which status transitions represent rejections and where they route back to.

Rejection routing relies on the hatless Ralph orchestrator. When a rejection event goes unmatched (no hat subscribes), Ralph examines the context, determines which work hat the reviewer rejected, and routes directly back to that hat.

???+ example "Example: scrum rejection loops"
    At review gates, the human (via the human-assistant) can reject work:

    - `status/po:design-review` → `status/arch:design` (with feedback comment)
    - `status/po:plan-review` → `status/arch:plan` (with feedback comment)
    - `status/po:accept` → `status/arch:in-progress` (with feedback comment)

## Comment format

All comments use emoji-attributed format to identify which role wrote them:

````markdown
### <emoji> <role> — <ISO-8601-UTC-timestamp>

Comment text here.
````

The emoji and role name are read from each member's `.botminter.yml` file. Since all agents share one `GH_TOKEN`, the role attribution in the comment body is the primary way to identify which agent wrote it.

???+ example "Example: scrum emoji mapping"
    | Role | Emoji |
    |------|-------|
    | po | `📝` |
    | architect | `🏗️` |
    | dev | `💻` |
    | qe | `🧪` |

    The specific roles and emojis are defined in each member's `.botminter.yml` and depend on the profile.

## Human-in-the-loop (HIL)

Profiles can designate a member as the human's interface to the team. This member relays human decisions and feedback to the board. No other team member communicates directly with the human — all human interaction goes through the designated member.

### HIL channels

Two HIL models are supported, depending on the profile:

| Model | Channel | Blocking | Used by |
|-------|---------|----------|---------|
| **GitHub comments** | Issue comments (`Approved` / `Rejected: <feedback>`) | Non-blocking — agent checks for response each scan cycle | `scrum-compact` |
| **Telegram** | `human.interact` via RObot | Blocking — agent waits for response within timeout | `scrum`, `scrum-compact-telegram` |

The GitHub comment model eliminates timeout-related issues — if the human hasn't responded, the agent moves on to other work and re-checks on the next scan. The Telegram model blocks the loop and may time out.

### Operating modes

Members support an operating mode toggle in their `PROMPT.md`. This acts as a configurable safety dial:

- **Training mode** — all decisions require human confirmation
- **Supervised mode** — only review gates require human input; routine transitions auto-advance
- **Autonomous mode** — agents act independently

???+ example "Example: compact profile HIL"
    In the `scrum-compact` profile, the single "superman" agent posts review request comments on GitHub issues at review gates (`po:design-review`, `po:plan-review`, `po:accept`). The human responds with `Approved` or `Rejected: <feedback>` as an issue comment. The agent checks for responses each scan cycle and advances or reverts status accordingly. If no response is found, no action is taken — the agent never auto-approves.

???+ example "Example: scrum HIL"
    In `scrum`, the `human-assistant` member acts as the PO's (Product Owner's) proxy. The human sends status updates and questions via RObot (Ralph's Telegram integration), and the human-assistant incorporates human guidance into team decisions.

## Related topics

- [Architecture](architecture.md) — two-layer runtime model
- [Process Conventions](../reference/process.md) — full scrum label scheme, issue format, comment format
- [Member Roles](../reference/member-roles.md) — scrum role definitions and hat models
