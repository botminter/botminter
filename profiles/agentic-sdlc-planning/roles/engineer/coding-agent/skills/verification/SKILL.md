# Verification

## Overview

This skill provides the second human touch point in the SDLC pipeline by walking the user through acceptance criteria for a completed work item. It loads acceptance criteria from planning artifacts, presents each criterion one at a time, records the user's assessment, and produces a `verification.md` document capturing results and any gaps.

Gaps identified during verification receive stable IDs for tracking:

| Entity | Format | Scope | Assigned By |
|--------|--------|-------|-------------|
| Gap | `GAP-NN` | Per verification session | Sequential in Step 4 |

This skill is **interactive-only** — it requires a human to assess criteria. It is invoked via `bm meetings verification`, not autonomously.

## Parameters

- **work_item** (required): Issue number or work item identifier to verify (e.g., `87`, `#87`)
- **artifact_path** (optional): Explicit path to planning artifacts. If not provided, the skill discovers artifacts automatically via the three discovery paths described in Step 1.

**Constraints for parameter acquisition:**
- You MUST have the work_item parameter before proceeding — it is required
- If work_item is not provided (e.g., the user invoked the skill without an argument), you MUST ask for it
- You MUST strip any leading `#` from the work_item to extract the numeric issue number
- You MUST NOT ask for artifact_path unless automatic discovery fails — the three discovery paths handle most cases

## Steps

### 1. Locate Planning Artifacts

Discover and load planning artifacts for the work item. The skill tries three discovery paths in order, using the first that succeeds.

**Constraints:**
- You MUST attempt discovery paths in this order:
  1. **Workspace convention:** Look for `team/specs/<issue#>-*/` directories matching the issue number. If exactly one match exists, use it. If multiple matches exist, present them to the user and ask which to use.
  2. **Team repo index:** Read `team/specs/index.md` and search for an entry matching the issue number. Extract the artifact path from the index entry.
  3. **Issue body links:** Read the GitHub issue body (using the `github-project` skill's query-issues operation with `--type single --issue <N>`) and extract any links to `team/specs/` artifact paths.
- If the user provided an explicit `artifact_path` parameter, you MUST use that path directly and skip discovery
- If all three discovery paths fail, you MUST inform the user and ask them to provide the artifact path manually
- You MUST NOT proceed to Step 2 without a valid artifact path

### 2. Load Acceptance Criteria

Extract acceptance criteria from the planning artifacts.

**Constraints:**
- You MUST look for acceptance criteria in this order:
  1. **Design document:** Read `<artifact_path>/design.md` and extract all `AC-NN` entries. These are the authoritative acceptance criteria for epics.
  2. **Story body:** If no design.md exists (story-level work items), read the GitHub issue body for acceptance criteria. Stories may have inline AC-NN criteria or GWT-formatted criteria without IDs.
  3. **Code task files:** If the artifact directory contains a `tasks/` subdirectory, check `tasks/README.md` for AC references that map tasks to acceptance criteria.
- You MUST collect all AC-NN entries with their full Given-When-Then text
- If criteria are found without AC-NN IDs (e.g., from a story body), you MUST assign temporary IDs (`AC-T01`, `AC-T02`, ...) for tracking within the verification session. Note these are session-local IDs in the verification output.
- If no acceptance criteria are found in any source, you MUST inform the user and ask whether to:
  - Proceed with free-form verification (no structured criteria)
  - Abort the session
- You MUST present a summary of loaded criteria to the user before beginning the walkthrough: total count, source document, and the first criterion preview

### 3. Present Criteria and Record Assessments

Walk the user through each acceptance criterion one at a time.

**Constraints:**
- You MUST present criteria in sequential order (AC-01, AC-02, ... or AC-T01, AC-T02, ...)
- For each criterion, you MUST display:
  - The criterion ID (e.g., `AC-01`)
  - The full Given-When-Then text
  - The progress indicator (e.g., "Criterion 3 of 12")
- You MUST ask the user for their assessment after presenting each criterion
- You MUST accept natural language responses — the user does not need to type "PASS", "SKIP", or "FAIL" explicitly
- You MUST interpret the user's response and classify it as one of:
  - **Pass:** Affirmative responses — "works", "yes", "looks good", "confirmed", "pass", "all good", "correct", "verified"
  - **Skip:** Deferral responses — "skip", "can't test", "not applicable", "n/a", "later", "don't know", "need staging", "can't verify"
  - **Fail:** Issue responses — any response describing a problem, deficiency, or unexpected behavior that is not a skip
- You MUST record the user's original natural language response as the Notes field, not your interpretation
- If the response is ambiguous (could be pass or fail), you MUST ask the user to clarify rather than guessing
- You MUST NOT batch multiple criteria — present exactly one at a time and wait for the response before presenting the next
- After the last criterion, you MUST confirm with the user that the walkthrough is complete before generating the report

### 4. Auto-Infer Gap Severity

For each criterion classified as Fail, infer the severity from the user's natural language.

**Constraints:**
- You MUST infer severity automatically — you MUST NEVER ask the user to classify severity explicitly (D-09)
- You MUST use these severity inference rules:

| Severity | Signal Words/Phrases | Examples |
|----------|---------------------|----------|
| **blocker** | crash, crashes, fatal, exception, error, breaks, broken completely, data loss, security, can't use, unusable, blocks, show-stopper, prevents | "it crashes when I click submit", "getting a fatal error", "data loss when saving" |
| **major** | doesn't work, broken, fails, incorrect, wrong, missing, not implemented, returns wrong, unexpected behavior, significant | "doesn't work at all", "returns the wrong value", "feature is missing entirely" |
| **minor** | slow, cosmetic, typo, alignment, ugly, minor, slightly off, formatting, visual, inconsistent, could be better, not ideal | "works but takes 10 seconds", "alignment is off", "there's a typo in the label" |

- When multiple severity signals appear in the same response, you MUST use the **highest** severity (blocker > major > minor)
- If no clear severity signal is present in a fail response, you MUST default to **major**
- You MUST assign each gap a sequential `GAP-NN` ID (GAP-01, GAP-02, ...) scoped to this verification session

### 5. Generate verification.md

Produce the verification report document.

**Constraints:**
- You MUST create `verification.md` in the artifact directory (`<artifact_path>/verification.md`)
- You MUST use the exact format specified below
- You MUST include all criteria in the Results table — not just failures
- You MUST include a Gaps section only if there are failures (omit the section entirely if all criteria pass or are skipped)
- You MUST include the Summary line with counts
- The Notes column for Pass criteria MUST contain `—` (em dash) unless the user provided specific observations
- The Notes column for Skip criteria MUST contain the user's reason for skipping
- The Notes column for Fail criteria MUST contain the user's original natural language description of the issue
- You MUST NOT modify or paraphrase the user's words in the Notes column

**verification.md format:**

````markdown
# Verification — [work item title]

**Work item:** #[issue number]
**Verified by:** [operator — infer from session context or use "operator"]
**Date:** [ISO 8601 date]
**Artifacts:** [artifact_path]

## Results

| AC | Criterion | Status | Notes |
|----|-----------|--------|-------|
| AC-01 | Given X, when Y, then Z | Pass | — |
| AC-02 | Given A, when B, then C | Fail | "it crashes when I click submit" |
| AC-03 | Given D, when E, then F | Skip | couldn't test — needs staging env |

## Gaps

### GAP-01: AC-02 — crashes on submit (blocker)
User reported: "it crashes when I click submit"

### GAP-02: AC-07 — slow response (minor)
User reported: "works but takes about 10 seconds"

## Summary
- Total: 12 | Passed: 9 | Failed: 2 | Skipped: 1
````

**Gap entry format:**
- The heading MUST follow: `### GAP-NN: AC-XX — <short description> (<severity>)`
- The short description MUST be derived from the user's response (3-6 words summarizing the issue)
- The body MUST contain: `User reported: "<exact user response>"`
- You MUST NOT editorialize or add your own analysis to gap entries — they capture what the user said

### 6. Update Specs Index

Update the team specs index to reflect verification status.

**Constraints:**
- You MUST read `team/specs/index.md`
- You MUST find the entry for the current work item (by issue number)
- You MUST update the status column to `verified` if all criteria passed, or `verified-with-gaps` if any failures were recorded
- If the work item is not found in the index, you MUST add a new entry with the appropriate status
- You MUST NOT modify entries for other work items

### 7. Present Summary

Summarize the verification session to the user.

**Constraints:**
- You MUST present:
  - Total criteria verified
  - Pass/Fail/Skip counts
  - Gap count and severity breakdown (if any)
  - Path to the generated `verification.md`
  - Index update confirmation
- If gaps were found, you MUST remind the user that gaps are informational — the user decides when and how to address them
- You MUST NOT automatically route gaps to other skills or create issues from gaps — the user controls what happens next

## Artifact Summary

| Artifact | Location | Content |
|----------|----------|---------|
| `verification.md` | `<artifact_path>/verification.md` | Results table, gap entries with GAP-NN IDs, summary stats |
| `index.md` (updated) | `team/specs/index.md` | Status updated to `verified` or `verified-with-gaps` |

## Examples

### Example Session

```
Operator: bm meetings verification 87

Skill: Loading acceptance criteria for issue #87...
Found 4 criteria in team/specs/87-token-refresh/design.md.

Criterion 1 of 4:
  AC-01: Given a valid refresh token, when the token endpoint is called,
  then a new access token is returned with a 200 status.

How does this look on your end?

Operator: works fine

Skill: [Recorded AC-01: Pass]

Criterion 2 of 4:
  AC-02: Given an expired refresh token, when the token endpoint is called,
  then a 401 Unauthorized is returned with an error message.

How does this look on your end?

Operator: it crashes when I click submit

Skill: [Recorded AC-02: Fail — severity: blocker (auto-inferred from "crashes")]

Criterion 3 of 4:
  AC-03: Given a revoked refresh token, when the token endpoint is called,
  then the token is rejected and the session is invalidated.

How does this look on your end?

Operator: skip — can't test without staging environment

Skill: [Recorded AC-03: Skip]

Criterion 4 of 4:
  AC-04: Given concurrent refresh requests, when both hit the endpoint,
  then only one new token is issued and the other request fails gracefully.

How does this look on your end?

Operator: works but takes about 10 seconds

Skill: [Recorded AC-04: Fail — severity: minor (auto-inferred from "slow")]

---

Verification complete for #87 — Token Refresh.

Results: 4 criteria | 1 passed | 2 failed | 1 skipped
Gaps:
  - GAP-01: AC-02 — crashes on submit (blocker)
  - GAP-02: AC-04 — slow response (minor)

Report saved to: team/specs/87-token-refresh/verification.md
Index updated: #87 status → verified-with-gaps

Gaps are informational — you decide when and how to address them.
```

### Example verification.md Output

````markdown
# Verification — Token Refresh

**Work item:** #87
**Verified by:** operator
**Date:** 2026-05-08
**Artifacts:** team/specs/87-token-refresh/

## Results

| AC | Criterion | Status | Notes |
|----|-----------|--------|-------|
| AC-01 | Given a valid refresh token, when the token endpoint is called, then a new access token is returned with a 200 status | Pass | — |
| AC-02 | Given an expired refresh token, when the token endpoint is called, then a 401 Unauthorized is returned with an error message | Fail | "it crashes when I click submit" |
| AC-03 | Given a revoked refresh token, when the token endpoint is called, then the token is rejected and the session is invalidated | Skip | can't test without staging environment |
| AC-04 | Given concurrent refresh requests, when both hit the endpoint, then only one new token is issued and the other request fails gracefully | Fail | "works but takes about 10 seconds" |

## Gaps

### GAP-01: AC-02 — crashes on submit (blocker)
User reported: "it crashes when I click submit"

### GAP-02: AC-04 — slow response (minor)
User reported: "works but takes about 10 seconds"

## Summary
- Total: 4 | Passed: 1 | Failed: 2 | Skipped: 1
````

## Troubleshooting

### No Planning Artifacts Found
If all three discovery paths fail:
- Ask the user for the explicit path to the planning artifacts
- If the work item has no planning artifacts at all (e.g., a task or bug that skipped planning), offer free-form verification where the user describes what they expected and what they observed

### No Acceptance Criteria in Artifacts
If the design doc or story has no AC-NN entries:
- Check if the issue body contains GWT-formatted criteria without formal IDs
- Check code task files for embedded acceptance criteria
- If no criteria exist anywhere, inform the user and offer to either abort or proceed with free-form verification

### User Response Ambiguity
If the user's response is unclear (e.g., "hmm" or "I'm not sure"):
- Ask a clarifying question: "Does this criterion work as expected, or is there an issue?"
- Do not guess — ambiguous responses must be clarified before recording

### Existing verification.md
If `<artifact_path>/verification.md` already exists from a previous session:
- Inform the user that a previous verification report exists
- Ask whether to overwrite with new results or abort
- You MUST NOT silently overwrite a previous verification report
