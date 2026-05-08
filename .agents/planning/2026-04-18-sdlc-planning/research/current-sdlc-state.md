# Current BotMinter Agentic SDLC — Planning State

Source: `team/PROCESS.md`, `team/botminter.yml`, `team/members/engineer-bob/ralph.yml`

## Team Structure

Profile: **agentic-sdlc-minimal** (v1.0.0). Three roles:
- **engineer-bob** — single agent wearing all dev-lifecycle hats (PO, architect, dev, QE, lead, content writer)
- **chief-of-staff-kevin** — operational health, process evolution
- **sentinel-tom** — PR merge gatekeeper

No standalone architect role. Architecture work done by engineer-bob via hat context switches.

## Epic Lifecycle (14 statuses)

```
eng:po:triage → eng:po:backlog → eng:arch:design → eng:lead:design-review
→ human:po:design-review → eng:arch:plan → eng:lead:plan-review
→ human:po:plan-review → eng:arch:breakdown → eng:lead:breakdown-review
→ eng:po:ready → eng:arch:in-progress → human:po:accept → done
```

Three human gates: design-review, plan-review, accept.
Any status can transition to `error` after 3 failures.

## Planning Hats (worn by engineer-bob)

### arch_designer (eng:arch:design)
- Produces design doc at `team/projects/<project>/knowledge/designs/epic-<number>.md`
- Required sections (from design-quality.md invariant): Overview, Architecture, Components/Interfaces, Data Models, Error Handling, Acceptance Criteria (Given-When-Then), Impact on Existing System, Security Considerations
- Rejection-aware: checks for prior lead/po rejection feedback

### arch_planner (eng:arch:plan)
- Decomposes approved design into story breakdown
- Each story: title, description, acceptance criteria (Given-When-Then), dependencies
- Output: GitHub issue COMMENT on the epic (not a file artifact)
- Rejection-aware

### arch_breakdown (eng:arch:breakdown)
- Creates actual GitHub Task sub-issues for each story
- Sets initial status to eng:qe:test-design

### lead_reviewer (eng:lead:design-review, plan-review, breakdown-review)
- Quality gate before human review
- Self-review by same agent with different hat context

## Current Planning Artifacts

Only two:
1. **Design document** — file in team repo (`designs/epic-<N>.md`)
2. **Story breakdown** — GitHub issue comment (no file artifact)

## Identified Gaps

1. **No research/spike mechanism** — epic goes straight to design, no status for investigation
2. **Story breakdown is a comment, not a versionable file** — can't be diffed, reviewed via PR, or referenced later
3. **No estimation or sizing** — no complexity, story points, T-shirt sizing
4. **No planning completion criteria** — design has section checklist, but no "planning done" definition
5. **No knowledge accumulation** — all hat knowledge/ directories are empty; learnings from rejections not captured
6. **No ADR convention** — decisions not recorded as architectural decision records
7. **No epic-level acceptance criteria** — only story-level Given-When-Then, no epic-level definition of done
8. **Design rejection can't send back to research** — only sends back within the same phase
9. **Lead review is self-review** — same agent, acknowledged but inherently limited
10. **No capacity/sizing/risk assessment** — no mechanism to flag "too large" or "too risky"
