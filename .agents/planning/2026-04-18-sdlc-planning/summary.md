# Summary — SDLC Planning & Acceptance Redesign

## Artifacts Produced

```
.agents/planning/2026-04-18-sdlc-planning/
  PROMPT.md                          # Session objective and state
  rough-idea.md                      # Original idea
  idea-honing.md                     # 19 questions (Q1–Q22, non-sequential): work items, planning modes,
                                     #   verification, review, artifacts, pluggability, bugs
  research/
    current-sdlc-state.md            # R-02: gap analysis of agentic-sdlc-minimal
    gsd-framework.md                 # R-01: GSD meta-prompting system overview
    gsd-artifacts-deep-dive.md       # R-03: GSD artifact types and lifecycle
    gsd-review-agents.md             # R-04: plan-checker and cross-AI peer review
    gsd-verification-deep-dive.md    # R-05: 4-level structural verification
    gsd-ids-and-uat-flow.md          # R-07: CATEGORY-NN IDs and UAT flow
    pdd-acceptance-criteria-examples.md  # R-06: GWT format validation from 4 BotMinter PDD projects
    cli-extension-architecture.md    # R-08: manifest-driven CLI extensions via Clap
  design/
    detailed-design.md               # Full design: 13 sections, 37 acceptance criteria
                                     #   (AC-01–AC-36, AC-18a), 13 design decisions, mermaid diagrams
  implementation/
    plan.md                          # 13 implementation steps (STEP-01–STEP-13) with checklist
  summary.md                         # This file
```

## Design Overview

Redesign the planning and acceptance phases of BotMinter's agentic SDLC by creating a new profile — `agentic-sdlc-planning` — forked from `agentic-sdlc-minimal`. The profile bakes in the Agent SOP planning pipeline (PDD + code-task-generator) with enhancements for traceability, adversarial review, and verification.

### Core Concepts

1. **Multi-level work items** — Epic, Story, Task, Bug. Entry at any level without requiring parent containers.
2. **Two human touch points** — Specs in (before implementation), verification out (after implementation). Non-negotiable at every level.
3. **Planning by level** — Epic uses PDD, Story uses code-task-generator, Task is direct implementation. Each level has appropriate planning depth.
4. **Two runtime contexts** — Interactive (`bm plan`, human present) and autonomous (Ralph loop). Same PDD skill, different behavior.
5. **Adversarial review** — 3 agents with per-artifact-type perspectives after each major artifact. Distinct from existing self-review pattern.
6. **Verification** — Conversational AC walkthrough via `bm verify`. Gap capture with auto-inferred severity.
7. **Full traceability** — Q-NN → CATEGORY-NN → AC-NN → STEP-NN → GAP-NN across the entire pipeline.

### Key Changes from Current Profile

| Area | Current (agentic-sdlc-minimal) | New (agentic-sdlc-planning) |
|------|-------------------------------|-------------------------------|
| Epic statuses | 14 (3 human gates) | 8 (2 human gates) |
| Engineer hats | 18 | 15 |
| Planning skills | None (ad-hoc) | PDD, code-task-generator, verification, ADR, backward-generation |
| CLI extensions | None | `bm plan`, `bm verify` |
| Artifact storage | Ad-hoc in team repo | `team/specs/` with index and traceability IDs |
| Review | Self-review by same agent | 3 adversarial reviewers with per-artifact perspectives |
| Verification | None | Conversational AC walkthrough with gap capture |
| Bug handling | Simple/complex with dedicated hats | All bugs create Stories; complexity determines auto-advance |

## Implementation Plan Overview

13 steps building incrementally from profile foundation to end-to-end integration:

| Step | What It Delivers | Key Dependencies |
|------|-----------------|------------------|
| STEP-01 | Profile foundation: status graph, PROCESS.md, artifact conventions | None |
| STEP-02 | PDD skill: ID system, standalone requirements.md, flat structure | STEP-01 |
| STEP-03 | PDD skill: runtime awareness (interactive/auto), commit-after-phase | STEP-02 |
| STEP-04 | Adversarial review system: 3 reviewers, per-artifact perspectives | STEP-03 |
| STEP-05 | PDD skill: traceability matrix, skill chaining, scope detection | STEP-04 |
| STEP-06 | code-task-generator enhancements: IDs, catalog, scope detection | STEP-05 |
| STEP-07 | ADR skill and PDD/code-task-generator ADR integration | STEP-02–05 |
| STEP-08 | Hat wiring, po_gate, board scanner dispatch | STEP-01, 04 |
| STEP-09 | CLI extension mechanism and `bm plan` | STEP-08 |
| STEP-10 | Verification skill and `bm verify` | STEP-09 |
| STEP-11 | Backward generation skill | STEP-02 |
| STEP-12 | Bug handling: simple/complex paths | STEP-01, 08 |
| STEP-13 | End-to-end integration and artifact index validation | All |

## Areas Needing Further Refinement

1. **Agent SOP pipeline vs SDLC hierarchy** — How PDD / code-task-generator / code-assist map against epic → story → task, and which skills go to which hats, was identified during idea-honing (Q20) but deferred to the design phase. The design resolves the mapping (D-06) but the interaction between code-assist's built-in Explore → Plan → Code → Commit cycle and the TDD hat phases (`dev_implement-red/green/refactor/review`) needs validation during implementation.

2. **Adversarial reviewer perspectives** — The perspective table (D-05) defines perspectives per artifact type, but the prompt engineering for each reviewer persona needs iterative tuning. The `lead_plan-review` hat instructions are specified (Section 5.2) but the sub-agent prompts for each perspective are not.

3. **"Work backwards" heuristics** — The backward generation trigger is label-only (`planning:backward-generate`), which is deliberately conservative. The idea-honing discussed auto-detection of scope mismatches and nudging (Q21), but the heuristics for "this is bigger than the level you entered at" are not specified.

4. **Gap severity inference** — D-09 says "auto-infer from natural language." The mapping ("crashes" → blocker, "slow" → minor) needs a classification model or heuristic table. GSD's approach (R-07) is the reference.

5. **Verification skill depth** — The current scope is conversational walkthrough only. The two-pair-of-eyes model (AI verifies independently, then compares with human) and the 4-level automated structural checks (EXISTS → SUBSTANTIVE → WIRED → DATA\_FLOWING from R-05) are deferred.

6. **Plugin architecture** — The design explicitly defers this (Section 3.2). The plugin abstraction will be derived from the diff between `agentic-sdlc-minimal` and `agentic-sdlc-planning` once both exist. This is by design — not a gap.

## Next Steps

1. **Review the design document** — `design/detailed-design.md` (37 acceptance criteria incl. AC-18a, 13 design decisions, complete skill specifications)
2. **Review the implementation plan** — `implementation/plan.md` (13 steps with checklist)
3. **Begin implementation** following the checklist in the implementation plan, starting with STEP-01 (Profile Foundation)

### Handoff to Implementation

To start implementation via Ralph loop:
- `ralph run --config presets/pdd-to-code-assist.yml --prompt "Implement STEP-01 from .agents/planning/2026-04-18-sdlc-planning/implementation/plan.md"`
- `ralph run -c ralph.yml -H builtin:pdd-to-code-assist -p "Implement STEP-01 from .agents/planning/2026-04-18-sdlc-planning/implementation/plan.md"`
