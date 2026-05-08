# Prompt-Driven Development

## Overview

This skill guides you through the process of transforming a rough idea into a detailed design document with an implementation plan and todo list. It follows the Prompt-Driven Development methodology to systematically refine your idea, conduct necessary research, create a comprehensive design, and develop an actionable implementation plan. The process is designed to be iterative, allowing movement between requirements clarification and research as needed.

All catalogable entities produced by this skill receive stable IDs for cross-referencing across the pipeline:

| Entity | Format | Scope | Assigned By |
|--------|--------|-------|-------------|
| Question | `Q-NN` | Per idea-honing session | Sequential in Step 3 |
| Requirement | `CATEGORY-NN` | Per requirements.md, restarts per category | Sequential in Step 6. Category: 3-5 uppercase chars abbreviated from heading. Number: zero-padded. |
| Research Topic | `R-NN` | Per epic | Sequential in Step 4 |
| Acceptance Criterion | `AC-NN` | Per design doc | Sequential in Step 7 |
| Decision | `D-NN` | Per design doc | Sequential in Step 7 |
| Implementation Step | `STEP-NN` | Per plan.md | Sequential in Step 8 |

## Parameters

- **rough_idea** (required): The initial concept or idea you want to develop into a detailed design
- **project** (required): Which BotMinter project (code repository) this epic is for. Corresponds to a directory under `projects/` in the workspace
- **epic_name** (optional): A short, descriptive name for the epic. If not provided, will be generated from the rough idea

The artifact directory (`{epic_dir}`) is derived, not user-specified: `team/specs/{project}/{epic_name}/`

**Constraints for parameter acquisition:**
- You MUST ask for all required parameters upfront in a single prompt rather than one at a time
- You MUST support multiple input methods including:
  - Direct input: Text provided directly in the conversation
  - File path: Path to a local file containing the rough idea
  - URL: Link to an internal resource (e.g., wiki page, design doc)
  - Other methods: You SHOULD be open to other ways the user might want to provide the idea
- You MUST use appropriate tools to access content based on the input method
- You MUST confirm successful acquisition of all parameters before proceeding
- If epic_name is not provided, You MUST generate a short kebab-case name from the rough idea, prefixed with the current date in YYYY-MM-DD format (e.g., "2026-01-30-template-manager", "2026-01-30-auth-system")
- You SHOULD save the acquired rough idea to a consistent location for use in subsequent steps
- You MUST NOT overwrite the existing epic directory because this could destroy previous work and cause data loss
- You MUST ask the operator for a different epic_name if the generated default directory already exists and has contents from a previous iteration

## Mode Behavior

This skill supports two runtime modes, determined by the context in which it is invoked:

| Mode | Trigger | Human Present | Behavior |
|------|---------|---------------|----------|
| **Interactive** | Human runs `bm meetings planning` or invokes skill directly | Yes | Ask questions, wait for answers, present options, solicit feedback at each step |
| **Auto** | Ralph loop picks up work item from the board | No | Execute autonomously — self-answer questions from available context, skip confirmation prompts, document all decisions inline |

**Mode detection:** At skill entry, determine the mode from the runtime context. If a human initiated the session (e.g., via `bm meetings planning` or direct conversation), use interactive mode. If the skill was invoked by a Ralph hat processing a board work item, use auto mode.

**Interactive mode rules:**
- Present proposed actions and ask for confirmation before proceeding
- When multiple approaches exist, explain pros/cons and ask for user preference
- Review artifacts and solicit specific feedback before moving forward
- Ask clarifying questions about ambiguous requirements
- Pause at key decision points to explain reasoning
- Adapt to user feedback and preferences

**Auto mode rules:**
- Execute all actions autonomously without user confirmation
- Document all decisions, assumptions, and reasoning directly in the produced artifacts
- When multiple approaches exist, select the most appropriate and document why in the artifact itself
- Self-answer idea-honing questions using available context: epic body, project codebase, team knowledge, existing artifacts
- Mark all self-generated answers clearly as agent-derived (e.g., `**Answer (agent-derived):**`)
- Provide comprehensive summaries at completion

## Resumability

This skill supports crash recovery and resumability. At the start of each run, check for existing artifacts from a previous incomplete run and resume from the next incomplete phase.

**Phase completion detection:** A phase is considered complete if its primary output artifact exists and is non-empty:

| Phase | Step | Completion Signal |
|-------|------|-------------------|
| Planning setup | 1 | `{epic_dir}/` directory exists with `rough-idea.md` |
| Idea-honing | 3 | `{epic_dir}/idea-honing.md` has Q-NN entries with answers |
| Research | 4 | `{epic_dir}/research/` directory contains R-NN files |
| Requirements | 6 | `{epic_dir}/requirements.md` exists with CATEGORY-NN entries |
| Design | 7 | `{epic_dir}/design.md` exists with AC-NN entries |
| Plan | 8 | `{epic_dir}/plan.md` exists with STEP-NN entries |

**Constraints:**
- You MUST check for existing artifacts at the start of Step 1 before creating the planning structure
- If artifacts from a previous run exist, you MUST identify the last completed phase and resume from the next phase
- You MUST NOT overwrite existing completed artifacts — they represent committed work from a previous phase
- You MUST inform the user (interactive) or log in the artifacts (auto) which phases are being skipped due to existing artifacts

## Commit After Phase

After completing each major phase, you MUST commit the artifacts in `{epic_dir}/` to version control with a descriptive commit message. This provides crash resilience — if the skill is interrupted, it can resume from the last committed phase.

**Commit points:**
| After Phase | Files to Commit | Commit Message Pattern |
|-------------|-----------------|----------------------|
| Idea-honing (Step 3) | `idea-honing.md` | `docs(specs): complete idea-honing for {epic_name}` |
| Research (Step 4) | `research/*.md` | `docs(specs): complete research for {epic_name}` |
| Requirements (Step 6) | `requirements.md` | `docs(specs): extract requirements for {epic_name}` |
| Design (Step 7) | `design.md` | `docs(specs): create design for {epic_name}` |
| Plan (Step 8) | `plan.md`, updated `requirements.md` (traceability) | `docs(specs): create implementation plan for {epic_name}` |

**Constraints:**
- You MUST `git add` and `git commit` the phase artifacts after each phase completes
- You MUST NOT include unrelated files in phase commits
- Commit messages MUST follow the project's commit convention
- On resumability detection (Step 1), committed artifacts are the source of truth for phase completion

## Adversarial Review

After each major planning artifact is produced (design document, implementation plan), you MUST spawn 3 adversarial reviewer sub-agents in parallel using the coding agent's sub-agent capability. Each reviewer examines the artifact from a distinct perspective tailored to the artifact type. The reviewers are internal to this skill — they are not separate hats.

**Perspectives by artifact type:**

| Artifact | Perspective 1 | Perspective 2 | Perspective 3 |
|----------|--------------|--------------|--------------|
| **Design Document** (`design.md`) | **Architecture** — Is the design sound? Separation of concerns? Scalability? Does it handle the requirements? | **Security** — Vulnerabilities? Input validation? Auth boundaries? Data exposure? | **Maintainability** — Complexity? Coupling? Will this be understandable in 6 months? Extension points? |
| **Implementation Plan** (`plan.md`) | **Scope** — Are steps appropriately sized? Any step too large to be demoable? Any step too trivial? | **Dependency Correctness** — Are step dependencies right? Missing prerequisites? Orphaned steps? Can this actually be built in this order? | **Risk** — What could go wrong at each step? Blast radius? Rollback strategy? Integration risks? |

**Review feedback format:**

Each reviewer MUST produce structured feedback in this format:

````markdown
### Review: [Perspective Name]

**Verdict:** PASS | REVISE | BLOCK

**Issues:**
1. [SEVERITY: blocker|major|minor] — [description]
   **Location:** [section/requirement/criterion reference]
   **Suggestion:** [concrete fix]

2. ...

**Strengths:** [what's done well — retained across revisions]
````

**Iteration protocol:**

| Mode | Behavior |
|------|----------|
| **Interactive** | Present consolidated feedback from all 3 reviewers to the human. The human selectively addresses issues (e.g., "fix #1 and #3, skip #2"). Revise only the items the human chooses to address. The human decides when the artifact is ready. |
| **Auto** | Iterate up to 3 rounds without human input. Round 1: initial review — all issues surfaced. Round 2: targeted revision — address only blocker and major issues, re-review changed sections plus regression check. Round 3: final pass — if blockers remain, emit rejection event with remaining issues listed. |

**Constraints:**
- You MUST spawn exactly 3 reviewer sub-agents in parallel for each reviewed artifact
- Each sub-agent MUST use a distinct perspective from the perspective table above, matched to the artifact type being reviewed
- Sub-agents MUST produce feedback in the structured format specified above
- You MUST NOT skip the adversarial review for any artifact listed in the perspective table
- In interactive mode: you MUST present all reviewer feedback before asking the human which issues to address
- In interactive mode: you MUST NOT auto-dismiss any reviewer feedback — only the human decides what to address or dismiss
- In auto mode: you MUST NOT exceed 3 review-revision rounds
- In auto mode: in round 2, you MUST only address issues with severity `blocker` or `major`
- In auto mode: after round 3, if any verdict is BLOCK, you MUST emit a rejection event listing all remaining blocker issues
- You MUST revise the artifact in-place after addressing feedback (do not create a new copy)
- If revisions are made, you MUST re-commit the revised artifact (see Commit After Phase section)

## Steps

### 0. Project Selection

Determine which BotMinter project (code repository) this epic belongs to.

**Constraints (interactive mode):**
- You MUST scan the `projects/` directory in the workspace to list available projects
- If only one project exists, auto-select it and inform the operator: "Auto-selected project `<name>` (only project in workspace)"
- If multiple projects exist, present the list and ask the operator which project this epic is for
- You MUST confirm the selected project before proceeding
- You MUST store the selected project name for use in constructing `{epic_dir}` = `team/specs/{project}/{epic_name}/`

**Constraints (auto mode):**
- You MUST derive the project from the issue's `project/<name>` label, or from context in the epic body
- If only one project exists in `projects/`, auto-select it
- If the project cannot be determined, you MUST log the ambiguity and halt with an actionable error

### 1. Create Planning Structure

Set up a directory structure to organize all planning artifacts created during the process.

**Constraints:**
- You MUST first check if `{epic_dir}/` already exists with artifacts from a previous run (see Resumability section)
- If existing artifacts are found, you MUST identify the last completed phase and skip to the next incomplete phase:
  - If `plan.md` exists with STEP-NN entries → all phases complete, proceed to Step 9
  - If `design.md` exists with AC-NN entries → resume at Step 8 (plan)
  - If `requirements.md` exists with CATEGORY-NN entries → resume at Step 7 (design)
  - If `research/` contains R-NN files → resume at Step 6 (requirements)
  - If `idea-honing.md` has Q-NN entries with answers → resume at Step 4 (research)
  - If only `rough-idea.md` exists → resume at Step 2 (process planning)
- In interactive mode: you MUST inform the user which phases are being skipped and why
- In auto mode: you MUST log the resumability detection in the first artifact produced
- You MUST create the epic directory if it doesn't already exist
- You MUST create the following files:
  - {epic_dir}/rough-idea.md (containing the provided rough idea)
  - {epic_dir}/idea-honing.md (for requirements clarification)
- You MUST create the following subdirectories:
  - {epic_dir}/research/ (directory for research notes)
- In interactive mode: you MUST notify the user when the structure has been created
- You MUST inform the user that all planning artifacts will remain available throughout the process
- You MUST explain that this will ensure all planning artifacts remain in context throughout the process

### 2. Initial Process Planning

Determine the initial approach and sequence for requirements clarification and research.

**Constraints (interactive mode):**
- You MUST ask the user if they prefer to:
  - Start with requirements clarification (default)
  - Start with preliminary research on specific topics
  - Provide additional context or information before proceeding
- You MUST adapt the subsequent process based on the user's preference
- You MUST explain that the process is iterative and the user can move between requirements clarification and research as needed
- You MUST wait for explicit user direction before proceeding to any subsequent step
- You MUST NOT automatically proceed to requirements clarification or research without user confirmation because this could lead the process in a direction the user doesn't want

**Constraints (auto mode):**
- You MUST skip this step entirely — proceed directly to Step 3 (requirements clarification) as the default sequence
- You MUST NOT prompt for user preferences

### 3. Requirements Clarification

Guide the development of a thorough specification through a series of questions. Each question receives a sequential `Q-NN` identifier for cross-referencing.

**Constraints (all modes):**
- You MUST create an empty {epic_dir}/idea-honing.md file if it doesn't already exist
- You MUST assign each question a sequential ID in `Q-NN` format (Q-01, Q-02, ..., Q-14, etc.)
- You MUST format the idea-honing.md document with clear question and answer sections, each prefixed by its Q-NN ID
- You MUST include the final chosen answer in the answer section
- You MAY include alternative options that were considered before the final decision
- You MUST continue asking questions until sufficient detail is gathered
- You SHOULD ask about edge cases, user experience, technical constraints, and success criteria
- You SHOULD adapt follow-up questions based on previous answers
- After completing this phase, you MUST commit `{epic_dir}/idea-honing.md` to version control (see Commit After Phase section)

**Constraints (interactive mode only):**
- You MUST ask ONLY ONE question at a time and wait for the user's response before asking the next question
- You MUST NOT list multiple questions for the user to answer at once because this overwhelms users and leads to incomplete responses
- You MUST NOT pre-populate answers to questions without user input because this assumes user preferences without confirmation
- You MUST NOT write multiple questions and answers to the idea-honing.md file at once because this skips the interactive clarification process
- You MUST follow this exact process for each question:
  1. Formulate a single question and assign it the next `Q-NN` ID
  2. Append the question (prefixed with its Q-NN ID) to {epic_dir}/idea-honing.md
  3. Present the question to the user in the conversation
  4. Wait for the user's complete response, which may require brief back-and-forth dialogue across multiple turns.
  5. Once you have their complete response, append the user's answer (or final decision) to {epic_dir}/idea-honing.md under the corresponding Q-NN entry
  6. Only then proceed to formulating the next question
- You MAY suggest possible answers when asking a question, but MUST wait for the user's actual response
- You MUST ensure you have the user's complete response before recording it and moving to the next question
- You MAY suggest options when the user is unsure about a particular aspect
- You MAY recognize when the requirements clarification process appears to have reached a natural conclusion
- You MUST explicitly ask the user if they feel the requirements clarification is complete before moving to the next step
- You MUST offer the option to conduct research if questions arise that would benefit from additional information
- You MUST be prepared to return to requirements clarification after research if new questions emerge
- You MUST NOT proceed with any other steps until explicitly directed by the user because this could skip important clarification steps

**Constraints (auto mode only):**
- You MUST formulate questions and self-answer them using available context: the epic body, project codebase, team knowledge, and existing artifacts
- You MUST write all questions and self-answers to {epic_dir}/idea-honing.md
- You MUST mark all self-generated answers with `**Answer (agent-derived):**` to clearly distinguish them from human-provided answers
- You MUST document the context source used for each answer (e.g., "Source: epic body", "Source: codebase analysis", "Source: team knowledge")
- You MUST NOT block or wait for human input
- You MAY formulate and answer multiple questions before writing them to the file
- You MUST proceed to the next step automatically when sufficient detail is gathered

**Scope Detection (all modes):**

After completing the initial round of requirements clarification (at least 3-5 questions answered), you MUST evaluate whether the work item is story-scope rather than epic-scope. Story-scope signals:

- **Clear single objective:** The idea describes one well-defined deliverable, not a system or set of features
- **No technology unknowns:** All technologies and patterns are established — no research phase would be needed
- **Single-step plan:** The implementation would be a single step (one PR, one deliverable), not multiple incremental steps
- **No architectural decisions:** No new patterns, no system design trade-offs, no cross-component coordination

If 3 or more story-scope signals are present:

| Mode | Behavior |
|------|----------|
| **Interactive** | Present the assessment to the user: explain which signals were detected, and ask whether to continue with full epic-level PDD or demote to story-level and switch to the code-task-generator skill instead |
| **Auto** | Demote to story-level automatically — stop the PDD pipeline and invoke the code-task-generator skill with the work item as input. Log the scope detection rationale in the work item or artifact before demoting |

**Scope detection constraints:**
- You MUST NOT trigger scope detection before at least 3 questions have been asked and answered — insufficient context leads to false positives
- You MUST list the specific signals detected when proposing a scope change
- In interactive mode: you MUST NOT auto-demote — only the user decides whether to continue with PDD or switch to code-task-generator
- In auto mode: you MUST log the scope detection rationale in the artifact before demoting

**Example idea-honing.md format (interactive mode):**

````markdown
## Q-01: What is the primary use case for this feature?

**Answer:** The primary use case is allowing team leads to create reusable document templates...

## Q-02: Who are the target users?

**Answer:** Internal team members — specifically engineering leads and project managers...

## Q-03: What existing systems does this need to integrate with?

**Answer:** It needs to integrate with the existing auth system (OAuth2) and the document storage API...
````

**Example idea-honing.md format (auto mode):**

````markdown
## Q-01: What is the primary use case for this feature?

**Answer (agent-derived):** Based on the epic body, the primary use case is allowing team leads to create reusable document templates for common deliverables.
Source: epic body

## Q-02: Who are the target users?

**Answer (agent-derived):** The target users are internal team members — specifically engineering leads and project managers, as inferred from the existing user roles in the codebase.
Source: codebase analysis (src/models/user.rs)

## Q-03: What existing systems does this need to integrate with?

**Answer (agent-derived):** It needs to integrate with the existing auth system (OAuth2) and the document storage API, based on the current architecture.
Source: codebase analysis, team knowledge
````

### 4. Research Relevant Information

Conduct research on relevant technologies, libraries, or existing code that could inform the design. Each research topic receives a sequential `R-NN` identifier.

**Constraints (all modes):**
- You MUST identify areas where research is needed based on the requirements
- You MUST document research findings in separate markdown files in the {epic_dir}/research/ directory, with filenames prefixed by the R-NN ID (e.g., `R-01-existing-code.md`, `R-02-technologies.md`)
- You MUST include mermaid diagrams when documenting system architectures, data flows, or component relationships in research
- You MUST include links to relevant references and sources when research is based on external materials (websites, documentation, articles, etc.)
- You MAY use available tools to search code, read files, or fetch web content to gather information
- You MUST summarize key findings that will inform the design
- You SHOULD cite sources and include relevant links in research documents
- After completing this phase, you MUST commit `{epic_dir}/research/` files to version control (see Commit After Phase section)

**Constraints (interactive mode only):**
- You MUST propose an initial research plan to the user, listing topics to investigate with `R-NN` IDs (R-01, R-02, etc.)
- You MUST ask the user for input on the research plan, including:
  - Additional topics that should be researched
  - Specific resources (files, websites, internal tools) the user recommends
  - Areas where the user has existing knowledge to contribute
- You MUST incorporate user suggestions into the research plan
- You MUST ask the user whether other available search tools should also be used
- You MUST periodically check with the user during the research process (these check-ins may involve brief dialogue to clarify feedback) to:
  - Share preliminary findings
  - Ask for feedback and additional guidance
  - Confirm if the research direction remains valuable
- You MUST ask the user if the research is sufficient before proceeding to the next step
- You MUST offer to return to requirements clarification if research uncovers new questions or considerations
- You MUST NOT automatically return to requirements clarification after research without explicit user direction because this could disrupt the user's intended workflow
- You MUST wait for the user to decide the next step after completing research

**Constraints (auto mode only):**
- You MUST determine the research plan autonomously based on the requirements and available context
- You MUST investigate all identified topics without waiting for human guidance
- You MUST document research decisions and topic selection rationale in the research files
- You MUST proceed to the next step automatically when research is sufficient

### 5. Iteration Checkpoint

Determine if further requirements clarification or research is needed before proceeding to design.

**Constraints (interactive mode):**
- You MUST summarize the current state of requirements and research to help the user make an informed decision
- You MUST explicitly ask the user if they want to:
  - Proceed to extracting requirements into a standalone document
  - Return to requirements clarification based on research findings
  - Conduct additional research based on requirements
- You MUST support iterating between requirements clarification and research as many times as needed
- You MUST ensure that both the requirements and research are sufficiently complete before proceeding to the requirements extraction step
- You MUST NOT proceed to the requirements extraction step without explicit user confirmation because this could skip important refinement steps

**Constraints (auto mode):**
- You MUST autonomously assess whether requirements and research are sufficiently complete
- If sufficient: proceed directly to Step 6 (extracting requirements)
- If gaps remain: iterate — return to Step 3 or Step 4 as needed, then re-evaluate
- You MUST document the assessment rationale in the next artifact produced

### 6. Extract Requirements

Extract and formalize requirements from the idea-honing Q&A into a standalone `requirements.md` document with categorized, uniquely identified requirements.

**Constraints (all modes):**
- You MUST create {epic_dir}/requirements.md
- You MUST read all Q-NN entries in {epic_dir}/idea-honing.md and extract requirement statements
- You MUST organize requirements into categories based on functional area or concern
- You MUST assign each category a 3-5 uppercase character abbreviation derived from the category heading (e.g., AUTH for Authentication, FORM for Form Handling, PLG for Pluggability, WIM for Work Items)
- You MUST assign each requirement a `CATEGORY-NN` ID with zero-padded sequential numbers within each category (e.g., AUTH-01, AUTH-02, FORM-01)
- After completing this phase, you MUST commit `{epic_dir}/requirements.md` to version control (see Commit After Phase section)
- You MUST use the following document structure:

````markdown
# Requirements — [Epic Name]

## [Category Name] (CATG)

| ID | Requirement | Priority | Source |
|----|-------------|----------|--------|
| CATG-01 | [Requirement text using MUST/SHOULD/MAY] | must-have | Q-03, Q-05 |
| CATG-02 | ... | should-have | Q-07 |

## [Another Category] (ANTH)

| ID | Requirement | Priority | Source |
|----|-------------|----------|--------|
| ANTH-01 | ... | must-have | Q-01 |

## Traceability Matrix

| Requirement | Acceptance Criteria | Implementation Step | Status |
|-------------|--------------------|--------------------|--------|
| CATG-01 | — | — | Pending |
````

- The **Source** column MUST reference the `Q-NN` IDs from idea-honing.md that informed the requirement
- **Priority** MUST be one of: `must-have`, `should-have`, `could-have`, `wont-have`
- Requirement text MUST use RFC 2119 keywords (MUST, SHOULD, MAY) to express obligation level
- The **Traceability Matrix** at the bottom MUST list every requirement ID. The Acceptance Criteria and Implementation Step columns are initially `—` (populated in later steps when the design doc and plan are produced)
**Constraints (interactive mode only):**
- You MUST present the extracted requirements to the user for review
- You MUST iterate on the requirements based on user feedback
- You MUST NOT proceed to the design step without explicit user confirmation
- You MUST offer to return to requirements clarification if gaps are identified during extraction

**Constraints (auto mode only):**
- You MUST extract and categorize all requirements autonomously
- You MUST proceed directly to Step 7 after requirements extraction is complete
- You MUST NOT block or wait for human review

### 7. Create Detailed Design

Develop a comprehensive design document based on the requirements and research. The design document references requirements by their `CATEGORY-NN` IDs from requirements.md rather than duplicating requirement text.

**Constraints (all modes):**
- You MUST create a detailed design document at {epic_dir}/design.md
- After completing this phase, you MUST commit `{epic_dir}/design.md` to version control (see Commit After Phase section)
- You MUST write the design as a standalone document that can be understood without reading other planning artifacts
- You MUST include the following sections in the design document:
  - Overview
  - Requirements Summary (reference requirements.md by CATEGORY-NN IDs — do NOT duplicate full requirement text)
  - Architecture Overview
  - Components and Interfaces
  - Data Models
  - Error Handling
  - Acceptance Criteria (each tagged with `AC-NN` ID — AC-01, AC-02, etc.)
  - Design Decisions (each tagged with `D-NN` ID — D-01, D-02, etc.)
  - Testing Strategy
  - Appendices (Technology Choices, Research Findings, Alternative Approaches)
  - Traceability Matrix (LAST section — maps requirements to acceptance criteria and implementation steps)
- You MUST assign each acceptance criterion a sequential `AC-NN` ID (AC-01, AC-02, ...). Acceptance criteria MUST be in Given-When-Then (GWT) format and reference the `CATEGORY-NN` requirement(s) they verify.
- You MUST assign each design decision a sequential `D-NN` ID (D-01, D-02, ...). Each decision MUST include the chosen option, alternatives considered, and rationale.
- The Requirements Summary section MUST reference requirements.md by CATEGORY-NN IDs and MUST NOT duplicate the full requirement text. A brief paraphrase or the requirement title is acceptable for readability, but the authoritative text lives in requirements.md.
- You MUST include an appendix section that summarizes key research findings, including:
  - Major technology choices with pros and cons
  - Existing solutions analysis
  - Alternative approaches considered
  - Key constraints and limitations identified during research
- You SHOULD include diagrams or visual representations when appropriate using mermaid syntax
- You MUST generate mermaid diagrams for architectural overviews, data flow, and component relationships
- You MUST ensure the design addresses all requirements identified in requirements.md
- You SHOULD highlight design decisions and their rationales, referencing research findings where applicable
- You MUST include a Traceability Matrix as the LAST section of the design document (after Appendices)
- The Traceability Matrix MUST list every requirement (`CATEGORY-NN`) from requirements.md, mapped to the acceptance criteria (`AC-NN`) defined in this design document that verify it
- The **Implementation Step** column MUST initially contain `—` (populated in Step 8 after the plan is produced)
- The **Verification Status** column MUST initially contain `Pending`
- Every requirement ID in requirements.md MUST appear in the matrix — a missing requirement indicates a gap in the design that must be addressed before proceeding

**Example traceability matrix format (in design.md):**

````markdown
## Traceability Matrix

| Requirement | Acceptance Criteria | Implementation Step | Verification Status |
|-------------|--------------------|--------------------|---------------------|
| AUTH-01 | AC-01, AC-02 | — | Pending |
| AUTH-02 | AC-03 | — | Pending |
| TMPL-01 | AC-04, AC-05 | — | Pending |
````

**Constraints (interactive mode only):**
- You MUST review the design with the user and iterate based on feedback
- You MUST explicitly ask the user if they are ready to proceed to implementation before moving to Step 8
- You MUST NOT proceed to the implementation plan step without explicit user confirmation because this could skip important design refinement
- You MUST offer to return to requirements clarification or research if gaps are identified during design

**Constraints (auto mode only):**
- You MUST produce the complete design document autonomously
- You MUST document all design decisions and their rationale inline in the document
- You MUST NOT block or wait for human review — the human reviews the complete output at the plan-review gate

**Adversarial Review (design document):**
- After producing and committing the design document, you MUST run the adversarial review process (see Adversarial Review section)
- Use the **Design Document** perspectives: Architecture, Security, Maintainability
- Apply the iteration protocol matching the current mode (interactive or auto)
- If revisions are made, re-commit `{epic_dir}/design.md` with message `docs(specs): revise design after review for {epic_name}`
- In interactive mode: complete the adversarial review before asking the user to proceed to Step 8
- In auto mode: complete the adversarial review (up to 3 rounds) before proceeding to Step 8 — if blockers remain after 3 rounds, emit a rejection event

**Example acceptance criterion format:**

````markdown
**AC-01:** Given [precondition referencing CATG-NN], when [action], then [observable outcome].
````

**Example decision format:**

````markdown
**D-01:** [Decision title]
- **Chosen:** [option]
- **Alternatives:** [other options considered]
- **Rationale:** [why this option was chosen, referencing R-NN research if applicable]
- **ADR:** ADR-NNNN (generated by ADR skill)
````

**ADR generation:**
- When an architectural decision (`D-NN`) is made during design, you MUST invoke the ADR skill to generate a formal `ADR-NNNN` document
- The `D-NN` entry in the design document is the lightweight inline record; the ADR skill produces the full formal document with context, decision rationale, alternatives, and consequences
- Pass the decision title and context (including the `D-NN` ID and the design document path) to the ADR skill
- After the ADR skill returns the `ADR-NNNN` ID, you MUST add an `**ADR:** ADR-NNNN` line to the corresponding `D-NN` entry in the design document
- ADRs are generated in `team/specs/adrs/` (the ADR skill's default directory)
- In interactive mode: the ADR skill presents the proposed ADR to the user for review before writing
- In auto mode: the ADR skill generates the ADR autonomously — all ADRs are committed as part of the design document commit-after-phase

### 8. Develop Implementation Plan

Create a structured implementation plan with a series of steps for implementing the design. Each step receives a sequential `STEP-NN` identifier.

**Constraints (all modes):**
- You MUST create an implementation plan at {epic_dir}/plan.md
- After completing this phase, you MUST commit `{epic_dir}/plan.md` and the updated `{epic_dir}/requirements.md` (traceability matrix) to version control (see Commit After Phase section)
- You MUST include a checklist at the beginning of the plan.md file to track implementation progress
- You MUST use the following specific instructions when creating the implementation plan:
  ```
  Convert the design into a series of implementation steps that will build each component in a test-driven manner following agile best practices. Each step must result in a working, demoable increment of functionality. Prioritize best practices, incremental progress, and early testing, ensuring no big jumps in complexity at any stage. Make sure that each step builds on the previous steps, and ends with wiring things together. There should be no hanging or orphaned code that isn't integrated into a previous step.
  ```
- You MUST assign each step a sequential `STEP-NN` ID (STEP-01, STEP-02, etc.)
- You MUST format the implementation plan as a numbered series of detailed steps, each prefixed with its STEP-NN ID
- Each step in the plan MUST be written as a clear implementation objective
- Each step MUST begin with "STEP-NN:" where NN is the zero-padded sequential number
- You MUST write each step as a **story** — the format MUST be directly usable by the breakdown hat to create a GitHub story issue without transformation. Each step maps 1:1 to a story issue.
- You MUST ensure each step includes:
  - **Title** — a concise, issue-title-ready summary (e.g., "Add OAuth2 token refresh endpoint")
  - **Objective** — what this story delivers as a working increment
  - **Implementation Guidance** — general approach, not excessive detail (the design doc has the details)
  - **Test Requirements** — what tests are written as part of this story
  - **Integration** — how this story connects to previous and subsequent stories
  - **Demo** — explicit description of the working functionality that can be demonstrated after completing this story
  - **Requirements** — which `CATEGORY-NN` requirement(s) this story addresses
  - **Acceptance Criteria** — which `AC-NN` criterion/criteria this story satisfies
  - **Dependencies** — which `STEP-NN` stories must be complete before this one can start (use `—` if none)
- You MUST ensure each step results in working, demoable functionality that provides value
- You MUST sequence steps so that core end-to-end functionality is available as early as possible
- You MUST NOT include excessive implementation details that are already covered in the design document because this creates redundancy and potential inconsistencies
- You MUST assume that all context documents (requirements, design, research) will be available during implementation
- You MUST break down the implementation into a series of discrete, manageable steps
- You MUST ensure each step builds incrementally on previous steps
- You MUST structure each step so that tests are written before or alongside the implementation code
- You MUST include test requirements as part of each step that introduces or modifies functionality, not as separate testing-only steps
- You MUST NOT create steps that are solely dedicated to testing or "adding tests" for functionality implemented in previous steps because this violates test-driven development principles and allows untested code to accumulate
- You MUST ensure the plan covers all aspects of the design
- You SHOULD sequence steps to validate core functionality early
- You MUST ensure the checklist items correspond directly to the steps in the implementation plan, using the STEP-NN IDs
- After the plan is finalized, you MUST update the Traceability Matrix in requirements.md: fill in the **Acceptance Criteria** column with the `AC-NN` IDs from the design doc and the **Implementation Step** column with the `STEP-NN` IDs from the plan for each requirement
- After the plan is finalized, you MUST also update the Traceability Matrix in design.md: fill in the **Implementation Step** column with the `STEP-NN` IDs from the plan for each requirement

**Constraints (auto mode only):**
- You MUST produce the complete implementation plan autonomously without user confirmation

**Adversarial Review (implementation plan):**
- After producing and committing the implementation plan, you MUST run the adversarial review process (see Adversarial Review section)
- Use the **Implementation Plan** perspectives: Scope, Dependency Correctness, Risk
- Apply the iteration protocol matching the current mode (interactive or auto)
- If revisions are made, re-commit `{epic_dir}/plan.md` and updated `{epic_dir}/requirements.md` (traceability matrix) with message `docs(specs): revise plan after review for {epic_name}`
- In interactive mode: complete the adversarial review before proceeding to Step 9
- In auto mode: complete the adversarial review (up to 3 rounds) before proceeding to Step 9 — if blockers remain after 3 rounds, emit a rejection event

### 9. Summarize and Present Results

Provide a summary of all artifacts created and next steps.

**Constraints (all modes):**
- You MUST create a summary document at {epic_dir}/summary.md
- You MUST list all artifacts created during the process with their ID counts:
  - Number of questions (Q-NN) in idea-honing.md
  - Number of requirements (CATEGORY-NN) in requirements.md, broken down by category
  - Number of research topics (R-NN)
  - Number of acceptance criteria (AC-NN) in design.md
  - Number of decisions (D-NN) in design.md
  - Number of implementation steps (STEP-NN) in plan.md
- You MUST provide a brief overview of the design and implementation plan
- You MUST suggest next steps for the user
- You SHOULD highlight any areas that may need further refinement

**Constraints (interactive mode only):**
- You MUST present this summary to the user in the conversation

**Constraints (auto mode only):**
- You MUST write the summary to `{epic_dir}/summary.md` without presenting it conversationally
- You MUST provide a brief completion notice documenting the total artifact counts and any areas flagged for human review

**Skill Chaining (interactive mode only):**

After presenting the summary, you MUST offer downstream actions based on the completed plan:

- You MUST ask the user whether to:
  1. **Create story issues** on the board from the plan steps (each `STEP-NN` maps 1:1 to a story issue)
  2. **Chain into code-task-generator** for task decomposition of one or more stories
  3. **Stop here** — the user will handle story creation and decomposition manually or via other workflows
- If the user chooses to create stories AND decompose tasks, you MUST ask about sequencing:
  - **All stories first:** Create all story issues from plan steps, then decompose each story into tasks
  - **Story-by-story:** Create one story issue, decompose it into tasks, then move to the next story
- You MUST pass the relevant planning context (design.md path, requirements.md path, `CATEGORY-NN` and `AC-NN` IDs) when chaining into code-task-generator
- You MUST NOT auto-chain without explicit user confirmation

**Skill Chaining (auto mode only):**

- You MUST NOT chain into code-task-generator or create story issues — planning is complete
- Ralph hat transitions handle downstream work (breakdown hat creates stories, developer hats decompose and implement)
- You MUST note in the summary that story creation and task decomposition are deferred to downstream hats

## Artifact Summary

The complete PDD pipeline produces the following artifacts:

| Artifact | Location | IDs | Content |
|----------|----------|-----|---------|
| `rough-idea.md` | {epic_dir}/ | — | The original idea verbatim |
| `idea-honing.md` | {epic_dir}/ | Q-NN | Question-and-answer pairs from requirements clarification |
| `requirements.md` | {epic_dir}/ | CATEGORY-NN | Standalone requirements organized by category with traceability matrix |
| `research/*.md` | {epic_dir}/research/ | R-NN | Research notes organized by topic |
| `design.md` | {epic_dir}/ | AC-NN, D-NN | Design document referencing requirements by CATEGORY-NN, with traceability matrix |
| `plan.md` | {epic_dir}/ | STEP-NN | Implementation steps, each referencing CATEGORY-NN and AC-NN |
| `summary.md` | {epic_dir}/ | — | Summary listing all artifacts and next steps |

## Examples

### Example Input

```
Rough idea: I want to build a feature for our team's internal tool that allows users to create and manage templates for common documents we produce. Users should be able to create, edit, and share templates, as well as use them to generate new documents with custom fields.

Project: my-project
Epic name: template-feature
```

### Example Output

```
# Research Phase Interaction

Based on your requirements, I've identified several areas that would benefit from research:

1. R-01: **Existing template solutions** — Understanding what's already available
2. R-02: **Storage and versioning approaches** — How to handle template versions and sharing
3. R-03: **Custom field validation patterns** — Best practices for dynamic field validation

I notice you have several additional search tools available. Should I incorporate these additional search tools into the research process for broader coverage of template management solutions and industry best practices?

---

# Planning Summary

I've completed the transformation of your rough idea into a detailed design with an implementation plan. Here's what was created:

## Directory Structure
- team/specs/my-project/template-feature/
  - rough-idea.md (your initial concept)
  - idea-honing.md (Q-01 through Q-12 — our requirements clarification)
  - requirements.md (AUTH-01 through AUTH-03, TMPL-01 through TMPL-05, SHARE-01 through SHARE-02)
  - research/
    - R-01-existing-templates.md
    - R-02-storage-options.md
    - R-03-external-solutions.md
  - design.md (AC-01 through AC-08, D-01 through D-03)
  - plan.md (STEP-01 through STEP-12, includes implementation checklist)
  - summary.md (this document)

## Key Design Elements
- Template management system with CRUD operations
- Role-based access control for sharing
- Versioning system for templates
- Custom fields with validation
- Document generation engine

## Implementation Approach
The implementation plan breaks down the work into 12 incremental steps (STEP-01 through STEP-12), starting with core data models and building up to the complete feature set. Each step references the requirements (CATEGORY-NN) and acceptance criteria (AC-NN) it addresses.

## Next Steps
1. Review the detailed design document at team/specs/my-project/template-feature/design.md
2. Check the implementation plan and checklist at team/specs/my-project/template-feature/plan.md
3. Begin implementation following the checklist in the implementation plan

Would you like me to explain any specific part of the design or implementation plan in more detail?
```

## Troubleshooting

### Requirements Clarification Stalls
If the requirements clarification process seems to be going in circles or not making progress:
- You SHOULD suggest moving to a different aspect of the requirements
- You MAY provide examples or options to help the user make decisions
- You SHOULD summarize what has been established so far and identify specific gaps
- You MAY suggest conducting research to inform requirements decisions

### Research Limitations
If you cannot access needed information:
- You SHOULD document what information is missing
- You SHOULD suggest alternative approaches based on available information
- You MAY ask the user to provide additional context or documentation
- You SHOULD continue with available information rather than blocking progress

### Design Complexity
If the design becomes too complex or unwieldy:
- You SHOULD suggest breaking it down into smaller, more manageable components
- You SHOULD focus on core functionality first
- You MAY suggest a phased approach to implementation
- You SHOULD return to requirements clarification to prioritize features if needed
