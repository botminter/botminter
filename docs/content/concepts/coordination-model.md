# Coordination Model

BotMinter teams use a **pull-based coordination model**. There is no central dispatcher. Each member independently scans the board for work matching its role, processes it, and hands off to the next member by updating the issue's status label.

## Pull-based work discovery

Each member scans GitHub issues on the team repo for status labels matching its role. When a member finds work, it processes the highest-priority issue, updates the status label to hand off to the next role, and rescans.

The specific roles, labels, and priorities are defined by the [profile](profiles.md). The generator provides the mechanism; the profile defines the vocabulary.

## Status label convention

Status labels follow the pattern `status/<role-slug>:<persona>:<activity>`:

- `<role-slug>` — the team member role responsible (e.g., `eng`, `cos`, `snt`)
- `<persona>` — the persona or sub-role acting (e.g., `po`, `arch`, `dev`, `exec`, `gate`)
- `<activity>` — the current activity within that persona's workflow

**Transition rule**: Only the role named in the status label may transition it. Profiles may designate a role with override authority.

???+ example "Example: scrum status labels"
    In the `scrum` profile, role slugs include `eng`, `cos`, `snt`, and `human`. A label like `status/eng:arch:design` means the engineer (wearing the architect persona) is responsible for producing a design. When finished, the engineer transitions the label to the next status (e.g., `status/human:po:design-review`).

    See [Process Conventions](../reference/process.md) for the full scrum label scheme.

## Board scanning

Each member has a **board-scanner skill** (auto-injected into the coordinator) that runs at the start of every loop cycle:

1. Sync the workspace (update `team/` and `projects/` submodules)
2. Clear the scratchpad and tasks (prevent context pollution between issues)
3. Query GitHub issues for status labels matching the member's role
4. Dispatch to the appropriate work hat based on the highest-priority match
5. If no work is found, publish `LOOP_COMPLETE` (idle)

Board scanning uses the `gh` skill, which wraps the `gh` CLI. The team repo is auto-detected from the `team/` submodule's Git remote.

### Project labels

Issues on the team repo are tagged with a `project/<name>` label (e.g., `project/my-app`) to associate them with a specific project codebase. This label is created automatically by `bm projects add`. When creating issues — whether manually or through the agent's architect hat — make sure the `project/<name>` label is applied so agents know which codebase the work relates to.

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
        participant Arch as engineer (architect)

        HA->>Board: Scan for status/eng:po:*
        Board-->>HA: #42 at status/eng:po:triage
        HA->>Board: Triage → status/eng:po:backlog
        HA->>Board: Activate → status/eng:arch:design

        Arch->>Board: Scan for status/eng:arch:*
        Board-->>Arch: #42 at status/eng:arch:design
        Arch->>Board: Produce design doc
        Arch->>Board: → status/human:po:design-review

        HA->>Board: Scan for status/human:po:*
        Board-->>HA: #42 at status/human:po:design-review
        HA->>Board: Human approves → status/eng:arch:plan
    ```

## Priority dispatch

When multiple issues match a member's role, the coordinator processes them by priority via the board-scanner skill. Priority orderings are defined per role in the profile's member skeletons.

One issue is processed per scan cycle. After processing, the board rescans.

???+ example "Example: scrum priority dispatch"
    **Engineer priority** (highest to lowest):
    `eng:arch:breakdown` > `eng:arch:plan` > `eng:arch:design` > `eng:arch:in-progress`

    **human-assistant priority** (highest to lowest):
    `eng:po:triage` > `human:po:design-review` > `human:po:plan-review` > `human:po:accept` > `eng:po:backlog` > `eng:po:ready`

    **chief-of-staff**: Single status — `cos:exec:todo` dispatches to the executor hat.

## Role-as-skill pattern

The **role-as-skill pattern** allows any hired member to be invoked interactively via `bm chat`, in addition to running autonomously in a Ralph loop. The member's knowledge, guardrails, and hat instructions are repackaged into a meta-prompt so the human operator can talk directly to the member — same capabilities, different interaction mode.

This pattern is useful when:

- The operator wants to direct a member's work interactively rather than through GitHub issues
- A quick task doesn't warrant the full issue lifecycle
- The operator wants to use a member's specialized knowledge in a conversation

The chief-of-staff role is the first role designed with this pattern in mind, but `bm chat` works with any hired member. See [CLI Reference — `bm chat`](../reference/cli.md#bm-chat) for usage.

### Meetings

While `bm chat` gives the operator a full role-based session, **meetings** provide a lighter-weight alternative: named shortcuts for launching an agent with specific instructions and an initial prompt. Meetings bypass the meta-prompt pipeline entirely — they don't load hats, guardrails, or skills from ralph.yml. Instead, the meeting's `instructions` field is the entire system prompt.

Meetings are defined in the profile's `botminter.yml` and are useful for repeatable, structured interactions — planning sessions, verification checks, retrospectives — where the full member identity isn't needed. See [Meetings](meetings.md) for details.

## Rejection loops

At review gates, a reviewer can reject work and send it back to the producing role. The rejection mechanism is profile-defined — profiles specify which status transitions represent rejections and where they route back to.

Rejection routing relies on the hatless Ralph orchestrator. When a rejection event goes unmatched (no hat subscribes), Ralph examines the context, determines which work hat the reviewer rejected, and routes directly back to that hat.

???+ example "Example: scrum rejection loops"
    At review gates, the human (via the human-assistant) can reject work:

    - `status/human:po:design-review` → `status/eng:arch:design` (with feedback comment)
    - `status/human:po:plan-review` → `status/eng:arch:plan` (with feedback comment)
    - `status/human:po:accept` → `status/eng:arch:in-progress` (with feedback comment)

## Comment format

All comments use emoji-attributed format to identify which role wrote them:

````markdown
### <emoji> <role> — <ISO-8601-UTC-timestamp>

Comment text here.
````

The emoji and role name are read from each member's `.botminter.yml` file. Each member posts as its own GitHub App bot user (e.g., `team-engineer[bot]`). The role attribution in the comment body provides additional context about which hat/role wrote the comment.

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
| **GitHub comments** | Issue comments (`Approved` / `Rejected: <feedback>`) | Non-blocking — agent checks for response each scan cycle | All profiles (default) |
| **Bridge messaging** | `human.interact` via RObot (Matrix, Telegram, or Rocket.Chat) | Blocking — agent waits for response within timeout | Any profile with a bridge configured |

The GitHub comment model eliminates timeout-related issues — if the human hasn't responded, the agent moves on to other work and re-checks on the next scan. The bridge messaging model blocks the loop and may time out.

### Operating modes

Members support an operating mode toggle in their `PROMPT.md`. This acts as a configurable safety dial:

- **Training mode** — all decisions require human confirmation
- **Supervised mode** — only review gates require human input; routine transitions auto-advance
- **Autonomous mode** — agents act independently

???+ example "Example: compact profile HIL"
    In the `agentic-sdlc-minimal` profile, three members collaborate — the **engineer** (handles architecture, development, and QE), the **chief-of-staff** (handles team management tasks), and the **sentinel** (handles merge gates). At review gates (`human:po:design-review`, `human:po:plan-review`, `human:po:accept`), the engineer posts review request comments on GitHub issues. The human responds with `Approved` or `Rejected: <feedback>` as an issue comment. The engineer checks for responses each scan cycle and advances or reverts status accordingly. If no response is found, no action is taken — the agent never auto-approves.

???+ example "Example: scrum HIL"
    In `scrum`, the `human-assistant` member acts as the PO's (Product Owner's) proxy. The human sends status updates and questions via RObot (Ralph's bridge integration - Matrix by default), and the human-assistant incorporates human guidance into team decisions.

## Related topics

- [Meetings](meetings.md) — focused agent sessions as an alternative to full chat
- [Architecture](architecture.md) — two-layer runtime model
- [Process Conventions](../reference/process.md) — full scrum label scheme, issue format, comment format
- [Member Roles](../reference/member-roles.md) — scrum role definitions and hat models
