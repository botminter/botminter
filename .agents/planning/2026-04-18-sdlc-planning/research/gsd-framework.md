# GSD (Get Shit Done) Framework Analysis

Source: `/opt/workspace/get-shit-done/`

## What GSD Is

A meta-prompting, context engineering, and spec-driven development system for Claude Code (and other AI coding tools). Designed for solo developer + AI agents. Solves "context rot" via fresh context per phase execution.

## Core Workflow

```
new-project --> [per phase: discuss --> plan --> execute --> verify --> ship] --> audit-milestone --> complete-milestone
```

### new-project (Dream Extraction)
- Collaborative questioning ("dream extraction, not requirements gathering")
- 4 parallel researcher agents (stack, features, architecture, pitfalls) + synthesizer
- Scoped requirements with v1/v2/out-of-scope and unique IDs (AUTH-01, CONT-02)
- Phased roadmap with dependency tracking and requirement traceability

### discuss-phase (Implementation Decisions)
- Identifies "gray areas" — decisions that could go multiple ways
- User resolves ambiguity; produces CONTEXT.md with locked decisions (D-01, D-02)
- Supports "assumptions mode" where GSD reads codebase and presents assumptions for correction

### plan-phase (Research + Plan + Verify)
1. 4 parallel phase researchers
2. Planner creates 2-3 atomic PLAN.md files (XML-structured prompts with `<task>` elements)
3. Plan-checker verifies against 8 dimensions, iterates up to 3 times

### execute-phase (Parallel Wave Execution)
- Plans grouped into dependency waves, executors run in parallel with fresh context
- Atomic git commits per task
- Post-execution: verifier does goal-backward analysis

### verify-work (User Acceptance Testing)
- Extracts testable deliverables from SUMMARY.md files
- Presents tests ONE at a time — "here is what should happen. Does it?"
- Pass/issue/skip/blocked responses; severity inferred from natural language
- On failures: auto-diagnosis via debug agents, auto fix-plan creation, plan verification
- Cold-start smoke test injected if server/DB files modified

### ship (PR Creation)
- Creates PR from verified phase work

### audit-milestone / complete-milestone
- Cross-phase integration verification
- 3-source cross-reference for requirements coverage
- Tech debt and deferred gap identification

## Artifacts Produced

| Artifact | Created By | Purpose |
|----------|-----------|---------|
| PROJECT.md | new-project | Vision, core value, constraints, key decisions |
| REQUIREMENTS.md | new-project | v1/v2/out-of-scope with traceability |
| ROADMAP.md | new-project | Phased roadmap with goals, success criteria |
| STATE.md | new-project | Living memory, position, metrics |
| research/*.md | new-project | Stack, features, architecture, pitfalls, summary |
| {N}-CONTEXT.md | discuss-phase | Implementation decisions per phase |
| {N}-RESEARCH.md | plan-phase | Phase-specific research |
| {N}-{M}-PLAN.md | plan-phase | Executable task plans (XML-structured) |
| {N}-VALIDATION.md | plan-phase | Nyquist test coverage mapping |
| {N}-{M}-SUMMARY.md | execute-phase | Execution outcomes per plan |
| {N}-VERIFICATION.md | execute-phase | Goal-backward verification |
| {N}-UAT.md | verify-work | User acceptance test results |

## Verification Architecture (4 Layers)

1. **Plan Verification** (pre-execution) — plan-checker, 8 dimensions, up to 3 iterations
2. **Automated Verification** (post-execution) — goal-backward analysis, observable truths, artifact existence, wiring checks
3. **User Acceptance Testing** (human-in-the-loop) — one test at a time, auto-diagnosis on failure
4. **Milestone Audit** (cross-phase) — integration checker, 3-source requirements cross-reference

## 18 Agents

Planner runs on Opus; most others on Sonnet; codebase-mapper on Haiku.

## GSD vs PDD

| Aspect | PDD | GSD |
|--------|-----|-----|
| Design doc | Single standalone document | Distributed across PROJECT.md, CONTEXT.md, RESEARCH.md |
| Planning granularity | One implementation plan | Per-phase, XML-structured executable plans |
| Verification | None | 4 layers including UAT |
| Research | Single phase | Project-level + per-phase level |
| Execution | Not managed | Parallel wave-based agent execution |
| ADRs | Not addressed | Decisions captured in PROJECT.md + CONTEXT.md |
| Iteration | Upfront, before implementation | Per-phase, right before execution |
