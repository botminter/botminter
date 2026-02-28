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
| Full Team + First Story | Planned | Dev, QE, reviewer members, full story kanban, TDD flow |
| Eval/Confidence System | Planned | Formalized eval framework, scored confidence, HIL graduation |

---

## Completed

### Structure + human-assistant

Built the foundational infrastructure:

- Profile skeleton — process-agnostic directory structure
- `scrum` profile — PROCESS.md, CLAUDE.md, team knowledge/invariants
- human-assistant member skeleton with three-hat model
- Workspace model — clone project, embed team repo, surface config
- Human-in-the-loop (HIL) via Telegram (RObot) — validated during development; available as the `scrum-compact-telegram` profile variant

**Proved**: Inner loop works (Ralph + hats). Workspace model works (clone, surface, run). HIL validated (human <> human-assistant via Telegram).

### Autonomous Ralph

Spike validating how Ralph runs autonomously in a persistent loop, pulling work from a board instead of a single-objective prompt:

- `persistent: true` with `task.resume` routing
- Self-clearing scratchpad/tasks between work items
- Idle behavior via `LOOP_COMPLETE`

Deliverable: validated `ralph.yml` pattern adopted directly by later milestones.

### Architect + First Epic

Added the architect as a second team member and validated two-member coordination:

- Architect member skeleton (ralph.yml, PROMPT.md, CLAUDE.md, five hats)
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
- Profiles embedded in the binary at compile time via `include_dir`
- Event-driven daemon with webhook and poll modes (`bm daemon start/stop/status`)
- Knowledge management commands (`bm knowledge list/show` + interactive mode)
- Formation-aware deployment (`bm start --formation`)
- Commands: `bm init`, `bm hire`, `bm start` (alias: `bm up`), `bm stop`, `bm status`, `bm teams list/show/sync`, `bm members list/show`, `bm roles list`, `bm profiles list/describe`, `bm projects add/list/show/sync`, `bm knowledge list/show` (+ interactive mode), `bm daemon start/stop/status`, `bm completions`
- Integration test suite covering full lifecycle, hire, sync, schema guard, multi-team

**Proved**: CLI-driven team management works. Versioned profile model enables future upgrades. Workzone model provides discoverability.

---

## Planned

### Full Team + First Story

Adds dev, QE, and reviewer as team members:

- Dev, QE, reviewer member skeletons
- Full story kanban statuses in PROCESS.md
- TDD flow: QE writes tests, dev implements, QE verifies, reviewer reviews, architect signs off, PO merges
- Codebase access model (project fork, agent-cloned)
- First real knowledge accumulation

**Proves**: Pull-based coordination across all five members. TDD flow end-to-end. Knowledge accumulates and flows to the right scope.

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
