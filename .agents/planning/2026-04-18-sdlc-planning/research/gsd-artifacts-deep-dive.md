# GSD Artifact Deep Dive

Source: `/opt/workspace/get-shit-done/`, `projects/botminter/.planning/`

## Existing GSD Artifacts in BotMinter

BotMinter adopted GSD at v0.06 and used it through v0.07. The `.planning/` directory contains ~95 phase artifacts plus top-level planning documents.

### Top-Level (`.planning/`)

| File | Purpose | Living/Static |
|------|---------|---------------|
| PROJECT.md | Vision, core value, constraints, key decisions | Living — updated at phase transitions |
| REQUIREMENTS.md | Scoped requirements with IDs (AUTH-01, etc.) and traceability matrix | Living — status updated per phase |
| ROADMAP.md | Phased roadmap with goals, success criteria, plan listings | Living — progress updated per plan |
| STATE.md | Machine-readable session state, position, velocity metrics | Living — updated after every action |
| MILESTONES.md | Milestone completion log (reverse chronological) | Append-only |
| RETROSPECTIVE.md | Cross-milestone lessons learned | Append-only |
| config.json | Workflow toggles (research, plan-check, verifier, nyquist, etc.) | Set at init, rarely changed |

### Research (`research/`)

| File | Created By | Purpose |
|------|-----------|---------|
| SUMMARY.md | gsd-research-synthesizer | Executive summary, implications for roadmap, confidence |
| STACK.md | gsd-project-researcher | Tech stack with alternatives, versions, rationale |
| FEATURES.md | gsd-project-researcher | Table stakes / differentiators / anti-features |
| ARCHITECTURE.md | gsd-project-researcher | Component boundaries, patterns, anti-patterns |
| PITFALLS.md | gsd-project-researcher | Critical/moderate/minor risks with prevention |

### Per-Phase (`phases/NN-slug/`)

| File | Created By | Purpose |
|------|-----------|---------|
| NN-CONTEXT.md | discuss-phase | Locked decisions (D-01, D-02), code context, deferred ideas |
| NN-RESEARCH.md | gsd-phase-researcher | Phase-specific tech research, don't-hand-roll list, pitfalls |
| DISCOVERY.md | plan-phase | Quick library/option evaluation (optional) |
| NN-VALIDATION.md | plan-phase | Nyquist test coverage mapping per task (optional) |
| NN-MM-PLAN.md | gsd-planner | Executable task plan with YAML frontmatter + XML tasks |
| NN-MM-SUMMARY.md | gsd-executor | Post-execution outcomes, commits, deviations, decisions |
| NN-VERIFICATION.md | gsd-verifier | Goal-backward verification with 4-level artifact checks |
| NN-UAT.md | verify-work | One-at-a-time user acceptance tests with auto-diagnosis |

## Artifact Detail: What Each Contains

### PROJECT.md
- **What This Is** — 2-3 sentence product description
- **Core Value** — THE one thing that must work (drives tradeoffs)
- **Requirements** — Validated (shipped), Active (current scope), Out of Scope (with reasoning)
- **Context** — Background, prior work, known issues
- **Constraints** — Hard limits with type and rationale
- **Key Decisions** — Table: Decision | Rationale | Outcome (Good/Revisit/Pending)
- **Evolution** — Update rules

### REQUIREMENTS.md
- v1 requirements grouped by category, each with checkbox and ID (e.g., `AUTH-01`)
- v2 requirements (deferred scope, no checkboxes)
- Out of Scope table: Feature | Reason
- **Traceability matrix**: Requirement → Phase → Status (Pending/In Progress/Complete/Blocked)
- Coverage stats: total v1, mapped, unmapped

### ROADMAP.md
Per phase:
- **Goal** — Outcome-shaped, not task-shaped
- **Depends on** — Phase dependencies
- **Requirements** — Mapped REQ-IDs
- **Success Criteria** — 2-5 observable behaviors (THE verification contract)
- **Plans** — List with checkboxes
- **Progress table** — Phase | Plans Complete | Status | Date

### STATE.md (must stay under 100 lines)
- Current position (phase X of Y, plan A of B)
- Performance metrics (velocity, avg duration, trend)
- Accumulated decisions (references + recent summaries)
- Pending todos, blockers
- Session continuity (last timestamp, stopped-at, resume path)

### NN-CONTEXT.md
- **Phase Boundary** — Scope anchor from ROADMAP
- **Implementation Decisions** — Numbered (D-01, D-02), organized by topic. Includes "Claude's Discretion" areas
- **Specific Ideas** — "I want it like X"
- **Canonical References** — MANDATORY: every spec, ADR, design doc relevant to this phase with paths
- **Code Context** — Reusable assets, patterns, integration points
- **Deferred Ideas** — Captured but out of scope

### NN-RESEARCH.md
- User constraints (verbatim from CONTEXT.md)
- Summary + primary recommendation
- Standard stack (libraries, versions, alternatives)
- Architecture patterns (structure, code examples, anti-patterns)
- Don't-hand-roll list (Problem | Don't Build | Use Instead | Why)
- Common pitfalls (3+)
- Code examples from official sources
- State of the art (old vs current approaches)
- Open questions
- Confidence breakdown per area

### NN-MM-PLAN.md
YAML frontmatter:
- phase, plan, type (execute/tdd), wave
- depends_on, files_modified, autonomous, requirements
- **must_haves**: truths (observable behaviors), artifacts (path + provides + contains), key_links (from → to via)

Body (XML):
- `<objective>` — What and why
- `<context>` — @-references to project files
- `<tasks>` — 2-3 tasks, each with: name, files, read_first, action, verify (automated command), acceptance_criteria, done
- Task types: auto, checkpoint:human-verify, checkpoint:decision, checkpoint:human-action

### NN-MM-SUMMARY.md
Frontmatter:
- subsystem, tags, requires/provides/affects (dependency graph)
- tech-stack (added libs, new patterns)
- key-files (created, modified), key-decisions, patterns-established
- requirements-completed, duration

Body:
- Accomplishments, task commits (hash + type)
- Files created/modified with purpose
- Decisions made with rationale
- Deviations from plan (auto-fixed issues)
- Next phase readiness

### NN-VERIFICATION.md
4-level artifact verification:
1. EXISTS — file at path
2. SUBSTANTIVE — min_lines, exports, contains (not a stub)
3. WIRED — imported AND used by other code
4. DATA FLOWING — data source produces real data

Sections:
- Observable truths — Truth | Status (VERIFIED/FAILED/UNCERTAIN) | Evidence
- Required artifacts — path + status
- Key link verification — from → to via | status
- Data-flow trace
- Behavioral spot-checks (command + result)
- Requirements coverage
- Anti-patterns found
- Gaps summary + recommended fix plans

### NN-UAT.md
- Tests presented one at a time
- User responds naturally; severity inferred (crash=blocker, doesn't work=major, works but...=minor)
- Gaps section: truth, status, reason, severity, test number
- After diagnosis: root_cause, artifacts (path + issue), missing fixes
- Diagnosed gaps feed into `/gsd:plan-phase --gaps` for targeted fix plans

### NN-VALIDATION.md (Nyquist)
- Test infrastructure (framework, config, commands)
- Sampling rate (quick after every task, full after every wave)
- Per-task verification map: Task | Plan | Wave | Requirement | Test Type | Command | Status
- Wave 0 requirements (stubs, fixtures needed before implementation)
- Manual-only verifications

## GSD Artifact Lifecycle

```
new-project → PROJECT.md, research/*.md, REQUIREMENTS.md, ROADMAP.md, STATE.md, config.json
discuss-phase → NN-CONTEXT.md
plan-phase → NN-RESEARCH.md, NN-MM-PLAN.md, DISCOVERY.md, NN-VALIDATION.md
execute-phase → NN-MM-SUMMARY.md (updates STATE, ROADMAP, REQUIREMENTS)
verify (auto) → NN-VERIFICATION.md
verify-work (human) → NN-UAT.md
  → gaps? → plan-phase --gaps → more PLAN.md → execute → re-verify
complete-milestone → MILESTONES.md entry, archive phases
```

## Key Differences: GSD vs Current BotMinter SDLC Artifacts

| Aspect | Current SDLC | GSD |
|--------|-------------|-----|
| Design doc | Single file in team repo | Distributed: PROJECT.md + CONTEXT.md + RESEARCH.md |
| Requirements | Not tracked with IDs | Scoped with IDs, traceability matrix, coverage stats |
| Story breakdown | GitHub issue comment | PLAN.md files with YAML frontmatter, executable tasks |
| Verification | None built-in | 4 layers: plan-check, automated, UAT, milestone audit |
| Research | None | Project-level + per-phase |
| State tracking | Not tracked | STATE.md — living memory, velocity, session continuity |
| Decisions | Not recorded | CONTEXT.md decisions + PROJECT.md key decisions |
| Execution outcomes | Not recorded | SUMMARY.md with dependency graph, commits, deviations |

## Questions for Design Phase

1. Which GSD artifacts map cleanly to BotMinter's needs and which need adaptation?
2. PROJECT.md vs a standalone design doc — GSD distributes design across files while PDD consolidates. Which approach for BotMinter?
3. PLAN.md is very execution-specific (XML tasks, wave grouping). How does this relate to the "implementation plan" vs "story breakdown" question?
4. STATE.md as living memory — does BotMinter need this, or does GitHub project board + issue status replace it?
5. The Nyquist validation layer — is this level of test coverage mapping desired?
