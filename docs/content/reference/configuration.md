# Configuration Files

This reference documents the configuration files that define team member behavior.

## File overview

Each team member is configured through several files, each with a specific purpose:

| File | Purpose | Scope | Surfacing |
|------|---------|-------|-----------|
| `ralph.yml` | Ralph orchestrator config (hats, events, persistence) | Per member | Copy |
| `PROMPT.md` | Role identity and cross-hat behavioral rules | Per member | Symlink |
| `AGENTS.md` | Role context (workspace model, knowledge paths, invariants) | Per member | Symlink |
| `.botminter.yml` | Member metadata (role name, emoji) | Per member | Read from `team/` |
| `PROCESS.md` | Team process conventions | Team-wide | Read from `team/` |

## Profile directory — `~/.config/botminter/profiles/`

Profiles are stored on disk at `~/.config/botminter/profiles/`. This directory is populated automatically on first use (via auto-prompt) or explicitly with `bm profiles init`.

```
~/.config/botminter/
  profiles/
    scrum/                          # Multi-member team profile
      botminter.yml                 # Profile manifest
      PROCESS.md                    # Process conventions
      context.md                    # Agent context template
      knowledge/                    # Team-level knowledge
      invariants/                   # Team-level quality rules
      roles/                        # Role skeletons
        architect/
        human-assistant/
      coding-agent/                 # Shared agent skills
      skills/                       # Profile skills
      formations/                   # Deployment targets
      .schema/                      # Schema validation layout
    scrum-compact/                  # Single-agent profile (optional bridge: Matrix or Telegram)
      ...
  minty/                            # Minty interactive assistant
    prompt.md                       # Minty persona + system instructions
    config.yml                      # Minty-specific settings
    skills/                         # Composable skills (populated by bm)
```

| Path | Purpose |
|------|---------|
| `~/.config/botminter/profiles/` | Root for all profile templates |
| `~/.config/botminter/profiles/<name>/botminter.yml` | Profile manifest (roles, statuses, coding agents, views) |
| `~/.config/botminter/profiles/<name>/roles/<role>/` | Member skeleton (ralph.yml, PROMPT.md, context.md, .botminter.yml) |
| `~/.config/botminter/profiles/<name>/knowledge/` | Shared knowledge files extracted into team repos |
| `~/.config/botminter/profiles/<name>/invariants/` | Quality rules extracted into team repos |
| `~/.config/botminter/minty/` | Minty interactive assistant config |
| `~/.config/botminter/minty/prompt.md` | Minty persona prompt and system instructions |
| `~/.config/botminter/minty/config.yml` | Minty-specific settings |
| `~/.config/botminter/minty/skills/` | Composable skills for Minty |

Profiles are editable — changes take effect the next time you run `bm init` or `bm hire`. To reset a profile to its built-in defaults, run `bm profiles init` and confirm the overwrite, or use `bm profiles init --force`.

See [Profiles](../concepts/profiles.md) for the full concept and [CLI Reference](../reference/cli.md#bm-profiles-init) for `bm profiles init` usage.

## botminter.yml (profile manifest)

The profile manifest defines the team methodology. It lives at the root of the team repo and is extracted from the profile on disk during `bm init`.

### Coding agent configuration

```yaml
coding_agents:
  claude-code:
    name: claude-code
    display_name: "Claude Code"
    context_file: "AGENTS.md"
    agent_dir: ".claude"
    binary: "claude"

default_coding_agent: claude-code
```

| Field | Required | Description |
|-------|----------|-------------|
| `coding_agents` | Yes | Map of supported coding agent definitions |
| `coding_agents.<name>.name` | Yes | Agent identifier (matches map key) |
| `coding_agents.<name>.display_name` | Yes | Human-readable name |
| `coding_agents.<name>.context_file` | Yes | Filename the agent reads for context (e.g., `AGENTS.md`) |
| `coding_agents.<name>.agent_dir` | Yes | Agent-specific config directory (e.g., `.claude`) |
| `coding_agents.<name>.binary` | Yes | Binary name for launching the agent (e.g., `claude`) |
| `default_coding_agent` | Yes | Key into `coding_agents` — the agent used unless overridden at team level |

The extraction pipeline uses these values to rename `context.md` → `context_file` and filter inline agent tags during `bm init` and `bm hire`. See [Profiles — Coding-agent abstraction](../concepts/profiles.md#coding-agent-abstraction) for details.

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
  designer:
    name: Designer
    triggers:
      - arch.design
    default_publishes: LOOP_COMPLETE
    instructions: |
      ## Designer
      ...

skills:
  overrides:
    board-scanner:
      auto_inject: true            # Board scanning is a coordinator skill, not a hat
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

- `starting_event` must not be set — all routing goes through the coordinator (via the board-scanner skill)
- `persistent: true` must be set — keeps the agent alive
- Each event must appear in exactly one hat's `triggers` list
- `LOOP_COMPLETE` must not appear in a hat's `publishes` list — only in `instructions`
- Work hats use `default_publishes: LOOP_COMPLETE` as a safety net; the coordinator does not need it since it publishes `LOOP_COMPLETE` via the board-scanner skill when idle
- `cooldown_delay_seconds` must not be set — agent processing time provides natural throttling

## Skills

Skills are composable, discoverable units of operational knowledge extracted from hat instructions. They follow the `SKILL.md` pattern and are loaded on demand by the coding agent during Ralph orchestration or interactive sessions.

### SKILL.md format

Each skill lives in its own directory with a `SKILL.md` file:

```
coding-agent/skills/
  status-workflow/
    SKILL.md                          # Skill definition (YAML frontmatter + instructions)
    references/                       # Supporting reference documents
      graphql-mutations.md
    scripts/                          # Optional executable scripts
  board-scanner/
    SKILL.md
  gh/
    SKILL.md
    scripts/
    references/
```

The `SKILL.md` file combines YAML frontmatter (metadata) with markdown instructions:

````markdown
---
name: status-workflow
description: >-
  Performs GitHub Projects v2 status transitions for issue workflow.
metadata:
  author: botminter
  version: 1.0.0
  category: workflow
  tags: [github, projects-v2, status, workflow]
  requires-tools: [gh, jq]
  requires-env: [GH_TOKEN]
  requires-scope: [project]
---

# Status Workflow

Instructions for performing status transitions...
````

| Frontmatter Field | Required | Description |
|-------------------|----------|-------------|
| `name` | Yes | Skill identifier (matches directory name) |
| `description` | Yes | What the skill does (used for discovery) |
| `metadata.author` | No | Who created the skill |
| `metadata.version` | No | Semantic version |
| `metadata.category` | No | Grouping category |
| `metadata.tags` | No | Searchable tags |
| `metadata.requires-tools` | No | CLI tools the skill depends on |
| `metadata.requires-env` | No | Environment variables needed |
| `metadata.requires-scope` | No | Scope requirements (e.g., `project`, `team`) |

### Skill scoping

Skills follow the same recursive scoping model as knowledge and invariants. The `skills.dirs` setting in `ralph.yml` defines the search path:

```yaml
skills:
  enabled: true
  dirs:
    - team/coding-agent/skills                          # Team-wide skills
    - team/projects/<project>/coding-agent/skills       # Project-specific skills
    - team/members/<member>/coding-agent/skills         # Member-specific skills
  overrides:
    board-scanner:
      auto_inject: true                                 # Inject into every iteration
```

| Scope | Path | Example Skills |
|-------|------|----------------|
| Team | `coding-agent/skills/` | `status-workflow`, `gh`, `board-scanner` |
| Project | `projects/<project>/coding-agent/skills/` | Project-specific workflows |
| Member | `members/<member>/coding-agent/skills/` | Role-specific skills |

Skills at more specific scopes can override team-level skills of the same name.

### Loading skills

Skills are loaded on demand by the agent:

```bash
ralph tools skill list              # List available skills
ralph tools skill load <name>       # Load a skill into the current context
```

Skills with `auto_inject: true` in the `overrides` section are loaded automatically every iteration (e.g., `board-scanner`).

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

## AGENTS.md

Provides role context — workspace model, codebase access, knowledge paths, invariant paths. Claude Code injects this into every hat prompt.

### Structure

```markdown
# <role-name> Context

Read `team/AGENTS.md` for team-wide context.

## Role

Brief role description.

## Workspace Model

<workspace layout diagram>

## Knowledge Resolution

| Level | Path |
|-------|------|
| Team  | `team/knowledge/` |
| ...   | ...  |

## Invariant Compliance

| Level | Path |
|-------|------|
| Team  | `team/invariants/` |
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
| `teams[].profile` | Yes | Profile name (e.g., `scrum`, `scrum-compact`) |
| `teams[].github_repo` | No | GitHub `org/repo` for team coordination |
| `teams[].coding_agent` | No | Override the profile's `default_coding_agent` for this team (e.g., `gemini-cli`) |
| `teams[].credentials.gh_token` | No | GitHub API token for `gh` CLI (auto-detected from `GH_TOKEN` env var or `gh auth token` during `bm init`) |
| `teams[].credentials.telegram_bot_token` | No | Legacy field. Bridge tokens are now stored per-member in the system keyring via `bm bridge identity add`. |
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

Profiles support formations — deployment targets for team members. Formation configs live in the team repo.

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
| `AGENTS.md` | Role context | Workspace model, invariant paths, references | Hat instructions, knowledge paths |
| Hat instructions (`ralph.yml`) | Operational details | Event publishing, knowledge paths, backpressure | Role identity, invariant declarations |
| `core.guardrails` (`ralph.yml`) | Universal rules | Lock discipline, cross-cutting constraints | Hat-specific rules |

## Related topics

- [Design Principles](design-principles.md) — validated rules for configuration
- [Member Roles](member-roles.md) — role-specific configurations
- [Workspace Model](../concepts/workspace-model.md) — how files are surfaced
