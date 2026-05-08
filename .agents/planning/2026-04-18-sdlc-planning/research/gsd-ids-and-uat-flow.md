# GSD ID System and UAT Flow Details

Source: `/opt/workspace/get-shit-done/`

## ID Generation System

### What Gets an ID

| Entity | Format | Example | Where Defined |
|--------|--------|---------|---------------|
| Requirements | `CATEGORY-NN` | AUTH-01, CONT-02, SOCL-03 | templates/requirements.md |
| Decisions | `D-NN` | D-01, D-02 | templates/context.md |
| Phases | Numeric | Phase 1, Phase 2.1 (decimal for insertions) | Positional |
| Plans | `{phase}-{plan}` | 01-01, 07-03 | Positional |
| Tasks | Sequential within plan | Task 1, Task 2 | Positional |

### Requirement ID Format

`[CATEGORY_ABBREVIATION]-[SEQUENTIAL_NUMBER]`

- Prefix: 3-5 uppercase chars, abbreviated from category heading
- Number: zero-padded two digits, sequential within category, restarts at 01 per category
- Categories derived from research FEATURES.md categories

Examples from template:
```
- [ ] AUTH-01: User can sign up with email and password
- [ ] AUTH-02: User receives email verification after signup
- [ ] PROF-01: User can create a profile with display name and avatar
- [ ] CONT-01: User can create posts with text content
```

### Where IDs Are Consumed

- ROADMAP.md — `**Requirements**: [AUTH-01, AUTH-02, PROF-01]` per phase
- PLAN.md — `requirements` frontmatter field lists covered IDs
- VERIFICATION.md — requirements coverage table checks each ID
- plan-checker — dimension 1 verifies every phase requirement has covering tasks
- roadmapper — validates 100% coverage (every req mapped to a phase)

## UAT Flow — Correction on "Two Pair of Eyes"

### What Actually Happens (Sequential, Not Comparative)

GSD's verification is two sequential layers, NOT "AI tests first, human compares":

1. **Layer 1 — Automated (VERIFICATION.md)**: gsd-verifier agent does structural codebase analysis — file existence, substantive content, wiring, data flow. No running the app.

2. **Layer 2 — Human UAT (UAT.md)**: verify-work workflow presents expected behaviors one at a time. "Show expected, ask if reality matches." Human confirms or reports issues. Claude does NOT independently test the same items.

They verify **different things**:
- AI verifies code structure and wiring (can it theoretically work?)
- Human verifies actual behavior (does it actually work?)

### UAT Presentation Flow

1. Extract testable deliverables from SUMMARY.md
2. Cold-start smoke test auto-injected if server files modified
3. Present one test at a time via structured checkpoint block
4. Human responds naturally — severity auto-inferred (never asked)
5. On completion: auto-diagnosis of issues via parallel debug agents
6. Auto-plan fixes from diagnosed gaps
7. Plan-checker verifies fix plans (max 3 iterations)

### Response Interpretation

- pass/yes/empty → pass
- skip/can't test → skip
- blocked + reason → blocked with category tag
- anything else → issue, severity inferred (crash=blocker, doesn't work=major, works but...=minor, visual=cosmetic)
