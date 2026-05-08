# GSD Review Agents Analysis

Source: `/opt/workspace/get-shit-done/agents/`, `/opt/workspace/get-shit-done/get-shit-done/workflows/`

## Plan-Checker (Primary Pre-Execution Reviewer)

The `gsd-plan-checker` is goal-backward verification of PLANs before execution. Core mindset: "Plans describe intent. You verify they deliver. A plan can have all tasks filled in but still miss the goal."

### 10 Verification Dimensions

| # | Dimension | What It Checks | Fail Severity |
|---|-----------|----------------|---------------|
| 1 | Requirement Coverage | Every requirement ID from ROADMAP appears in at least one plan's frontmatter | BLOCKER |
| 2 | Task Completeness | Every task has Files + Action + Verify + Done elements. Flags vague actions | BLOCKER |
| 3 | Dependency Correctness | Parses depends_on, builds graph, checks for cycles, missing refs, wave consistency | BLOCKER |
| 4 | Key Links Planned | Artifacts in must_haves.key_links are wired together (imports, connections) | WARNING |
| 5 | Scope Sanity | Tasks/plan (2-3 target, 5+ blocker), files/plan (5-8 target, 15+ blocker) | BLOCKER at limits |
| 6 | Verification Derivation | must_haves trace to phase goal. Truths must be user-observable, not implementation-focused | WARNING |
| 7 | Context Compliance | Plans honor locked CONTEXT.md decisions, don't implement deferred ideas | BLOCKER |
| 8 | Nyquist Compliance | Every task has automated verify commands. Flags watch modes, sampling gaps | BLOCKER |
| 9 | Cross-Plan Data Contracts | Plans sharing data pipelines don't have incompatible transformations | BLOCKER |
| 10 | CLAUDE.md Compliance | Plans respect project conventions, forbidden patterns, required tools | BLOCKER/WARNING |

### Feedback Loop

- **Max 3 iterations** (initial + 2 revision rounds)
- Auto-iterates: checker issues go back to planner as structured YAML
- Revision is targeted ("Do NOT replan from scratch unless issues are fundamental")
- Acceptance: `VERIFICATION PASSED` — all dimensions clear
- At max iterations: user chooses force-proceed, provide guidance, or abandon
- **SDK/headless mode**: simplified to 1 re-plan + 1 re-check, then proceed with warning

## Other Pre-Execution Review Agents

### Cross-AI Peer Review (`review.md` workflow)
- Invokes external AI CLIs (Gemini, Claude, Codex) independently
- Each gets same structured prompt with all planning artifacts
- Reviews combined into `REVIEWS.md` with consensus summary
- NOT auto-iterated — user must explicitly invoke `--reviews` flag to incorporate
- This is the closest to what we want for adversarial arch_reviewer agents

### UI-Checker (`gsd-ui-checker`)
- 6 dimensions: Copywriting, Visuals, Color, Typography, Spacing, Registry Safety
- PASS/FLAG/BLOCK verdicts, max 2 iterations
- Only relevant for UI-heavy phases

### Assumptions Analyzer (`gsd-assumptions-analyzer`)
- Pre-planning codebase analysis, not a gatekeeper
- Surfaces assumptions the user must confirm before planning proceeds
- Confidence tiers: Confident / Likely / Unclear

### Integration Checker (`gsd-integration-checker`)
- Cross-phase integration verification
- Checks exports→imports, API routes→consumers, auth coverage, E2E flows
- Runs at milestone audit (pre-completion), not per-phase

## Key Takeaways for BotMinter Arch Reviewer

1. GSD's plan-checker is highly specific to GSD's PLAN.md format (frontmatter fields, XML tasks, wave grouping). Not directly reusable for PDD artifacts.

2. The cross-AI peer review pattern (independent reviewers with same context, combined consensus) is the closest match to what we want for adversarial arch_reviewer agents.

3. The 3-iteration feedback loop with structured issue format is a proven pattern.

4. The 10 dimensions provide a good reference for what to check, but need adaptation:
   - Requirement coverage → do artifacts address all requirements from idea-honing?
   - Completeness → are all design sections present and substantive?
   - Scope sanity → is the design appropriately scoped?
   - Context compliance → does the design honor constraints and decisions?
   - Verification derivation → are acceptance criteria testable and user-observable?

5. GSD separates "plan quality" (plan-checker) from "plan correctness" (cross-AI review). We might want both: structural quality checks + adversarial "is this the right design?" review.
