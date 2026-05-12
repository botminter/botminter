---
name: story-mgmt
description: >-
  Manages story and task lifecycle. Use for story decomposition, task
  generation, task planning, code task creation, story management, task
  management, TDD task structuring, acceptance criteria traceability, and
  adversarial review of task breakdowns. Also triggers on "decompose a story",
  "generate tasks", "break down a story", or /story-mgmt. Do NOT use for
  epics or feature-level planning — use the epic-mgmt skill instead.
metadata:
  author: botminter
  version: 1.0.0
  category: planning
---

# Story Management

## Overview

This skill generates structured code task files from stories or rough descriptions. It analyzes the input, determines the task breakdown, and creates properly formatted `.code-task-NN.md` files with full traceability back to requirements and acceptance criteria. For stories that originate from a PDD story breakdown, tasks carry `CATEGORY-NN` and `AC-NN` IDs from the parent design artifacts.

Tasks produced by this skill are implemented by the agent runtime (Ralph loops) — each `.code-task-NN.md` is picked up by a developer hat and executed in a TDD cycle. This skill handles decomposition only, not implementation.

Output is stored in the team repo under `team/specs/<project>/` alongside the parent epic's planning artifacts, using issue-number-based naming for discoverability.

## Parameters

- **input** (required): Story description, file path, or PDD plan path. Can be a story issue body, a sentence/paragraph describing the work, or a path to a PDD plan.
- **story_number** (optional): For PDD plans only — specific story to process. If not provided, automatically determines the next uncompleted story from the checklist.
- **output_dir** (optional, default: `team/specs/<project>/<issue#>-<epic-slug>/tasks/<issue#>-<story-slug>/`): Directory where task files will be created. When invoked from a story with a parent epic, the default is derived from the project name, issue numbers, and slugs.
- **project** (required): BotMinter project name (code repository, e.g., `botminter`, `ralph-orchestrator`). Determines the `<project>/` segment in output paths.
- **epic_name** (optional): Epic name for organizing tasks. If processing a PDD plan, inferred from the plan path. Otherwise, generated from the description as a short kebab-case name.

**Constraints for parameter acquisition:**
- You MUST ask for all required parameters upfront in a single prompt rather than one at a time
- You MUST support multiple input methods for input including:
  - Direct text input
  - File path containing the description or PDD plan
  - Directory path (will look for plan.md within it)
  - Story issue number (load from GitHub)
- You MUST confirm successful acquisition of all parameters before proceeding

## Mode Behavior

This skill supports two runtime modes, determined by the context in which it is invoked:

| Mode | Trigger | Human Present | Behavior |
|------|---------|---------------|----------|
| **Interactive** | Human runs `bm meetings planning` on a story or invokes skill directly | Yes | Present task breakdown for approval, ask questions, solicit feedback |
| **Auto** | Ralph loop picks up work item from the board | No | Analyze story, determine breakdown, generate tasks without approval — decisions documented in catalog README |

**Mode detection:** At skill entry, determine the mode from the runtime context. If a human initiated the session (e.g., via `bm meetings planning` or direct conversation), use interactive mode. If the skill was invoked by a Ralph hat processing a board work item, use auto mode.

**Interactive mode rules:**
- Present proposed actions and ask for confirmation before proceeding
- When multiple approaches exist, explain pros/cons and ask for user preference
- Ask clarifying questions about ambiguous requirements
- Present task breakdown for approval before generating files (Step 5)
- Allow user to request modifications to the breakdown

**Auto mode rules:**
- Execute all actions autonomously without user confirmation
- Skip Step 5 (user approval of task breakdown) — proceed directly from verification to generation
- Document all decisions about decomposition granularity, task sequencing, and complexity assessment in the catalog README
- When multiple approaches exist, select the most appropriate and document why in the catalog README
- Provide comprehensive summaries at completion

## Adversarial Review

After task files are produced (Step 6), you MUST spawn adversarial reviewer sub-agents in parallel using the coding agent's sub-agent capability. Each reviewer adopts a distinct professional persona and reviews the task decomposition holistically from that viewpoint — they apply their full professional judgment, not a narrow topic checklist. The reviewers are internal to this skill — they are not separate hats.

**Task Decomposition personas (3 reviewers):**

| Persona | Review lens |
|---------|-------------|
| **Staff Engineer** | Reviews as a senior IC who will own the technical quality of this system long-term. Applies holistic judgment across architecture, security, maintainability, performance, and production-readiness. Connects decisions to their downstream implications — a design choice that's architecturally clean but operationally fragile gets flagged as one issue, not missed because it falls between two isolated topic reviews. |
| **UX Engineer** | Acts as the voice of every user persona who will interact with this feature — end users, operators, admins. Evaluates whether users can find it, learn it, use it, and recover from mistakes. Catches documentation gaps (missing sections, thin content, incomplete guides), poor error messages, CLI ergonomics issues, onboarding friction, and discoverability problems. The test: "when this ships, will users succeed without hand-holding?" For task decompositions, also evaluates whether user-facing behavior is decomposed in a way that enables early UX validation and doesn't defer all user-visible quality to the final task. |
| **QE Engineer** | Reviews as the engineer who will verify this feature works end-to-end. Evaluates whether the design is testable, acceptance criteria are verifiable in Given-When-Then form, edge cases are covered, the test strategy catches regressions, and observability supports debugging failures. Catches untestable requirements, missing test categories, and gaps between ACs and actual verification. |

**Review feedback format:**

Each reviewer MUST produce structured feedback in this format:

````markdown
### Review: [Persona]

**Verdict:** PASS | REVISE | BLOCK (PASS = no issues or minor issues only; REVISE = major issues present but resolvable; BLOCK = blocker issues that prevent proceeding)

**Issues:**
1. [SEVERITY: blocker|major|minor] — [description]
   **Location:** [task file / section / criterion reference]
   **Suggestion:** [concrete fix]

2. ...

**Strengths:** [what's done well — retained across revisions]
````

**All-PASS fast path:** If all 3 reviewers return PASS with no issues, skip iteration rounds and proceed directly to the next step. In interactive mode, present the clean verdicts to the human for acknowledgment. In auto mode, log the clean review in the catalog README Decisions section.

**Overlap guidance:** Persona-based reviewers may flag the same concern from different angles. This is expected and is a signal of severity — if both the Staff Engineer and QE flag weak acceptance criteria, that issue is more important than one flagged by a single reviewer. In auto mode, overlapping issues are addressed once (not duplicated); in interactive mode, all feedback is presented and the human decides.

**Iteration protocol:**

| Mode | Behavior |
|------|----------|
| **Interactive** | Present consolidated feedback from all reviewers to the human. The human selectively addresses issues (e.g., "fix #1 and #3, skip #2"). Revise only the items the human chooses to address. The human decides when the artifact is ready. |
| **Auto** | Iterate up to 3 rounds without human input. Round 1: initial review — all issues surfaced. Round 2: targeted revision — address only blocker and major issues, re-review changed sections plus regression check. Round 3: final pass — if blockers remain, reject the decomposition (see rejection behavior below). |

**Constraints:**
- You MUST spawn 3 reviewer sub-agents for every task decomposition
- Each sub-agent MUST adopt a distinct persona from the persona table above
- Persona-based reviewers review holistically — do NOT constrain them to a narrow topic checklist. The persona description defines their lens, not their scope.
- Sub-agents MUST produce feedback in the structured format specified above
- You MUST NOT skip the adversarial review for any task decomposition
- In interactive mode: you MUST present all reviewer feedback before asking the human which issues to address
- In interactive mode: you MUST NOT auto-dismiss any reviewer feedback — only the human decides what to address or dismiss
- In auto mode: you MUST NOT exceed 3 review-revision rounds
- In auto mode: in round 2, you MUST only address issues with severity `blocker` or `major`
- **Rejection behavior (auto mode):** After round 3, if any verdict is BLOCK, you MUST reject the decomposition: post a comment on the story issue listing all remaining blocker issues, move the story back to `eng:lead:plan` using the `status-workflow` skill, and exit the skill without proceeding further
- You MUST revise the task files in-place after addressing feedback (do not create new copies)
- When task files are revised, you MUST also update the catalog README to reflect any changes to task titles, complexity assessments, requirement traceability, or acceptance criteria
- If revisions are made, the revised task files are included in the commit at Step 8

## Steps

### 1. Detect Input Mode

Automatically determine whether input is a description or PDD plan.

**Constraints:**
- You MUST check if input is a file path that exists
- If file exists, you MUST read it and check for PDD plan structure (checklist, numbered stories with `STORY-NN` IDs)
- If file contains PDD checklist format, you MUST set mode to "pdd"
- If input is a story issue number, you MUST load the issue body from GitHub and set mode to "description"
- You MUST ensure a GitHub story issue exists for this work item. If invoked with an existing issue number, use that issue. Otherwise, create a new story issue using the `github-project` skill. If the story has a parent epic, link it as a sub-issue. The issue number becomes `{issue#}` for the directory name.
- If input is text or file without PDD structure, you MUST set mode to "description"
- You MUST inform user which mode was detected (interactive) or log the detection (auto)
- You MUST validate that PDD plans follow expected format with `STORY-NN` numbered stories

### 2. Analyze Input

Parse and understand the input content based on detected mode.

**Constraints:**
- For PDD mode: you MUST parse the story breakdown and extract stories/checklist status
- For PDD mode: you MUST determine target story based on story_number parameter or first uncompleted story
- For PDD mode: you MUST locate the parent design artifacts (`requirements.md`, `design.md`) and extract `CATEGORY-NN` and `AC-NN` IDs relevant to the target story
- For description mode: you MUST identify the core functionality being requested
- You MUST extract any technical requirements, constraints, or preferences mentioned
- You MUST determine the appropriate complexity level (Low/Medium/High)
- You MUST identify the likely technology stack or domain area

### 3. Structure Requirements

Organize requirements and determine task breakdown based on mode.

**Constraints:**
- For PDD mode: you MUST extract the target story's title, description, demo requirements, and constraints
- For PDD mode: you MUST preserve integration notes with previous stories
- For PDD mode: you MUST identify which specific research documents (if any) are directly relevant to each task being created
- For PDD mode: you MUST extract `CATEGORY-NN` requirement IDs and `AC-NN` acceptance criteria IDs from the parent story's design artifacts
- For description mode: you MUST identify specific functional requirements from the description
- You MUST infer reasonable technical constraints and dependencies
- You MUST create measurable acceptance criteria using Given-When-Then format
- You MUST prepare task breakdown plan for approval (interactive) or for generation (auto)

**Scope Detection (all modes):**

During requirements structuring, you MUST evaluate whether the work item is actually epic-scope rather than story-scope. Epic-scope signals:

- **Vague input:** The description is ambiguous, underspecified, or uses high-level language without concrete deliverables
- **Research needed:** The work requires investigation of unknowns, technology evaluation, or feasibility analysis
- **Architecture decisions emerging:** The decomposition surfaces design trade-offs, new patterns, or cross-component coordination
- **Multi-component:** The work spans 3 or more distinct areas, modules, or services
- **Too many tasks:** The decomposition produces more than 5 tasks
- **Open questions:** Significant questions remain unanswered that would require idea-honing to resolve

If multiple epic-scope signals are present:

| Mode | Behavior |
|------|----------|
| **Interactive** | Present the assessment to the user: explain which signals were detected, and ask whether to continue with story-level decomposition or escalate to epic-level and switch to the epic-mgmt skill (`bm meetings planning`) instead |
| **Auto** | Escalate automatically — create an epic issue on the board, link the current story to it, and invoke the epic-mgmt skill (`bm meetings planning`) with the work item as input. Log the scope detection rationale in the epic body |

**Scope detection constraints:**
- You MUST list the specific signals detected when proposing a scope change
- In interactive mode: you MUST NOT auto-escalate — only the user decides whether to continue with story-mgmt or switch to epic-mgmt
- In auto mode: you MUST create an epic issue and link the story before switching to epic-mgmt
- In auto mode: you MUST log the scope detection rationale in the epic body before switching

### 4. Verify Against Codebase

Independently verify claims in the design artifacts against the actual project codebase before planning tasks.

**Why this step exists:** PDD artifacts (design.md, plan.md) make claims about the codebase — file paths, function signatures, existing patterns, module structure, helper functions. These claims may be stale, inaccurate, or based on assumptions. Tasks generated from unverified claims produce incorrect implementation guidance that wastes developer time.

**Constraints (PDD mode — required):**
- You MUST read the actual source files referenced in the design document and story description
- You MUST verify that claimed file paths exist and contain what the design describes
- You MUST verify that claimed function signatures, struct definitions, and type names match the current codebase
- You MUST verify that claimed patterns (e.g., "atomic write pattern in module X", "helper function Y") actually exist and work as described
- You MUST verify that claimed module structure and import paths are accurate
- You MUST form your own implementation approach independently — as if you were told "implement this objective" without the PDD artifacts — and then compare it against the design's proposed approach
- You MUST document all discrepancies found between the design artifacts and the actual codebase
- You MUST use the verified understanding of the codebase — not the PDD claims — when generating task files in subsequent steps
- You SHOULD check for existing patterns in the codebase that the design may have missed or mischaracterized (e.g., two different atomic write patterns where the design assumes one)
- You SHOULD verify that helper functions referenced by the design (e.g., `config_dir()`) have the return types and behavior the design assumes

**Constraints (PDD mode — reporting):**
- In interactive mode: you MUST present a summary of verification results before proceeding to Step 5, listing confirmed claims and discrepancies found
- In auto mode: you MUST document verification results in the catalog README's Decisions section

**Verification checklist:**

| Check | How |
|-------|-----|
| File paths exist | Read each source file referenced in the design |
| Function signatures match | Compare actual signatures against design's claimed signatures |
| Module structure is accurate | List actual modules and compare against design's description |
| Helper functions behave as claimed | Read the implementation, verify return types and side effects |
| Existing patterns are correctly described | Find actual usage sites and compare |
| No better existing patterns exist | Search for similar functionality already in the codebase |

**Constraints (description mode):**
- This step is OPTIONAL for description mode since there are no PDD artifacts to verify
- You SHOULD still read relevant source files to understand the codebase context before planning tasks
- You MAY skip this step if the description is self-contained and does not reference existing code

### 5. Plan Tasks

Present task breakdown for user approval before generation.

**Constraints (interactive mode):**
- You MUST analyze content to identify logical sub-tasks for implementation
- You MUST present concise one-line summary for each planned code task
- You MUST show proposed task sequence and dependencies
- You MUST incorporate any discrepancies found during verification (Step 4) into the task descriptions
- You MUST ask user to approve the plan before proceeding
- You MUST allow user to request modifications to the task breakdown
- You MUST NOT proceed to generate actual code task files until user explicitly approves

**Constraints (auto mode):**
- You MUST skip this step entirely — proceed directly to Step 6 (Generate Tasks)
- Decomposition decisions and verification results MUST be documented in the catalog README (Step 6)

### 6. Generate Tasks

Create task files, catalog README, and organize output.

**Output location:**
- When invoked from a story with a parent epic: `team/specs/<project>/<issue#>-<epic-slug>/tasks/<issue#>-<story-slug>/`
- When invoked from a standalone story (no parent epic): `team/specs/<project>/tasks/<issue#>-<story-slug>/`
- When invoked from a PDD plan: `team/specs/<project>/<issue#>-<epic-slug>/tasks/<issue#>-<story-slug>/` where the issue numbers and slugs come from the story being decomposed
- Fallback (no issue context): `team/specs/<project>/tasks/<issue#>-<slug>/` (a story issue is created first to obtain the issue number)

**Folder naming:** Folders use `<issue#>-<story-slug>/` format (e.g., `42-add-oauth-endpoint/`). The folder name comes from the story issue number and slug.

**Constraints:**
- You MUST create `.code-task-NN.md` files within the output directory, named sequentially: `.code-task-01.md`, `.code-task-02.md`, etc.
- You MUST break down the story into logical implementation phases focusing on functional components, NOT separate testing tasks
- You MUST follow the exact format specified in the Code Task Format section below
- You MUST include comprehensive acceptance criteria that cover the main functionality
- You MUST include unit test requirements as part of the acceptance criteria for each implementation task
- You MUST NOT create separate tasks for "add unit tests" or "write tests" — testing is integrated into each functional task
- You MUST provide realistic complexity assessment and required skills
- You MUST save files to the output directory

**Catalog README:**
- You MUST generate a `README.md` in the output directory cataloging all tasks
- The README MUST include: task number, title, status (pending/in-progress/done), requirement IDs (`CATEGORY-NN`), acceptance criteria IDs (`AC-NN`), and complexity
- The README serves as the agentic legibility index for the story's decomposition

**Catalog README format:**

````markdown
# Tasks — [Story Title]

**Parent Story:** [story reference or issue link]
**Parent Epic:** [epic reference or issue link, if applicable]
**Design Doc:** [path to design.md]
**Requirements Doc:** [path to requirements.md]

## Decisions (auto mode only)

[Document decomposition decisions, granularity choices, task sequencing rationale, and complexity assessments made during autonomous generation]

## Task Catalog

| # | Title | Status | Requirements | Acceptance Criteria | Complexity |
|---|-------|--------|--------------|--------------------|----|
| 01 | [Task title] | pending | CATEGORY-NN | AC-NN | Low |
| 02 | [Task title] | pending | CATEGORY-NN, CATEGORY-NN | AC-NN, AC-NN | Medium |

## Task Sequence

[Description of task ordering and dependencies]
````

**Traceability:**
- Every `.code-task-NN.md` file MUST include a Traceability section carrying `CATEGORY-NN` requirement IDs and `AC-NN` acceptance criteria IDs from the parent story/design doc
- Every requirement ID and AC ID referenced in the parent story MUST appear in at least one task's Traceability section
- The catalog README MUST aggregate all traceability IDs for quick reference

**Index update:**
- You MUST update `team/specs/index.md` with an entry for this story. Create the file if it doesn't exist. Each entry should include the issue number, title, parent epic reference, project, and link to the task directory.

### 7. Adversarial Review

Run the adversarial review process on the generated task files before reporting results or opening a PR.

**Constraints (all modes):**
- After producing the task files (Step 6), you MUST run the adversarial review process (see Adversarial Review section)
- You MUST spawn 3 reviewer sub-agents in parallel, one for each persona: Staff Engineer, UX Engineer, QE Engineer
- Each reviewer receives all `.code-task-NN.md` files, the catalog README, the parent story context (design doc, requirements doc, story description from the plan — whichever of these exist for the current mode; in description mode, provide the original story description and any codebase context gathered in Step 4), and the verification results from Step 4 (if PDD mode)
- Reviewers evaluate the task decomposition holistically from their persona's viewpoint

**Constraints (interactive mode):**
- You MUST present consolidated feedback from all 3 reviewers to the human
- You MUST NOT auto-dismiss any reviewer feedback — only the human decides what to address or dismiss
- After the human selects which issues to address, revise the task files in-place
- You MUST complete the adversarial review before proceeding to Step 8

**Constraints (auto mode):**
- Follow the auto-mode iteration protocol defined in the Adversarial Review section (up to 3 rounds, severity filtering, rejection on persistent blockers)
- You MUST complete the adversarial review before proceeding to Step 8

### 8. Commit

Commit this story's task files to version control.

**Constraints (all modes):**
- You MUST commit all task files, the catalog README, and index update to version control
- If revisions were made during adversarial review (Step 7), the commit includes those revisions
- Commit message pattern: `docs(specs): generate tasks for [story reference]`
- On failure and retry, the skill MUST detect existing task files and resume — do not overwrite existing completed tasks

**ADR invocation:**
- When task decomposition surfaces an architectural decision (e.g., choosing between implementation approaches, introducing a new pattern), you MUST invoke the ADR skill to generate a formal `ADR-NNNN` document
- Pass the decision title and context (including the story reference and task context where the decision emerged) to the ADR skill
- After the ADR skill returns the `ADR-NNNN` ID, reference it in the catalog README's Decisions section
- In interactive mode: the ADR skill presents the proposed ADR to the user for review before writing
- In auto mode: the ADR skill generates the ADR autonomously — the ADR is committed alongside the task files

**Skill chaining (interactive mode):**

After committing, you MUST ask the user whether to decompose the next story or proceed to submit for review:
- If next story: loop back to Step 1 with the next story number. Subsequent stories commit to the same branch — no new branch is created.
- If done (or single story, or all stories decomposed): proceed to Step 9.

**Constraints (auto mode):**
- After commit, proceed directly to Step 9 (no chaining — auto mode processes one story per invocation)

### 9. Submit for Review

Move stories to `human:po:plan-review` for human approval. In interactive mode, open a PR first.

**Constraints (interactive mode):**
- You MUST open a PR on the team repo with the task files, linked to the story issue(s). Use a single PR per session, not one per story. The PR title should reference the epic (e.g., `[#1] Tasks: Tmux agent sessions`). The PR body should list all stories decomposed in this session and link to each catalog README.
- If a PR already exists on the current branch, push to the same branch and update the PR body to include the new stories.
- You MUST move all decomposed stories to `human:po:plan-review` using the `status-workflow` skill.
- You MUST inform the user that the PR is open and stories are in plan-review.

**Constraints (auto mode):**
- You MUST NOT open a PR — specs are committed directly to the team repo
- You MUST move the story to `human:po:plan-review` using the `status-workflow` skill
- You MUST post a summary comment on the story issue with links to the task catalog
- You MUST write a completion summary to the catalog README
- **END** — the skill exits here in auto mode. Steps 10–12 run after human approval in interactive mode only. In auto mode, when the `po_gate` hat detects approval at `human:po:plan-review`, it routes the story to `eng:lead:breakdown`, where the `lead_breakdown` hat externalizes the tasks (creating GitHub issues from `.code-task-NN.md` files based on story labels) and advances the story to `eng:dev:implement`.

### 10. Await Approval (Interactive Only)

Wait for human review of the PR. Auto mode ends at Step 9.

**Constraints:**
- You MUST wait for the user to review the PR
- If the user rejects (requests changes): address the feedback, revise task files in-place, re-commit, push to the PR branch, and remain in this step until the user approves
- If the user approves: proceed to Step 11

### 11. Merge PR (Interactive Only)

Merge the approved PR.

**Constraints:**
- You MUST ask the user for explicit confirmation before merging
- You MUST merge the PR using the `github-project` skill only after the user confirms
- You MUST NOT merge without user confirmation

### 12. Externalize and Advance (Interactive Only)

Externalize tasks to GitHub and advance stories to implementation. In auto mode, this is handled by the `lead_breakdown` hat at `eng:lead:breakdown` after human approval at `human:po:plan-review`.

**Task externalization:**

Handle externalization for ALL stories decomposed in this session, based on labels on each parent story issue:

| Label | Behavior | GitHub Impact |
|-------|----------|---------------|
| *(default — no label)* | Each task becomes a GitHub issue with the `agent-internal` label. Does NOT appear in the default board view. | New issues created |
| `tasks:inline` | Tasks tracked inside the parent story issue as a structured section. No separate issues. | Story issue updated |
| `tasks:off` | Tasks exist only as `.code-task-NN.md` files in the repo. No GitHub issues or story updates. | None |

**Externalization constraints:**
- You MUST check for `tasks:inline` and `tasks:off` labels on each parent story issue before externalizing
- Default (no label): create GitHub issues with `agent-internal` label for each task
- `tasks:inline`: add a structured task catalog section to the parent story issue body
- `tasks:off`: skip externalization entirely — repo files only
- The `.code-task-NN.md` files and catalog README are ALWAYS generated regardless of externalization mode
- Each story's tasks are externalized independently — different stories in the same batch MAY have different externalization modes based on their labels

**Status transition:**
- You MUST move all decomposed stories to `eng:dev:implement` using the `status-workflow` skill

**Report results:**
- You MUST list all generated code task files with their paths
- You MUST report the externalization mode used for each story and the results (issues created, story updated, or repo-only)
- For PDD mode: you MUST provide the story demo requirements for context
- For description mode: you MUST provide a brief summary of what was created

## Code Task Format Specification

Each code task file MUST follow this exact structure:

````markdown
# Task NN: [Task Name]

## Context
- **Story**: [story reference or issue link]
- **Requirements**: [CATEGORY-NN IDs this task addresses]
- **Acceptance Criteria**: [AC-NN IDs this task satisfies]
- **Design Doc**: [path to design.md]

## Objective
[What to implement — clear, bounded description of the deliverable]

## Background
[Relevant context and background information needed to understand the task]

## Reference Documentation
**Required:**
- Design: [path to detailed design document]

**Additional References (if relevant to this task):**
- [Specific research document or section]

**Note:** You MUST read the detailed design document before beginning implementation. Read additional references as needed for context.

## Technical Requirements
1. [First requirement]
2. [Second requirement]
3. [Third requirement]

## Dependencies
- [First dependency with details]
- [Second dependency with details]

## Implementation Approach
1. [First implementation step or approach]
2. [Second implementation step or approach]

## Acceptance Criteria

1. **[Criterion Name]**
   - Given [precondition]
   - When [action]
   - Then [expected result]

2. **[Another Criterion]**
   - Given [precondition]
   - When [action]
   - Then [expected result]

## Traceability
- **Requirements**: [CATEGORY-NN, CATEGORY-NN]
- **Acceptance Criteria**: [AC-NN, AC-NN]
- **Parent Story**: [story reference or issue link]
- **Design Doc**: [path to design.md]

## Metadata
- **Complexity**: [Low/Medium/High]
- **Labels**: [Comma-separated list of labels]
- **Required Skills**: [Skills needed for implementation]
````

### Code Task Format Example

````markdown
# Task 01: Create Email Validator Function

## Context
- **Story**: #42 — Add input validation to user registration
- **Requirements**: FORM-01, FORM-02
- **Acceptance Criteria**: AC-03, AC-04
- **Design Doc**: team/specs/my-project/15-user-registration/design.md

## Objective
Create a function that validates email addresses and returns detailed error messages for invalid formats. This will be used across the registration flow to ensure data quality.

## Background
The registration flow currently accepts any string as an email address, leading to data quality issues and failed communications. We need a robust validation function that can identify common email format errors and provide specific feedback to users.

## Reference Documentation
**Required:**
- Design: team/specs/my-project/15-user-registration/design.md

**Additional References (if relevant to this task):**
- team/specs/my-project/15-user-registration/research/R-02-validation-libraries.md

**Note:** You MUST read the detailed design document before beginning implementation. Read additional references as needed for context.

## Technical Requirements
1. Create a function that accepts an email string and returns validation results
2. Implement comprehensive email format validation using regex or email parsing library
3. Return detailed error messages for specific validation failures
4. Support common email formats including international domains

## Dependencies
- Email validation library or regex patterns
- Error handling framework for structured error responses

## Implementation Approach
1. Research and select appropriate email validation approach (regex vs library)
2. Implement core validation logic with specific error categorization
3. Add comprehensive error messaging for different failure types

## Acceptance Criteria

1. **Valid Email Acceptance**
   - Given a properly formatted email address
   - When the validation function is called
   - Then the function returns success with no errors

2. **Invalid Format Detection**
   - Given an email with invalid format (missing @, invalid characters, etc.)
   - When the validation function is called
   - Then the function returns failure with specific error message

3. **Unit Test Coverage**
   - Given the email validator implementation
   - When running the test suite
   - Then all validation scenarios have corresponding unit tests

## Traceability
- **Requirements**: FORM-01, FORM-02
- **Acceptance Criteria**: AC-03, AC-04
- **Parent Story**: #42 — Add input validation to user registration
- **Design Doc**: team/specs/my-project/15-user-registration/design.md

## Metadata
- **Complexity**: Low
- **Labels**: Validation, Email, Data Quality
- **Required Skills**: Regular expressions, email standards, unit testing
````

## Examples

### Example Input (Description Mode)
```
input: "I need a function that validates email addresses and returns detailed error messages"
```

### Example Output (Description Mode)
```
Detected mode: description

Generated code tasks: team/specs/my-project/tasks/23-email-validator/

Created tasks:
- .code-task-01.md — Create email validator function
- README.md — Task catalog

Externalization: tasks:off (no parent story issue)

Next steps: Tasks are ready for implementation by the developer hat.
```

### Example Input (PDD Mode)
```
input: "team/specs/my-project/15-my-epic/plan.md"
story_number: 2
```

### Example Output (PDD Mode)
```
Detected mode: pdd

Generated code tasks for STORY-02: team/specs/my-project/15-my-epic/tasks/42-add-data-models/

Created tasks:
- .code-task-01.md — Create data models
- .code-task-02.md — Implement validation
- .code-task-03.md — Add serialization
- README.md — Task catalog (3 tasks, CATG-01 through CATG-03, AC-01 through AC-04)

Externalization: default (3 GitHub issues created with agent-internal label)

Story demo: Working data models with validation that can create, validate, and serialize/deserialize data objects

Next steps: Tasks are ready for implementation. Would you like to decompose the next story?
```

## Troubleshooting

### Vague Description (Description Mode)
If the task description is too vague or unclear:
- You SHOULD ask clarifying questions about specific requirements (interactive) or document assumptions (auto)
- You SHOULD suggest common patterns or approaches for the domain
- You SHOULD create a basic task and offer to refine it based on feedback (interactive)

### Complex Description (Description Mode)
If the description suggests a very large or complex task:
- You SHOULD check for epic-scope signals (see Scope Detection in Step 3)
- You SHOULD suggest breaking it into multiple smaller tasks
- You SHOULD focus on the core functionality for the initial task
- You SHOULD offer to create additional related tasks (interactive)

### Missing Technical Details (Description Mode)
If technical implementation details are unclear:
- You SHOULD make reasonable assumptions based on common practices
- You SHOULD include multiple implementation approaches in the task
- You SHOULD note areas where the user should make technical decisions

### Plan File Not Found (PDD Mode)
If the specified plan file doesn't exist:
- You SHOULD check if the path is a directory and look for plan.md within it
- You SHOULD suggest common locations where PDD plans might be stored
- You SHOULD validate the file path format and suggest corrections

### Invalid Plan Format (PDD Mode)
If the plan doesn't follow expected PDD format:
- You SHOULD identify what sections are missing or malformed
- You SHOULD suggest running the epic-mgmt skill (`bm meetings planning`) to generate a proper plan
- You SHOULD attempt to extract what information is available

### No Uncompleted Stories (PDD Mode)
If all stories in the checklist are marked complete:
- You SHOULD inform the user that all stories appear to be complete
- You SHOULD ask if they want to generate tasks for a specific story anyway (interactive)
- You SHOULD suggest reviewing the story breakdown for potential new stories

### Existing Task Files Found
If the output directory already contains task files from a previous run:
- You MUST detect existing `.code-task-NN.md` files
- You MUST NOT overwrite existing task files — they may represent completed or in-progress work
- You SHOULD resume from the next task number if generating additional tasks
- You SHOULD inform the user about existing tasks and ask how to proceed (interactive)
