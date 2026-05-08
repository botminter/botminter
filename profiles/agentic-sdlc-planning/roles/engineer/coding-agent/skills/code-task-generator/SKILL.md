# Code Task Generator

## Overview

This skill generates structured code task files from stories or rough descriptions. It analyzes the input, determines the task breakdown, and creates properly formatted `.code-task-NN.md` files with full traceability back to requirements and acceptance criteria. For stories that originate from a PDD implementation plan, tasks carry `CATEGORY-NN` and `AC-NN` IDs from the parent design artifacts.

Tasks produced by this skill are implemented by the agent runtime (Ralph loops) — each `.code-task-NN.md` is picked up by a developer hat and executed in a TDD cycle. This skill handles decomposition only, not implementation.

Output is stored in the team repo under `team/specs/<project>/` alongside the parent epic's planning artifacts, using issue-number-based naming for discoverability.

## Parameters

- **input** (required): Story description, file path, or PDD plan path. Can be a story issue body, a sentence/paragraph describing the work, or a path to a PDD plan.
- **step_number** (optional): For PDD plans only — specific step to process. If not provided, automatically determines the next uncompleted step from the checklist.
- **output_dir** (optional, default: `team/specs/<project>/<issue#>-<epic-slug>/tasks/<issue#>-<story-slug>/`): Directory where task files will be created. When invoked from a story with a parent epic, the default is derived from the project name, issue numbers, and slugs.
- **project** (required): BotMinter project name (code repository, e.g., `botminter`, `ralph-orchestrator`). Determines the `<project>/` segment in output paths.
- **epic_name** (optional): Epic name for organizing tasks. If processing a PDD plan, inferred from the plan path. Otherwise, generated from the description with a YYYY-MM-DD date prefix.

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
- Present task breakdown for approval before generating files (Step 4)
- Allow user to request modifications to the breakdown

**Auto mode rules:**
- Execute all actions autonomously without user confirmation
- Skip Step 4 (user approval of task breakdown) — proceed directly from planning to generation
- Document all decisions about decomposition granularity, task sequencing, and complexity assessment in the catalog README
- When multiple approaches exist, select the most appropriate and document why in the catalog README
- Provide comprehensive summaries at completion

## Steps

### 1. Detect Input Mode

Automatically determine whether input is a description or PDD plan.

**Constraints:**
- You MUST check if input is a file path that exists
- If file exists, you MUST read it and check for PDD plan structure (checklist, numbered steps with `STEP-NN` IDs)
- If file contains PDD checklist format, you MUST set mode to "pdd"
- If input is a story issue number, you MUST load the issue body from GitHub and set mode to "description"
- If input is text or file without PDD structure, you MUST set mode to "description"
- You MUST inform user which mode was detected (interactive) or log the detection (auto)
- You MUST validate that PDD plans follow expected format with `STEP-NN` numbered steps

### 2. Analyze Input

Parse and understand the input content based on detected mode.

**Constraints:**
- For PDD mode: you MUST parse implementation plan and extract steps/checklist status
- For PDD mode: you MUST determine target step based on step_number parameter or first uncompleted step
- For PDD mode: you MUST locate the parent design artifacts (`requirements.md`, `design.md`) and extract `CATEGORY-NN` and `AC-NN` IDs relevant to the target step
- For description mode: you MUST identify the core functionality being requested
- You MUST extract any technical requirements, constraints, or preferences mentioned
- You MUST determine the appropriate complexity level (Low/Medium/High)
- You MUST identify the likely technology stack or domain area

### 3. Structure Requirements

Organize requirements and determine task breakdown based on mode.

**Constraints:**
- For PDD mode: you MUST extract the target step's title, description, demo requirements, and constraints
- For PDD mode: you MUST preserve integration notes with previous steps
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
| **Interactive** | Present the assessment to the user: explain which signals were detected, and ask whether to continue with story-level decomposition or escalate to epic-level and switch to the PDD skill (`bm meetings planning`) instead |
| **Auto** | Escalate automatically — create an epic issue on the board, link the current story to it, and invoke the PDD skill (`bm meetings planning`) with the work item as input. Log the scope detection rationale in the epic body |

**Scope detection constraints:**
- You MUST list the specific signals detected when proposing a scope change
- In interactive mode: you MUST NOT auto-escalate — only the user decides whether to continue with code-task-generator or switch to PDD
- In auto mode: you MUST create an epic issue and link the story before switching to PDD
- In auto mode: you MUST log the scope detection rationale in the epic body before switching

### 4. Plan Tasks

Present task breakdown for user approval before generation.

**Constraints (interactive mode):**
- You MUST analyze content to identify logical sub-tasks for implementation
- You MUST present concise one-line summary for each planned code task
- You MUST show proposed task sequence and dependencies
- You MUST ask user to approve the plan before proceeding
- You MUST allow user to request modifications to the task breakdown
- You MUST NOT proceed to generate actual code task files until user explicitly approves

**Constraints (auto mode):**
- You MUST skip this step entirely — proceed directly to Step 5 (Generate Tasks)
- Decomposition decisions MUST be documented in the catalog README (Step 5)

### 5. Generate Tasks

Create task files, catalog README, and organize output.

**Output location:**
- When invoked from a story with a parent epic: `team/specs/<project>/<issue#>-<epic-slug>/tasks/<issue#>-<story-slug>/`
- When invoked from a standalone story (no parent epic): `team/specs/<project>/tasks/<issue#>-<story-slug>/`
- When invoked from a PDD plan: `team/specs/<project>/<issue#>-<epic-slug>/tasks/<issue#>-<story-slug>/` where the issue numbers and slugs come from the story being decomposed
- Fallback (no issue context): `team/specs/<project>/tasks/{epic_name}/`

**Folder naming:** Folders use `<issue#>-<story-slug>/` format (e.g., `42-add-oauth-endpoint/`), NOT `step{NN}/`. Since plan steps = stories, the folder name comes from the story issue, not the step number.

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

**Commit after generation:**
- After all task files and the catalog README are generated, you MUST commit them to version control
- Commit message pattern: `docs(specs): generate tasks for [story reference]`
- On failure and retry, the skill MUST detect existing task files and resume — do not overwrite existing completed tasks

**Task externalization:**

After generating task files, handle externalization based on labels on the parent story issue:

| Label | Behavior | GitHub Impact |
|-------|----------|---------------|
| *(default — no label)* | Each task becomes a GitHub issue with the `agent-internal` label. Does NOT appear in the default board view. | New issues created |
| `tasks:inline` | Tasks tracked inside the parent story issue as a structured section. No separate issues. | Story issue updated |
| `tasks:off` | Tasks exist only as `.code-task-NN.md` files in the repo. No GitHub issues or story updates. | None |

**Externalization constraints:**
- You MUST check for `tasks:inline` and `tasks:off` labels on the parent story issue before externalizing
- Default (no label): create GitHub issues with `agent-internal` label for each task
- `tasks:inline`: add a structured task catalog section to the parent story issue body
- `tasks:off`: skip externalization entirely — repo files only
- In all modes: the `.code-task-NN.md` files and catalog README are ALWAYS generated regardless of externalization mode

**ADR invocation:**
- When task decomposition surfaces an architectural decision (e.g., choosing between implementation approaches, introducing a new pattern), you MUST invoke the ADR skill to generate a formal `ADR-NNNN` document
- Pass the decision title and context (including the story reference and task context where the decision emerged) to the ADR skill
- After the ADR skill returns the `ADR-NNNN` ID, reference it in the catalog README's Decisions section
- In interactive mode: the ADR skill presents the proposed ADR to the user for review before writing
- In auto mode: the ADR skill generates the ADR autonomously — the ADR is committed alongside the task files in the commit-after-generation step

### 6. Report Results

Inform user about generated tasks and next steps.

**Constraints (all modes):**
- You MUST list all generated code task files with their paths
- You MUST report the externalization mode used and the results (issues created, story updated, or repo-only)

**Constraints (interactive mode):**
- For PDD mode: you MUST provide the step demo requirements for context
- For description mode: you MUST provide a brief summary of what was created
- You MUST offer to decompose additional stories if working through a PDD plan
- For description mode: you MUST offer to create additional related tasks if the scope seems large

**Constraints (auto mode):**
- You MUST write a completion summary to the catalog README
- You MUST NOT block or wait for human input

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
- **Parent Story**: [story reference or plan step]
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

Generated code tasks: team/specs/my-project/tasks/2026-05-08-email-validator/

Created tasks:
- .code-task-01.md — Create email validator function
- README.md — Task catalog

Externalization: tasks:off (no parent story issue)

Next steps: Tasks are ready for implementation by the developer hat.
```

### Example Input (PDD Mode)
```
input: "team/specs/my-project/plan.md"
step_number: 2
```

### Example Output (PDD Mode)
```
Detected mode: pdd

Generated code tasks for STEP-02: team/specs/my-project/15-my-epic/tasks/42-add-data-models/

Created tasks:
- .code-task-01.md — Create data models
- .code-task-02.md — Implement validation
- .code-task-03.md — Add serialization
- README.md — Task catalog (3 tasks, CATG-01 through CATG-03, AC-01 through AC-04)

Externalization: default (3 GitHub issues created with agent-internal label)

Step demo: Working data models with validation that can create, validate, and serialize/deserialize data objects

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
- You SHOULD suggest running the PDD skill (`bm meetings planning`) to generate a proper plan
- You SHOULD attempt to extract what information is available

### No Uncompleted Steps (PDD Mode)
If all steps in the checklist are marked complete:
- You SHOULD inform the user that all steps appear to be complete
- You SHOULD ask if they want to generate a task for a specific step anyway (interactive)
- You SHOULD suggest reviewing the implementation plan for potential new steps

### Existing Task Files Found
If the output directory already contains task files from a previous run:
- You MUST detect existing `.code-task-NN.md` files
- You MUST NOT overwrite existing task files — they may represent completed or in-progress work
- You SHOULD resume from the next task number if generating additional tasks
- You SHOULD inform the user about existing tasks and ask how to proceed (interactive)
