# SDLC Planning & Acceptance Redesign

## Objective

Implement the `agentic-sdlc-planning` profile — a fork of `agentic-sdlc-minimal` with the Agent SOP planning pipeline (PDD + code-task-generator) baked in, adversarial review, verification, and a new status graph.

Follow the implementation plan at `.agents/planning/2026-04-18-sdlc-planning/implementation/plan.md`. Work through STEP-01 to STEP-13 in order. Each step builds on the previous one. Track progress by checking off items in `implementation/plan.md`'s checklist — that file is the progress tracker.

## Prerequisites

- The `agentic-sdlc-minimal` profile exists at `profiles/agentic-sdlc-minimal/` and is the base for the fork.
- Agent SOP skills (PDD, code-task-generator, codebase-summary) are Claude Code skills installed via the `agent-sops` plugin — they are NOT files in this repo. Source SOPs are at the paths listed in the design doc Section 7 (e.g., `agent-sops/pdd.sop.md` refers to the plugin's embedded file). When the plan says "enhance the PDD skill," it means creating a modified SOP file within the new profile's skills directory (e.g., `profiles/agentic-sdlc-planning/coding-agent/skills/pdd/`), not modifying the upstream plugin.

## Key References

- Design: `.agents/planning/2026-04-18-sdlc-planning/design/detailed-design.md`
- Implementation plan: `.agents/planning/2026-04-18-sdlc-planning/implementation/plan.md`
- Idea-honing (raw Q&A): `.agents/planning/2026-04-18-sdlc-planning/idea-honing.md` (Q1–Q22)
- Formalized requirements: design doc Section 2 (WIM-01 through OPS-02)
- Research: `.agents/planning/2026-04-18-sdlc-planning/research/`
- Base profile: `profiles/agentic-sdlc-minimal/`
- Base profile skills: `profiles/agentic-sdlc-minimal/coding-agent/skills/` (board-scanner, github-project, status-workflow)
- Base profile engineer skills: `profiles/agentic-sdlc-minimal/roles/engineer/coding-agent/skills/`



## Build Notes

### Skill file locations

The existing profile has skills at three levels:
- `profiles/agentic-sdlc-minimal/skills/` — team-level skills (knowledge-manager)
- `profiles/agentic-sdlc-minimal/coding-agent/skills/` — coding-agent-level skills (board-scanner, github-project, status-workflow)
- `profiles/agentic-sdlc-minimal/roles/engineer/coding-agent/skills/` — engineer-specific skills (member-tuning, process-evolution, retrospective, role-management, team-design)

New skills (pdd, code-task-generator, verification, adr, backward-generation) go in the engineer's coding-agent skills directory within the new profile. The profile manifest (`botminter.yml`) declares CLI extensions.

### Test expectations

- Adding a new profile (`agentic-sdlc-planning`) requires new E2E test scenarios per CLAUDE.md ("E2E test coverage per profile variation").
- Existing tests for `agentic-sdlc-minimal` MUST continue to pass — the base profile is not modified.
- `just test` (unit + conformance + e2e) and `just clippy` must pass after each step.
- `just exploratory-test` must pass after changes touching bridge, workspace, or sync.
- New profile-specific E2E tests should follow the pattern in `crates/bm/tests/e2e/`.

### What NOT to modify

- The `agentic-sdlc-minimal` profile — fork it, don't change it.
- The upstream Agent SOP plugin files — create modified copies in the new profile.
- The `scrum` profile — unrelated.

## Non-Goals

- Spike work items (deferred)
- BotMinter core abstraction changes (hub, board, identity provider)
- Full plugin system implementation (only the PDD plugin POC baked into a profile)
- Cross-model adversarial review (same-model multi-perspective only)
- Estimation, sizing, or capacity planning

## Requirements

### Work Items (WIM)

1. WIM-01: Work MUST be expressible at multiple granularity levels (epic, story, task, bug).
2. WIM-02: Users MUST be able to begin work at any level without creating parent containers.
3. WIM-03: Planning depth MUST scale proportionally to work item scope.
4. WIM-04: Every implemented change MUST pass through two human touch points — specification (before) and verification (after).
5. WIM-05: Bug fixes MUST have a lightweight path for simple fixes with escalation for complex bugs.

### Planning (PLN)

6. PLN-01: Planning artifacts MUST be producible in interactive sessions, autonomous iterations, and accept externally provided artifacts.
7. PLN-02: In autonomous mode, the agent MUST determine from the work item whether to proceed or wait. No guessing.
8. PLN-03: Same artifact types regardless of interactive or autonomous mode.
9. PLN-04: Implementation MUST NOT begin until planning artifacts exist.
10. PLN-05: Parent artifacts MUST be reusable by children.
11. PLN-06: Support forward planning and backward artifact generation.
12. PLN-07: Scope detection — offer to start at the right level; backward generation configurable per-issue.

### Verification (VER)

13. VER-01: Mechanism for user to verify implementation matches specs.
14. VER-02: Present acceptance criteria alongside implementation for assessment.
15. VER-03: Capture gaps during verification in a readable document.

### Review (REV)

16. REV-01: Planning artifacts reviewed from multiple independent perspectives before implementation.
17. REV-02: Review perspectives relevant to the artifact type.
18. REV-03: Operator can selectively accept or dismiss review feedback in interactive sessions.

### Artifacts & Traceability (ART)

19. ART-01: Artifacts stored durably in version control.
20. ART-02: Multiple discovery paths for artifacts.
21. ART-03: Every catalogable entity gets a stable ID (Q-NN, CATEGORY-NN, AC-NN, D-NN, STEP-NN).
22. ART-04: End-to-end traceability from requirements through verification.

### Operations (OPS)

23. OPS-01: Operator controls how agent-internal tasks are externalized.
24. OPS-02: Agent-internal tasks clearly distinguishable from human-facing work.

## Implementation Steps

Work through these in order. Track progress in `implementation/plan.md`'s checklist. Each step has detailed guidance, test requirements, and integration notes there.

- [ ] STEP-01: Profile Foundation — fork `agentic-sdlc-minimal`, new status graph (8 epic, 7 story, 4 bug), PROCESS.md, `team/specs/` conventions, labels
- [ ] STEP-02: PDD Skill — ID system (Q-NN, CATEGORY-NN, R-NN, AC-NN, D-NN, STEP-NN), standalone `requirements.md`, flat output structure, remove tool-specific references
- [ ] STEP-03: PDD Skill — Runtime awareness (interactive/auto mode detection), commit-after-phase for crash resilience
- [ ] STEP-04: Adversarial Review — 3 reviewer sub-agents per artifact, per-artifact-type perspectives, `lead_plan-review` hat with zero-trust quality gate
- [ ] STEP-05: PDD Skill — Traceability matrix, skill chaining to code-task-generator, downward scope detection
- [ ] STEP-06: code-task-generator Enhancements — traceability IDs, catalog README, task externalization modes, upward scope detection
- [ ] STEP-07: ADR Skill — `ADR-NNNN` management, PDD/code-task-generator integration for D-NN decisions
- [ ] STEP-08: Hat Wiring — 15 engineer hats in ralph.yml, `po_gate` for all `human:*` statuses, board scanner dispatch
- [ ] STEP-09: CLI Extension Mechanism — manifest-driven extensions in `botminter.yml`, `bm plan` as first extension
- [ ] STEP-10: Verification Skill — `bm verify` extension, conversational AC walkthrough, gap capture with `GAP-NN`
- [ ] STEP-11: Backward Generation — composite skill (codebase-summary + synthesis), `planning:backward-generate` label trigger
- [ ] STEP-12: Bug Handling — simple (story with `plan:auto`) and complex (story with full gates), `qe_investigate`/`qe_monitor` hats
- [ ] STEP-13: End-to-End Integration — full lifecycle validation across all work item types and entry levels, specs index accuracy

## Design Decisions

| ID | Decision |
|----|----------|
| D-01 | Sizing recalibrated — Epic = large body of work, Story = single deliverable |
| D-02 | Default requires human approval. `plan:auto` and `accept:auto` are opt-in labels |
| D-03 | Scope detection based on signal differences between epic-scope and story-scope |
| D-04 | Backward generation is label-triggered only (`planning:backward-generate`) |
| D-05 | Adversarial review perspectives vary by artifact type, 3 sub-agents per artifact |
| D-06 | Pipeline mapping: PDD skill at epic level, code-task-generator at story level, agent runtime (Ralph loop) at task level. Tasks are not a skill — they are what the agent does in its normal runtime. |
| D-07 | 15 engineer hats (down from 18), 8 epic statuses (down from 14). See Section 5.2 for the hat rename table. |
| D-08 | Task externalization defaults to full issues; `tasks:inline`/`tasks:off` per-issue |
| D-09 | Gap severity auto-inferred from natural language |
| D-10 | Simplified status graph — adversarial review is internal to planning, not a board status |
| D-11 | Artifacts in `team/specs/` (team repo), not project repo |
| D-12 | Bug determination: label-first → agent judgment → default-to-simple |
| D-13 | CLI extensions are manifest-driven top-level `bm` subcommands declared in `botminter.yml` |

## Acceptance Criteria

37 acceptance criteria (AC-01 through AC-36, plus AC-18a) are defined in the design doc Section 12. Key ACs per step:

| Step | Key ACs |
|------|---------|
| STEP-01 | AC-26, AC-27, AC-28, AC-29, AC-31 (partial — profile structure only) |
| STEP-02 | AC-08, AC-09 (partial — matrix structure only) |
| STEP-03 | AC-01 (partial — auto-mode artifact production) |
| STEP-04 | AC-05, AC-06, AC-07 |
| STEP-05 | AC-09 (complete), AC-10, AC-14, AC-23 |
| STEP-06 | AC-12, AC-15, AC-16, AC-17, AC-18, AC-18a, AC-22 |
| STEP-07 | supports AC-08, AC-09 |
| STEP-08 | AC-01 (complete), AC-02, AC-26–28 (complete), AC-31 (complete) |
| STEP-09 | AC-03, AC-04, AC-32 |
| STEP-10 | AC-19, AC-20, AC-21 |
| STEP-11 | AC-24, AC-25 |
| STEP-12 | AC-33, AC-34, AC-35, AC-36 |
| STEP-13 | AC-11, AC-13, AC-16 (integration verification), AC-30 |

### Regression ACs

- **REG-01:** Given `just test`, when run, then all existing tests pass (no regressions from new profile).
- **REG-02:** Given `just clippy`, when run, then zero warnings.
- **REG-03:** Given `just exploratory-test`, when run after changes touching bridge/workspace/sync, then all phases pass.

### Key ACs (GWT text for builder reference)

**AC-01:** Given an epic in `eng:lead:plan` with `plan:auto`, when `lead_plan-create` activates, then all planning artifacts are generated autonomously, internal review runs, and `po_gate` auto-advances at `human:po:plan-review`.

**AC-03:** Given `bm plan` is invoked with a rough idea, when the session starts, then the engineer activates in `lead_plan-create` hat and initiates collaborative PDD. (Verifiable only after STEP-09 delivers the CLI extension.)

**AC-05:** Given a completed planning artifact, when review triggers, then 3 adversarial sub-agents spawn in parallel with distinct per-artifact-type perspectives.

**AC-08:** Given PDD produces requirements.md, then each requirement has `CATEGORY-NN` format (3-5 uppercase chars + zero-padded sequential number).

**AC-16:** Given a `.code-task-01.md` file, when the developer hat implements it in a Ralph loop, then it produces working code with tests and atomic commits.

**AC-19:** Given `bm verify 87`, when the session starts, then ACs are loaded from planning artifacts and presented one at a time.

**AC-26:** Given the new status graph, an epic passes through: `human:po:triage` → `human:po:backlog` → `eng:lead:plan` → `human:po:plan-review` → `eng:lead:breakdown` → `eng:lead:monitor` → `human:po:accept` → `done`.

**AC-31:** Given `bm init --profile agentic-sdlc-planning`, then the engineer's ralph.yml includes `lead_plan-create`, `lead_plan-review`, and `qe_verify` hats with PDD skills pre-wired, and PROCESS.md contains the new 8-status epic lifecycle.

**AC-33:** Given a simple bug, the agent writes a regression test and implements a fix.

**AC-34:** Given a bug where the agent fails after 3 attempts, a Story issue is created linked to the original bug.
