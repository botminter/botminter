# Roadmap

BotMinter is developed through incremental milestones. Each milestone builds on the previous one, adding features to the CLI and validating them with synthetic test tasks before operational use.

!!! warning "Pre-Alpha"
    BotMinter is under active development and not yet ready for production use. The information on this page reflects the current plan and is subject to change.

## Overview

| Milestone | Status | Description |
|-----------|--------|-------------|
| Structure + human-assistant | **Complete** | Profile skeleton, `scrum` profile, workspace model, first member |
| Autonomous Ralph | **Complete** | Spike validating persistent loop pattern for pull-based agents |
| Architect + First Epic | **Complete** | Second member, epic lifecycle, two-member coordination |
| GitHub Integration | **Complete** | Replaced file-based coordination with real GitHub via `gh` CLI |
| `bm` CLI | **Complete** | Rust CLI, single operator interface, workzone model |
| Minty and Friends | **Complete** | Team Manager role, profile externalization, Minty assistant |
| Team Bridge | **Complete** | Bridge plugin system, Matrix (default) + Telegram + Rocket.Chat bridges |
| Full Team + First Story | Future | Dev, QE, reviewer members, full story kanban, TDD flow |
| Eval/Confidence System | Future | Formalized eval framework, scored confidence, HIL graduation |

---

## Completed

### Structure + human-assistant

Built the foundational infrastructure:

- Profile skeleton — process-agnostic directory structure
- `scrum` profile — PROCESS.md, AGENTS.md, team knowledge/invariants
- human-assistant member skeleton with three-hat model
- Workspace model — clone project, embed team repo, surface config
- Human-in-the-loop (HIL) via RObot — validated during development; available as an optional bridge on any profile

**Proved**: Inner loop works (Ralph + hats). Workspace model works (clone, surface, run). HIL validated (human <> human-assistant via Telegram).

### Autonomous Ralph

Spike validating how Ralph runs autonomously in a persistent loop, pulling work from a board instead of a single-objective prompt:

- `persistent: true` with `task.resume` routing
- Self-clearing scratchpad/tasks between work items
- Idle behavior via `LOOP_COMPLETE`

Deliverable: validated `ralph.yml` pattern adopted directly by later milestones.

### Architect + First Epic

Added the architect as a second team member and validated two-member coordination:

- Architect member skeleton (ralph.yml, PROMPT.md, AGENTS.md, five hats)
- human-assistant evolution — new hats for epic creation, design gating
- Epic lifecycle statuses in PROCESS.md
- Two-member outer loop coordination validated with synthetic epics

**Proved**: Outer loop works (GitHub issues, status labels, knowledge resolution). Pull model works (architect picks up work via status watch). Two-member coordination works (PO creates, architect designs, PO gates).

### GitHub Integration

Replaced file-based coordination with real GitHub (pulled forward from original planning):

- `gh` CLI calls replaced file operations (1:1 mapping)
- Unified `gh` skill as single interaction point
- Coordination model unchanged — only backing store moved

### `bm` CLI

Replaced Justfile-based tooling with a Rust CLI binary (`bm`):

- Single operator interface for managing agentic teams
- Workzone model with known, discoverable workspace directory
- Profile restructuring — collapsed into single layer with `botminter.yml` + `.schema/`
- Profiles shipped with the binary, extracted to disk on first use (`bm profiles init`)
- Event-driven daemon with webhook and poll modes (`bm daemon start/stop/status`)
- Knowledge management commands (`bm knowledge list/show` + interactive mode)
- Formation-aware deployment (`bm start --formation`)
- Commands: `bm init`, `bm hire`, `bm start` (alias: `bm up`), `bm stop`, `bm status`, `bm teams list/show/sync`, `bm members list/show`, `bm roles list`, `bm profiles list/describe`, `bm projects add/list/show/sync`, `bm knowledge list/show` (+ interactive mode), `bm daemon start/stop/status`, `bm completions`
- Integration test suite covering full lifecycle, hire, sync, schema guard, multi-team

**Proved**: CLI-driven team management works. Versioned profile model enables future upgrades. Workzone model provides discoverability.

### Minty and Friends

Multiple UX enhancements to improve the operator experience:

- **Team Manager role** — a new team-scoped role for process improvement tasks, operating independently from dev workflow
- **Profile externalization** — profiles extracted to disk on first use, editable and customizable without rebuilding the binary
- **Workspace repository model** — dedicated git repo per agent with submodules for team repo and project forks, replacing the earlier embedded workspace model
- **Minty** — BotMinter's interactive assistant persona with composable skills
- **Coding-agent-agnostic cleanup** — abstracted Claude Code-specific assumptions from profiles and CLI

**Proved**: Role-as-skill pattern works. Skill-driven architecture viable. Disk-based profiles enable operator customization. Stack is coding-agent-agnostic end-to-end.

---

### Team Bridge

Bridge plugin system for connecting team members to messaging platforms:

- **Bridge plugin contract** — Knative-style resource format (bridge.yml + schema.json + Justfile), no Rust code needed for new bridges
- **Matrix / Tuwunel bridge** (local, default) — single Podman container with Tuwunel homeserver, pre-selected during `bm init`
- **Telegram bridge** (external, experimental) — operator-managed bot tokens, per-member identity
- **Rocket.Chat bridge** (local, experimental) — Podman pod with RC + MongoDB, auto-provisioned bot accounts
- **Bridge CLI** — `bm bridge start/stop/status`, `bm bridge identity add/show/rotate/remove`, `bm bridge room create/list`
- **Per-member start/stop** — `bm start <member>` and `bm stop <member>` for individual member lifecycle
- **Credential storage** — system keyring with env var fallback, formation-aware via CredentialStore trait
- **E2E test coverage** — operator journey scenarios for Matrix and Telegram bridges

**Proved**: Bridge plugin model is extensible. Local bridges can self-provision. Credential management works across formation types.

---

## Future

### Full Team + First Story

Adds dev, QE, and reviewer as team members:

- Dev, QE, reviewer member skeletons
- Full story kanban statuses in PROCESS.md
- TDD flow: QE writes tests, dev implements, QE verifies, reviewer reviews, architect signs off, PO merges
- Codebase access model (project fork, agent-cloned)
- First real knowledge accumulation

### Eval/Confidence System

Formalizes the distributed eval framework:

- Eval framework across recursive scopes (team, project, member, member+project)
- Hard-gate vs advisory eval distinction
- Scored confidence metrics
- Evidence chain verification
- HIL graduation path (training, supervised, autonomous)

## Future ideas

- Extract human-assistant from profiles (infrastructure, not a team role)
- Access control per scope + SLSA-style attestation for PRs
- `ralph.yml` hot-reload (eliminate need for sync + restart)
- Knowledge observation mechanism for automatic knowledge capture
- Hat-level skill filtering (pending Ralph runtime support)

## Related topics

- [Architecture](concepts/architecture.md) — profile-based generation model and two-layer runtime
- [Member Roles](reference/member-roles.md) — current and planned role definitions
- [Bridges](concepts/bridges.md) — bridge types and plugin contract
