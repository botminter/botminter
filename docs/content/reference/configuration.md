# Configuration Files

This reference documents the configuration files that define team member behavior.

## File overview

Each team member is configured through several files, each with a specific purpose:

| File | Purpose | Scope | Surfacing |
|------|---------|-------|-----------|
| `ralph.yml` | Ralph orchestrator config (hats, events, persistence) | Per member | Copy |
| `PROMPT.md` | Role identity and cross-hat behavioral rules | Per member | Symlink |
| `CLAUDE.md` | Role context (workspace model, knowledge paths, invariants) | Per member | Symlink |
| `.botminter.yml` | Member metadata (role name, emoji) | Per member | Read from `.botminter/` |
| `PROCESS.md` | Team process conventions | Team-wide | Read from `.botminter/` |

## ralph.yml

The Ralph orchestrator configuration. Defines hats, event routing, persistence, and runtime behavior.

### Key settings

**Persistence and event loop:**

```yaml
persistent: true
event_loop:
  completion_promise: LOOP_COMPLETE
  max_iterations: 10000
  max_runtime_seconds: 86400
```

**Hats** define specialized behaviors activated by events. The specific hats and their triggers depend on the [profile](../concepts/profiles.md) and role:

```yaml
# Example from scrum architect role
hats:
  board_scanner:
    name: Board Scanner
    triggers:
      - board.scan
    instructions: |
      ## Board Scanner
      ...

  designer:
    name: Designer
    triggers:
      - arch.design
    default_publishes: LOOP_COMPLETE
    instructions: |
      ## Designer
      ...
```

**Core guardrails** are injected into every hat prompt:

```yaml
core:
  guardrails:
    - "999. Lock discipline: ..."
    - "1000. Invariant compliance: ..."
```

### Rules

These rules are validated design principles (see [Design Principles](design-principles.md)):

- `starting_event` must not be set — all routing goes through an orchestrator hat (board scanner)
- `persistent: true` must be set — keeps the agent alive
- Each event must appear in exactly one hat's `triggers` list
- `LOOP_COMPLETE` must not appear in a hat's `publishes` list — only in `instructions`
- Work hats use `default_publishes: LOOP_COMPLETE` as a safety net; the board scanner does not need it since it explicitly publishes `LOOP_COMPLETE` in its instructions when idle
- `cooldown_delay_seconds` must not be set — agent processing time provides natural throttling

## PROMPT.md

Defines role identity and cross-hat behavioral rules. This file is symlinked from the team repo to the workspace root.

### Structure

```markdown
# <role-name>

You are the <role> for an agentic scrum team. <brief description>.

## !IMPORTANT — OPERATING MODE

**TRAINING MODE: ENABLED**

- Present all decisions to human for confirmation
- Do NOT act autonomously while training mode is enabled

## Hat Model

| Hat | Triggers | Role |
|-----|----------|------|
| ... | ...      | ...  |

## Event Dispatch Model

<table mapping status labels to events and hats>

## Constraints

- NEVER publish LOOP_COMPLETE except when idle
- ALWAYS log to poll-log.txt before publishing events
```

### Rules

- Must not prompt about hats — Ralph handles hat prompting
- Cross-hat concerns (apply to all hats) go here
- Hat-specific concerns go in `ralph.yml` hat instructions
- Training mode is declared as a `## !IMPORTANT` section

## CLAUDE.md

Provides role context — workspace model, codebase access, knowledge paths, invariant paths. Claude Code injects this into every hat prompt.

### Structure

```markdown
# <role-name> Context

Read `.botminter/CLAUDE.md` for team-wide context.

## Role

Brief role description.

## Workspace Model

<workspace layout diagram>

## Knowledge Resolution

| Level | Path |
|-------|------|
| Team  | `.botminter/knowledge/` |
| ...   | ...  |

## Invariant Compliance

| Level | Path |
|-------|------|
| Team  | `.botminter/invariants/` |
| ...   | ...  |
```

### Rules

- Must not prompt about hats — Ralph handles hat prompting
- Knowledge paths must not appear here — they go in each hat's `### Knowledge` section
- Generic invariants (team/project/member scope) go here
- Hat-specific quality gates go in `### Backpressure` in hat instructions

## .botminter.yml

Member metadata file read by the `gh` skill at runtime. The values depend on the profile and role.

```yaml
# Example from scrum architect role
role: architect
comment_emoji: "🏗️"
```

The emoji is used in comment attribution (see [Process Conventions](process.md#comment-format)).

## Global config — `~/.botminter/config.yml`

The global configuration file stores team registrations and credentials. Created by `bm init` with `0600` permissions (owner read/write only).

```yaml
workzone: /home/user/workspaces
default_team: my-team
teams:
  - name: my-team
    path: /home/user/workspaces/my-team
    profile: scrum
    github_repo: org/my-team
    credentials:
      gh_token: ghp_...
      telegram_bot_token: bot123:ABC...
      webhook_secret: my-secret
```

| Field | Required | Description |
|-------|----------|-------------|
| `workzone` | Yes | Root directory for all team workspaces |
| `default_team` | No | Team to operate on when `-t` flag is omitted |
| `teams[].name` | Yes | Team identifier |
| `teams[].path` | Yes | Absolute path to team directory |
| `teams[].profile` | Yes | Profile name (e.g., `scrum`, `scrum-compact`, `scrum-compact-telegram`) |
| `teams[].github_repo` | No | GitHub `org/repo` for team coordination |
| `teams[].credentials.gh_token` | No | GitHub API token for `gh` CLI (auto-detected from `GH_TOKEN` env var or `gh auth token` during `bm init`) |
| `teams[].credentials.telegram_bot_token` | No | Telegram bot token for HIL (Human-in-the-Loop) notifications. Required for `scrum-compact-telegram` profile; optional for others. |
| `teams[].credentials.webhook_secret` | No | HMAC secret for daemon webhook signature validation |

## Daemon runtime files

The daemon writes several runtime files to `~/.botminter/`:

| File | Format | Purpose |
|------|--------|---------|
| `daemon-{team}.pid` | Plain text | Process ID of the running daemon |
| `daemon-{team}.json` | JSON | Daemon config (team, mode, port, interval, PID, start time) |
| `daemon-{team}-poll.json` | JSON | Poll state (last event ID, last poll timestamp) |
| `logs/daemon-{team}.log` | Text | Timestamped log entries; rotates at 10 MB |

## Formation config — `formations/{name}/formation.yml`

Schema v2 profiles support formations — deployment targets for team members. Formation configs live in the team repo.

```yaml
name: local
description: Local development deployment
type: local
```

For non-local formations (e.g., Kubernetes):

```yaml
name: k8s-prod
description: Kubernetes production deployment
type: k8s
k8s:
  context: prod-cluster
  image: botminter/team:latest
  namespace_prefix: BotMinter
manager:
  ralph_yml: manager/ralph.yml
  prompt: manager/PROMPT.md
  hats_dir: manager/hats
```

| Field | Required | Description |
|-------|----------|-------------|
| `name` | Yes | Formation identifier |
| `description` | Yes | Human-readable description |
| `type` | Yes | `local` or `k8s` |
| `k8s` | For `k8s` type | Kubernetes deployment config |
| `manager` | For non-local types | Ralph session config for the formation manager |

## Topology file — `.topology`

`bm start` writes a `.topology` file in the team directory tracking member endpoints. This file is managed by the CLI and should not be edited manually.

## Separation of concerns

| Layer | Purpose | What goes here | What does not |
|-------|---------|----------------|---------------|
| `PROMPT.md` | Role identity | Training mode, cross-hat rules, event dispatch | Hat-specific instructions, knowledge paths |
| `CLAUDE.md` | Role context | Workspace model, invariant paths, references | Hat instructions, knowledge paths |
| Hat instructions (`ralph.yml`) | Operational details | Event publishing, knowledge paths, backpressure | Role identity, invariant declarations |
| `core.guardrails` (`ralph.yml`) | Universal rules | Lock discipline, cross-cutting constraints | Hat-specific rules |

## Related topics

- [Design Principles](design-principles.md) — validated rules for configuration
- [Member Roles](member-roles.md) — role-specific configurations
- [Workspace Model](../concepts/workspace-model.md) — how files are surfaced
