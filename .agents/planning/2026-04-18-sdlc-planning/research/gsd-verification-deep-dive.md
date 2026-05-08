# GSD Verification Architecture: Planning → Verification Traceability

Source: `/opt/workspace/get-shit-done/agents/`, `/opt/workspace/get-shit-done/get-shit-done/workflows/`

## The Core Insight

GSD defines verification criteria **during planning**, and those become the invariants that UAT checks against after implementation. It's a closed loop:

```
Planning defines "done" → Implementation happens → Verification checks against "done" → Gaps feed back into planning
```

## Three Sources of Verification Criteria (Defined During Planning)

### 1. must_haves in PLAN.md Frontmatter (Primary)

Every PLAN.md has a required `must_haves` block with three sub-structures:

- **truths** — Observable behaviors from the user's perspective (e.g., "User can see existing messages"). These become the testable assertions.
- **artifacts** — File paths that must exist, with constraints: `provides`, `min_lines` (detect stubs), `exports` (symbols), `contains` (patterns)
- **key_links** — Critical wiring connections: `from` → `to` via `pattern` (regex for grep verification)

Derived using goal-backward methodology (5 steps):
1. State the goal (outcome, not task)
2. Derive observable truths (3-7, user perspective)
3. Derive required artifacts (specific files)
4. Derive required wiring (connections)
5. Identify key links (critical failure points)

**must_haves is a required field** in the PLAN.md frontmatter schema.

### 2. Success Criteria in ROADMAP.md (Contract-Level)

Each phase has 2-5 observable behaviors from the user's perspective. These are the **contract** — they override plan-level must_haves when both exist.

Success criteria are stable (set during roadmap creation). must_haves are plan-specific (each plan has its own, collectively covering the phase's success criteria).

### 3. Requirements from REQUIREMENTS.md (Traceability)

Categorized, ID-tagged requirements (AUTH-01, CONT-02) mapped to phases via a traceability matrix. Each PLAN.md has a `requirements` field listing which IDs it addresses. A coverage gate during planning ensures every requirement is claimed.

## Two Post-Execution Verification Mechanisms

### VERIFICATION.md (Automated, Code-Level)

Created by gsd-verifier agent automatically after execution.

**What it checks (4 levels):**
1. EXISTS — file at path
2. SUBSTANTIVE — real implementation, not placeholder (min_lines, contains, exports)
3. WIRED — imported AND used by other code
4. DATA FLOWING — data source produces real data, not hardcoded

**Priority cascade for what to verify:**
- Option A (preferred): must_haves from PLAN.md frontmatter
- Option B (contract override): Success Criteria from ROADMAP.md
- Option C (fallback): Derived from phase goal text

**Output:** Observable truth table, artifact table, key link table, requirements coverage, anti-pattern scan, gaps with fix suggestions.

**Statuses:** passed / gaps_found / human_needed

### UAT.md (Human, Behavioral)

Created by verify-work workflow (user-invoked).

**What it reads:** SUMMARY.md files (what was actually built), NOT PLAN.md or must_haves directly.

**Key distinction:**
- VERIFICATION.md asks: "Did the codebase achieve the **planned** goals?"
- UAT.md asks: "Does the built feature actually **work** when a human uses it?"

**How tests are presented:** One at a time. User responds naturally:
- pass/yes → pass
- skip/can't test → skip
- blocked → blocked (auto-categorized)
- anything else → issue (severity auto-inferred: crash=blocker, doesn't work=major, works but...=minor, visual=cosmetic)

**Cold-start smoke test:** Auto-injected if server/DB files were modified.

## The Gap → Fix → Re-Verify Cycle

```
1. Execute phase
2. gsd-verifier → VERIFICATION.md (automated)
   → gaps_found? → offer plan-phase --gaps
   → human_needed? → direct to UAT
3. User runs verify-work → UAT.md (human)
   → issues found → gaps appended
4. diagnose-issues → parallel debug agents
   → root causes added to UAT.md (status: diagnosed)
5. gsd-planner in gap_closure mode → fix plans
6. gsd-plan-checker verifies fix plans (max 3 iterations)
7. execute-phase --gaps-only
8. gsd-verifier re-verifies (re-verification mode)
9. Repeat if needed
```

### Gap Structure

```yaml
- truth: "expected behavior"
  status: failed
  reason: "User reported: verbatim"
  severity: blocker | major | minor | cosmetic
  test: N
  root_cause: ""        # Filled by diagnosis
  artifacts: []         # Filled by diagnosis
  missing: []           # Filled by diagnosis
```

### Re-Verification Mode

When re-verifying after gap closure:
- Failed items → full 3-level verification
- Passed items → quick regression checks only
- Tracks gaps_closed, gaps_remaining, regressions

## The Complete Traceability Chain

```
ROADMAP.md (success criteria)  ──────────────────────────┐
                                                          │
REQUIREMENTS.md (IDs, traceability) ──→ coverage gate     │
                                                          │
PLAN.md (must_haves: truths, artifacts, key_links) ──→ planning defines "done"
    │                                                     │
    ├──→ plan-checker verifies plans will achieve goal     │
    │                                                     │
    v                                                     v
SUMMARY.md (what was actually built)              VERIFICATION.md (automated)
    │                                              checks must_haves at 4 levels
    v                                                     │
UAT.md (human tests from summaries)                       │
    │                                                     │
    └──────── gaps from both ──────────→ fix plans ───→ re-verify
```

## Key Takeaway for PDD Plugin

PDD currently has no equivalent of must_haves. To create a verification layer, PDD's planning artifacts need to produce something analogous — testable acceptance criteria derived during planning that become the invariants checked after implementation. The specific format doesn't need to match GSD's YAML frontmatter, but the principle is the same: planning defines "done", verification checks against it.
